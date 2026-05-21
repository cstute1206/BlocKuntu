use serde::Deserialize;
use serde_json::json;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

const SOCKET_PATH: &str = "/run/blockuntu/blockuntud.sock";
const RUNTIME_DIR: &str = "/run/blockuntu";
const FIREFOX_POLICY_PATH: &str = "/etc/firefox/policies/policies.json";
const POLICY_CHECK_INTERVAL: Duration = Duration::from_secs(1);
const EXTENSION_MONITOR_INTERVAL: Duration = Duration::from_secs(5);
const EXTENSION_HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(30);
const EXTENSION_INACTIVE_ACTION_DELAY: Duration = Duration::from_secs(120);
const EXTENSION_INACTIVE_ACTION_ENV: &str = "BLOCKUNTU_EXTENSION_INACTIVE_ACTION";
const MAX_PAYLOAD_BYTES: usize = 1024 * 1024;
const BLOCKUNTU_UNITS: &[&str] = &["blockuntu-watchdog.service", "blockuntu.service"];
const SYSTEMD_RUNTIME_DIR: &str = "/run/systemd/system";
const SYSTEMD_UNINSTALL_OVERRIDE: &str = r#"[Unit]
RefuseManualStop=no
BindsTo=
Wants=

[Service]
Restart=no
"#;
const FIREFOX_POLICY_JSON: &str = r#"{
  "policies": {
    "BlockAboutConfig": true,
    "DisableSafeMode": true,
    "PrivateBrowsingModeAvailability": 1,
    "ExtensionSettings": {
      "blockuntu-poc@example.local": {
        "installation_mode": "force_installed",
        "install_url": "file:///usr/local/share/blockuntu/BlocKuntu-PoC.xpi",
        "default_area": "navbar",
        "private_browsing": true
      }
    },
    "Preferences": {
      "extensions.quarantinedDomains.enabled": {
        "Value": false,
        "Status": "locked"
      }
    }
  }
}
"#;

#[derive(Debug, Deserialize)]
struct EvaluationRequest {
    #[serde(rename = "type")]
    message_type: Option<String>,
    #[serde(rename = "extensionId")]
    extension_id: Option<String>,
    #[serde(rename = "extensionVersion")]
    extension_version: Option<String>,
    url: Option<String>,
}

type ExtensionActivityHandle = Arc<Mutex<ExtensionActivity>>;

#[derive(Debug, Default)]
struct ExtensionActivity {
    last_seen: Option<Instant>,
    extension_id: Option<String>,
    extension_version: Option<String>,
    reported_status: Option<ExtensionRuntimeStatus>,
    inactive_since: Option<Instant>,
    inactive_action_started: bool,
}

#[tokio::main]
async fn main() -> io::Result<()> {
    if env::args_os().any(|arg| arg == "--uninstall") {
        return run_uninstall();
    }

    if let Err(err) = verify_and_repair_browser_policies() {
        eprintln!("failed to verify Firefox policy at startup: {err}");
    }
    spawn_browser_policy_guard();
    let extension_activity = Arc::new(Mutex::new(ExtensionActivity::default()));

    prepare_socket(SOCKET_PATH)?;

    let listener = UnixListener::bind(SOCKET_PATH)?;
    fs::set_permissions(SOCKET_PATH, fs::Permissions::from_mode(0o666))?;

    println!("blockuntud listening on {SOCKET_PATH}");
    spawn_extension_activity_monitor(extension_activity.clone());

    loop {
        let (stream, _addr) = match listener.accept().await {
            Ok(accepted) => accepted,
            Err(err) => {
                eprintln!("failed to accept Unix socket client: {err}");
                continue;
            }
        };

        let extension_activity = extension_activity.clone();
        tokio::spawn(async move {
            if let Err(err) = handle_client(stream, extension_activity).await {
                eprintln!("client handling error: {err}");
            }
        });
    }
}

fn spawn_browser_policy_guard() {
    thread::spawn(|| loop {
        match verify_and_repair_browser_policies() {
            Ok(PolicyRepairStatus::AlreadyCompliant) => {}
            Ok(PolicyRepairStatus::Repaired) => {
                println!("repaired Firefox enterprise policy at {FIREFOX_POLICY_PATH}");
            }
            Err(err) => {
                eprintln!("Firefox policy verification failed: {err}");
            }
        }

        thread::sleep(POLICY_CHECK_INTERVAL);
    });
}

fn spawn_extension_activity_monitor(activity: ExtensionActivityHandle) {
    thread::spawn(move || loop {
        let should_run_inactive_action = match activity.lock() {
            Ok(mut state) => update_extension_activity_status(&mut state, Instant::now()),
            Err(err) => {
                eprintln!("Firefox extension activity monitor lock failed: {err}");
                false
            }
        };

        if should_run_inactive_action {
            match run_extension_inactive_action() {
                Ok(()) => {}
                Err(err) => eprintln!("Firefox extension inactive action failed: {err}"),
            }
        }

        thread::sleep(EXTENSION_MONITOR_INTERVAL);
    });
}

fn update_extension_activity_status(state: &mut ExtensionActivity, now: Instant) -> bool {
    if extension_seen_recently(state.last_seen, now) {
        if state.reported_status != Some(ExtensionRuntimeStatus::Active) {
            println!(
                "Firefox extension active{}",
                extension_identity_for_log(state).unwrap_or_default()
            );
        }

        state.reported_status = Some(ExtensionRuntimeStatus::Active);
        state.inactive_since = None;
        state.inactive_action_started = false;
        return false;
    }

    let inactive_since = *state.inactive_since.get_or_insert(now);

    if state.reported_status != Some(ExtensionRuntimeStatus::Inactive) {
        eprintln!(
            "Firefox extension inactive: {}",
            extension_inactive_reason(state.last_seen, now)
        );
    }

    state.reported_status = Some(ExtensionRuntimeStatus::Inactive);

    if !state.inactive_action_started
        && now.duration_since(inactive_since) >= EXTENSION_INACTIVE_ACTION_DELAY
    {
        state.inactive_action_started = true;
        return true;
    }

    false
}

fn extension_seen_recently(last_seen: Option<Instant>, now: Instant) -> bool {
    last_seen
        .map(|last_seen| now.duration_since(last_seen) <= EXTENSION_HEARTBEAT_TIMEOUT)
        .unwrap_or(false)
}

fn extension_inactive_reason(last_seen: Option<Instant>, now: Instant) -> String {
    match last_seen {
        Some(last_seen) => format!(
            "last heartbeat was {} second(s) ago",
            now.duration_since(last_seen).as_secs()
        ),
        None => "no heartbeat has been received yet".to_string(),
    }
}

fn extension_identity_for_log(state: &ExtensionActivity) -> Option<String> {
    match (&state.extension_id, &state.extension_version) {
        (Some(extension_id), Some(extension_version)) => {
            Some(format!(" ({extension_id}, version {extension_version})"))
        }
        (Some(extension_id), None) => Some(format!(" ({extension_id})")),
        (None, Some(extension_version)) => Some(format!(" (version {extension_version})")),
        (None, None) => None,
    }
}

fn record_extension_heartbeat(
    activity: &ExtensionActivityHandle,
    request: &EvaluationRequest,
) -> io::Result<()> {
    let mut state = activity
        .lock()
        .map_err(|err| io::Error::new(io::ErrorKind::Other, format!("{err}")))?;

    state.last_seen = Some(Instant::now());
    state.extension_id = request.extension_id.clone();
    state.extension_version = request.extension_version.clone();

    if state.reported_status != Some(ExtensionRuntimeStatus::Active) {
        println!(
            "Firefox extension active{}",
            extension_identity_for_log(&state).unwrap_or_default()
        );
    }

    state.reported_status = Some(ExtensionRuntimeStatus::Active);
    state.inactive_since = None;
    state.inactive_action_started = false;

    Ok(())
}

fn run_extension_inactive_action() -> io::Result<()> {
    match configured_extension_inactive_action() {
        ExtensionInactiveAction::LogOnly => {
            eprintln!(
                "Firefox extension has been inactive for {} second(s); configured action is log-only",
                EXTENSION_INACTIVE_ACTION_DELAY.as_secs()
            );
            Ok(())
        }
        ExtensionInactiveAction::Poweroff => {
            eprintln!(
                "Firefox extension has been inactive for {} second(s); requesting system poweroff",
                EXTENSION_INACTIVE_ACTION_DELAY.as_secs()
            );
            run_systemctl(&["poweroff"])
        }
    }
}

fn configured_extension_inactive_action() -> ExtensionInactiveAction {
    configured_extension_inactive_action_from(
        env::var(EXTENSION_INACTIVE_ACTION_ENV).ok().as_deref(),
    )
}

fn configured_extension_inactive_action_from(value: Option<&str>) -> ExtensionInactiveAction {
    match value {
        Some(value) if value.eq_ignore_ascii_case("poweroff") => ExtensionInactiveAction::Poweroff,
        _ => ExtensionInactiveAction::LogOnly,
    }
}

fn verify_and_repair_browser_policies() -> io::Result<PolicyRepairStatus> {
    verify_and_repair_browser_policies_at(Path::new(FIREFOX_POLICY_PATH))
}

fn verify_and_repair_browser_policies_at(policy_path: &Path) -> io::Result<PolicyRepairStatus> {
    match fs::read(policy_path) {
        Ok(contents) if browser_policy_is_expected(&contents) => {
            Ok(PolicyRepairStatus::AlreadyCompliant)
        }
        Ok(_) => {
            write_expected_browser_policy(policy_path)?;
            Ok(PolicyRepairStatus::Repaired)
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            write_expected_browser_policy(policy_path)?;
            Ok(PolicyRepairStatus::Repaired)
        }
        Err(err) => Err(err),
    }
}

fn browser_policy_is_expected(contents: &[u8]) -> bool {
    let Ok(current) = serde_json::from_slice::<serde_json::Value>(contents) else {
        return false;
    };
    let Ok(expected) = serde_json::from_str::<serde_json::Value>(FIREFOX_POLICY_JSON) else {
        return false;
    };

    current == expected
}

fn write_expected_browser_policy(policy_path: &Path) -> io::Result<()> {
    let parent = policy_path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("policy path has no parent: {}", policy_path.display()),
        )
    })?;

    fs::create_dir_all(parent)?;
    fs::set_permissions(parent, fs::Permissions::from_mode(0o755))?;

    let temporary_path = temporary_policy_path(policy_path);
    let write_result = write_policy_atomically(policy_path, &temporary_path);

    if write_result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }

    write_result
}

fn temporary_policy_path(policy_path: &Path) -> PathBuf {
    let file_name = policy_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("policies.json");

    policy_path.with_file_name(format!(".{file_name}.blockuntu.{}.tmp", std::process::id()))
}

fn write_policy_atomically(policy_path: &Path, temporary_path: &Path) -> io::Result<()> {
    let mut temporary_file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(temporary_path)?;

    temporary_file.set_permissions(fs::Permissions::from_mode(0o644))?;
    temporary_file.write_all(FIREFOX_POLICY_JSON.as_bytes())?;
    temporary_file.sync_all()?;
    drop(temporary_file);

    fs::rename(temporary_path, policy_path)?;
    fs::set_permissions(policy_path, fs::Permissions::from_mode(0o644))
}

fn run_uninstall() -> io::Result<()> {
    println!("starting BlocKuntu administrative uninstall");

    let mut failures = Vec::new();

    record_step(
        &mut failures,
        "disable systemd restart/stop guards",
        disable_systemd_restart_guards(),
    );
    record_step(
        &mut failures,
        "reload systemd",
        run_systemctl(&["daemon-reload"]),
    );
    record_step(
        &mut failures,
        "disable blockuntu units",
        run_systemctl(&["disable", "blockuntu.service", "blockuntu-watchdog.service"]),
    );
    record_step(
        &mut failures,
        "stop blockuntu units",
        run_systemctl(&["stop", "blockuntu-watchdog.service", "blockuntu.service"]),
    );
    record_step(
        &mut failures,
        "reset blockuntu unit state",
        run_systemctl(&[
            "reset-failed",
            "blockuntu-watchdog.service",
            "blockuntu.service",
        ]),
    );
    record_step(
        &mut failures,
        "remove Firefox enterprise policy",
        remove_file_if_exists(Path::new(FIREFOX_POLICY_PATH)),
    );
    record_step(
        &mut failures,
        "clear runtime socket directory",
        clear_runtime_dir(Path::new(RUNTIME_DIR)),
    );

    if failures.is_empty() {
        println!("BlocKuntu administrative uninstall completed");
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!(
                "BlocKuntu uninstall completed with {} failed step(s): {}",
                failures.len(),
                failures.join("; ")
            ),
        ))
    }
}

fn record_step(failures: &mut Vec<String>, label: &str, result: io::Result<()>) {
    match result {
        Ok(()) => println!("{label}: ok"),
        Err(err) => {
            eprintln!("{label}: failed: {err}");
            failures.push(format!("{label}: {err}"));
        }
    }
}

fn disable_systemd_restart_guards() -> io::Result<()> {
    for unit in BLOCKUNTU_UNITS {
        let override_dir = Path::new(SYSTEMD_RUNTIME_DIR).join(format!("{unit}.d"));
        fs::create_dir_all(&override_dir)?;

        let override_path = override_dir.join("50-blockuntu-uninstall.conf");
        let mut override_file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&override_path)?;
        override_file.set_permissions(fs::Permissions::from_mode(0o644))?;
        override_file.write_all(SYSTEMD_UNINSTALL_OVERRIDE.as_bytes())?;
        override_file.sync_all()?;
    }

    Ok(())
}

fn run_systemctl(args: &[&str]) -> io::Result<()> {
    let status = Command::new("systemctl").args(args).status()?;

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!("systemctl {} exited with {status}", args.join(" ")),
        ))
    }
}

fn remove_file_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

fn clear_runtime_dir(path: &Path) -> io::Result<()> {
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err),
    };

    for entry in entries {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let entry_path = entry.path();

        if file_type.is_dir() {
            fs::remove_dir_all(entry_path)?;
        } else {
            fs::remove_file(entry_path)?;
        }
    }

    match fs::remove_dir(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::DirectoryNotEmpty => Ok(()),
        Err(err) => Err(err),
    }
}

fn prepare_socket(socket_path: &str) -> io::Result<()> {
    let path = Path::new(socket_path);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o755))?;
    }

    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

async fn handle_client(
    mut stream: UnixStream,
    extension_activity: ExtensionActivityHandle,
) -> io::Result<()> {
    let request_bytes = match read_limited(&mut stream).await? {
        Some(request_bytes) => request_bytes,
        None => {
            write_json(
                &mut stream,
                json!({ "action": "allow", "error": "request too large" }),
            )
            .await?;
            return Ok(());
        }
    };

    if request_bytes.is_empty() {
        eprintln!("received empty request payload");
        write_json(
            &mut stream,
            json!({ "action": "allow", "error": "empty request" }),
        )
        .await?;
        return Ok(());
    }

    let request = match serde_json::from_slice::<EvaluationRequest>(&request_bytes) {
        Ok(request) => request,
        Err(err) => {
            eprintln!("invalid JSON request: {err}");
            write_json(
                &mut stream,
                json!({ "action": "allow", "error": "invalid JSON" }),
            )
            .await?;
            return Ok(());
        }
    };

    if request.message_type.as_deref() == Some("extension_heartbeat") {
        record_extension_heartbeat(&extension_activity, &request)?;
        write_json(
            &mut stream,
            json!({ "type": "extension_heartbeat", "status": "ok" }),
        )
        .await?;
        return Ok(());
    }

    let Some(url) = request.url else {
        eprintln!("request missing string field: url");
        write_json(
            &mut stream,
            json!({ "action": "allow", "error": "missing url" }),
        )
        .await?;
        return Ok(());
    };

    println!("Received evaluation request for: {url}");

    let action = decide_action(&url);

    write_json(&mut stream, json!({ "action": action })).await
}

fn decide_action(url: &str) -> &'static str {
    let normalized_url = url.to_ascii_lowercase();

    if normalized_url.contains("instagram.com") || normalized_url.contains("twitter.com") {
        "block"
    } else {
        "allow"
    }
}

async fn read_limited(stream: &mut UnixStream) -> io::Result<Option<Vec<u8>>> {
    let mut payload = Vec::new();
    let mut buffer = [0_u8; 8192];

    loop {
        let read = stream.read(&mut buffer).await?;
        if read == 0 {
            return Ok(Some(payload));
        }

        if payload.len() + read > MAX_PAYLOAD_BYTES {
            eprintln!("request exceeded {MAX_PAYLOAD_BYTES} bytes");
            return Ok(None);
        }

        payload.extend_from_slice(&buffer[..read]);
    }
}

async fn write_json(stream: &mut UnixStream, value: serde_json::Value) -> io::Result<()> {
    let bytes = serde_json::to_vec(&value)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;

    stream.write_all(&bytes).await?;
    stream.shutdown().await
}

#[derive(Debug, Eq, PartialEq)]
enum PolicyRepairStatus {
    AlreadyCompliant,
    Repaired,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum ExtensionRuntimeStatus {
    Active,
    Inactive,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum ExtensionInactiveAction {
    LogOnly,
    Poweroff,
}

#[cfg(test)]
mod tests {
    use super::{
        browser_policy_is_expected, configured_extension_inactive_action_from, decide_action,
        extension_seen_recently, verify_and_repair_browser_policies_at, ExtensionInactiveAction,
        PolicyRepairStatus, EXTENSION_HEARTBEAT_TIMEOUT, FIREFOX_POLICY_JSON,
    };
    use std::fs;
    use std::time::{Duration, Instant};

    #[test]
    fn blocks_configured_hosts() {
        assert_eq!(decide_action("https://instagram.com/reels"), "block");
        assert_eq!(decide_action("https://mobile.twitter.com/home"), "block");
        assert_eq!(decide_action("https://TWITTER.com/home"), "block");
    }

    #[test]
    fn allows_other_hosts() {
        assert_eq!(decide_action("https://example.com/"), "allow");
    }

    #[test]
    fn accepts_expected_firefox_policy_shape() {
        assert!(browser_policy_is_expected(FIREFOX_POLICY_JSON.as_bytes()));
    }

    #[test]
    fn rejects_corrupted_firefox_policy_shape() {
        assert!(!browser_policy_is_expected(br#"{"policies":{}}"#));
        assert!(!browser_policy_is_expected(b"not json"));
    }

    #[test]
    fn repairs_missing_firefox_policy() {
        let test_dir =
            std::env::temp_dir().join(format!("blockuntu-policy-test-{}", std::process::id()));
        let policy_path = test_dir.join("policies.json");
        let _ = fs::remove_dir_all(&test_dir);

        let status = verify_and_repair_browser_policies_at(&policy_path)
            .expect("missing policy should repair");

        assert_eq!(status, PolicyRepairStatus::Repaired);
        assert!(browser_policy_is_expected(
            &fs::read(&policy_path).expect("policy should exist")
        ));

        fs::remove_dir_all(&test_dir).expect("test directory should clean up");
    }

    #[test]
    fn marks_extension_active_only_with_recent_heartbeat() {
        let now = Instant::now();

        assert!(extension_seen_recently(
            Some(now - Duration::from_secs(5)),
            now
        ));
        assert!(!extension_seen_recently(
            Some(now - EXTENSION_HEARTBEAT_TIMEOUT - Duration::from_secs(1)),
            now
        ));
        assert!(!extension_seen_recently(None, now));
    }

    #[test]
    fn inactive_action_defaults_to_log_only() {
        assert_eq!(
            configured_extension_inactive_action_from(None),
            ExtensionInactiveAction::LogOnly
        );
    }

    #[test]
    fn inactive_action_can_be_poweroff() {
        assert_eq!(
            configured_extension_inactive_action_from(Some("poweroff")),
            ExtensionInactiveAction::Poweroff
        );
    }
}
