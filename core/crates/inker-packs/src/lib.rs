//! inker-packs: loads detection packs (form templates and codename rules).
//!
//! A pack is a directory with a `pack.toml` manifest and a `forms/` folder of
//! TOML form templates. The tax pack (`packs/tax` at the repo root) is embedded
//! in the binary at build time, so the CLI works without a packs path; an
//! override directory can replace it.
//!
//! Packs are data, not code: a tax professional can add a form template
//! without writing Rust. Everything is validated at load time (TOML shape,
//! regex syntax, roles, treatments), so a broken template fails loudly here
//! rather than silently missing values during detection.

mod error;
mod model;

pub use error::PackError;
pub use model::*;

/// Alias for results in this crate.
pub type Result<T> = std::result::Result<T, PackError>;
