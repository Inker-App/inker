//! Pack loading errors. Messages name the file and field, never document values.

use thiserror::Error;

/// Why a pack failed to load.
#[derive(Debug, Error)]
pub enum PackError {
    /// The file is not valid TOML or does not match the schema.
    #[error("{file}: {message}")]
    Parse { file: String, message: String },

    /// A regex in a template does not compile.
    #[error("{file}: field {field}: bad regex: {message}")]
    Regex {
        file: String,
        field: String,
        message: String,
    },

    /// The TOML parsed but breaks a rule (unknown role, missing form, ...).
    #[error("{file}: {message}")]
    Invalid { file: String, message: String },

    #[error("{path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

impl From<PackError> for inker_core::Error {
    fn from(e: PackError) -> Self {
        inker_core::Error::Detect(e.to_string())
    }
}
