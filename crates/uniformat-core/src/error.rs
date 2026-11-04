//! Error type shared by every UniFormat crate.

/// Result alias.
pub type Result<T> = std::result::Result<T, Error>;

/// Error of a read, write or registry operation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// I/O error (file system, stream).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The file is corrupt, truncated or not of the expected format.
    #[error("{format}: {message}")]
    Format {
        /// Name of the format that reported the problem.
        format: &'static str,
        /// Description (never contains raw data from the file).
        message: String,
    },
    /// A value cannot be represented in the target format.
    #[error("{format}: cannot represent value: {message}")]
    Unrepresentable {
        /// Target format.
        format: &'static str,
        /// Description with the position of the value.
        message: String,
    },
    /// Valid request that the format does not support.
    #[error("unsupported: {0}")]
    Unsupported(String),
    /// No registered format matches the name or the file extension.
    #[error("unknown format: {0}")]
    UnknownFormat(String),
    /// Invalid option or column name.
    #[error("invalid option {key:?}: {message}")]
    InvalidOption {
        /// Option (or argument) name.
        key: String,
        /// Description.
        message: String,
    },
    /// Arrow error while building or converting a batch.
    #[error("Arrow error: {0}")]
    Arrow(#[from] arrow_schema::ArrowError),
}

impl Error {
    /// A [`Error::Format`] error for `format`.
    pub fn format(format: &'static str, message: impl std::fmt::Display) -> Self {
        Self::Format {
            format,
            message: message.to_string(),
        }
    }

    /// An [`Error::InvalidOption`] error.
    pub fn option(key: impl Into<String>, message: impl std::fmt::Display) -> Self {
        Self::InvalidOption {
            key: key.into(),
            message: message.to_string(),
        }
    }
}
