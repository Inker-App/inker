//! The macOS capabilities the core needs, supplied by Swift through one
//! UniFFI callback interface and adapted here to the core's ports
//! (`KeyStore`, `TextSource`, `OcrSource`; stack-decision section 3.3).
//!
//! Real values cross this boundary inbound only: Swift hands Rust the text it
//! read and the key it holds. Nothing here logs them, and the records'
//! `Debug` output carries lengths, never content.

use std::fmt;
use std::sync::Arc;

use camino::Utf8Path;
use inker_core::ports::{KeyStore, OcrSource, TextSource};
use inker_core::{Error, Extracted, FormField, PageText, TextOrigin};

/// What a Swift provider throws. The reason must be safe to log: no document
/// text, no values, no file names.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum ProviderError {
    #[error("provider failed: {reason}")]
    Failed { reason: String },
}

impl From<uniffi::UnexpectedUniFFICallbackError> for ProviderError {
    fn from(e: uniffi::UnexpectedUniFFICallbackError) -> Self {
        Self::Failed { reason: e.reason }
    }
}

/// One page of text as PDFKit or Vision read it. `page` is 1-based.
#[derive(Clone, PartialEq, Eq, uniffi::Record)]
pub struct ExtractedPage {
    pub page: u32,
    pub text: String,
}

/// An AcroForm widget: PDFKit's `fieldName` and `widgetStringValue`.
#[derive(Clone, PartialEq, Eq, uniffi::Record)]
pub struct ExtractedField {
    pub name: String,
    pub value: String,
    pub page: Option<u32>,
}

/// Everything a provider read from one file.
#[derive(Clone, PartialEq, Eq, Default, uniffi::Record)]
pub struct ExtractedFfi {
    pub pages: Vec<ExtractedPage>,
    pub fields: Vec<ExtractedField>,
}

impl fmt::Debug for ExtractedPage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExtractedPage")
            .field("page", &self.page)
            .field("len", &self.text.len())
            .finish()
    }
}

impl fmt::Debug for ExtractedField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExtractedField")
            .field("name", &self.name)
            .field("value_len", &self.value.len())
            .field("page", &self.page)
            .finish()
    }
}

impl fmt::Debug for ExtractedFfi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExtractedFfi")
            .field("pages", &self.pages)
            .field("fields", &self.fields)
            .finish()
    }
}

/// Implemented in Swift: Keychain, PDFKit and Vision. Called from Rust
/// background threads, never the main thread, and may block.
#[uniffi::export(callback_interface)]
pub trait HostProviders: Send + Sync {
    /// The 32-byte vault key, created and stored on first use.
    fn load_or_create_key(&self) -> Result<Vec<u8>, ProviderError>;
    /// The PDF's text layer and form fields, or `None` when it has no usable
    /// text layer (a scan).
    fn pdf_text(&self, path: String) -> Result<Option<ExtractedFfi>, ProviderError>;
    /// OCR of an image, or of every page of a scanned PDF.
    fn ocr(&self, path: String) -> Result<ExtractedFfi, ProviderError>;
}

/// Shared by the three adapters.
pub(crate) type SharedProviders = Arc<dyn HostProviders>;

/// Convert a provider's result, stamping every page with `origin`.
pub(crate) fn to_extracted(ffi: ExtractedFfi, origin: TextOrigin) -> Extracted {
    Extracted {
        pages: ffi
            .pages
            .into_iter()
            .map(|p| PageText {
                page: p.page,
                text: p.text,
                origin,
            })
            .collect(),
        fields: ffi
            .fields
            .into_iter()
            .map(|f| FormField {
                name: f.name,
                value: f.value,
                page: f.page,
            })
            .collect(),
    }
}

/// Whether anything was read: a page with non-blank text, or a form field
/// with a value.
pub(crate) fn has_text(e: &Extracted) -> bool {
    e.pages.iter().any(|p| !p.text.trim().is_empty())
        || e.fields.iter().any(|f| !f.value.trim().is_empty())
}

fn provider_error(make: fn(String) -> Error, e: ProviderError) -> Error {
    let ProviderError::Failed { reason } = e;
    make(reason)
}

/// `KeyStore` over [`HostProviders::load_or_create_key`].
pub(crate) struct ProviderKeyStore(pub(crate) SharedProviders);

impl KeyStore for ProviderKeyStore {
    fn load_or_create(&self) -> inker_core::Result<[u8; 32]> {
        let bytes = self
            .0
            .load_or_create_key()
            .map_err(|e| provider_error(Error::Vault, e))?;
        bytes
            .as_slice()
            .try_into()
            .map_err(|_| Error::Vault("the key store returned a key of the wrong length".into()))
    }
}

/// `TextSource` over [`HostProviders::pdf_text`].
pub(crate) struct ProviderTextSource(pub(crate) SharedProviders);

impl TextSource for ProviderTextSource {
    fn extract(&self, path: &Utf8Path) -> inker_core::Result<Option<Extracted>> {
        let got = self
            .0
            .pdf_text(path.to_string())
            .map_err(|e| provider_error(Error::Extract, e))?;
        Ok(got
            .map(|f| to_extracted(f, TextOrigin::TextLayer))
            .filter(has_text))
    }
}

/// `OcrSource` over [`HostProviders::ocr`].
pub(crate) struct ProviderOcrSource(pub(crate) SharedProviders);

impl OcrSource for ProviderOcrSource {
    fn ocr(&self, path: &Utf8Path) -> inker_core::Result<Extracted> {
        let got = self
            .0
            .ocr(path.to_string())
            .map_err(|e| provider_error(Error::Extract, e))?;
        Ok(to_extracted(got, TextOrigin::Ocr))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn page(n: u32, t: &str) -> ExtractedPage {
        ExtractedPage {
            page: n,
            text: t.into(),
        }
    }

    #[test]
    fn to_extracted_keeps_pages_and_fields_and_stamps_the_origin() {
        let ffi = ExtractedFfi {
            pages: vec![page(1, "one"), page(2, "two")],
            fields: vec![ExtractedField {
                name: "f1".into(),
                value: "v".into(),
                page: Some(2),
            }],
        };
        let e = to_extracted(ffi, TextOrigin::Ocr);
        assert_eq!(e.pages.len(), 2);
        assert_eq!(e.pages[1].page, 2);
        assert_eq!(e.pages[1].text, "two");
        assert!(e.pages.iter().all(|p| p.origin == TextOrigin::Ocr));
        assert_eq!(e.fields[0].name, "f1");
        assert_eq!(e.fields[0].value, "v");
        assert_eq!(e.fields[0].page, Some(2));
    }

    #[test]
    fn blank_pages_and_empty_fields_have_no_text() {
        let blank = to_extracted(
            ExtractedFfi {
                pages: vec![page(1, " \n\t"), page(2, "")],
                fields: vec![ExtractedField {
                    name: "f".into(),
                    value: "  ".into(),
                    page: None,
                }],
            },
            TextOrigin::TextLayer,
        );
        assert!(!has_text(&blank));
        assert!(!has_text(&Extracted::default()));
        let words = to_extracted(
            ExtractedFfi {
                pages: vec![page(1, ""), page(2, "x")],
                fields: vec![],
            },
            TextOrigin::TextLayer,
        );
        assert!(has_text(&words));
        let field_only = to_extracted(
            ExtractedFfi {
                pages: vec![],
                fields: vec![ExtractedField {
                    name: "f".into(),
                    value: "v".into(),
                    page: None,
                }],
            },
            TextOrigin::TextLayer,
        );
        assert!(has_text(&field_only));
    }

    #[test]
    fn debug_never_prints_text_or_values() {
        let ffi = ExtractedFfi {
            pages: vec![page(1, "secret-page-text")],
            fields: vec![ExtractedField {
                name: "f".into(),
                value: "secret-field-value".into(),
                page: None,
            }],
        };
        let d = format!("{ffi:?}");
        assert!(!d.contains("secret"), "{d}");
    }

    #[test]
    fn provider_errors_become_extract_and_vault_errors() {
        let e = || ProviderError::Failed {
            reason: "no".into(),
        };
        assert!(matches!(provider_error(Error::Extract, e()), Error::Extract(r) if r == "no"));
        assert!(matches!(provider_error(Error::Vault, e()), Error::Vault(_)));
        let unexpected = uniffi::UnexpectedUniFFICallbackError::new("boom");
        assert_eq!(
            ProviderError::from(unexpected),
            ProviderError::Failed {
                reason: "boom".into()
            }
        );
    }
}
