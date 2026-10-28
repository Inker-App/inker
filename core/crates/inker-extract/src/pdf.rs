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

        // pdf-extract panics on some malformed input; contain it.
        let texts = catch_unwind(AssertUnwindSafe(|| {
            pdf_extract::extract_text_from_mem_by_pages(&bytes)
        }))
        .map_err(|_| Error::Extract("PDF text extraction panicked".into()))?
        .map_err(|e| Error::Extract(format!("PDF text layer: {e}")))?;
        let Some(pages) = pages_from_texts(texts) else {
            tracing::debug!("PDF has no text layer; caller should OCR");
            return Ok(None);
        };

        let doc = lopdf::Document::load_mem(&bytes)
            .map_err(|e| Error::Extract(format!("PDF structure: {e}")))?;
        let fields = acroform::read_fields(&doc)?;
        tracing::debug!(
            pages = pages.len(),
            fields = fields.len(),
            "extracted PDF text layer"
        );
        Ok(Some(Extracted { pages, fields }))
    }
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
