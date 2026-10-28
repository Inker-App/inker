//! PDF text layer (`pdf-extract`) and AcroForm fields (`lopdf`).

use std::panic::{AssertUnwindSafe, catch_unwind};

use camino::Utf8Path;
use inker_core::{Error, Extracted, PageText, Result, TextOrigin, ports::TextSource};

use crate::{acroform, kind::is_pdf};

/// Text layer and AcroForm fields of a PDF, in pure Rust.
///
/// Returns `Ok(None)` when no page has any non-whitespace text, so the caller
/// falls back to OCR.
#[derive(Debug, Clone, Copy, Default)]
pub struct PdfTextSource;

impl PdfTextSource {
    pub fn new() -> Self {
        Self
    }
}

/// Number the pages from 1; `None` when every page is blank.
pub(crate) fn pages_from_texts(texts: Vec<String>) -> Option<Vec<PageText>> {
    if texts.iter().all(|t| t.trim().is_empty()) {
        return None;
    }
    Some(
        (1u32..)
            .zip(texts)
            .map(|(page, text)| PageText {
                page,
                text,
                origin: TextOrigin::TextLayer,
            })
            .collect(),
    )
}

impl TextSource for PdfTextSource {
    fn extract(&self, path: &Utf8Path) -> Result<Option<Extracted>> {
        let bytes = std::fs::read(path)?;
        if !is_pdf(&bytes) {
            return Err(Error::Extract("not a PDF".into()));
        }

        // pdf-extract panics on some malformed input; contain it, and keep its
        // panic message (which can quote document bytes) off stderr. The
        // library's error text can quote operands too, so it is not forwarded.
        install_quiet_panic_hook();
        let texts = catch_unwind(AssertUnwindSafe(|| {
            pdf_extract::extract_text_from_mem_by_pages(&bytes)
        }))
        .map_err(|_| Error::Extract("PDF text extraction panicked".into()))?
        .map_err(|_| Error::Extract("PDF text layer could not be read".into()))?;
        let Some(pages) = pages_from_texts(texts) else {
            tracing::debug!("PDF has no text layer; caller should OCR");
            return Ok(None);
        };

        let doc = lopdf::Document::load_mem(&bytes)
            .map_err(|_| Error::Extract("PDF structure could not be read".into()))?;
        let fields = acroform::read_fields(&doc)?;
        tracing::debug!(
            pages = pages.len(),
            fields = fields.len(),
            "extracted PDF text layer"
        );
        Ok(Some(Extracted { pages, fields }))
    }
}

/// Whether a panic from this source file should be kept off stderr: the PDF
/// libraries' panic messages can quote document bytes.
pub fn should_silence_panic(file: &str) -> bool {
    ["/pdf-extract-", "/lopdf-", "\\pdf-extract-", "\\lopdf-"]
        .iter()
        .any(|needle| file.contains(needle))
}

/// Installs, once per process, a panic hook that drops panics raised inside
/// the PDF libraries and forwards everything else to the previous hook.
pub fn install_quiet_panic_hook() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let silenced = info
                .location()
                .map(|l| should_silence_panic(l.file()))
                .unwrap_or(false);
            if silenced {
                tracing::debug!("a PDF library panicked; message withheld");
            } else {
                previous(info);
            }
        }));
    });
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use inker_core::TextOrigin;

    use super::pages_from_texts;

    #[test]
    fn empty_or_whitespace_text_layer_is_none() {
        assert_eq!(pages_from_texts(vec![]), None);
        assert_eq!(pages_from_texts(vec![String::new()]), None);
        assert_eq!(
            pages_from_texts(vec!["\n\n \t".into(), "\u{a0}\n".into()]),
            None
        );
    }

    #[test]
    fn pages_are_one_based_and_keep_blank_pages() {
        let pages = pages_from_texts(vec!["one".into(), "  ".into(), "three".into()]).unwrap();
        let got: Vec<(u32, &str)> = pages.iter().map(|p| (p.page, p.text.as_str())).collect();
        assert_eq!(got, vec![(1, "one"), (2, "  "), (3, "three")]);
        assert!(pages.iter().all(|p| p.origin == TextOrigin::TextLayer));
    }
}
