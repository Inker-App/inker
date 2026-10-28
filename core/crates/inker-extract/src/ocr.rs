//! OCR sources for Linux: a fixture reader and a refusal.

use camino::{Utf8Path, Utf8PathBuf};
use inker_core::{Error, Extracted, PageText, Result, TextOrigin, ports::OcrSource};
use serde::Deserialize;

/// Suffix of the OCR fixture sidecar: `w2.pdf` -> `w2.ocr.json`.
pub const OCR_SIDECAR_SUFFIX: &str = ".ocr.json";

/// OCR from a sidecar fixture. What Linux CI uses in place of Apple Vision.
///
/// For `dir/name.ext` it reads `dir/name.ocr.json`, falling back to
/// `dir/name.ext.ocr.json`. Format:
/// `{ "pages": [ { "page": 1, "text": "..." } ] }`, pages 1-based.
#[derive(Debug, Clone, Copy, Default)]
pub struct FixtureOcr;

impl FixtureOcr {
    pub fn new() -> Self {
        Self
    }

    fn sidecar(path: &Utf8Path) -> Option<Utf8PathBuf> {
        let dir = path.parent().unwrap_or(Utf8Path::new(""));
        let by_stem = path
            .file_stem()
            .map(|s| dir.join(format!("{s}{OCR_SIDECAR_SUFFIX}")));
        let by_name = path
            .file_name()
            .map(|n| dir.join(format!("{n}{OCR_SIDECAR_SUFFIX}")));
        [by_stem, by_name]
            .into_iter()
            .flatten()
            .find(|p| p.is_file())
    }
}

#[derive(Deserialize)]
struct Fixture {
    pages: Vec<FixturePage>,
}

#[derive(Deserialize)]
struct FixturePage {
    page: u32,
    text: String,
}

impl OcrSource for FixtureOcr {
    fn ocr(&self, path: &Utf8Path) -> Result<Extracted> {
        let sidecar =
            Self::sidecar(path).ok_or_else(|| Error::Extract("no OCR fixture sidecar".into()))?;
        let raw = std::fs::read_to_string(&sidecar)?;
        // serde_json messages can quote input; report position only.
        let fixture: Fixture = serde_json::from_str(&raw).map_err(|e| {
            Error::Extract(format!(
                "OCR fixture is malformed at line {} column {}",
                e.line(),
                e.column()
            ))
        })?;
        if fixture.pages.iter().any(|p| p.page == 0) {
            return Err(Error::Extract("OCR fixture pages are 1-based".into()));
        }
        let mut pages: Vec<PageText> = fixture
            .pages
            .into_iter()
            .map(|p| PageText {
                page: p.page,
                text: p.text,
                origin: TextOrigin::Ocr,
            })
            .collect();
        pages.sort_by_key(|p| p.page);
        tracing::debug!(pages = pages.len(), "read OCR fixture");
        Ok(Extracted {
            pages,
            fields: Vec::new(),
        })
    }
}

/// No OCR available. Always fails with `Error::Extract`.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoOcr;

impl OcrSource for NoOcr {
    fn ocr(&self, _path: &Utf8Path) -> Result<Extracted> {
        Err(Error::Extract("OCR is not available".into()))
    }
}
