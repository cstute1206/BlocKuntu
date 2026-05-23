use std::env;
use std::fs;
use std::os::fd::FromRawFd;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use tokio::net::UnixListener;

use crate::error::{DaemonError, Result};

const SD_LISTEN_FDS_START: i32 = 3;

pub fn listener_from_systemd_or_path(
    socket_path: &Path,
    dev_bind_socket: bool,
) -> Result<UnixListener> {
    if let Some(listener) = listener_from_systemd()? {
        return Ok(listener);
    }

    if !dev_bind_socket {
        return Err(DaemonError::SocketActivationUnavailable(
            socket_path.to_path_buf(),
        ));
    }

    bind_development_socket(socket_path)
}

fn listener_from_systemd() -> Result<Option<UnixListener>> {
    let listen_fds = match env::var("LISTEN_FDS") {
        Ok(value) => value.parse::<usize>().unwrap_or(0),
        Err(_) => 0,
    };
    if listen_fds == 0 {
        return Ok(None);
    }
    if listen_fds != 1 {
        return Err(DaemonError::InvalidSocketActivation { fds: listen_fds });
    }

    if let Ok(listen_pid) = env::var("LISTEN_PID") {
        if listen_pid.parse::<u32>().ok() != Some(std::process::id()) {
            return Ok(None);
        }
    }

    let std_listener =
        unsafe { std::os::unix::net::UnixListener::from_raw_fd(SD_LISTEN_FDS_START) };
    std_listener.set_nonblocking(true)?;
    Ok(Some(UnixListener::from_std(std_listener)?))
}

fn bind_development_socket(socket_path: &Path) -> Result<UnixListener> {
    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent)?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o755))?;
    }
    match fs::remove_file(socket_path) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(err.into()),
    }

    let listener = UnixListener::bind(socket_path)?;
    fs::set_permissions(socket_path, fs::Permissions::from_mode(0o660))?;
    Ok(listener)
}
