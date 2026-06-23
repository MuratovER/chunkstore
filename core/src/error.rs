use std::io;

use thiserror::Error;

/// Errors returned by chunkstore core operations.
#[derive(Debug, Error)]
pub enum ChunkStoreError {
    #[error("backend error: {0}")]
    BackendError(String),

    #[error("digest mismatch: expected {expected}, got {actual}")]
    DigestMismatch { expected: String, actual: String },

    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    #[error("lock poisoned")]
    LockError,

    #[error("io error: {0}")]
    IoError(#[from] io::Error),
}

impl ChunkStoreError {
    pub fn backend(msg: impl Into<String>) -> Self {
        Self::BackendError(msg.into())
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    pub fn invalid_argument(msg: impl Into<String>) -> Self {
        Self::InvalidArgument(msg.into())
    }
}
