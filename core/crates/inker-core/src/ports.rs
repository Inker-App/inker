//! Ports: capabilities the core needs from the host.
//!
//! On macOS the Swift shell implements these with PDFKit, Vision, ImageIO and
//! the Keychain, and hands them to the core through UniFFI. On Linux (CI and
//! the headless CLI) `inker-extract` provides pure-Rust implementations so the
//! whole pipeline is testable without a Mac.

use camino::Utf8Path;

use crate::{Extracted, Result};

/// Reads the text layer and form fields of a PDF. Returns `Ok(None)` when the
/// file has no usable text layer (a scan), so the caller falls back to OCR.
pub trait TextSource: Send + Sync {
    fn extract(&self, path: &Utf8Path) -> Result<Option<Extracted>>;
}

/// Runs OCR on an image or a scanned PDF.
pub trait OcrSource: Send + Sync {
    fn ocr(&self, path: &Utf8Path) -> Result<Extracted>;
}

/// Provides the 32-byte vault key. On macOS this is the Keychain.
pub trait KeyStore: Send + Sync {
    /// Load the key, creating and persisting a fresh random one on first use.
    fn load_or_create(&self) -> Result<[u8; 32]>;
}

/// Where the host wants things. The private folder is the user's choice at
/// onboarding; the data dir is Application Support on macOS.
#[derive(Debug, Clone)]
pub struct HostPaths {
    pub private_folder: camino::Utf8PathBuf,
    pub data_dir: camino::Utf8PathBuf,
}
