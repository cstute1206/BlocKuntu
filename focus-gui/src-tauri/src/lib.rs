use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

const DEFAULT_SOCKET_PATH: &str = "/run/blockuntu/blockuntud.sock";
const DEV_SOCKET_PATH: &str = "/tmp/blockuntu/blockuntud.sock";
const DEFAULT_FIREFOX_POLICY_PATH: &str = "/etc/firefox/policies/policies.json";
const DEV_FIREFOX_POLICY_PATH: &str = "/tmp/blockuntu/firefox/policies.json";
const SYSTEM_NATIVE_HOST_MANIFEST: &str =
    "/usr/lib/mozilla/native-messaging-hosts/blockuntu_native.json";

#[derive(Debug, Error)]
enum GuiError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("daemon JSON-RPC error: {0}")]
    Rpc(String),
    #[error("daemon returned an invalid response")]
    InvalidRpcResponse,
}

impl Serialize for GuiError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DaemonStatus {
    status: String,
    rules: u32,
    schedules: u32,
    allowances: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HealthCheck {
    key: String,
    label: String,
    state: HealthState,
    detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum HealthState {
    Ok,
    Warn,
    Error,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SystemHealth {
    checked_at: DateTime<Utc>,
    socket_path: String,
    checks: Vec<HealthCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UnlockRequest {
    target: String,
    minutes: u32,
    reason: String,
}

#[tauri::command]
fn daemon_rpc(
    method: String,
    params: Option<Value>,
    socket_path: Option<String>,
) -> Result<Value, GuiError> {
    let socket = resolve_socket_path(socket_path.as_deref());
    call_daemon(&socket, &method, params.unwrap_or_else(|| json!({})))
}

#[tauri::command]
fn daemon_status(socket_path: Option<String>) -> Result<DaemonStatus, GuiError> {
    let socket = resolve_socket_path(socket_path.as_deref());
    let value = call_daemon(&socket, "status", json!({}))?;
    Ok(serde_json::from_value(value)?)
}

#[tauri::command]
fn config_snapshot(socket_path: Option<String>) -> Result<Value, GuiError> {
    let socket = resolve_socket_path(socket_path.as_deref());
    call_daemon(&socket, "config_snapshot", json!({}))
}

#[tauri::command]
fn recent_events(limit: Option<u32>, socket_path: Option<String>) -> Result<Value, GuiError> {
    let socket = resolve_socket_path(socket_path.as_deref());
    call_daemon(
        &socket,
        "recent_events",
        json!({ "limit": limit.unwrap_or(50) }),
    )
}

#[tauri::command]
fn evaluate_url(url: String, socket_path: Option<String>) -> Result<Value, GuiError> {
    let socket = resolve_socket_path(socket_path.as_deref());
    call_daemon(
        &socket,
        "evaluate_url",
        json!({
            "url": url,
            "now": Utc::now().to_rfc3339()
        }),
    )
}

#[tauri::command]
fn request_unlock(request: UnlockRequest, socket_path: Option<String>) -> Result<Value, GuiError> {
    let socket = resolve_socket_path(socket_path.as_deref());
    call_daemon(
        &socket,
        "request_unlock",
        json!({
            "target": request.target,
            "minutes": request.minutes,
            "reason": request.reason,
            "now": Utc::now().to_rfc3339()
        }),
    )
}

#[tauri::command]
fn system_health(socket_path: Option<String>) -> SystemHealth {
    let socket = resolve_socket_path(socket_path.as_deref());
    let using_dev_socket = socket == DEV_SOCKET_PATH;
    let mut checks = Vec::new();

    checks.push(socket_check(Path::new(&socket)));
    if using_dev_socket {
        checks.push(development_runtime_check());
        checks.push(policy_file_check(Path::new(DEV_FIREFOX_POLICY_PATH)));
    } else {
        checks.push(systemd_check("blockuntu.socket"));
        checks.push(systemd_check("blockuntu.service"));
        checks.push(systemd_check("blockuntu-watchdog.service"));
        checks.push(systemd_check("blockuntu-hosts.path"));
        checks.push(policy_file_check(Path::new(DEFAULT_FIREFOX_POLICY_PATH)));
    }
    checks.push(native_host_manifest_check());

    SystemHealth {
        checked_at: Utc::now(),
        socket_path: socket,
        checks,
    }
}

fn resolve_socket_path(socket_path: Option<&str>) -> String {
    let explicit = socket_path.map(str::trim).unwrap_or_default();
    if !explicit.is_empty() {
        return explicit.to_string();
    }

    if Path::new(DEFAULT_SOCKET_PATH).exists() {
        DEFAULT_SOCKET_PATH.to_string()
    } else if Path::new(DEV_SOCKET_PATH).exists() {
        DEV_SOCKET_PATH.to_string()
    } else {
        DEFAULT_SOCKET_PATH.to_string()
    }
}

fn call_daemon(socket_path: &str, method: &str, params: Value) -> Result<Value, GuiError> {
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params
    });
    let request_bytes = serde_json::to_vec(&request)?;

    let mut stream = UnixStream::connect(socket_path)?;
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    stream.write_all(&request_bytes)?;
    stream.shutdown(std::net::Shutdown::Write)?;

    let mut response_bytes = Vec::new();
    stream.read_to_end(&mut response_bytes)?;
    let response: Value = serde_json::from_slice(&response_bytes)?;

    if let Some(error) = response.get("error") {
        return Err(GuiError::Rpc(error.to_string()));
    }

    response
        .get("result")
        .cloned()
        .ok_or(GuiError::InvalidRpcResponse)
}

fn development_runtime_check() -> HealthCheck {
    HealthCheck {
        key: "development_runtime".to_string(),
        label: "Development runtime".to_string(),
        state: HealthState::Ok,
        detail: "Using /tmp/blockuntu; production systemd checks are skipped".to_string(),
    }
}

fn socket_check(path: &Path) -> HealthCheck {
    match fs::metadata(path) {
        Ok(metadata) => {
            let mode = metadata.permissions().mode() & 0o777;
            let state = if mode == 0o660 {
                HealthState::Ok
            } else {
                HealthState::Warn
            };
            HealthCheck {
                key: "socket".to_string(),
                label: "Daemon socket".to_string(),
                state,
                detail: format!("{} mode {:o}", path.display(), mode),
            }
        }
        Err(err) => HealthCheck {
            key: "socket".to_string(),
            label: "Daemon socket".to_string(),
            state: HealthState::Error,
            detail: format!("{}: {err}", path.display()),
        },
    }
}

fn systemd_check(unit: &str) -> HealthCheck {
    match Command::new("systemctl")
        .args(["is-active", "--quiet", unit])
        .status()
    {
        Ok(status) if status.success() => HealthCheck {
            key: unit.to_string(),
            label: unit.to_string(),
            state: HealthState::Ok,
            detail: "active".to_string(),
        },
        Ok(status) => HealthCheck {
            key: unit.to_string(),
            label: unit.to_string(),
            state: HealthState::Warn,
            detail: format!("inactive or failed ({status})"),
        },
        Err(err) => HealthCheck {
            key: unit.to_string(),
            label: unit.to_string(),
            state: HealthState::Unknown,
            detail: err.to_string(),
        },
    }
}

fn policy_file_check(path: &Path) -> HealthCheck {
    match fs::read_to_string(path) {
        Ok(contents) => {
            let valid_json = serde_json::from_str::<Value>(&contents).is_ok();
            HealthCheck {
                key: "firefox_policy".to_string(),
                label: "Firefox policy".to_string(),
                state: if valid_json {
                    HealthState::Ok
                } else {
                    HealthState::Error
                },
                detail: path.display().to_string(),
            }
        }
        Err(err) => HealthCheck {
            key: "firefox_policy".to_string(),
            label: "Firefox policy".to_string(),
            state: HealthState::Warn,
            detail: format!("{}: {err}", path.display()),
        },
    }
}

fn native_host_manifest_check() -> HealthCheck {
    let user_manifest = std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".mozilla/native-messaging-hosts/blockuntu_native.json"));

    let candidate = user_manifest
        .filter(|path| path.exists())
        .unwrap_or_else(|| PathBuf::from(SYSTEM_NATIVE_HOST_MANIFEST));

    match fs::read_to_string(&candidate) {
        Ok(contents) => {
            let valid_json = serde_json::from_str::<Value>(&contents).is_ok();
            HealthCheck {
                key: "native_host_manifest".to_string(),
                label: "Native host manifest".to_string(),
                state: if valid_json {
                    HealthState::Ok
                } else {
                    HealthState::Error
                },
                detail: candidate.display().to_string(),
            }
        }
        Err(err) => HealthCheck {
            key: "native_host_manifest".to_string(),
            label: "Native host manifest".to_string(),
            state: HealthState::Warn,
            detail: format!("{}: {err}", candidate.display()),
        },
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            daemon_rpc,
            daemon_status,
            config_snapshot,
            recent_events,
            evaluate_url,
            request_unlock,
            system_health
        ])
        .run(tauri::generate_context!())
        .expect("error while running BlocKuntu GUI");
}
