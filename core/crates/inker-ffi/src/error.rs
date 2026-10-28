//! The one error type that crosses the FFI.

/// Errors surfaced to Swift as `FfiError`. Messages never contain real values.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FfiError {
    /// A codename, path or id the caller passed was malformed.
    #[error("invalid argument: {reason}")]
    InvalidArgument { reason: String },
    /// No document, folder or question with that id.
    #[error("not found: {reason}")]
    NotFound { reason: String },
    /// The backend failed. The reason is safe to show and to log.
    #[error("backend error: {reason}")]
    Backend { reason: String },
}
