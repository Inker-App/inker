//! inker-core: the platform-independent heart of INKER.
//!
//! Owns the vault (real value <-> stable token), token minting, handles and
//! codenames, the document model and its state machine, and the index store.
//! Everything in here builds and tests on Linux. macOS-only capabilities
//! (OCR, PDFKit, Keychain) are reached through the traits in [`ports`], which
//! the Swift shell implements through UniFFI and the CLI implements in Rust.
//!
//! See `docs/stack-decision.md` sections 3.3 to 3.5.

// Tests may unwrap; library code may not (workspace lints).
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod codename;
pub mod error;
pub mod facade;
pub mod find;
pub mod fsutil;
pub mod index;
pub mod ingest;
pub mod peek;
pub mod ports;
pub mod rehydrate;
pub mod tokenize;
pub mod types;
pub mod vault;

pub use error::{Error, Result};
pub use facade::Core;
pub use types::*;
