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
const DEFAULT_CHROME_POLICY_PATH: &str = "/etc/opt/chrome/policies/managed/blockuntu.json";
const DEV_CHROME_POLICY_PATH: &str = "/tmp/blockuntu/chrome/policies/managed/blockuntu.json";
const SYSTEM_NATIVE_HOST_MANIFEST: &str =
    "/usr/lib/mozilla/native-messaging-hosts/blockuntu_native.json";
const CHROME_EXTENSION_ID: &str = "odedgejjcdilkoibeljkeohekonmdfea";
const CHROME_USER_NATIVE_HOST_MANIFEST: &str =
    ".config/google-chrome/NativeMessagingHosts/blockuntu_native.json";
const CHROMIUM_USER_NATIVE_HOST_MANIFEST: &str =
    ".config/chromium/NativeMessagingHosts/blockuntu_native.json";
const CHROME_SYSTEM_NATIVE_HOST_MANIFEST: &str =
    "/etc/opt/chrome/native-messaging-hosts/blockuntu_native.json";
const CHROMIUM_SYSTEM_NATIVE_HOST_MANIFEST: &str =
    "/etc/chromium/native-messaging-hosts/blockuntu_native.json";
const UNSUPPORTED_BROWSER_RULE_ID: &str = "unsupported-browsers-hard";

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
    enforcement_state: Option<String>,
    rules: u32,
    app_rules: u32,
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
fn enforcement_status(socket_path: Option<String>) -> Result<Value, GuiError> {
    let socket = resolve_socket_path(socket_path.as_deref());
    call_daemon(&socket, "enforcement_status", json!({}))
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
    let enforcement = call_daemon(&socket, "enforcement_status", json!({})).ok();

    checks.push(socket_check(Path::new(&socket)));
    if using_dev_socket {
        checks.push(development_runtime_check());
    } else {
        checks.push(systemd_check("blockuntu.socket"));
        checks.push(systemd_check("blockuntu.service"));
        checks.push(systemd_check("blockuntu-watchdog.service"));
        checks.push(systemd_check("blockuntu-hosts.path"));
    }
    if let Some(enforcement) = &enforcement {
        checks.push(enforcement_mode_check(enforcement));
        checks.push(policy_enforcement_check(enforcement));
        checks.push(chrome_policy_enforcement_check(enforcement));
        checks.push(hosts_enforcement_check(enforcement));
    } else if using_dev_socket {
        checks.push(policy_file_check(Path::new(DEV_FIREFOX_POLICY_PATH)));
        checks.push(chrome_policy_file_check(Path::new(DEV_CHROME_POLICY_PATH)));
        checks.push(hosts_file_check(Path::new("/tmp/blockuntu/hosts")));
    } else {
        checks.push(policy_file_check(Path::new(DEFAULT_FIREFOX_POLICY_PATH)));
        checks.push(chrome_policy_file_check(Path::new(
            DEFAULT_CHROME_POLICY_PATH,
        )));
        checks.push(hosts_file_check(Path::new("/etc/hosts")));
    }
    checks.push(native_host_manifest_check());
    checks.push(chrome_native_host_manifest_check());
    checks.push(unsupported_browser_rule_check(&socket));
    checks.push(browser_extension_runtime_check(
        &socket,
        "firefox_extension",
        "firefox_extension",
        "Firefox extension",
    ));
    checks.push(browser_extension_runtime_check(
        &socket,
        "chrome_extension",
        "chrome_extension",
        "Chrome extension",
    ));

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

fn chrome_policy_file_check(path: &Path) -> HealthCheck {
    match fs::read_to_string(path) {
        Ok(contents) => {
            let valid_json = serde_json::from_str::<Value>(&contents).is_ok();
            HealthCheck {
                key: "chrome_policy".to_string(),
                label: "Chrome policy".to_string(),
                state: if valid_json {
                    HealthState::Ok
                } else {
                    HealthState::Error
                },
                detail: path.display().to_string(),
            }
        }
        Err(err) => HealthCheck {
            key: "chrome_policy".to_string(),
            label: "Chrome policy".to_string(),
            state: HealthState::Warn,
            detail: format!("{}: {err}", path.display()),
        },
    }
}

fn hosts_file_check(path: &Path) -> HealthCheck {
    match fs::read_to_string(path) {
        Ok(contents) => {
            let has_block = contents.contains("# BEGIN BLOCKUNTU MANAGED")
                && contents.contains("# END BLOCKUNTU MANAGED");
            HealthCheck {
                key: "hosts_file".to_string(),
                label: "Hosts file".to_string(),
                state: if has_block {
                    HealthState::Ok
                } else {
                    HealthState::Warn
                },
                detail: path.display().to_string(),
            }
        }
        Err(err) => HealthCheck {
            key: "hosts_file".to_string(),
            label: "Hosts file".to_string(),
            state: HealthState::Warn,
            detail: format!("{}: {err}", path.display()),
        },
    }
}

fn enforcement_mode_check(status: &Value) -> HealthCheck {
    let state = status
        .get("enforcement_state")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    HealthCheck {
        key: "enforcement_mode".to_string(),
        label: "Enforcement mode".to_string(),
        state: if state == "active" {
            HealthState::Ok
        } else if state == "stopped" {
            HealthState::Warn
        } else {
            HealthState::Unknown
        },
        detail: state.to_string(),
    }
}

fn policy_enforcement_check(status: &Value) -> HealthCheck {
    let active = status
        .get("enforcement_state")
        .and_then(Value::as_str)
        .unwrap_or("active")
        == "active";
    let policy = status.get("firefox_policy").unwrap_or(&Value::Null);
    let compliant = bool_field(policy, "compliant");
    let private_browsing = bool_field(policy, "private_browsing_enabled");
    let private_browsing_available = bool_field(policy, "private_browsing_available");
    let xpi_exists = bool_field(policy, "extension_xpi_exists");
    let path = string_field(policy, "path").unwrap_or("unknown");
    let detail = string_field(policy, "detail").unwrap_or("no policy detail");
    let state = if !active {
        HealthState::Warn
    } else if compliant && private_browsing && private_browsing_available && xpi_exists {
        HealthState::Ok
    } else {
        HealthState::Error
    };

    HealthCheck {
        key: "firefox_policy".to_string(),
        label: "Firefox policy".to_string(),
        state,
        detail: format!("{path}: {detail}"),
    }
}

fn chrome_policy_enforcement_check(status: &Value) -> HealthCheck {
    let active = status
        .get("enforcement_state")
        .and_then(Value::as_str)
        .unwrap_or("active")
        == "active";
    let policy = status.get("chrome_policy").unwrap_or(&Value::Null);
    let compliant = bool_field(policy, "compliant");
    let force_install = bool_field(policy, "force_install_configured");
    let update_manifest = bool_field(policy, "update_manifest_compliant");
    let override_update_url = bool_field(policy, "override_update_url");
    let path = string_field(policy, "path").unwrap_or("unknown");
    let detail = string_field(policy, "detail").unwrap_or("no Chrome policy detail");
    let state = if !active {
        HealthState::Warn
    } else if compliant && force_install && update_manifest && override_update_url {
        HealthState::Ok
    } else {
        HealthState::Error
    };

    HealthCheck {
        key: "chrome_policy".to_string(),
        label: "Chrome policy".to_string(),
        state,
        detail: format!("{path}: {detail}"),
    }
}

fn hosts_enforcement_check(status: &Value) -> HealthCheck {
    let active = status
        .get("enforcement_state")
        .and_then(Value::as_str)
        .unwrap_or("active")
        == "active";
    let hosts = status.get("hosts_file").unwrap_or(&Value::Null);
    let compliant = bool_field(hosts, "managed_block_compliant");
    let immutable_required = bool_field(hosts, "immutable_required");
    let immutable_state = string_field(hosts, "immutable_state").unwrap_or("unknown");
    let path = string_field(hosts, "path").unwrap_or("unknown");
    let detail = string_field(hosts, "detail").unwrap_or("no hosts detail");
    let immutable_detail = string_field(hosts, "immutable_detail").unwrap_or("");
    let immutable_ok = !immutable_required || immutable_state == "enabled";
    let state = if !active {
        HealthState::Warn
    } else if compliant && immutable_ok {
        HealthState::Ok
    } else {
        HealthState::Error
    };

    HealthCheck {
        key: "hosts_file".to_string(),
        label: "Hosts file".to_string(),
        state,
        detail: format!("{path}: {detail}; {immutable_detail}"),
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

fn chrome_native_host_manifest_check() -> HealthCheck {
    let mut candidates = Vec::new();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        candidates.push(home.join(CHROME_USER_NATIVE_HOST_MANIFEST));
        candidates.push(home.join(CHROMIUM_USER_NATIVE_HOST_MANIFEST));
    }
    candidates.push(PathBuf::from(CHROME_SYSTEM_NATIVE_HOST_MANIFEST));
    candidates.push(PathBuf::from(CHROMIUM_SYSTEM_NATIVE_HOST_MANIFEST));

    let candidate = candidates
        .iter()
        .find(|path| path.exists())
        .cloned()
        .unwrap_or_else(|| PathBuf::from(CHROME_SYSTEM_NATIVE_HOST_MANIFEST));

    match fs::read_to_string(&candidate) {
        Ok(contents) => {
            let parsed = serde_json::from_str::<Value>(&contents);
            let valid_json = parsed.is_ok();
            let expected_origin = format!("chrome-extension://{CHROME_EXTENSION_ID}/");
            let origin_allowed = parsed
                .as_ref()
                .ok()
                .and_then(|value| value.get("allowed_origins"))
                .and_then(Value::as_array)
                .map(|origins| {
                    origins
                        .iter()
                        .any(|origin| origin.as_str() == Some(expected_origin.as_str()))
                })
                .unwrap_or(false);
            HealthCheck {
                key: "chrome_native_host_manifest".to_string(),
                label: "Chrome Native host".to_string(),
                state: if valid_json && origin_allowed {
                    HealthState::Ok
                } else {
                    HealthState::Error
                },
                detail: candidate.display().to_string(),
            }
        }
        Err(err) => HealthCheck {
            key: "chrome_native_host_manifest".to_string(),
            label: "Chrome Native host".to_string(),
            state: HealthState::Warn,
            detail: format!("{}: {err}", candidate.display()),
        },
    }
}

fn unsupported_browser_rule_check(socket_path: &str) -> HealthCheck {
    match call_daemon(socket_path, "config_snapshot", json!({})) {
        Ok(config) => {
            let app_rules = config
                .get("app_rules")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let rule = app_rules.iter().find(|rule| {
                rule.get("id").and_then(Value::as_str) == Some(UNSUPPORTED_BROWSER_RULE_ID)
            });
            let hard_enabled = rule
                .map(|rule| {
                    rule.get("enabled")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                        && rule.get("tier").and_then(Value::as_str) == Some("hard")
                })
                .unwrap_or(false);
            let matcher_count = rule
                .and_then(|rule| rule.get("matchers"))
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0);

            HealthCheck {
                key: "unsupported_browser_hard_block".to_string(),
                label: "Unsupported browsers".to_string(),
                state: if hard_enabled {
                    HealthState::Ok
                } else {
                    HealthState::Error
                },
                detail: if hard_enabled {
                    format!("Tier 1 hard app rule active with {matcher_count} matcher(s)")
                } else {
                    "mandatory Tier 1 hard app rule is missing or disabled".to_string()
                },
            }
        }
        Err(err) => HealthCheck {
            key: "unsupported_browser_hard_block".to_string(),
            label: "Unsupported browsers".to_string(),
            state: HealthState::Unknown,
            detail: format!("daemon config unavailable: {err}"),
        },
    }
}

fn browser_extension_runtime_check(
    socket_path: &str,
    component: &str,
    key: &str,
    label: &str,
) -> HealthCheck {
    match call_daemon(
        socket_path,
        "extension_status",
        json!({ "component": component }),
    ) {
        Ok(status) => {
            let state = status
                .get("state")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            HealthCheck {
                key: key.to_string(),
                label: label.to_string(),
                state: browser_extension_health_state(state),
                detail: browser_extension_health_detail(&status),
            }
        }
        Err(err) => HealthCheck {
            key: key.to_string(),
            label: label.to_string(),
            state: HealthState::Unknown,
            detail: format!("daemon {label} status unavailable: {err}"),
        },
    }
}

fn browser_extension_health_state(state: &str) -> HealthState {
    match state {
        "active" => HealthState::Ok,
        "stale" => HealthState::Error,
        "missing" => HealthState::Warn,
        _ => HealthState::Unknown,
    }
}

fn browser_extension_health_detail(status: &Value) -> String {
    let state = status
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let detail = status
        .get("detail")
        .and_then(Value::as_str)
        .unwrap_or("daemon returned no detail");
    let identity = browser_extension_identity(status);

    if identity.is_empty() {
        format!("{state}: {detail}")
    } else {
        format!("{state}: {detail}; {identity}")
    }
}

fn browser_extension_identity(status: &Value) -> String {
    let browser = status.get("browser").and_then(Value::as_str);
    let extension_id = status.get("extension_id").and_then(Value::as_str);
    let extension_version = status.get("extension_version").and_then(Value::as_str);

    match (browser, extension_id, extension_version) {
        (Some(browser), Some(extension_id), Some(extension_version)) => {
            format!("{browser}; {extension_id}, version {extension_version}")
        }
        (None, Some(extension_id), Some(extension_version)) => {
            format!("{extension_id}, version {extension_version}")
        }
        (Some(browser), Some(extension_id), None) => format!("{browser}; {extension_id}"),
        (None, Some(extension_id), None) => extension_id.to_string(),
        (Some(browser), None, Some(extension_version)) => {
            format!("{browser}; version {extension_version}")
        }
        (None, None, Some(extension_version)) => format!("version {extension_version}"),
        (Some(_), None, None) => String::new(),
        (None, None, None) => String::new(),
    }
}

fn bool_field(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn string_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            daemon_rpc,
            daemon_status,
            enforcement_status,
            config_snapshot,
            recent_events,
            evaluate_url,
            request_unlock,
            system_health
        ])
        .run(tauri::generate_context!())
        .expect("error while running BlocKuntu GUI");
}
