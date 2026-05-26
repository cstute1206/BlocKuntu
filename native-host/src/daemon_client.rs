use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::error::{NativeHostError, Result};
use crate::native_messaging::MAX_MESSAGE_BYTES;

#[derive(Debug)]
pub struct DaemonClient {
    socket_path: PathBuf,
    timeout: Duration,
    revival: Option<DaemonRevivalConfig>,
    last_revival_attempt: Mutex<Option<Instant>>,
}

#[derive(Debug, Clone)]
pub struct DaemonRevivalConfig {
    command: PathBuf,
    wait_timeout: Duration,
    retry_interval: Duration,
    min_interval: Duration,
}

pub trait DaemonTransport {
    fn send(&self, payload: &[u8]) -> Result<Vec<u8>>;
}

impl DaemonClient {
    pub fn new(socket_path: impl Into<PathBuf>, timeout: Duration) -> Self {
        Self {
            socket_path: socket_path.into(),
            timeout,
            revival: None,
            last_revival_attempt: Mutex::new(None),
        }
    }

    pub fn with_revival(mut self, revival: DaemonRevivalConfig) -> Self {
        self.revival = Some(revival);
        self
    }

    pub fn send(&self, payload: &[u8]) -> Result<Vec<u8>> {
        self.send_with_optional_revival(payload)
    }
}

impl DaemonTransport for DaemonClient {
    fn send(&self, payload: &[u8]) -> Result<Vec<u8>> {
        self.send_with_optional_revival(payload)
    }
}

impl DaemonRevivalConfig {
    pub fn new(
        command: PathBuf,
        wait_timeout: Duration,
        retry_interval: Duration,
        min_interval: Duration,
    ) -> Self {
        Self {
            command,
            wait_timeout,
            retry_interval,
            min_interval,
        }
    }
}

impl DaemonClient {
    fn send_with_optional_revival(&self, payload: &[u8]) -> Result<Vec<u8>> {
        match forward_to_daemon(&self.socket_path, self.timeout, payload) {
            Ok(response) => Ok(response),
            Err(first_error) => {
                let Some(revival) = &self.revival else {
                    return Err(first_error);
                };
                if !should_attempt_revival(&first_error) {
                    return Err(first_error);
                }

                self.spawn_revival_command_if_due(revival);
                self.retry_after_revival(payload, revival)
                    .or(Err(first_error))
            }
        }
    }

    fn spawn_revival_command_if_due(&self, revival: &DaemonRevivalConfig) {
        let now = Instant::now();
        let should_spawn = {
            let mut last_attempt = self
                .last_revival_attempt
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let should_spawn = last_attempt
                .map(|last| now.duration_since(last) >= revival.min_interval)
                .unwrap_or(true);
            if should_spawn {
                *last_attempt = Some(now);
            }
            should_spawn
        };

        if !should_spawn {
            return;
        }

        match Command::new(&revival.command)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => {
                eprintln!(
                    "blockuntu-native: started daemon revival command {} as pid {}",
                    revival.command.display(),
                    child.id()
                );
            }
            Err(err) => {
                eprintln!(
                    "blockuntu-native: failed to start daemon revival command {}: {err}",
                    revival.command.display()
                );
            }
        }
    }

    fn retry_after_revival(
        &self,
        payload: &[u8],
        revival: &DaemonRevivalConfig,
    ) -> Result<Vec<u8>> {
        let deadline = Instant::now() + revival.wait_timeout;

        loop {
            if !revival.retry_interval.is_zero() {
                std::thread::sleep(revival.retry_interval);
            }

            match forward_to_daemon(&self.socket_path, self.timeout, payload) {
                Ok(response) => return Ok(response),
                Err(err) if should_attempt_revival(&err) && Instant::now() < deadline => {}
                Err(err) => return Err(err),
            }
        }
    }
}

fn should_attempt_revival(error: &NativeHostError) -> bool {
    matches!(
        error,
        NativeHostError::Io(err)
            if matches!(
                err.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
            )
    )
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io;
    use std::os::unix::fs::PermissionsExt;
    use std::thread;
    use std::time::SystemTime;

    use super::*;

    #[test]
    fn connection_failures_are_revival_candidates() {
        assert!(should_attempt_revival(&NativeHostError::Io(
            io::Error::from(io::ErrorKind::NotFound)
        )));
        assert!(should_attempt_revival(&NativeHostError::Io(
            io::Error::from(io::ErrorKind::ConnectionRefused)
        )));
        assert!(!should_attempt_revival(&NativeHostError::Io(
            io::Error::from(io::ErrorKind::TimedOut)
        )));
    }

    #[test]
    fn due_revival_command_is_spawned() {
        let root = std::env::temp_dir().join(format!(
            "blockuntu-native-revive-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .expect("system time should be after epoch")
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("temp directory should be created");

        let marker = root.join("started");
        let script = root.join("revive.sh");
        fs::write(
            &script,
            format!(
                "#!/usr/bin/env bash\nset -euo pipefail\nprintf started > \"{}\"\n",
                marker.display()
            ),
        )
        .expect("revival script should be written");
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755))
            .expect("revival script should be executable");

        let client = DaemonClient::new(root.join("missing.sock"), Duration::from_millis(20))
            .with_revival(DaemonRevivalConfig::new(
                script,
                Duration::from_millis(20),
                Duration::from_millis(5),
                Duration::from_millis(0),
            ));

        let revival = client.revival.as_ref().expect("revival should be set");
        client.spawn_revival_command_if_due(revival);

        for _ in 0..20 {
            if marker.exists() {
                let _ = fs::remove_dir_all(&root);
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }

        panic!("revival command did not create marker");
    }
}
