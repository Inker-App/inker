//! inker-core: the platform-independent heart of INKER.
//!
//! Owns the vault (real value <-> stable token), token minting, handles and
//! codenames, the document model and its state machine, and the index store.
//! Everything in here builds and tests on Linux. macOS-only capabilities
//! (OCR, PDFKit, Keychain) are reached through the traits in [`ports`], which
//! the Swift shell implements through UniFFI and the CLI implements in Rust.
//!
//! See `docs/stack-decision.md` sections 3.3 to 3.5.

pub mod error;
pub mod ports;
pub mod types;

pub use error::{Error, Result};
pub use types::*;
