//! Error type shared across the core crates.

use thiserror::Error;

/// Every fallible core operation returns this.
#[derive(Debug, Error)]
pub enum Error {
    /// A handle string did not parse (`🐙/<codename>` or `ink/<codename>`).
    #[error("invalid handle: {0}")]
    InvalidHandle(String),

    /// The handle parsed but nothing in the index matches it.
    #[error("unknown handle: {0}")]
    UnknownHandle(String),

    /// The document exists but is held in "needs attention" until the user answers.
    #[error("pending review: {0}")]
    PendingReview(String),

    /// The document is still being processed.
    #[error("not ready: {0}")]
    NotReady(String),

    /// Vault could not be opened, decrypted or written.
    #[error("vault: {0}")]
    Vault(String),

    /// Index store (SQLite) failure.
    #[error("index: {0}")]
    Index(String),

    /// Text extraction failed for a file.
    #[error("extract: {0}")]
    Extract(String),

    /// Detection or template failure.
    #[error("detect: {0}")]
    Detect(String),

    /// The file could not be moved or copied into the private folder.
    #[error("ingest: {0}")]
    Ingest(String),

    /// An operation refused because it would place real values somewhere an agent can see.
    #[error("refused: {0}")]
    Refused(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Alias used throughout the core crates.
pub type Result<T> = std::result::Result<T, Error>;
