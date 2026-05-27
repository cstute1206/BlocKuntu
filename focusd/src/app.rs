use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use focus_core::{
    AppMatcherConfig, AppMatcherKind, AppRuleConfig, BlockReason, Database, Decision,
    EvaluationContext, FocusCore, RuleTier,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

use crate::cli::{Args, DEFAULT_HOSTS_PATH};
use crate::error::{DaemonError, Result};
use crate::firefox_policy::{FirefoxPolicyManager, RepairStatus};
use crate::hosts::{HostsManager, HostsRepairStatus};
use crate::process_scan::{
    attach_window_titles, kill_processes, scan_procfs, LinuxSignalKiller, WmctrlWindowTitleProvider,
};
use crate::rpc::{handle_payload, RpcContext};
use crate::socket::listener_from_systemd_or_path;

const MAX_PAYLOAD_BYTES: usize = 1024 * 1024;
const UNSUPPORTED_BROWSER_RULE_ID: &str = "unsupported-browsers-hard";

#[derive(Clone)]
pub struct DaemonApp {
    core: Arc<Mutex<FocusCore>>,
    rpc_context: RpcContext,
    firefox_policy: FirefoxPolicyManager,
    hosts: HostsManager,
    process_scan_interval: Duration,
    policy_repair_interval: Duration,
}

impl DaemonApp {
    pub fn load(args: &Args) -> Result<Self> {
        create_parent_dir(&args.database, 0o700)?;

        let database = Database::open(&args.database)?;
        let mut config = if database.has_policy_config()? {
            database.load_policy_config()?
        } else {
            let config = focus_core::load_config(&args.config)?;
            database.replace_policy_config(&config)?;
            config
        };
        if ensure_mandatory_app_rules(&mut config) {
            database.replace_policy_config(&config)?;
        }
        let core = Arc::new(Mutex::new(FocusCore::new(config, database)?));
        let rpc_context = RpcContext::new(core.clone())
            .with_extension_heartbeat_timeout_seconds(args.extension_heartbeat_timeout_seconds);
        let firefox_policy = FirefoxPolicyManager::new(
            &args.firefox_policy,
            &args.extension_id,
            &args.extension_xpi,
        );
        let hosts = HostsManager::new_with_immutable(&args.hosts, hosts_immutable_enabled(args));
        let rpc_context =
            rpc_context.with_enforcement_managers(firefox_policy.clone(), hosts.clone());

        Ok(Self {
            core,
            rpc_context,
            firefox_policy,
            hosts,
            process_scan_interval: Duration::from_secs(args.process_scan_interval_seconds),
            policy_repair_interval: Duration::from_secs(args.policy_repair_interval_seconds),
        })
    }

    pub fn check(&self) -> Result<()> {
        let core = self.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
        focus_core::validate_config(core.config())?;
        core.database()
            .set_service_state("last_check", "ok", chrono::Utc::now())?;
        Ok(())
    }

    pub fn repair_firefox_policy(&self) -> Result<RepairStatus> {
        if !self.enforcement_is_active()? {
            return Ok(RepairStatus::SkippedStopped);
        }
        self.firefox_policy.verify_and_repair()
    }

    pub fn repair_hosts(&self) -> Result<HostsRepairStatus> {
        if !self.enforcement_is_active()? {
            return Ok(HostsRepairStatus::SkippedStopped);
        }
        let core = self.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
        self.hosts.verify_and_repair(core.config())
    }

    pub async fn serve(self, args: &Args) -> Result<()> {
        self.repair_firefox_policy()?;
        self.repair_hosts()?;
        self.spawn_repair_loop();
        self.spawn_process_scan_loop();

        let listener = listener_from_systemd_or_path(&args.socket, args.dev_bind_socket)?;
        self.accept_loop(listener).await
    }

    fn spawn_repair_loop(&self) {
        let app = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(app.policy_repair_interval);
            loop {
                interval.tick().await;
                if let Err(err) = app.repair_firefox_policy() {
                    eprintln!("Firefox policy repair failed: {err}");
                }
                if let Err(err) = app.repair_hosts() {
                    eprintln!("hosts repair failed: {err}");
                }
            }
        });
    }

    fn spawn_process_scan_loop(&self) {
        let app = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(app.process_scan_interval);
            loop {
                interval.tick().await;
                if let Err(err) = app.scan_processes_once() {
                    eprintln!("process scan failed: {err}");
                }
            }
        });
    }

    fn scan_processes_once(&self) -> Result<()> {
        if !self.enforcement_is_active()? {
            return Ok(());
        }

        {
            let core = self.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
            if core.config().app_rules.is_empty() {
                return Ok(());
            }
        }

        let mut processes = scan_procfs(Path::new("/proc"))?;
        attach_window_titles(&mut processes, &WmctrlWindowTitleProvider)?;
        let now = chrono::Local::now().fixed_offset();
        let mut blocked_pids = Vec::new();
        let mut blocked_rule_by_pid = std::collections::HashMap::new();

        {
            let core = self.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
            let context = EvaluationContext::new(core.config(), core.database(), now);
            for process in &processes {
                if process.pid <= 1 || process.pid == std::process::id() {
                    continue;
                }
                let decision = focus_core::evaluate_app(&process.identity(), &context);
                if let Decision::Block(reason) = decision {
                    blocked_pids.push(process.pid);
                    if let Some(rule_id) = blocked_rule_id(&reason) {
                        blocked_rule_by_pid.insert(process.pid, rule_id.to_string());
                    }
                }
            }
        }

        if blocked_pids.is_empty() {
            return Ok(());
        }

        let events = kill_processes(&processes, &blocked_pids, &LinuxSignalKiller)?;
        if events.is_empty() {
            return Ok(());
        }

        let core = self.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
        let now = chrono::Utc::now();
        for event in events {
            let rule_id = blocked_rule_by_pid
                .get(&event.pid)
                .map(String::as_str)
                .unwrap_or("<unknown>");
            core.database().record_event(
                "app_killed",
                event
                    .command_name
                    .as_deref()
                    .or(event.executable_basename.as_deref())
                    .or(event.desktop_id.as_deref()),
                Some(&format!(
                    "pid={};rule_id={};exe={};basename={};command={};desktop_id={};window_titles={}",
                    event.pid,
                    rule_id,
                    event
                        .executable_path
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "<unknown>".to_string()),
                    event.executable_basename.as_deref().unwrap_or("<unknown>"),
                    event.command_name.as_deref().unwrap_or("<unknown>"),
                    event.desktop_id.as_deref().unwrap_or("<unknown>"),
                    if event.window_titles.is_empty() {
                        "<none>".to_string()
                    } else {
                        event.window_titles.join(" | ")
                    }
                )),
                now,
            )?;
        }
        Ok(())
    }

    fn enforcement_is_active(&self) -> Result<bool> {
        let core = self.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
        let state = core.database().service_state("enforcement_state")?;
        Ok(state.as_deref() != Some("stopped"))
    }

    async fn accept_loop(self, listener: UnixListener) -> Result<()> {
        loop {
            tokio::select! {
                accepted = listener.accept() => {
                    let (stream, _) = accepted?;
                    let context = self.rpc_context.clone();
                    tokio::spawn(async move {
                        if let Err(err) = handle_client(stream, context).await {
                            eprintln!("client error: {err}");
                        }
                    });
                }
                signal = tokio::signal::ctrl_c() => {
                    signal?;
                    return Ok(());
                }
            }
        }
    }
}

async fn handle_client(mut stream: UnixStream, context: RpcContext) -> Result<()> {
    let request = read_limited(&mut stream).await?;
    let response = handle_payload(&context, &request);
    stream.write_all(&response).await?;
    stream.shutdown().await?;
    Ok(())
}

async fn read_limited(stream: &mut UnixStream) -> Result<Vec<u8>> {
    let mut payload = Vec::new();
    let mut buffer = [0_u8; 8192];

    loop {
        let read = stream.read(&mut buffer).await?;
        if read == 0 {
            return Ok(payload);
        }
        if payload.len() + read > MAX_PAYLOAD_BYTES {
            return Err(DaemonError::InvalidRequest(format!(
                "request exceeds {MAX_PAYLOAD_BYTES} bytes"
            )));
        }
        payload.extend_from_slice(&buffer[..read]);
    }
}

fn create_parent_dir(path: &Path, mode: u32) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(parent, fs::Permissions::from_mode(mode))?;
        }
    }
    Ok(())
}

fn hosts_immutable_enabled(args: &Args) -> bool {
    if args.hosts_immutable {
        return true;
    }
    if args.no_hosts_immutable {
        return false;
    }
    args.hosts == Path::new(DEFAULT_HOSTS_PATH)
}

fn ensure_mandatory_app_rules(config: &mut focus_core::Config) -> bool {
    let rule = unsupported_browser_rule();
    match config
        .app_rules
        .iter()
        .position(|candidate| candidate.id == rule.id)
    {
        Some(index) if config.app_rules[index] == rule => false,
        Some(index) => {
            config.app_rules[index] = rule;
            true
        }
        None => {
            config.app_rules.push(rule);
            true
        }
    }
}

fn unsupported_browser_rule() -> AppRuleConfig {
    AppRuleConfig {
        id: UNSUPPORTED_BROWSER_RULE_ID.to_string(),
        name: "Unsupported browsers hard block".to_string(),
        tier: RuleTier::Hard,
        enabled: true,
        matchers: unsupported_browser_matchers()
            .into_iter()
            .map(|(kind, value)| AppMatcherConfig {
                kind,
                value: value.to_string(),
            })
            .collect(),
        schedule_ids: Vec::new(),
        allowance_id: None,
        unlock_policy: None,
    }
}

fn unsupported_browser_matchers() -> Vec<(AppMatcherKind, &'static str)> {
    use AppMatcherKind::{CommandName, DesktopId, ExecutableBasename, WindowTitleContains};

    vec![
        (ExecutableBasename, "chromium"),
        (CommandName, "chromium"),
        (ExecutableBasename, "chromium-browser"),
        (CommandName, "chromium-browser"),
        (DesktopId, "chromium.desktop"),
        (DesktopId, "org.chromium.Chromium.desktop"),
        (ExecutableBasename, "brave"),
        (CommandName, "brave"),
        (ExecutableBasename, "brave-browser"),
        (CommandName, "brave-browser"),
        (DesktopId, "brave-browser.desktop"),
        (DesktopId, "com.brave.Browser.desktop"),
        (ExecutableBasename, "microsoft-edge"),
        (CommandName, "microsoft-edge"),
        (ExecutableBasename, "microsoft-edge-stable"),
        (CommandName, "microsoft-edge-stable"),
        (DesktopId, "microsoft-edge.desktop"),
        (DesktopId, "com.microsoft.Edge.desktop"),
        (ExecutableBasename, "opera"),
        (CommandName, "opera"),
        (DesktopId, "opera.desktop"),
        (DesktopId, "com.opera.Opera.desktop"),
        (ExecutableBasename, "vivaldi"),
        (CommandName, "vivaldi"),
        (ExecutableBasename, "vivaldi-stable"),
        (CommandName, "vivaldi-stable"),
        (DesktopId, "vivaldi-stable.desktop"),
        (DesktopId, "com.vivaldi.Vivaldi.desktop"),
        (ExecutableBasename, "librewolf"),
        (CommandName, "librewolf"),
        (DesktopId, "librewolf.desktop"),
        (DesktopId, "io.gitlab.librewolf-community.desktop"),
        (ExecutableBasename, "waterfox"),
        (CommandName, "waterfox"),
        (DesktopId, "waterfox.desktop"),
        (ExecutableBasename, "epiphany"),
        (CommandName, "epiphany"),
        (DesktopId, "org.gnome.Epiphany.desktop"),
        (ExecutableBasename, "falkon"),
        (CommandName, "falkon"),
        (DesktopId, "org.kde.falkon.desktop"),
        (ExecutableBasename, "qutebrowser"),
        (CommandName, "qutebrowser"),
        (DesktopId, "org.qutebrowser.qutebrowser.desktop"),
        (ExecutableBasename, "midori"),
        (CommandName, "midori"),
        (DesktopId, "midori.desktop"),
        (ExecutableBasename, "min"),
        (CommandName, "min"),
        (DesktopId, "min.desktop"),
        (ExecutableBasename, "nyxt"),
        (CommandName, "nyxt"),
        (DesktopId, "nyxt.desktop"),
        (ExecutableBasename, "torbrowser"),
        (CommandName, "torbrowser"),
        (ExecutableBasename, "start-tor-browser"),
        (CommandName, "start-tor-browser"),
        (DesktopId, "torbrowser.desktop"),
        (WindowTitleContains, "Tor Browser"),
    ]
}

fn blocked_rule_id(reason: &BlockReason) -> Option<&str> {
    match reason {
        BlockReason::HardBlock { rule_id, .. } | BlockReason::ControlledAccess { rule_id, .. } => {
            Some(rule_id.as_str())
        }
        BlockReason::InvalidUrl { .. } | BlockReason::RuntimeError { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use focus_core::{Config, ProcessIdentity};

    use super::{ensure_mandatory_app_rules, UNSUPPORTED_BROWSER_RULE_ID};

    #[test]
    fn injects_unsupported_browser_hard_rule() {
        let mut config = Config::default();

        assert!(ensure_mandatory_app_rules(&mut config));
        assert!(config
            .app_rules
            .iter()
            .any(|rule| rule.id == UNSUPPORTED_BROWSER_RULE_ID));
        assert!(!ensure_mandatory_app_rules(&mut config));
    }

    #[test]
    fn unsupported_browser_rule_blocks_chromium_but_not_supported_browsers() {
        let mut config = Config::default();
        ensure_mandatory_app_rules(&mut config);
        let database = focus_core::Database::in_memory().expect("database should initialize");
        let core = focus_core::FocusCore::new(config, database).expect("core should initialize");
        let now = chrono::Local::now().fixed_offset();
        let context = focus_core::EvaluationContext::new(core.config(), core.database(), now);

        let chromium = process("chromium");
        let firefox = process("firefox");
        let chrome = process("google-chrome");

        assert!(focus_core::evaluate_app(&chromium, &context).is_block());
        assert!(!focus_core::evaluate_app(&firefox, &context).is_block());
        assert!(!focus_core::evaluate_app(&chrome, &context).is_block());
    }

    fn process(name: &str) -> ProcessIdentity {
        ProcessIdentity {
            pid: Some(100),
            executable_path: None,
            executable_basename: Some(name.to_string()),
            command_name: Some(name.to_string()),
            desktop_id: None,
            window_titles: Vec::new(),
        }
    }
}
