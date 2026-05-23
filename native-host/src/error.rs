use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum NativeHostError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("native message length {length} exceeds limit {limit}")]
    MessageTooLarge { length: usize, limit: usize },
    #[error("native response is too large")]
    ResponseTooLarge,
    #[error("daemon response exceeded limit {limit}")]
    DaemonResponseTooLarge { limit: usize },
    #[error("daemon returned an empty response from {socket}")]
    EmptyDaemonResponse { socket: PathBuf },
    #[error("daemon response is not valid JSON: {0}")]
    InvalidDaemonResponse(String),
}

pub type Result<T> = std::result::Result<T, NativeHostError>;
