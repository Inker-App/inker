//! Routes a file to the right source.

use std::fmt;

use camino::Utf8Path;
use inker_core::{
    Error, Extracted, PageText, Result, TextOrigin,
    ports::{OcrSource, TextSource},
};

use crate::kind::{FileKind, detect_file_kind};

/// Composes a [`TextSource`] and an [`OcrSource`]:
///
/// - PDF with a text layer: the text source.
/// - PDF without one (the text source returns `None`): OCR.
/// - Image: OCR.
/// - Text file: one page, [`TextOrigin::TextLayer`].
/// - Anything else: `Error::Extract`.
pub struct Extractor {
    text: Box<dyn TextSource>,
    ocr: Box<dyn OcrSource>,
}

impl fmt::Debug for Extractor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Extractor").finish_non_exhaustive()
    }
}

impl Extractor {
    pub fn new(text: impl TextSource + 'static, ocr: impl OcrSource + 'static) -> Self {
        Self {
            text: Box::new(text),
            ocr: Box::new(ocr),
        }
    }

    pub fn extract(&self, path: &Utf8Path) -> Result<Extracted> {
        let kind = detect_file_kind(path);
        tracing::debug!(?kind, "extracting");
        match kind {
            FileKind::Pdf => match self.text.extract(path)? {
                Some(extracted) => Ok(extracted),
                None => self.ocr.ocr(path),
            },
            FileKind::Image => self.ocr.ocr(path),
            FileKind::Text => {
                let bytes = std::fs::read(path)?;
                let text = String::from_utf8(bytes)
                    .map_err(|_| Error::Extract("text file is not UTF-8".into()))?;
                Ok(Extracted {
                    pages: vec![PageText {
                        page: 1,
                        text,
                        origin: TextOrigin::TextLayer,
                    }],
                    fields: Vec::new(),
                })
            }
            FileKind::Unsupported => Err(Error::Extract("unsupported file type".into())),
        }
    }
}
