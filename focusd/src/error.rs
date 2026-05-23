use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum DaemonError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Core(#[from] focus_core::Error),
    #[error(transparent)]
    Config(#[from] focus_core::ConfigError),
    #[error("daemon state lock was poisoned")]
    LockPoisoned,
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("unsupported method: {0}")]
    UnsupportedMethod(String),
    #[error("systemd passed {fds} socket file descriptors; expected exactly one")]
    InvalidSocketActivation { fds: usize },
    #[error("socket activation is unavailable and development bind was not enabled for {0}")]
    SocketActivationUnavailable(PathBuf),
    #[error("process {pid} could not be killed: errno {errno}")]
    KillFailed { pid: u32, errno: i32 },
}

pub type Result<T> = std::result::Result<T, DaemonError>;
