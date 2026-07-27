use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use focus_core::{emergency_uninstall_code_is_valid, installation_serial_is_valid};
use notify_rust::{Hint, Urgency};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Emitter, Manager, WindowEvent, Wry};
use thiserror::Error;

const DEFAULT_SOCKET_PATH: &str = "/run/blockuntu/blockuntud.sock";
const DEV_SOCKET_PATH: &str = "/tmp/blockuntu/blockuntud.sock";
const DEFAULT_FIREFOX_POLICY_PATH: &str = "/etc/firefox/policies/policies.json";
const DEV_FIREFOX_POLICY_PATH: &str = "/tmp/blockuntu/firefox/policies.json";
const DEFAULT_CHROME_POLICY_PATH: &str = "/etc/opt/chrome/policies/managed/blockuntu.json";
const DEV_CHROME_POLICY_PATH: &str = "/tmp/blockuntu/chrome/policies/managed/blockuntu.json";
const FIREFOX_EXTENSION_IDS: [&str; 2] = [
    "blockuntu@example.local",
    "{a7c3f3c4-6b1e-4c6f-9f2a-8d4e5b7c1a90}",
];
const FIREFOX_COMMANDS: [&str; 2] = ["/usr/bin/firefox", "/bin/firefox"];
const FIREFOX_USER_NATIVE_HOST_MANIFEST: &str =
    ".mozilla/native-messaging-hosts/blockuntu_native.json";
const SYSTEM_NATIVE_HOST_MANIFEST: &str =
    "/usr/lib/mozilla/native-messaging-hosts/blockuntu_native.json";
const FLATPAK_FIREFOX_APP_ROOT: &str = ".var/app/org.mozilla.firefox";
const FLATPAK_FIREFOX_NATIVE_HOST_MANIFEST: &str =
    ".var/app/org.mozilla.firefox/.mozilla/native-messaging-hosts/blockuntu_native.json";
const FLATPAK_FIREFOX_SYSTEMCONFIG_ROOT: &str =
    "flatpak/extension/org.mozilla.firefox.systemconfig";
const SNAP_FIREFOX_APP_ROOT: &str = "snap/firefox/common";
const SNAP_FIREFOX_NATIVE_HOST_MANIFEST: &str =
    "snap/firefox/common/.mozilla/native-messaging-hosts/blockuntu_native.json";
const CHROME_EXTENSION_ID: &str = "odedgejjcdilkoibeljkeohekonmdfea";
const CHROME_COMMANDS: [&str; 4] = [
    "/usr/bin/google-chrome",
    "/usr/bin/google-chrome-stable",
    "/bin/google-chrome",
    "/bin/google-chrome-stable",
];
const CHROMIUM_COMMANDS: [&str; 4] = [
    "/usr/bin/chromium",
    "/usr/bin/chromium-browser",
    "/bin/chromium",
    "/bin/chromium-browser",
];
const CHROME_USER_NATIVE_HOST_MANIFEST: &str =
    ".config/google-chrome/NativeMessagingHosts/blockuntu_native.json";
const CHROMIUM_USER_NATIVE_HOST_MANIFEST: &str =
    ".config/chromium/NativeMessagingHosts/blockuntu_native.json";
const CHROME_SYSTEM_NATIVE_HOST_MANIFEST: &str =
    "/etc/opt/chrome/native-messaging-hosts/blockuntu_native.json";
const CHROMIUM_SYSTEM_NATIVE_HOST_MANIFEST: &str =
    "/etc/chromium/native-messaging-hosts/blockuntu_native.json";
const UNSUPPORTED_BROWSER_RULE_ID: &str = "unsupported-browsers-hard";
const UNINSTALL_PHRASE_FILE: &str = "uninstall-confirmation.txt";
const INSTALLATION_SERIAL_FILE: &str = "/etc/blockuntu/installation-id";
const BUILD_NUMBER: &str = match option_env!("BLOCKUNTU_BUILD_NUMBER") {
    Some(value) => value,
    None => env!("CARGO_PKG_VERSION"),
};
const DEBIAN_PACKAGE_NAME: &str = "blockuntu";
const CONFINED_FIREFOX_SETUP_COMMANDS: [&str; 2] = [
    "/usr/bin/blockuntu-setup-confined-firefox",
    "/bin/blockuntu-setup-confined-firefox",
];
const BROWSER_UNINSTALL_NOTICE_WAIT_SECONDS: u64 = 6;
const TRAY_REFRESH_INTERVAL_SECONDS: u64 = 5;
const TRAY_OPEN_VIEW_EVENT: &str = "blockuntu-open-view";
const TRAY_RUNTIME_REFRESH_EVENT: &str = "blockuntu-runtime-refresh";
const TRAY_MENU_SHOW: &str = "show";
const TRAY_MENU_OPEN_DETOX: &str = "open_detox";
const TRAY_MENU_OPEN_ADMIN: &str = "open_admin";
const TRAY_MENU_REFRESH: &str = "refresh";
const TRAY_MENU_QUIT: &str = "quit";
const DESKTOP_ENTRY_ID: &str = "local.blockuntu.gui";
const NOTIFICATION_TIMEOUT_SECONDS: u64 = 10;

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
    #[error("HOME is not set; cannot store the uninstall confirmation phrase")]
    HomeNotSet,
    #[error("uninstall confirmation phrase does not match")]
    InvalidUninstallPhrase,
    #[error("BlocKuntu installation serial is missing or invalid")]
    InvalidInstallationSerial,
    #[error("GUI uninstall requires pkexec, but pkexec was not found")]
    MissingPkexec,
    #[error("uninstall command failed: {0}")]
    UninstallCommand(String),
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum HealthState {
    Ok,
    Inactive,
    Pending,
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
    reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InstallationInfo {
    installation_serial: Option<String>,
    build_number: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UninstallResult {
    status: String,
    detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PolicyFileResult {
    status: String,
    detail: String,
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    config: Option<Value>,
}

#[derive(Clone)]
struct TrayMenuState {
    daemon_status: MenuItem<Wry>,
    enforcement_status: MenuItem<Wry>,
    detox_status: MenuItem<Wry>,
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
fn export_policy_toml(socket_path: Option<String>) -> Result<PolicyFileResult, GuiError> {
    let Some(path) = policy_export_path() else {
        return Ok(PolicyFileResult {
            status: "cancelled".to_string(),
            detail: "Export cancelled.".to_string(),
            path: None,
            config: None,
        });
    };
    let path = with_toml_extension(path);
    let socket = resolve_socket_path(socket_path.as_deref());
    let value = call_daemon(&socket, "export_policy_toml", json!({}))?;
    let toml = value
        .get("toml")
        .and_then(Value::as_str)
        .ok_or(GuiError::InvalidRpcResponse)?;
    fs::write(&path, toml)?;

    Ok(PolicyFileResult {
        status: "ok".to_string(),
        detail: format!("Policy exported to {}.", path.display()),
        path: Some(path.display().to_string()),
        config: None,
    })
}

#[tauri::command]
fn import_policy_toml(socket_path: Option<String>) -> Result<PolicyFileResult, GuiError> {
    let Some(path) = policy_import_path() else {
        return Ok(PolicyFileResult {
            status: "cancelled".to_string(),
            detail: "Import cancelled.".to_string(),
            path: None,
            config: None,
        });
    };
    let toml = fs::read_to_string(&path)?;
    let socket = resolve_socket_path(socket_path.as_deref());
    let value = call_daemon(
        &socket,
        "import_policy_toml",
        json!({
            "toml": toml,
            "now": Utc::now().to_rfc3339()
        }),
    )?;
    let config = value.get("config").cloned();

    Ok(PolicyFileResult {
        status: "ok".to_string(),
        detail: format!("Policy appended from {}.", path.display()),
        path: Some(path.display().to_string()),
        config,
    })
}

#[tauri::command]
fn evaluate_url(
    url: String,
    socket_path: Option<String>,
    probe: Option<bool>,
) -> Result<Value, GuiError> {
    let socket = resolve_socket_path(socket_path.as_deref());
    call_daemon(
        &socket,
        "evaluate_url",
        json!({
            "url": url,
            "probe": probe.unwrap_or(false),
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
            "reason": request.reason,
            "now": Utc::now().to_rfc3339()
        }),
    )
}

#[tauri::command]
fn uninstall_phrase_configured() -> Result<bool, GuiError> {
    Ok(load_uninstall_phrase().is_ok())
}

#[tauri::command]
fn configure_uninstall_phrase(phrase: String) -> Result<(), GuiError> {
    let phrase = phrase.trim();
    if phrase.chars().count() < 16 {
        return Err(GuiError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "uninstall phrase must contain at least 16 characters",
        )));
    }
    if load_uninstall_phrase().is_ok() {
        return Err(GuiError::Io(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "uninstall phrase is already configured",
        )));
    }
    write_uninstall_phrase(&uninstall_phrase_path()?, phrase)
}

#[tauri::command]
fn installation_info() -> InstallationInfo {
    installation_info_from_path(Path::new(INSTALLATION_SERIAL_FILE))
}

fn installation_info_from_path(path: &Path) -> InstallationInfo {
    InstallationInfo {
        installation_serial: load_installation_serial_from_path(path).ok(),
        build_number: BUILD_NUMBER.to_string(),
    }
}

#[tauri::command]
fn uninstall_blockuntu(phrase: String) -> Result<UninstallResult, GuiError> {
    let candidate = phrase.trim();
    let emergency_authorized = load_installation_serial()
        .ok()
        .is_some_and(|serial| emergency_uninstall_code_is_valid(candidate, &serial));
    if !emergency_authorized {
        if !uninstall_phrase_matches(candidate)? {
            return Err(GuiError::InvalidUninstallPhrase);
        }
    }

    if !debian_package_installed()? {
        return Err(GuiError::UninstallCommand(
            "Debian package blockuntu is not installed on this system".to_string(),
        ));
    }

    let package_removal_lease = prepare_package_removal(emergency_authorized.then_some(candidate))?;
    std::thread::sleep(Duration::from_secs(BROWSER_UNINSTALL_NOTICE_WAIT_SECONDS));

    let pkexec =
        command_path(&["/usr/bin/pkexec", "/bin/pkexec"]).ok_or(GuiError::MissingPkexec)?;
    let dpkg = command_path(&["/usr/bin/dpkg", "/bin/dpkg"]).ok_or_else(|| {
        GuiError::UninstallCommand("dpkg was not found on this system".to_string())
    })?;
    let output = Command::new(pkexec)
        .arg("/usr/bin/env")
        .arg(format!(
            "BLOCKUNTU_PACKAGE_REMOVAL_LEASE={package_removal_lease}"
        ))
        .arg(dpkg)
        .args(["--purge", DEBIAN_PACKAGE_NAME])
        .output()?;

    if !output.status.success() {
        return Err(GuiError::UninstallCommand(command_failure_detail(
            "dpkg --purge blockuntu",
            &output,
        )));
    }

    Ok(UninstallResult {
        status: "ok".to_string(),
        detail:
            "BlocKuntu package removal completed. Close this window after reviewing the result."
                .to_string(),
    })
}

#[tauri::command]
fn system_health(socket_path: Option<String>) -> SystemHealth {
    let socket = resolve_socket_path(socket_path.as_deref());
    let using_dev_socket = socket == DEV_SOCKET_PATH;
    let mut checks = Vec::new();
    let enforcement = call_daemon(&socket, "enforcement_status", json!({})).ok();
    let system_firefox_present = system_firefox_available();
    let chrome_family_present = chrome_family_available();

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
        if system_firefox_present {
            checks.push(policy_enforcement_check(enforcement));
        }
        if chrome_family_present {
            checks.push(chrome_policy_enforcement_check(enforcement));
        }
        checks.push(hosts_enforcement_check(enforcement));
    } else if using_dev_socket {
        if system_firefox_present {
            checks.push(policy_file_check(Path::new(DEV_FIREFOX_POLICY_PATH)));
        }
        if chrome_family_present {
            checks.push(chrome_policy_file_check(Path::new(DEV_CHROME_POLICY_PATH)));
        }
        checks.push(hosts_file_check(Path::new("/tmp/blockuntu/hosts")));
    } else {
        if system_firefox_present {
            checks.push(policy_file_check(Path::new(DEFAULT_FIREFOX_POLICY_PATH)));
        }
        if chrome_family_present {
            checks.push(chrome_policy_file_check(Path::new(
                DEFAULT_CHROME_POLICY_PATH,
            )));
        }
        checks.push(hosts_file_check(Path::new("/etc/hosts")));
    }
    if system_firefox_present {
        checks.push(native_host_manifest_check());
    }
    checks.extend(confined_firefox_native_host_checks());
    checks.extend(confined_firefox_policy_checks());
    if chrome_family_present {
        checks.push(chrome_native_host_manifest_check());
    }
    checks.push(unsupported_browser_rule_check(&socket));
    if firefox_family_available() {
        checks.push(browser_extension_runtime_check(
            &socket,
            "firefox_extension",
            "firefox_extension",
            "Firefox extension",
        ));
    }
    if chrome_family_present {
        checks.push(browser_extension_runtime_check(
            &socket,
            "chrome_extension",
            "chrome_extension",
            "Chrome extension",
        ));
    }

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

fn prepare_package_removal(emergency_code: Option<&str>) -> Result<String, GuiError> {
    let socket = resolve_socket_path(None);
    let result = call_daemon(
        &socket,
        "prepare_uninstall",
        json!({
            "now": Utc::now().to_rfc3339(),
            "emergency_code": emergency_code
        }),
    )?;
    result
        .get("package_removal_lease")
        .and_then(Value::as_str)
        .filter(|lease| !lease.trim().is_empty())
        .map(str::to_string)
        .ok_or(GuiError::InvalidRpcResponse)
}

fn start_confined_firefox_setup() {
    let Some(helper) = command_path(&CONFINED_FIREFOX_SETUP_COMMANDS) else {
        return;
    };

    if let Err(error) = Command::new(helper).args(["--targets", "auto"]).spawn() {
        eprintln!("BlocKuntu confined Firefox setup could not start: {error}");
    }
}

fn load_installation_serial() -> Result<String, GuiError> {
    load_installation_serial_from_path(Path::new(INSTALLATION_SERIAL_FILE))
}

fn load_installation_serial_from_path(path: &Path) -> Result<String, GuiError> {
    let serial = fs::read_to_string(path)?.trim().to_string();
    if !installation_serial_is_valid(&serial) {
        return Err(GuiError::InvalidInstallationSerial);
    }
    Ok(serial)
}

fn load_uninstall_phrase() -> Result<String, GuiError> {
    let path = uninstall_phrase_path()?;
    let phrase = fs::read_to_string(&path)?.trim().to_string();
    if phrase.is_empty() {
        return Err(GuiError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "uninstall phrase is not configured",
        )));
    }
    Ok(phrase)
}

fn uninstall_phrase_matches(candidate: &str) -> Result<bool, GuiError> {
    if candidate.is_empty() {
        return Ok(false);
    }

    Ok(candidate == load_uninstall_phrase()?.trim())
}

fn write_uninstall_phrase(path: &Path, phrase: &str) -> Result<(), GuiError> {
    let parent = path.parent().ok_or_else(|| {
        GuiError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("uninstall phrase path has no parent: {}", path.display()),
        ))
    })?;

    fs::create_dir_all(parent)?;
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(phrase.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

fn uninstall_phrase_path() -> Result<PathBuf, GuiError> {
    Ok(blockuntu_data_dir()?.join(UNINSTALL_PHRASE_FILE))
}

fn blockuntu_data_dir() -> Result<PathBuf, GuiError> {
    if let Some(path) = std::env::var_os("XDG_DATA_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    {
        return Ok(path.join("blockuntu"));
    }

    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .map(|home| home.join(".local/share/blockuntu"))
        .ok_or(GuiError::HomeNotSet)
}

fn debian_package_installed() -> Result<bool, GuiError> {
    let Some(dpkg_query) = command_path(&["/usr/bin/dpkg-query", "/bin/dpkg-query"]) else {
        return Ok(false);
    };
    let output = Command::new(dpkg_query)
        .args(["-W", "-f=${db:Status-Abbrev}", DEBIAN_PACKAGE_NAME])
        .output()?;
    Ok(output.status.success() && String::from_utf8_lossy(&output.stdout).starts_with("ii"))
}

fn command_path(candidates: &[&str]) -> Option<PathBuf> {
    candidates
        .iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
}

fn system_firefox_available() -> bool {
    command_path(&FIREFOX_COMMANDS).is_some()
}

fn firefox_family_available() -> bool {
    system_firefox_available() || flatpak_firefox_available() || snap_firefox_available()
}

fn chrome_family_available() -> bool {
    command_path(&CHROME_COMMANDS).is_some() || command_path(&CHROMIUM_COMMANDS).is_some()
}

fn flatpak_firefox_available() -> bool {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(FLATPAK_FIREFOX_APP_ROOT).exists())
        .unwrap_or(false)
}

fn snap_firefox_available() -> bool {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(SNAP_FIREFOX_APP_ROOT).exists())
        .unwrap_or(false)
}

fn command_failure_detail(command: &str, output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        "no command output".to_string()
    };
    format!("{command} exited with {}: {detail}", output.status)
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

fn policy_export_path() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Export BlocKuntu policy")
        .set_file_name("blockuntu-policy.toml")
        .add_filter("TOML policy", &["toml"])
        .save_file()
}

fn policy_import_path() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title("Append BlocKuntu policy")
        .add_filter("TOML policy", &["toml"])
        .pick_file()
}

fn with_toml_extension(mut path: PathBuf) -> PathBuf {
    if path.extension().is_none() {
        path.set_extension("toml");
    }
    path
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
        } else if state == "uninstalling" {
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
    let managed = policy
        .get("managed")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let deferred = bool_field(policy, "deferred_until_heartbeat");
    let active_after_heartbeat = bool_field(policy, "active_after_heartbeat");
    let compliant = bool_field(policy, "compliant");
    let private_browsing = bool_field(policy, "private_browsing_enabled");
    let private_browsing_available = bool_field(policy, "private_browsing_available");
    let xpi_exists = bool_field(policy, "extension_xpi_exists");
    let path = string_field(policy, "path").unwrap_or("unknown");
    let detail = string_field(policy, "detail").unwrap_or("no policy detail");
    let state = if !active {
        HealthState::Warn
    } else if !managed {
        HealthState::Ok
    } else if deferred && !active_after_heartbeat {
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
    let managed = policy
        .get("managed")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let deferred = bool_field(policy, "deferred_until_heartbeat");
    let active_after_heartbeat = bool_field(policy, "active_after_heartbeat");
    let compliant = bool_field(policy, "compliant");
    let force_install = bool_field(policy, "force_install_configured");
    let update_manifest = bool_field(policy, "update_manifest_compliant");
    let override_update_url = bool_field(policy, "override_update_url");
    let path = string_field(policy, "path").unwrap_or("unknown");
    let detail = string_field(policy, "detail").unwrap_or("no Chrome policy detail");
    let state = if !active {
        HealthState::Warn
    } else if !managed {
        HealthState::Ok
    } else if deferred && !active_after_heartbeat {
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
        .map(|home| home.join(FIREFOX_USER_NATIVE_HOST_MANIFEST));

    let candidate = user_manifest
        .filter(|path| path.exists())
        .unwrap_or_else(|| PathBuf::from(SYSTEM_NATIVE_HOST_MANIFEST));

    firefox_manifest_check(
        "native_host_manifest",
        "Firefox Native host",
        &candidate,
        "Install the system Native Messaging manifest.",
    )
}

fn confined_firefox_native_host_checks() -> Vec<HealthCheck> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };

    let mut checks = Vec::new();
    if home.join(FLATPAK_FIREFOX_APP_ROOT).exists() {
        checks.push(firefox_manifest_check(
            "firefox_flatpak_native_host_manifest",
            "Firefox Flatpak browser integration",
            &home.join(FLATPAK_FIREFOX_NATIVE_HOST_MANIFEST),
            "BlocKuntu automatically prepares this when the GUI starts. Restart Firefox Flatpak; if it remains unavailable, run `blockuntu-setup-confined-firefox` manually.",
        ));
    }
    if home.join(SNAP_FIREFOX_APP_ROOT).exists() {
        checks.push(firefox_manifest_check(
            "firefox_snap_native_host_manifest",
            "Firefox Snap browser integration",
            &home.join(SNAP_FIREFOX_NATIVE_HOST_MANIFEST),
            "BlocKuntu automatically prepares this when the GUI starts. Restart Firefox Snap; if it remains unavailable, run `blockuntu-setup-confined-firefox` manually.",
        ));
    }

    checks
}

fn confined_firefox_policy_checks() -> Vec<HealthCheck> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    if !home.join(FLATPAK_FIREFOX_APP_ROOT).exists() {
        return Vec::new();
    }

    vec![firefox_flatpak_policy_check(&home)]
}

fn firefox_flatpak_policy_check(home: &Path) -> HealthCheck {
    let candidate = flatpak_firefox_policy_path(home);
    match fs::read_to_string(&candidate) {
        Ok(contents) => {
            let parsed = serde_json::from_str::<Value>(&contents);
            let extension_settings = parsed
                .as_ref()
                .ok()
                .and_then(|value| value.get("policies"))
                .and_then(|policies| policies.get("ExtensionSettings"))
                .and_then(|settings| {
                    FIREFOX_EXTENSION_IDS
                        .iter()
                        .find_map(|id| settings.get(*id))
                });
            let force_installed = extension_settings
                .and_then(|settings| settings.get("installation_mode"))
                .and_then(Value::as_str)
                == Some("force_installed");
            let install_url = extension_settings
                .and_then(|settings| settings.get("install_url"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let xpi_exists = install_url
                .strip_prefix("file://")
                .map(|path| Path::new(path).exists())
                .unwrap_or(false);

            HealthCheck {
                key: "firefox_flatpak_policy".to_string(),
                label: "Firefox Flatpak policy".to_string(),
                state: if parsed.is_ok() && force_installed && xpi_exists {
                    HealthState::Ok
                } else {
                    HealthState::Error
                },
                detail: candidate.display().to_string(),
            }
        }
        Err(err) => HealthCheck {
            key: "firefox_flatpak_policy".to_string(),
            label: "Firefox Flatpak policy".to_string(),
            state: HealthState::Warn,
            detail: format!(
                "{}: {err}; BlocKuntu automatically prepares this when the GUI starts. Restart Firefox Flatpak; if it remains unavailable, run `blockuntu-setup-confined-firefox` manually.",
                candidate.display()
            ),
        },
    }
}

fn flatpak_firefox_policy_path(home: &Path) -> PathBuf {
    let root = xdg_data_home(home).join(FLATPAK_FIREFOX_SYSTEMCONFIG_ROOT);
    let arch = Command::new("flatpak")
        .arg("--default-arch")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|stdout| stdout.trim().to_string())
        .filter(|stdout| !stdout.is_empty())
        .unwrap_or_else(|| std::env::consts::ARCH.to_string());

    root.join(arch).join("stable/policies/policies.json")
}

fn xdg_data_home(home: &Path) -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".local/share"))
}

fn firefox_manifest_check(
    key: &str,
    label: &str,
    candidate: &Path,
    missing_detail: &str,
) -> HealthCheck {
    match fs::read_to_string(&candidate) {
        Ok(contents) => {
            let parsed = serde_json::from_str::<Value>(&contents);
            let valid_json = parsed.is_ok();
            let extension_allowed = parsed
                .as_ref()
                .ok()
                .and_then(|value| value.get("allowed_extensions"))
                .and_then(Value::as_array)
                .map(|extensions| {
                    FIREFOX_EXTENSION_IDS.iter().any(|expected| {
                        extensions
                            .iter()
                            .any(|extension| extension.as_str() == Some(expected))
                    })
                })
                .unwrap_or(false);
            let host_path = parsed
                .as_ref()
                .ok()
                .and_then(|value| value.get("path"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let host_executable = executable_file(Path::new(host_path));
            let detail = if host_path.is_empty() {
                candidate.display().to_string()
            } else {
                format!("{} -> {host_path}", candidate.display())
            };
            HealthCheck {
                key: key.to_string(),
                label: label.to_string(),
                state: if valid_json && extension_allowed && host_executable {
                    HealthState::Ok
                } else {
                    HealthState::Error
                },
                detail,
            }
        }
        Err(err) => HealthCheck {
            key: key.to_string(),
            label: label.to_string(),
            state: HealthState::Warn,
            detail: format!("{}: {err}; {missing_detail}", candidate.display()),
        },
    }
}

fn executable_file(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
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
                    format!("Tier 1 hard application rule active with {matcher_count} matcher(s)")
                } else {
                    "mandatory Tier 1 hard application rule is missing".to_string()
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
        "inactive" => HealthState::Inactive,
        "starting" => HealthState::Pending,
        "stale" => HealthState::Error,
        "missing" => HealthState::Error,
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

fn setup_tray(app: &mut App<Wry>) -> tauri::Result<TrayMenuState> {
    let show = MenuItem::with_id(app, TRAY_MENU_SHOW, "Show BlocKuntu", true, None::<&str>)?;
    let open_detox =
        MenuItem::with_id(app, TRAY_MENU_OPEN_DETOX, "Open Detox", true, None::<&str>)?;
    let open_admin = MenuItem::with_id(
        app,
        TRAY_MENU_OPEN_ADMIN,
        "Open Settings",
        true,
        None::<&str>,
    )?;
    let refresh = MenuItem::with_id(app, TRAY_MENU_REFRESH, "Refresh status", true, None::<&str>)?;
    let daemon_status = MenuItem::with_id(
        app,
        "daemon_status",
        "Daemon: Checking",
        false,
        None::<&str>,
    )?;
    let enforcement_status = MenuItem::with_id(
        app,
        "enforcement_status",
        "Enforcement: Checking",
        false,
        None::<&str>,
    )?;
    let detox_status =
        MenuItem::with_id(app, "detox_status", "Detox: Checking", false, None::<&str>)?;
    let quit = MenuItem::with_id(app, TRAY_MENU_QUIT, "Quit GUI", true, None::<&str>)?;
    let separator_one = PredefinedMenuItem::separator(app)?;
    let separator_two = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[
            &show,
            &open_detox,
            &open_admin,
            &separator_one,
            &daemon_status,
            &enforcement_status,
            &detox_status,
            &separator_two,
            &refresh,
            &quit,
        ],
    )?;
    let menu_state = TrayMenuState {
        daemon_status,
        enforcement_status,
        detox_status,
    };

    let menu_state_for_events = menu_state.clone();
    let mut tray = TrayIconBuilder::with_id("blockuntu")
        .menu(&menu)
        .tooltip("BlocKuntu")
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            TRAY_MENU_SHOW => {
                show_main_window(app);
                emit_runtime_refresh(app);
            }
            TRAY_MENU_OPEN_DETOX => {
                show_main_window(app);
                let _ = app.emit(TRAY_OPEN_VIEW_EVENT, "detox");
                emit_runtime_refresh(app);
            }
            TRAY_MENU_OPEN_ADMIN => {
                show_main_window(app);
                let _ = app.emit(TRAY_OPEN_VIEW_EVENT, "admin");
                emit_runtime_refresh(app);
            }
            TRAY_MENU_REFRESH => {
                refresh_tray_menu_async(menu_state_for_events.clone());
            }
            TRAY_MENU_QUIT => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
                emit_runtime_refresh(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;

    refresh_tray_menu_async(menu_state.clone());
    start_tray_refresh_loop(menu_state.clone());

    Ok(menu_state)
}

fn start_tray_refresh_loop(menu: TrayMenuState) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(TRAY_REFRESH_INTERVAL_SECONDS));
        refresh_tray_menu(&menu);
    });
}

fn refresh_tray_menu_async(menu: TrayMenuState) {
    std::thread::spawn(move || {
        refresh_tray_menu(&menu);
    });
}

fn refresh_tray_menu(menu: &TrayMenuState) {
    let socket = resolve_socket_path(None);
    match call_daemon(&socket, "status", json!({})) {
        Ok(status) => {
            let enforcement_state = status
                .get("enforcement_state")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            set_menu_text(&menu.daemon_status, "Daemon: Online");
            set_menu_text(
                &menu.enforcement_status,
                format!("Enforcement: {}", tray_enforcement_label(enforcement_state)),
            );
            update_tray_detox_status(&socket, menu);
            deliver_pending_notifications(&socket);
        }
        Err(_) => {
            set_menu_text(&menu.daemon_status, "Daemon: Offline");
            set_menu_text(&menu.enforcement_status, "Enforcement: Unknown");
            set_menu_text(&menu.detox_status, "Detox: Unknown");
        }
    }
}

fn deliver_pending_notifications(socket: &str) {
    let Ok(value) = call_daemon(socket, "pending_notifications", json!({ "limit": 20 })) else {
        return;
    };
    let Some(notifications) = value.get("notifications").and_then(Value::as_array) else {
        return;
    };
    for notification in notifications {
        let Some(id) = notification.get("id").and_then(Value::as_i64) else {
            continue;
        };
        let Some(title) = notification.get("title").and_then(Value::as_str) else {
            continue;
        };
        let Some(body) = notification.get("body").and_then(Value::as_str) else {
            continue;
        };
        let delivery = notify_rust::Notification::new()
            .appname("BlocKuntu")
            .summary(title)
            .body(body)
            .icon("blockuntu-gui")
            .hint(Hint::DesktopEntry(DESKTOP_ENTRY_ID.to_string()))
            .hint(Hint::Transient(false))
            .urgency(Urgency::Normal)
            .timeout(Duration::from_secs(NOTIFICATION_TIMEOUT_SECONDS))
            .show();
        let (accepted, detail) = match delivery {
            Ok(handle) => {
                retain_notification_handle(handle);
                (
                    true,
                    "accepted by org.freedesktop.Notifications; D-Bus handle retained until close"
                        .to_string(),
                )
            }
            Err(error) => (false, error.to_string()),
        };
        if let Err(error) = call_daemon(
            socket,
            "record_notification_delivery",
            json!({
                "id": id,
                "delivered": accepted,
                "detail": detail
            }),
        ) {
            eprintln!("could not record notification delivery result: {error}");
        }
    }
}

fn retain_notification_handle(handle: notify_rust::NotificationHandle) {
    tauri::async_runtime::spawn(async move {
        handle.wait_for_action_async(|_| {}).await;
    });
}

fn update_tray_detox_status(socket: &str, menu: &TrayMenuState) {
    match call_daemon(
        socket,
        "detox_sessions",
        json!({
            "active_only": true,
            "limit": 80,
            "now": Utc::now().to_rfc3339()
        }),
    ) {
        Ok(value) => {
            let active_count = value
                .get("sessions")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0);
            set_menu_text(&menu.detox_status, format!("Detox: {active_count} active"));
        }
        Err(_) => {
            set_menu_text(&menu.detox_status, "Detox: Unknown");
        }
    }
}

fn tray_enforcement_label(state: &str) -> &'static str {
    match state {
        "active" => "Active",
        "uninstalling" => "Uninstalling",
        _ => "Unknown",
    }
}

fn show_main_window(app: &AppHandle<Wry>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn emit_runtime_refresh(app: &AppHandle<Wry>) {
    let _ = app.emit(TRAY_RUNTIME_REFRESH_EVENT, ());
}

fn set_menu_text<S: AsRef<str>>(item: &MenuItem<Wry>, text: S) {
    let _ = item.set_text(text);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let tray_available = Arc::new(AtomicBool::new(false));
    let tray_available_for_setup = Arc::clone(&tray_available);
    let tray_available_for_window = Arc::clone(&tray_available);

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main_window(app);
            emit_runtime_refresh(app);
        }))
        .setup(move |app| {
            match setup_tray(app) {
                Ok(_) => tray_available_for_setup.store(true, Ordering::SeqCst),
                Err(err) => eprintln!("BlocKuntu tray setup failed: {err}"),
            }
            start_confined_firefox_setup();
            Ok(())
        })
        .on_window_event(move |window, event| {
            if window.label() != "main" || !tray_available_for_window.load(Ordering::SeqCst) {
                return;
            }
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            daemon_rpc,
            daemon_status,
            enforcement_status,
            config_snapshot,
            export_policy_toml,
            import_policy_toml,
            evaluate_url,
            request_unlock,
            installation_info,
            uninstall_phrase_configured,
            configure_uninstall_phrase,
            uninstall_blockuntu,
            system_health
        ])
        .run(tauri::generate_context!())
        .expect("error while running BlocKuntu GUI");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let unique = format!(
            "blockuntu-gui-{name}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        );
        std::env::temp_dir().join(unique)
    }

    #[test]
    fn installation_serial_loader_accepts_the_packaged_format() {
        let path = temp_path("installation-serial");
        fs::write(&path, "BKI-7A91C246-398AF072-5E70DA11-D9B4C83F\n")
            .expect("write installation serial");

        let serial = load_installation_serial_from_path(&path)
            .expect("installation serial should be accepted");

        let _ = fs::remove_file(&path);
        assert_eq!(serial, "BKI-7A91C246-398AF072-5E70DA11-D9B4C83F");
    }

    #[test]
    fn installation_serial_loader_rejects_malformed_content() {
        let path = temp_path("invalid-installation-serial");
        fs::write(&path, "BKI-not-valid\n").expect("write installation serial");

        let result = load_installation_serial_from_path(&path);

        let _ = fs::remove_file(&path);
        assert!(matches!(result, Err(GuiError::InvalidInstallationSerial)));
    }

    #[test]
    fn installation_info_keeps_build_number_when_serial_is_unavailable() {
        let path = temp_path("missing-installation-serial");

        let info = installation_info_from_path(&path);

        assert_eq!(info.installation_serial, None);
        assert!(!info.build_number.trim().is_empty());
    }

    #[test]
    fn browser_extension_health_maps_lifecycle_states_without_false_warnings() {
        assert_eq!(
            browser_extension_health_state("inactive"),
            HealthState::Inactive
        );
        assert_eq!(
            browser_extension_health_state("starting"),
            HealthState::Pending
        );
        assert_eq!(browser_extension_health_state("active"), HealthState::Ok);
        assert_eq!(
            browser_extension_health_state("missing"),
            HealthState::Error
        );
        assert_eq!(browser_extension_health_state("stale"), HealthState::Error);
    }
}
