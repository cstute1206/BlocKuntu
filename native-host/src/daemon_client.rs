use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::{NativeHostError, Result};
use crate::native_messaging::MAX_MESSAGE_BYTES;

#[derive(Debug, Clone)]
pub struct DaemonClient {
    socket_path: PathBuf,
    timeout: Duration,
}

pub trait DaemonTransport {
    fn send(&self, payload: &[u8]) -> Result<Vec<u8>>;
}

impl DaemonClient {
    pub fn new(socket_path: impl Into<PathBuf>, timeout: Duration) -> Self {
        Self {
            socket_path: socket_path.into(),
            timeout,
        }
    }

    pub fn send(&self, payload: &[u8]) -> Result<Vec<u8>> {
        forward_to_daemon(&self.socket_path, self.timeout, payload)
    }
}

impl DaemonTransport for DaemonClient {
    fn send(&self, payload: &[u8]) -> Result<Vec<u8>> {
        forward_to_daemon(&self.socket_path, self.timeout, payload)
    }
}

pub fn forward_to_daemon(socket_path: &Path, timeout: Duration, payload: &[u8]) -> Result<Vec<u8>> {
    let mut stream = UnixStream::connect(socket_path)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;

    stream.write_all(payload)?;
    stream.shutdown(std::net::Shutdown::Write)?;

    let response = read_daemon_response(&mut stream)?;
    if response.is_empty() {
        return Err(NativeHostError::EmptyDaemonResponse {
            socket: socket_path.to_path_buf(),
        });
    }

    serde_json::from_slice::<serde_json::Value>(&response)
        .map_err(|err| NativeHostError::InvalidDaemonResponse(err.to_string()))?;

    Ok(response)
}

fn read_daemon_response(stream: &mut UnixStream) -> Result<Vec<u8>> {
    let mut response = Vec::new();
    let mut buffer = [0_u8; 8192];

    loop {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            return Ok(response);
        }

        if response.len() + read > MAX_MESSAGE_BYTES {
            return Err(NativeHostError::DaemonResponseTooLarge {
                limit: MAX_MESSAGE_BYTES,
            });
        }

        response.extend_from_slice(&buffer[..read]);
    }
}
