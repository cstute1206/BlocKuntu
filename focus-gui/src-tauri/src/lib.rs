use std::fs;
use std::io::{Read, Write};
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
const DEFAULT_LIBREWOLF_POLICY_PATH: &str = "/usr/share/librewolf/distribution/policies.json";
const DEV_LIBREWOLF_POLICY_PATH: &str = "/tmp/blockuntu/librewolf/policies.json";
const DEFAULT_WATERFOX_POLICY_PATH: &str = "/usr/lib/waterfox/distribution/policies.json";
const DEV_WATERFOX_POLICY_PATH: &str = "/tmp/blockuntu/waterfox/policies.json";
const DEFAULT_CHROME_POLICY_PATH: &str = "/etc/opt/chrome/policies/managed/blockuntu.json";
const DEV_CHROME_POLICY_PATH: &str = "/tmp/blockuntu/chrome/policies/managed/blockuntu.json";
const DEFAULT_CHROMIUM_POLICY_PATH: &str = "/etc/chromium/policies/managed/blockuntu.json";
const DEV_CHROMIUM_POLICY_PATH: &str = "/tmp/blockuntu/chromium/policies/managed/blockuntu.json";
const DEFAULT_BRAVE_POLICY_PATH: &str = "/etc/brave/policies/managed/blockuntu.json";
const DEV_BRAVE_POLICY_PATH: &str = "/tmp/blockuntu/brave/policies/managed/blockuntu.json";
const DEFAULT_OPERA_POLICY_PATH: &str = "/etc/opt/opera/policies/managed/blockuntu.json";
const DEV_OPERA_POLICY_PATH: &str = "/tmp/blockuntu/opera/policies/managed/blockuntu.json";
const DEFAULT_EDGE_POLICY_PATH: &str = "/etc/opt/edge/policies/managed/blockuntu.json";
const DEV_EDGE_POLICY_PATH: &str = "/tmp/blockuntu/edge/policies/managed/blockuntu.json";
const DEFAULT_VIVALDI_POLICY_PATH: &str = "/etc/vivaldi/policies/managed/blockuntu.json";
const DEV_VIVALDI_POLICY_PATH: &str = "/tmp/blockuntu/vivaldi/policies/managed/blockuntu.json";
const FIREFOX_EXTENSION_IDS: [&str; 2] = [
    "blockuntu@example.local",
    "{a7c3f3c4-6b1e-4c6f-9f2a-8d4e5b7c1a90}",
];
const FIREFOX_EXTENSION_INSTALL_URL: &str =
    "https://addons.mozilla.org/firefox/downloads/latest/blockuntu/latest.xpi";
const FIREFOX_EXTENSION_STORE_URL: &str =
    "https://addons.mozilla.org/en-US/firefox/addon/blockuntu/";
const CHROME_EXTENSION_STORE_URL: &str =
    "https://chromewebstore.google.com/detail/blockuntu/opfljaancedgklbpnbpjfhdbbhbfpnoc";
const FIREFOX_COMMANDS: [&str; 2] = ["/usr/bin/firefox", "/bin/firefox"];
const LIBREWOLF_COMMANDS: [&str; 3] = [
    "/usr/bin/librewolf",
    "/bin/librewolf",
    "/usr/lib/librewolf/librewolf",
];
const WATERFOX_COMMANDS: [&str; 3] = [
    "/usr/bin/waterfox",
    "/bin/waterfox",
    "/usr/lib/waterfox/waterfox",
];
const FIREFOX_USER_NATIVE_HOST_MANIFEST: &str =
    ".mozilla/native-messaging-hosts/blockuntu_native.json";
const SYSTEM_FIREFOX_NATIVE_HOST_MANIFESTS: [&str; 2] = [
    "/usr/lib/mozilla/native-messaging-hosts/blockuntu_native.json",
    "/usr/lib64/mozilla/native-messaging-hosts/blockuntu_native.json",
];
const LIBREWOLF_USER_NATIVE_HOST_MANIFEST: &str =
    ".librewolf/native-messaging-hosts/blockuntu_native.json";
const WATERFOX_USER_NATIVE_HOST_MANIFEST: &str =
    ".waterfox/native-messaging-hosts/blockuntu_native.json";
const SYSTEM_LIBREWOLF_NATIVE_HOST_MANIFESTS: [&str; 3] = [
    "/usr/lib/librewolf/native-messaging-hosts/blockuntu_native.json",
    "/usr/lib64/librewolf/native-messaging-hosts/blockuntu_native.json",
    "/usr/share/librewolf/native-messaging-hosts/blockuntu_native.json",
];
const SYSTEM_WATERFOX_NATIVE_HOST_MANIFESTS: [&str; 3] = [
    "/usr/lib/waterfox/native-messaging-hosts/blockuntu_native.json",
    "/usr/lib64/waterfox/native-messaging-hosts/blockuntu_native.json",
    "/usr/share/waterfox/native-messaging-hosts/blockuntu_native.json",
];
const FLATPAK_FIREFOX_APP_ROOT: &str = ".var/app/org.mozilla.firefox";
const FLATPAK_FIREFOX_DEPLOYMENT_ROOT: &str = "/var/lib/flatpak/app/org.mozilla.firefox";
const FLATPAK_FIREFOX_USER_DEPLOYMENT_ROOT: &str = ".local/share/flatpak/app/org.mozilla.firefox";
const FLATPAK_FIREFOX_NATIVE_HOST_MANIFEST: &str =
    ".var/app/org.mozilla.firefox/.mozilla/native-messaging-hosts/blockuntu_native.json";
const FLATPAK_FIREFOX_SYSTEMCONFIG_ROOT: &str =
    "flatpak/extension/org.mozilla.firefox.systemconfig";
const SNAP_FIREFOX_NATIVE_HOST_MANIFEST: &str =
    "snap/firefox/common/.mozilla/native-messaging-hosts/blockuntu_native.json";
const SNAP_CHROMIUM_NATIVE_HOST_MANIFEST: &str =
    "snap/chromium/common/chromium/NativeMessagingHosts/blockuntu_native.json";
const SNAP_BRAVE_NATIVE_HOST_MANIFEST: &str =
    "snap/brave/current/.config/BraveSoftware/Brave-Browser/NativeMessagingHosts/blockuntu_native.json";
const SNAP_OPERA_NATIVE_HOST_MANIFEST: &str =
    "snap/opera/current/.config/google-chrome/NativeMessagingHosts/blockuntu_native.json";
const SNAP_VIVALDI_NATIVE_HOST_MANIFEST: &str =
    "snap/vivaldi/current/.config/vivaldi/NativeMessagingHosts/blockuntu_native.json";
const CHROME_EXTENSION_ID: &str = "opfljaancedgklbpnbpjfhdbbhbfpnoc";
const CHROME_COMMANDS: [&str; 4] = [
    "/usr/bin/google-chrome",
    "/usr/bin/google-chrome-stable",
    "/bin/google-chrome",
    "/bin/google-chrome-stable",
];
const CHROMIUM_COMMANDS: [&str; 5] = [
    "/usr/bin/chromium",
    "/usr/bin/chromium-browser",
    "/bin/chromium",
    "/bin/chromium-browser",
    "/snap/bin/chromium",
];
const BRAVE_COMMANDS: [&str; 3] = [
    "/usr/bin/brave-browser",
    "/bin/brave-browser",
    "/snap/bin/brave",
];
const OPERA_COMMANDS: [&str; 3] = ["/usr/bin/opera", "/bin/opera", "/snap/bin/opera"];
const EDGE_COMMANDS: [&str; 3] = [
    "/usr/bin/microsoft-edge",
    "/usr/bin/microsoft-edge-stable",
    "/bin/microsoft-edge",
];
const VIVALDI_COMMANDS: [&str; 4] = [
    "/usr/bin/vivaldi",
    "/usr/bin/vivaldi-stable",
    "/bin/vivaldi",
    "/snap/bin/vivaldi.vivaldi-stable",
];
const CHROME_USER_NATIVE_HOST_MANIFEST: &str =
    ".config/google-chrome/NativeMessagingHosts/blockuntu_native.json";
const CHROMIUM_USER_NATIVE_HOST_MANIFEST: &str =
    ".config/chromium/NativeMessagingHosts/blockuntu_native.json";
const BRAVE_USER_NATIVE_HOST_MANIFEST: &str =
    ".config/BraveSoftware/Brave-Browser/NativeMessagingHosts/blockuntu_native.json";
const OPERA_USER_NATIVE_HOST_MANIFEST: &str =
    ".config/opera/NativeMessagingHosts/blockuntu_native.json";
const EDGE_USER_NATIVE_HOST_MANIFEST: &str =
    ".config/microsoft-edge/NativeMessagingHosts/blockuntu_native.json";
const VIVALDI_USER_NATIVE_HOST_MANIFEST: &str =
    ".config/vivaldi/NativeMessagingHosts/blockuntu_native.json";
const NATIVE_HOST_COMMANDS: [&str; 2] = [
    "/usr/bin/blockuntu-native",
    "/usr/local/bin/blockuntu-native",
];
const CHROME_SYSTEM_NATIVE_HOST_MANIFEST: &str =
    "/etc/opt/chrome/native-messaging-hosts/blockuntu_native.json";
const CHROMIUM_SYSTEM_NATIVE_HOST_MANIFEST: &str =
    "/etc/chromium/native-messaging-hosts/blockuntu_native.json";
const EDGE_SYSTEM_NATIVE_HOST_MANIFEST: &str =
    "/etc/opt/edge/native-messaging-hosts/blockuntu_native.json";
const VIVALDI_SYSTEM_NATIVE_HOST_MANIFESTS: [&str; 2] = [
    "/etc/opt/vivaldi/native-messaging-hosts/blockuntu_native.json",
    "/etc/vivaldi/native-messaging-hosts/blockuntu_native.json",
];
const UNSUPPORTED_BROWSER_RULE_ID: &str = "unsupported-browsers-hard";
const SYSTEM_UNINSTALL_RECOVERY_PHRASE_FILE: &str = "/etc/blockuntu/uninstall-recovery.txt";
const TIER1_EDIT_KEY_FILE: &str = "/etc/blockuntu/tier1-edit-key.txt";
const INSTALLATION_SERIAL_FILE: &str = "/etc/blockuntu/installation-id";
const BUILD_NUMBER: &str = match option_env!("BLOCKUNTU_BUILD_NUMBER") {
    Some(value) => value,
    None => env!("CARGO_PKG_VERSION"),
};
const PACKAGE_NAME: &str = "blockuntu";
const CONFINED_FIREFOX_SETUP_COMMANDS: [&str; 3] = [
    "/usr/bin/blockuntu-setup-confined-firefox",
    "/usr/local/bin/blockuntu-setup-confined-firefox",
    "/bin/blockuntu-setup-confined-firefox",
];
const CONFINED_CHROMIUM_SETUP_COMMANDS: [&str; 3] = [
    "/usr/bin/blockuntu-setup-confined-chromium",
    "/usr/local/bin/blockuntu-setup-confined-chromium",
    "/bin/blockuntu-setup-confined-chromium",
];
static CONFINED_FIREFOX_POLICY_SETUP_STARTED: AtomicBool = AtomicBool::new(false);
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
    #[error("uninstall confirmation phrase does not match")]
    InvalidUninstallPhrase,
    #[error("BlocKuntu installation serial is missing or invalid")]
    InvalidInstallationSerial,
    #[error("GUI uninstall requires pkexec, but pkexec was not found")]
    MissingPkexec,
    #[error("GUI uninstall requires {0}, but it was not found")]
    MissingPackageCommand(&'static str),
    #[error("BlocKuntu is not installed through a supported package manager")]
    UnsupportedPackageInstallation,
    #[error("uninstall command failed: {0}")]
    UninstallCommand(String),
    #[error("unsupported extension store URL")]
    UnsupportedExtensionStoreUrl,
    #[error("opening an extension store requires xdg-open, but it was not found")]
    MissingUrlOpener,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PackageManager {
    Debian,
    Rpm,
    Pacman,
}

impl PackageManager {
    fn uninstall_command_name(self) -> &'static str {
        match self {
            Self::Debian => "dpkg",
            Self::Rpm => "dnf",
            Self::Pacman => "pacman",
        }
    }

    fn uninstall_command_label(self) -> &'static str {
        match self {
            Self::Debian => "dpkg --purge blockuntu",
            Self::Rpm => "dnf remove --assumeyes blockuntu",
            Self::Pacman => "pacman -R --noconfirm blockuntu",
        }
    }

    fn command_candidates(self) -> &'static [&'static str] {
        match self {
            Self::Debian => &["/usr/bin/dpkg", "/bin/dpkg"],
            Self::Rpm => &["/usr/bin/dnf", "/bin/dnf"],
            Self::Pacman => &["/usr/bin/pacman", "/bin/pacman"],
        }
    }

    fn uninstall_command_args(self) -> &'static [&'static str] {
        match self {
            Self::Debian => &["--purge", PACKAGE_NAME],
            Self::Rpm => &["remove", "--assumeyes", PACKAGE_NAME],
            Self::Pacman => &["-R", "--noconfirm", PACKAGE_NAME],
        }
    }
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
struct RecoveryCredentials {
    uninstall_phrase: String,
    tier1_edit_key: String,
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
fn recovery_credentials() -> Result<RecoveryCredentials, GuiError> {
    Ok(RecoveryCredentials {
        uninstall_phrase: read_recovery_credential(SYSTEM_UNINSTALL_RECOVERY_PHRASE_FILE)?,
        tier1_edit_key: read_recovery_credential(TIER1_EDIT_KEY_FILE)?,
    })
}

#[tauri::command]
fn installation_info() -> InstallationInfo {
    installation_info_from_path(Path::new(INSTALLATION_SERIAL_FILE))
}

#[tauri::command]
fn open_extension_store(url: String) -> Result<(), GuiError> {
    if !is_extension_store_url(&url) {
        return Err(GuiError::UnsupportedExtensionStoreUrl);
    }

    let opener =
        command_path(&["/usr/bin/xdg-open", "/bin/xdg-open"]).ok_or(GuiError::MissingUrlOpener)?;
    Command::new(opener).arg(url).spawn()?;
    Ok(())
}

fn is_extension_store_url(url: &str) -> bool {
    matches!(
        url,
        FIREFOX_EXTENSION_STORE_URL | CHROME_EXTENSION_STORE_URL
    )
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

    let package_manager =
        installed_package_manager()?.ok_or(GuiError::UnsupportedPackageInstallation)?;

    let package_removal_lease = prepare_package_removal(emergency_authorized.then_some(candidate))?;
    std::thread::sleep(Duration::from_secs(BROWSER_UNINSTALL_NOTICE_WAIT_SECONDS));

    let pkexec =
        command_path(&["/usr/bin/pkexec", "/bin/pkexec"]).ok_or(GuiError::MissingPkexec)?;
    let package_command = command_path(package_manager.command_candidates())
        .ok_or_else(|| GuiError::MissingPackageCommand(package_manager.uninstall_command_name()))?;
    let mut command = Command::new(pkexec);
    command
        .arg("/usr/bin/env")
        .arg(format!(
            "BLOCKUNTU_PACKAGE_REMOVAL_LEASE={package_removal_lease}"
        ))
        .arg(package_command)
        .args(package_manager.uninstall_command_args());
    let output = command.output()?;

    if !output.status.success() {
        return Err(GuiError::UninstallCommand(command_failure_detail(
            package_manager.uninstall_command_label(),
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
    let librewolf_present = librewolf_available();
    let waterfox_present = waterfox_available();
    let chrome_present = chrome_available();
    let chromium_present = chromium_available();
    let brave_present = brave_available();
    let opera_present = opera_available();
    let edge_present = edge_available();
    let vivaldi_present = vivaldi_available();
    let chromium_classic_present = chromium_classic_available();
    let brave_classic_present = brave_classic_available();
    let opera_classic_present = opera_classic_available();
    let vivaldi_classic_present = vivaldi_classic_available();
    let chromium_family_classic_present = chrome_present
        || chromium_classic_present
        || brave_classic_present
        || opera_classic_present
        || edge_present
        || vivaldi_classic_present;
    let brave_native_host_setup_error = (!using_dev_socket && brave_classic_present)
        .then(|| ensure_chromium_user_native_host_manifest(BRAVE_USER_NATIVE_HOST_MANIFEST))
        .transpose()
        .err();
    let vivaldi_native_host_setup_error = (!using_dev_socket && vivaldi_classic_present)
        .then(|| ensure_chromium_user_native_host_manifest(VIVALDI_USER_NATIVE_HOST_MANIFEST))
        .transpose()
        .err();

    if enforcement
        .as_ref()
        .is_some_and(firefox_policy_activated_after_heartbeat)
    {
        start_confined_firefox_policy_setup();
    }

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
        for (present, policy_key, label) in [
            (librewolf_present, "librewolf", "LibreWolf"),
            (waterfox_present, "waterfox", "Waterfox"),
        ] {
            if present {
                let policy = enforcement
                    .get("firefox_family_policies")
                    .and_then(|policies| policies.get(policy_key))
                    .unwrap_or(&Value::Null);
                checks.push(firefox_family_policy_enforcement_check(
                    enforcement,
                    policy,
                    &format!("{policy_key}_policy"),
                    &format!("{label} policy"),
                ));
            }
        }
        if chrome_present {
            checks.push(chrome_policy_enforcement_check(enforcement));
        }
        for (present, policy_key, label) in [
            (chromium_present, "chromium", "Chromium"),
            (brave_present, "brave", "Brave"),
            (opera_present, "opera", "Opera"),
            (edge_present, "edge", "Microsoft Edge"),
            (vivaldi_present, "vivaldi", "Vivaldi"),
        ] {
            if present {
                let policy = enforcement
                    .get("chromium_policies")
                    .and_then(|policies| policies.get(policy_key))
                    .unwrap_or(&Value::Null);
                checks.push(chromium_policy_enforcement_check(
                    enforcement,
                    policy,
                    &format!("{policy_key}_policy"),
                    &format!("{label} policy"),
                ));
            }
        }
        checks.push(hosts_enforcement_check(enforcement));
    } else if using_dev_socket {
        if system_firefox_present {
            checks.push(policy_file_check(Path::new(DEV_FIREFOX_POLICY_PATH)));
        }
        for (present, path, key, label) in [
            (
                librewolf_present,
                DEV_LIBREWOLF_POLICY_PATH,
                "librewolf_policy",
                "LibreWolf policy",
            ),
            (
                waterfox_present,
                DEV_WATERFOX_POLICY_PATH,
                "waterfox_policy",
                "Waterfox policy",
            ),
        ] {
            if present {
                checks.push(policy_file_check_named(Path::new(path), key, label));
            }
        }
        if chrome_present {
            checks.push(chrome_policy_file_check(Path::new(DEV_CHROME_POLICY_PATH)));
        }
        for (present, path, key, label) in [
            (
                chromium_present,
                DEV_CHROMIUM_POLICY_PATH,
                "chromium_policy",
                "Chromium policy",
            ),
            (
                brave_present,
                DEV_BRAVE_POLICY_PATH,
                "brave_policy",
                "Brave policy",
            ),
            (
                opera_present,
                DEV_OPERA_POLICY_PATH,
                "opera_policy",
                "Opera policy",
            ),
            (
                edge_present,
                DEV_EDGE_POLICY_PATH,
                "edge_policy",
                "Microsoft Edge policy",
            ),
            (
                vivaldi_present,
                DEV_VIVALDI_POLICY_PATH,
                "vivaldi_policy",
                "Vivaldi policy",
            ),
        ] {
            if present {
                checks.push(chromium_policy_file_check(Path::new(path), key, label));
            }
        }
        checks.push(hosts_file_check(Path::new("/tmp/blockuntu/hosts")));
    } else {
        if system_firefox_present {
            checks.push(policy_file_check(Path::new(DEFAULT_FIREFOX_POLICY_PATH)));
        }
        for (present, path, key, label) in [
            (
                librewolf_present,
                DEFAULT_LIBREWOLF_POLICY_PATH,
                "librewolf_policy",
                "LibreWolf policy",
            ),
            (
                waterfox_present,
                DEFAULT_WATERFOX_POLICY_PATH,
                "waterfox_policy",
                "Waterfox policy",
            ),
        ] {
            if present {
                checks.push(policy_file_check_named(Path::new(path), key, label));
            }
        }
        if chrome_present {
            checks.push(chrome_policy_file_check(Path::new(
                DEFAULT_CHROME_POLICY_PATH,
            )));
        }
        for (present, path, key, label) in [
            (
                chromium_present,
                DEFAULT_CHROMIUM_POLICY_PATH,
                "chromium_policy",
                "Chromium policy",
            ),
            (
                brave_present,
                DEFAULT_BRAVE_POLICY_PATH,
                "brave_policy",
                "Brave policy",
            ),
            (
                opera_present,
                DEFAULT_OPERA_POLICY_PATH,
                "opera_policy",
                "Opera policy",
            ),
            (
                edge_present,
                DEFAULT_EDGE_POLICY_PATH,
                "edge_policy",
                "Microsoft Edge policy",
            ),
            (
                vivaldi_present,
                DEFAULT_VIVALDI_POLICY_PATH,
                "vivaldi_policy",
                "Vivaldi policy",
            ),
        ] {
            if present {
                checks.push(chromium_policy_file_check(Path::new(path), key, label));
            }
        }
        checks.push(hosts_file_check(Path::new("/etc/hosts")));
    }
    if system_firefox_present {
        checks.push(native_host_manifest_check());
    }
    if librewolf_present {
        checks.push(librewolf_native_host_manifest_check());
    }
    if waterfox_present {
        checks.push(waterfox_native_host_manifest_check());
    }
    checks.extend(confined_firefox_native_host_checks());
    checks.extend(confined_firefox_policy_checks());
    checks.extend(confined_chromium_native_host_checks());
    if chromium_family_classic_present {
        checks.push(chrome_native_host_manifest_check());
    }
    if let Some(error) = brave_native_host_setup_error {
        checks.push(HealthCheck {
            key: "brave_native_host_setup".to_string(),
            label: "Brave Native host setup".to_string(),
            state: HealthState::Error,
            detail: format!("Could not install the Brave user Native Messaging manifest: {error}"),
        });
    }
    if let Some(error) = vivaldi_native_host_setup_error {
        checks.push(HealthCheck {
            key: "vivaldi_native_host_setup".to_string(),
            label: "Vivaldi Native host setup".to_string(),
            state: HealthState::Error,
            detail: format!(
                "Could not install the Vivaldi user Native Messaging manifest: {error}"
            ),
        });
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
    for (present, component, label) in [
        (
            librewolf_present,
            "librewolf_extension",
            "LibreWolf extension",
        ),
        (waterfox_present, "waterfox_extension", "Waterfox extension"),
    ] {
        if present {
            checks.push(browser_extension_runtime_check(
                &socket, component, component, label,
            ));
        }
    }
    if chrome_present {
        checks.push(browser_extension_runtime_check(
            &socket,
            "chrome_extension",
            "chrome_extension",
            "Chrome extension",
        ));
    }
    for (present, component, label) in [
        (chromium_present, "chromium_extension", "Chromium extension"),
        (brave_present, "brave_extension", "Brave extension"),
        (opera_present, "opera_extension", "Opera extension"),
        (edge_present, "edge_extension", "Microsoft Edge extension"),
        (vivaldi_present, "vivaldi_extension", "Vivaldi extension"),
    ] {
        if present {
            checks.push(browser_extension_runtime_check(
                &socket, component, component, label,
            ));
        }
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

fn start_confined_chromium_setup() {
    let Some(helper) = command_path(&CONFINED_CHROMIUM_SETUP_COMMANDS) else {
        return;
    };

    if let Err(error) = Command::new(helper).args(["--targets", "auto"]).spawn() {
        eprintln!("BlocKuntu confined Chromium-family setup could not start: {error}");
    }
}

fn firefox_policy_activated_after_heartbeat(enforcement: &Value) -> bool {
    enforcement
        .get("firefox_policy")
        .and_then(|policy| policy.get("active_after_heartbeat"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn start_confined_firefox_policy_setup() {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return;
    };
    if !flatpak_firefox_available()
        || !home.join(FLATPAK_FIREFOX_APP_ROOT).exists()
        || flatpak_firefox_policy_uses_amo_install_url(&home)
        || CONFINED_FIREFOX_POLICY_SETUP_STARTED.swap(true, Ordering::SeqCst)
    {
        return;
    }

    let Some(helper) = command_path(&CONFINED_FIREFOX_SETUP_COMMANDS) else {
        CONFINED_FIREFOX_POLICY_SETUP_STARTED.store(false, Ordering::SeqCst);
        return;
    };

    if let Err(error) = Command::new(helper)
        .args(["--targets", "flatpak", "--write-flatpak-policy"])
        .spawn()
    {
        CONFINED_FIREFOX_POLICY_SETUP_STARTED.store(false, Ordering::SeqCst);
        eprintln!("BlocKuntu Flatpak Firefox policy setup could not start: {error}");
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

fn read_recovery_credential(path: &str) -> Result<String, GuiError> {
    let phrase = fs::read_to_string(path)?.trim().to_string();
    if phrase.is_empty() {
        return Err(GuiError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("recovery credential is empty: {path}"),
        )));
    }
    Ok(phrase)
}

fn uninstall_phrase_matches(candidate: &str) -> Result<bool, GuiError> {
    if candidate.is_empty() {
        return Ok(false);
    }

    Ok(candidate == read_recovery_credential(SYSTEM_UNINSTALL_RECOVERY_PHRASE_FILE)?.trim())
}

fn debian_package_installed() -> Result<bool, GuiError> {
    let Some(dpkg_query) = command_path(&["/usr/bin/dpkg-query", "/bin/dpkg-query"]) else {
        return Ok(false);
    };
    let output = Command::new(dpkg_query)
        .args(["-W", "-f=${db:Status-Abbrev}", PACKAGE_NAME])
        .output()?;
    Ok(output.status.success() && String::from_utf8_lossy(&output.stdout).starts_with("ii"))
}

fn installed_package_manager() -> Result<Option<PackageManager>, GuiError> {
    if debian_package_installed()? {
        return Ok(Some(PackageManager::Debian));
    }
    if rpm_package_installed()? {
        return Ok(Some(PackageManager::Rpm));
    }
    if pacman_package_installed()? {
        return Ok(Some(PackageManager::Pacman));
    }
    Ok(None)
}

fn rpm_package_installed() -> Result<bool, GuiError> {
    let Some(rpm) = command_path(&["/usr/bin/rpm", "/bin/rpm"]) else {
        return Ok(false);
    };
    Ok(Command::new(rpm)
        .args(["-q", "--quiet", PACKAGE_NAME])
        .status()?
        .success())
}

fn pacman_package_installed() -> Result<bool, GuiError> {
    let Some(pacman) = command_path(&["/usr/bin/pacman", "/bin/pacman"]) else {
        return Ok(false);
    };
    Ok(Command::new(pacman)
        .args(["-Q", "--quiet", PACKAGE_NAME])
        .status()?
        .success())
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

fn librewolf_available() -> bool {
    command_path(&LIBREWOLF_COMMANDS).is_some()
}

fn waterfox_available() -> bool {
    command_path(&WATERFOX_COMMANDS).is_some()
}

fn firefox_family_available() -> bool {
    system_firefox_available() || flatpak_firefox_available() || snap_firefox_available()
}

fn chrome_available() -> bool {
    command_path(&CHROME_COMMANDS).is_some()
}

fn chromium_available() -> bool {
    command_path(&CHROMIUM_COMMANDS).is_some()
}

fn chromium_classic_available() -> bool {
    command_path(&CHROMIUM_COMMANDS[..4]).is_some()
}

fn brave_available() -> bool {
    command_path(&BRAVE_COMMANDS).is_some()
}

fn brave_classic_available() -> bool {
    command_path(&BRAVE_COMMANDS[..2]).is_some()
}

fn opera_available() -> bool {
    command_path(&OPERA_COMMANDS).is_some()
}

fn opera_classic_available() -> bool {
    command_path(&OPERA_COMMANDS[..2]).is_some()
}

fn edge_available() -> bool {
    command_path(&EDGE_COMMANDS).is_some()
}

fn vivaldi_available() -> bool {
    command_path(&VIVALDI_COMMANDS).is_some()
}

fn vivaldi_classic_available() -> bool {
    command_path(&VIVALDI_COMMANDS[..3]).is_some()
}

fn ensure_chromium_user_native_host_manifest(user_manifest: &str) -> std::io::Result<()> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "HOME is not set"))?;
    let manifest_path = home.join(user_manifest);
    let native_host = command_path(&NATIVE_HOST_COMMANDS).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "BlocKuntu native host is not installed",
        )
    })?;
    let manifest = format!(
        concat!(
            "{{\n",
            "  \"name\": \"blockuntu_native\",\n",
            "  \"description\": \"BlocKuntu Chromium-family Native Messaging bridge\",\n",
            "  \"path\": \"{}\",\n",
            "  \"type\": \"stdio\",\n",
            "  \"allowed_origins\": [\"chrome-extension://{}/\"]\n",
            "}}\n"
        ),
        native_host.display(),
        CHROME_EXTENSION_ID,
    );

    if fs::read_to_string(&manifest_path).ok().as_deref() == Some(manifest.as_str()) {
        return Ok(());
    }

    let parent = manifest_path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Chromium-family Native Messaging manifest has no parent directory",
        )
    })?;
    fs::create_dir_all(parent)?;
    fs::write(&manifest_path, manifest)?;
    fs::set_permissions(manifest_path, fs::Permissions::from_mode(0o644))
}

fn flatpak_firefox_available() -> bool {
    flatpak_app_deployed(Path::new(FLATPAK_FIREFOX_DEPLOYMENT_ROOT))
        || std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| flatpak_app_deployed(&home.join(FLATPAK_FIREFOX_USER_DEPLOYMENT_ROOT)))
            .unwrap_or(false)
}

fn snap_firefox_available() -> bool {
    command_path(&["/snap/bin/firefox"]).is_some()
}

fn flatpak_app_deployed(app_root: &Path) -> bool {
    fs::read_dir(app_root).ok().is_some_and(|architectures| {
        architectures.flatten().any(|architecture| {
            fs::read_dir(architecture.path())
                .ok()
                .is_some_and(|branches| {
                    branches
                        .flatten()
                        .any(|branch| branch.path().join("active").is_dir())
                })
        })
    })
}

fn snap_chromium_available() -> bool {
    command_path(&CHROMIUM_COMMANDS[4..]).is_some()
}

fn snap_brave_available() -> bool {
    command_path(&BRAVE_COMMANDS[2..]).is_some()
}

fn snap_opera_available() -> bool {
    command_path(&OPERA_COMMANDS[2..]).is_some()
}

fn snap_vivaldi_available() -> bool {
    command_path(&VIVALDI_COMMANDS[3..]).is_some()
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
    policy_file_check_named(path, "firefox_policy", "Firefox policy")
}

fn policy_file_check_named(path: &Path, key: &str, label: &str) -> HealthCheck {
    match fs::read_to_string(path) {
        Ok(contents) => {
            let valid_json = serde_json::from_str::<Value>(&contents).is_ok();
            HealthCheck {
                key: key.to_string(),
                label: label.to_string(),
                state: if valid_json {
                    HealthState::Ok
                } else {
                    HealthState::Error
                },
                detail: path.display().to_string(),
            }
        }
        Err(err) => HealthCheck {
            key: key.to_string(),
            label: label.to_string(),
            state: HealthState::Warn,
            detail: format!("{}: {err}", path.display()),
        },
    }
}

fn chrome_policy_file_check(path: &Path) -> HealthCheck {
    chromium_policy_file_check(path, "chrome_policy", "Chrome policy")
}

fn chromium_policy_file_check(path: &Path, key: &str, label: &str) -> HealthCheck {
    match fs::read_to_string(path) {
        Ok(contents) => {
            let valid_json = serde_json::from_str::<Value>(&contents).is_ok();
            HealthCheck {
                key: key.to_string(),
                label: label.to_string(),
                state: if valid_json {
                    HealthState::Ok
                } else {
                    HealthState::Error
                },
                detail: path.display().to_string(),
            }
        }
        Err(err) => HealthCheck {
            key: key.to_string(),
            label: label.to_string(),
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
    firefox_family_policy_enforcement_check(
        status,
        status.get("firefox_policy").unwrap_or(&Value::Null),
        "firefox_policy",
        "Firefox policy",
    )
}

fn firefox_family_policy_enforcement_check(
    status: &Value,
    policy: &Value,
    key: &str,
    label: &str,
) -> HealthCheck {
    let active = status
        .get("enforcement_state")
        .and_then(Value::as_str)
        .unwrap_or("active")
        == "active";
    let managed = policy
        .get("managed")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let deferred = bool_field(policy, "deferred_until_heartbeat");
    let active_after_heartbeat = bool_field(policy, "active_after_heartbeat");
    let compliant = bool_field(policy, "compliant");
    let private_browsing = bool_field(policy, "private_browsing_enabled");
    let private_browsing_available = bool_field(policy, "private_browsing_available");
    let path = string_field(policy, "path").unwrap_or("unknown");
    let detail = string_field(policy, "detail").unwrap_or("no policy detail");
    let state = if !active {
        HealthState::Warn
    } else if !managed {
        HealthState::Ok
    } else if deferred && !active_after_heartbeat {
        HealthState::Warn
    } else if compliant && private_browsing && private_browsing_available {
        HealthState::Ok
    } else {
        HealthState::Error
    };

    HealthCheck {
        key: key.to_string(),
        label: label.to_string(),
        state,
        detail: format!("{path}: {detail}"),
    }
}

fn chrome_policy_enforcement_check(status: &Value) -> HealthCheck {
    chromium_policy_enforcement_check(
        status,
        status.get("chrome_policy").unwrap_or(&Value::Null),
        "chrome_policy",
        "Chrome policy",
    )
}

fn chromium_policy_enforcement_check(
    status: &Value,
    policy: &Value,
    key: &str,
    label: &str,
) -> HealthCheck {
    let active = status
        .get("enforcement_state")
        .and_then(Value::as_str)
        .unwrap_or("active")
        == "active";
    let managed = policy
        .get("managed")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let deferred = bool_field(policy, "deferred_until_heartbeat");
    let active_after_heartbeat = bool_field(policy, "active_after_heartbeat");
    let compliant = bool_field(policy, "compliant");
    let force_install = bool_field(policy, "force_install_configured");
    let incognito_mode = string_field(policy, "incognito_mode").unwrap_or("policy_url_blocking");
    let incognito_configured = policy
        .get("incognito_mode_configured")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let incognito_unsupported_patterns = policy
        .get("incognito_unsupported_pattern_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let incognito_url_block_limit_exceeded =
        bool_field(policy, "incognito_url_block_limit_exceeded");
    let path = string_field(policy, "path").unwrap_or("unknown");
    let detail = string_field(policy, "detail").unwrap_or("no browser policy detail");
    let state = if !active {
        HealthState::Warn
    } else if !managed {
        HealthState::Ok
    } else if deferred && !active_after_heartbeat {
        HealthState::Warn
    } else if incognito_url_block_limit_exceeded || !incognito_configured {
        HealthState::Error
    } else if incognito_mode == "policy_url_blocking" && incognito_unsupported_patterns > 0 {
        HealthState::Warn
    } else if compliant && force_install {
        HealthState::Ok
    } else {
        HealthState::Error
    };

    HealthCheck {
        key: key.to_string(),
        label: label.to_string(),
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
    firefox_family_native_host_manifest_check(
        "native_host_manifest",
        "Firefox Native host",
        FIREFOX_USER_NATIVE_HOST_MANIFEST,
        &SYSTEM_FIREFOX_NATIVE_HOST_MANIFESTS,
    )
}

fn librewolf_native_host_manifest_check() -> HealthCheck {
    firefox_family_native_host_manifest_check(
        "librewolf_native_host_manifest",
        "LibreWolf Native host",
        LIBREWOLF_USER_NATIVE_HOST_MANIFEST,
        &SYSTEM_LIBREWOLF_NATIVE_HOST_MANIFESTS,
    )
}

fn waterfox_native_host_manifest_check() -> HealthCheck {
    firefox_family_native_host_manifest_check(
        "waterfox_native_host_manifest",
        "Waterfox Native host",
        WATERFOX_USER_NATIVE_HOST_MANIFEST,
        &SYSTEM_WATERFOX_NATIVE_HOST_MANIFESTS,
    )
}

fn firefox_family_native_host_manifest_check(
    key: &str,
    label: &str,
    user_manifest_path: &str,
    system_manifests: &[&str],
) -> HealthCheck {
    let user_manifest = std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(user_manifest_path));

    let candidate = user_manifest
        .filter(|path| path.exists())
        .or_else(|| {
            system_manifests
                .iter()
                .map(PathBuf::from)
                .find(|path| path.exists())
        })
        .unwrap_or_else(|| PathBuf::from(system_manifests[0]));

    firefox_manifest_check(
        key,
        label,
        &candidate,
        "Install the system Native Messaging manifest.",
    )
}

fn confined_firefox_native_host_checks() -> Vec<HealthCheck> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };

    let mut checks = Vec::new();
    if flatpak_firefox_available() {
        checks.push(firefox_manifest_check(
            "firefox_flatpak_native_host_manifest",
            "Firefox Flatpak browser integration",
            &home.join(FLATPAK_FIREFOX_NATIVE_HOST_MANIFEST),
            "BlocKuntu automatically prepares this when the GUI starts. Restart Firefox Flatpak; if it remains unavailable, run `blockuntu-setup-confined-firefox` manually.",
        ));
    }
    if snap_firefox_available() {
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
    if !flatpak_firefox_available() {
        return Vec::new();
    }

    vec![firefox_flatpak_policy_check(&home)]
}

fn confined_chromium_native_host_checks() -> Vec<HealthCheck> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };

    let mut checks = Vec::new();
    for (present, manifest, key, label) in [
        (
            snap_chromium_available(),
            SNAP_CHROMIUM_NATIVE_HOST_MANIFEST,
            "chromium_snap_native_host_manifest",
            "Chromium Snap browser integration",
        ),
        (
            snap_brave_available(),
            SNAP_BRAVE_NATIVE_HOST_MANIFEST,
            "brave_snap_native_host_manifest",
            "Brave Snap browser integration",
        ),
        (
            snap_opera_available(),
            SNAP_OPERA_NATIVE_HOST_MANIFEST,
            "opera_snap_native_host_manifest",
            "Opera Snap browser integration",
        ),
        (
            snap_vivaldi_available(),
            SNAP_VIVALDI_NATIVE_HOST_MANIFEST,
            "vivaldi_snap_native_host_manifest",
            "Vivaldi Snap browser integration",
        ),
    ] {
        if present {
            checks.push(chromium_manifest_check(
                key,
                label,
                &home.join(manifest),
                "BlocKuntu automatically prepares this when the GUI starts. Restart the Snap browser; if it remains unavailable, run `blockuntu-setup-confined-chromium` manually.",
            ));
        }
    }

    checks
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
            let private_browsing = extension_settings
                .and_then(|settings| settings.get("private_browsing"))
                .and_then(Value::as_bool)
                == Some(true);
            let private_browsing_available = parsed
                .as_ref()
                .ok()
                .and_then(|value| value.get("policies"))
                .and_then(|policies| policies.get("PrivateBrowsingModeAvailability"))
                .and_then(Value::as_i64)
                == Some(0);

            HealthCheck {
                key: "firefox_flatpak_policy".to_string(),
                label: "Firefox Flatpak policy".to_string(),
                state: if parsed.is_ok()
                    && force_installed
                    && install_url == FIREFOX_EXTENSION_INSTALL_URL
                    && private_browsing
                    && private_browsing_available
                {
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
                "{}: {err}; BlocKuntu writes this after the first verified Firefox extension heartbeat. Restart Firefox Flatpak; if it remains unavailable, run `blockuntu-setup-confined-firefox` manually.",
                candidate.display()
            ),
        },
    }
}

fn flatpak_firefox_policy_uses_amo_install_url(home: &Path) -> bool {
    let candidate = flatpak_firefox_policy_path(home);
    let Ok(contents) = fs::read_to_string(candidate) else {
        return false;
    };
    let Ok(parsed) = serde_json::from_str::<Value>(&contents) else {
        return false;
    };
    let private_browsing_available = parsed
        .get("policies")
        .and_then(|policies| policies.get("PrivateBrowsingModeAvailability"))
        .and_then(Value::as_i64)
        == Some(0);
    let extension_settings = parsed
        .get("policies")
        .and_then(|policies| policies.get("ExtensionSettings"))
        .and_then(|settings| {
            FIREFOX_EXTENSION_IDS
                .iter()
                .find_map(|id| settings.get(*id))
        });
    extension_settings
        .and_then(|settings| settings.get("installation_mode"))
        .and_then(Value::as_str)
        == Some("force_installed")
        && extension_settings
            .and_then(|settings| settings.get("install_url"))
            .and_then(Value::as_str)
            == Some(FIREFOX_EXTENSION_INSTALL_URL)
        && extension_settings
            .and_then(|settings| settings.get("private_browsing"))
            .and_then(Value::as_bool)
            == Some(true)
        && private_browsing_available
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
        candidates.push(home.join(BRAVE_USER_NATIVE_HOST_MANIFEST));
        candidates.push(home.join(OPERA_USER_NATIVE_HOST_MANIFEST));
        candidates.push(home.join(EDGE_USER_NATIVE_HOST_MANIFEST));
        candidates.push(home.join(VIVALDI_USER_NATIVE_HOST_MANIFEST));
    }
    candidates.push(PathBuf::from(CHROME_SYSTEM_NATIVE_HOST_MANIFEST));
    candidates.push(PathBuf::from(CHROMIUM_SYSTEM_NATIVE_HOST_MANIFEST));
    candidates.push(PathBuf::from(EDGE_SYSTEM_NATIVE_HOST_MANIFEST));
    candidates.extend(
        VIVALDI_SYSTEM_NATIVE_HOST_MANIFESTS
            .iter()
            .map(PathBuf::from),
    );

    let candidate = candidates
        .iter()
        .find(|path| path.exists())
        .cloned()
        .unwrap_or_else(|| PathBuf::from(CHROME_SYSTEM_NATIVE_HOST_MANIFEST));

    chromium_manifest_check(
        "chromium_native_host_manifest",
        "Chromium-family Native host",
        &candidate,
        "Install the system Native Messaging manifest.",
    )
}

fn chromium_manifest_check(
    key: &str,
    label: &str,
    candidate: &Path,
    missing_detail: &str,
) -> HealthCheck {
    match fs::read_to_string(candidate) {
        Ok(contents) => {
            let parsed = serde_json::from_str::<Value>(&contents);
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
                state: if parsed.is_ok() && origin_allowed && host_executable {
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
                    format!(
                        "Tier 1 hard application rule active with {matcher_count} matcher(s), including Chromium Flatpak and Brave, Opera, and Vivaldi Snaps"
                    )
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
            start_confined_chromium_setup();
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
            open_extension_store,
            recovery_credentials,
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

    #[test]
    fn package_managers_use_their_native_authorized_uninstall_commands() {
        assert_eq!(PackageManager::Debian.uninstall_command_name(), "dpkg");
        assert_eq!(
            PackageManager::Debian.uninstall_command_label(),
            "dpkg --purge blockuntu"
        );
        assert_eq!(PackageManager::Rpm.uninstall_command_name(), "dnf");
        assert_eq!(
            PackageManager::Rpm.uninstall_command_label(),
            "dnf remove --assumeyes blockuntu"
        );
        assert_eq!(PackageManager::Pacman.uninstall_command_name(), "pacman");
        assert_eq!(
            PackageManager::Pacman.uninstall_command_label(),
            "pacman -R --noconfirm blockuntu"
        );
        assert_eq!(
            PackageManager::Pacman.command_candidates(),
            ["/usr/bin/pacman", "/bin/pacman"]
        );
        assert_eq!(
            PackageManager::Pacman.uninstall_command_args(),
            ["-R", "--noconfirm", PACKAGE_NAME]
        );
    }

    #[test]
    fn firefox_system_manifest_candidates_cover_debian_and_fedora_library_paths() {
        assert_eq!(
            SYSTEM_FIREFOX_NATIVE_HOST_MANIFESTS,
            [
                "/usr/lib/mozilla/native-messaging-hosts/blockuntu_native.json",
                "/usr/lib64/mozilla/native-messaging-hosts/blockuntu_native.json",
            ]
        );
    }

    #[test]
    fn chromium_family_command_paths_include_the_supported_snap_browsers() {
        assert!(BRAVE_COMMANDS.contains(&"/snap/bin/brave"));
        assert!(OPERA_COMMANDS.contains(&"/snap/bin/opera"));
        assert!(VIVALDI_COMMANDS.contains(&"/snap/bin/vivaldi.vivaldi-stable"));
    }

    #[test]
    fn chromium_snap_native_host_paths_match_the_visible_browser_profiles() {
        assert_eq!(
            SNAP_CHROMIUM_NATIVE_HOST_MANIFEST,
            "snap/chromium/common/chromium/NativeMessagingHosts/blockuntu_native.json"
        );
        assert_eq!(
            SNAP_BRAVE_NATIVE_HOST_MANIFEST,
            "snap/brave/current/.config/BraveSoftware/Brave-Browser/NativeMessagingHosts/blockuntu_native.json"
        );
        assert_eq!(
            SNAP_OPERA_NATIVE_HOST_MANIFEST,
            "snap/opera/current/.config/google-chrome/NativeMessagingHosts/blockuntu_native.json"
        );
        assert_eq!(
            SNAP_VIVALDI_NATIVE_HOST_MANIFEST,
            "snap/vivaldi/current/.config/vivaldi/NativeMessagingHosts/blockuntu_native.json"
        );
    }

    #[test]
    fn extension_store_opener_accepts_only_the_official_store_urls() {
        assert!(is_extension_store_url(FIREFOX_EXTENSION_STORE_URL));
        assert!(is_extension_store_url(CHROME_EXTENSION_STORE_URL));
        assert!(!is_extension_store_url("https://example.com/"));
    }
}
