//! inker-extract: pure-Rust text sources for the pipeline.
//!
//! See `docs/stack-decision.md` section 3.3. On macOS the Swift shell provides
//! PDFKit and Vision implementations of the same traits; these keep the whole
//! pipeline testable on Linux.
//!
//! - [`PdfTextSource`]: PDF text layer (`pdf-extract`) plus AcroForm fields (`lopdf`).
//! - [`FixtureOcr`]: OCR from a `<name>.ocr.json` sidecar, for CI. [`NoOcr`] refuses.
//! - [`detect_file_kind`]: PDF, image, text or unsupported, by magic bytes and extension.
//! - [`Extractor`]: composes a text source and an OCR source.

mod acroform;
mod extractor;
mod kind;
mod ocr;
mod pdf;
mod pdfdoc;

pub use extractor::Extractor;
pub use kind::{FileKind, detect_file_kind};
pub use ocr::{FixtureOcr, NoOcr, OCR_SIDECAR_SUFFIX};
pub use pdf::{PdfTextSource, install_quiet_panic_hook, should_silence_panic};
