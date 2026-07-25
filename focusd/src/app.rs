use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{
    DateTime, Datelike, Days, Duration as ChronoDuration, FixedOffset, Local, TimeZone, Utc,
};
use focus_core::{
    allowance_statuses, schedule_ids_are_active_at, AppMatcherConfig, AppMatcherKind,
    AppRuleConfig, BlockReason, Database, Decision, DetoxSession, EvaluationContext, FocusCore,
    ProcessIdentity, RuleTier, StrictModeConfig, Weekday,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

use crate::chrome_policy::{ChromePolicyManager, ChromePolicyRepairStatus};
use crate::cli::{Args, DEFAULT_HOSTS_PATH, DEFAULT_POLICY_RECOVERY_PATH};
use crate::clock_guard;
use crate::error::{DaemonError, Result};
use crate::firefox_policy::{FirefoxPolicyManager, RepairStatus};
use crate::hosts::{HostsManager, HostsRepairStatus};
use crate::policy_recovery::PolicyRecoveryManager;
use crate::process_scan::{
    attach_detected_window_titles, kill_processes, scan_procfs, supported_browser_for_process,
    LinuxSignalKiller, ProcessInfo, SupportedBrowser,
};
use crate::rpc::{handle_payload, RpcContext};
use crate::socket::listener_from_systemd_or_path;

const MAX_PAYLOAD_BYTES: usize = 1024 * 1024;
const UNSUPPORTED_BROWSER_RULE_ID: &str = "unsupported-browsers-hard";
const FIREFOX_EXTENSION_HEARTBEAT_COMPONENT: &str = "firefox_extension";
const CHROME_EXTENSION_HEARTBEAT_COMPONENT: &str = "chrome_extension";
const STRICT_FIREFOX_MISSING_SINCE_KEY: &str = "strict_mode.firefox_missing_since";
const STRICT_CHROME_MISSING_SINCE_KEY: &str = "strict_mode.chrome_missing_since";
const STRICT_FIREFOX_BROWSER_SESSION_STARTED_AT_KEY: &str =
    "strict_mode.firefox_browser_session_started_at";
const STRICT_CHROME_BROWSER_SESSION_STARTED_AT_KEY: &str =
    "strict_mode.chrome_browser_session_started_at";
const MIN_BROWSER_STARTUP_HEARTBEAT_GRACE_SECONDS: i64 = 60;
const NOTIFICATION_STATE_INTERVAL_SECONDS: u64 = 5;
const ALLOWANCE_NOTIFICATION_TTL_MINUTES: i64 = 5;
const LIFECYCLE_NOTIFICATION_TTL_MINUTES: i64 = 10;

#[derive(Clone)]
pub struct DaemonApp {
    core: Arc<Mutex<FocusCore>>,
    rpc_context: RpcContext,
    firefox_policy: FirefoxPolicyManager,
    chrome_policy: ChromePolicyManager,
    hosts: HostsManager,
    manage_firefox_policy: bool,
    manage_chrome_policy: bool,
    defer_firefox_policy_repair_until_heartbeat: bool,
    defer_chrome_policy_repair_until_heartbeat: bool,
    process_scan_interval: Duration,
    policy_repair_interval: Duration,
}

impl DaemonApp {
    pub fn load(args: &Args) -> Result<Self> {
        create_parent_dir(&args.database, 0o700)?;

        let database_preexisting = args.database.exists();
        let mut database = Database::open(&args.database)?;
        database.set_event_log_path(&args.event_log)?;
        let policy_recovery = PolicyRecoveryManager::new(
            &args.policy_recovery,
            policy_recovery_immutable_enabled(args),
        );
        let (mut config, recovered) = load_startup_policy(
            &database,
            &args.config,
            &policy_recovery,
            database_preexisting,
        )?;
        if ensure_mandatory_app_rules(&mut config) {
            database.replace_policy_config(&config)?;
        }
        policy_recovery.write(&config)?;
        if recovered {
            database.record_event(
                "policy_recovered",
                Some("policy"),
                Some(&format!(
                    "restored from {}",
                    policy_recovery.path().display()
                )),
                Utc::now(),
            )?;
        }
        let core = Arc::new(Mutex::new(FocusCore::new(config, database)?));
        {
            let core = core.lock().map_err(|_| DaemonError::LockPoisoned)?;
            core.database().sync_schedule_activity_totals(
                &core.config().schedules,
                Local::now().fixed_offset(),
            )?;
        }
        let rpc_context = RpcContext::new(core.clone())
            .with_policy_recovery(policy_recovery)
            .with_event_log_path(&args.event_log)
            .with_extension_heartbeat_timeout_seconds(args.extension_heartbeat_timeout_seconds);
        let firefox_policy = FirefoxPolicyManager::new(
            &args.firefox_policy,
            &args.extension_id,
            &args.extension_xpi,
        );
        let chrome_policy = ChromePolicyManager::new(
            &args.chrome_policy,
            &args.chrome_update_manifest,
            &args.chrome_extension_id,
            &args.chrome_extension_version,
            &args.chrome_extension_crx_url,
        );
        let hosts = HostsManager::new_with_immutable(&args.hosts, hosts_immutable_enabled(args));
        let rpc_context = rpc_context
            .with_enforcement_managers(firefox_policy.clone(), chrome_policy.clone(), hosts.clone())
            .with_browser_policy_management(
                args.manage_firefox_policy(),
                args.manage_chrome_policy(),
            )
            .with_deferred_browser_policy_repair(
                args.defer_firefox_policy_repair_until_heartbeat(),
                args.defer_chrome_policy_repair_until_heartbeat(),
            );

        Ok(Self {
            core,
            rpc_context,
            firefox_policy,
            chrome_policy,
            hosts,
            manage_firefox_policy: args.manage_firefox_policy(),
            manage_chrome_policy: args.manage_chrome_policy(),
            defer_firefox_policy_repair_until_heartbeat: args
                .defer_firefox_policy_repair_until_heartbeat(),
            defer_chrome_policy_repair_until_heartbeat: args
                .defer_chrome_policy_repair_until_heartbeat(),
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
        if !self.manage_firefox_policy {
            return Ok(RepairStatus::SkippedDisabled);
        }
        if self.defer_firefox_policy_repair_until_heartbeat
            && !self.has_extension_heartbeat(FIREFOX_EXTENSION_HEARTBEAT_COMPONENT)?
        {
            return Ok(RepairStatus::SkippedDeferred);
        }
        if !self.enforcement_is_active()? {
            return Ok(RepairStatus::SkippedInactive);
        }
        self.firefox_policy.verify_and_repair()
    }

    pub fn repair_chrome_policy(&self) -> Result<ChromePolicyRepairStatus> {
        if !self.manage_chrome_policy {
            return Ok(ChromePolicyRepairStatus::SkippedDisabled);
        }
        if self.defer_chrome_policy_repair_until_heartbeat
            && !self.has_extension_heartbeat(CHROME_EXTENSION_HEARTBEAT_COMPONENT)?
        {
            return Ok(ChromePolicyRepairStatus::SkippedDeferred);
        }
        if !self.enforcement_is_active()? {
            return Ok(ChromePolicyRepairStatus::SkippedInactive);
        }
        self.chrome_policy.verify_and_repair()
    }

    pub fn repair_hosts(&self) -> Result<HostsRepairStatus> {
        if !self.enforcement_is_active()? {
            return Ok(HostsRepairStatus::SkippedInactive);
        }
        let core = self.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
        let guarded = clock_guard::guarded_now(core.database(), None, false)?;
        let active_detox_sessions = hosts_detox_sessions_for_clock(
            &core,
            guarded.now.with_timezone(&Utc),
            guarded.integrity.state == "tampered",
        )?;
        self.hosts.verify_and_repair_with_active_detox(
            core.config(),
            &active_detox_sessions,
            guarded.now,
            guarded.integrity.state == "tampered",
        )
    }

    pub async fn serve(self, args: &Args) -> Result<()> {
        self.repair_firefox_policy()?;
        self.repair_chrome_policy()?;
        self.repair_hosts()?;
        self.spawn_repair_loop();
        self.spawn_process_scan_loop();
        self.spawn_notification_loop();

        let listener = listener_from_systemd_or_path(&args.socket, args.dev_bind_socket)?;
        self.accept_loop(listener).await
    }

    fn spawn_repair_loop(&self) {
        let app = self.clone();
        tokio::spawn(async move {
            loop {
                if let Err(err) = app.repair_firefox_policy() {
                    eprintln!("Firefox policy repair failed: {err}");
                }
                if let Err(err) = app.repair_chrome_policy() {
                    eprintln!("Chrome policy repair failed: {err}");
                }
                if let Err(err) = app.repair_hosts() {
                    eprintln!("hosts repair failed: {err}");
                }
                tokio::time::sleep(app.next_policy_repair_delay()).await;
            }
        });
    }

    fn next_policy_repair_delay(&self) -> Duration {
        let Ok(core) = self.core.lock() else {
            return self.policy_repair_interval;
        };
        let Ok(guarded) = clock_guard::guarded_now(core.database(), None, false) else {
            return self.policy_repair_interval;
        };
        if guarded.integrity.state == "tampered" {
            return self.policy_repair_interval;
        }

        next_tier2_site_schedule_boundary(core.config(), guarded.now)
            .map(|delay| delay.min(self.policy_repair_interval))
            .unwrap_or(self.policy_repair_interval)
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

    fn spawn_notification_loop(&self) {
        let app = self.clone();
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(Duration::from_secs(NOTIFICATION_STATE_INTERVAL_SECONDS));
            loop {
                interval.tick().await;
                if let Err(err) = app.sync_notification_state_once() {
                    eprintln!("notification state sync failed: {err}");
                }
            }
        });
    }

    fn sync_notification_state_once(&self) -> Result<()> {
        let core = self.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
        let guarded = clock_guard::guarded_now(core.database(), None, false)?;
        if guarded.integrity.state == "tampered" {
            return Ok(());
        }
        sync_schedule_notifications(&core, guarded.now)?;
        sync_detox_notifications(&core, guarded.now)?;
        sync_allowance_notifications(&core, guarded.now)?;
        Ok(())
    }

    fn scan_processes_once(&self) -> Result<()> {
        if !self.enforcement_is_active()? {
            self.end_open_app_usage_sessions()?;
            return Ok(());
        }

        {
            let core = self.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
            if core.config().app_rules.is_empty()
                && !strict_supported_browser_enforcement_enabled(&core.config().strict_mode)
            {
                core.database()
                    .end_open_app_usage_sessions(chrono::Utc::now())?;
                return Ok(());
            }
        }

        let mut processes = scan_procfs(Path::new("/proc"))?;
        attach_detected_window_titles(&mut processes);
        let mut blocked_pids = Vec::new();
        let mut kill_details_by_pid = HashMap::new();
        let mut kill_event_kind_by_pid = HashMap::new();
        let mut blocked_rule_id_by_pid = HashMap::new();

        {
            let core = self.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
            let guarded = clock_guard::guarded_now(core.database(), None, false)?;
            let now = guarded.now;
            let clock_tampered = guarded.integrity.state == "tampered";
            if !clock_tampered {
                sync_metered_app_usage_sessions(&core, &processes, now)?;
            }
            let context = EvaluationContext::new(core.config(), core.database(), now)
                .with_clock_tampered(clock_tampered);
            for process in &processes {
                if process.pid <= 1
                    || process.pid == std::process::id()
                    || is_blockuntu_process(&process.identity())
                {
                    continue;
                }
                let decision = focus_core::evaluate_app(&process.identity(), &context);
                if let Decision::Block(reason) = decision {
                    blocked_pids.push(process.pid);
                    if let Some(rule_id) = blocked_rule_id(&reason) {
                        kill_details_by_pid.insert(process.pid, format!("rule_id={rule_id}"));
                        kill_event_kind_by_pid.insert(process.pid, "app_killed");
                        blocked_rule_id_by_pid.insert(process.pid, rule_id.to_string());
                    }
                }
            }

            let mut strict_mode = core.config().strict_mode.clone();
            if self.defer_firefox_policy_repair_until_heartbeat
                && !self
                    .has_extension_heartbeat_locked(&core, FIREFOX_EXTENSION_HEARTBEAT_COMPONENT)?
            {
                strict_mode.require_firefox_extension = false;
            }
            if self.defer_chrome_policy_repair_until_heartbeat
                && !self
                    .has_extension_heartbeat_locked(&core, CHROME_EXTENSION_HEARTBEAT_COMPONENT)?
            {
                strict_mode.require_chrome_extension = false;
            }

            for (pid, detail) in strict_browser_kill_details(
                &processes,
                strict_mode,
                core.database(),
                now.with_timezone(&Utc),
            )? {
                if !blocked_pids.contains(&pid) {
                    blocked_pids.push(pid);
                }
                kill_details_by_pid.insert(pid, detail);
                kill_event_kind_by_pid.insert(pid, "browser_killed_extension_stale");
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
            let detail_prefix = kill_details_by_pid
                .get(&event.pid)
                .map(String::as_str)
                .unwrap_or("reason=<unknown>");
            let event_kind = kill_event_kind_by_pid
                .get(&event.pid)
                .copied()
                .unwrap_or("app_killed");
            let application_name = event
                .command_name
                .as_deref()
                .or(event.executable_basename.as_deref())
                .or(event.desktop_id.as_deref())
                .unwrap_or("Application");
            core.database().record_event(
                event_kind,
                Some(application_name),
                Some(&format!(
                    "pid={};{};exe={};basename={};command={};desktop_id={};window_titles={}",
                    event.pid,
                    detail_prefix,
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
            if let Some(rule_id) = blocked_rule_id_by_pid.get(&event.pid) {
                if let Err(error) = crate::rpc::enqueue_application_block_notification(
                    &core,
                    rule_id,
                    application_name,
                    now,
                ) {
                    eprintln!("could not queue application block notification: {error}");
                }
            }
        }
        Ok(())
    }

    fn has_extension_heartbeat(&self, component: &str) -> Result<bool> {
        let core = self.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
        self.has_extension_heartbeat_locked(&core, component)
    }

    fn has_extension_heartbeat_locked(&self, core: &FocusCore, component: &str) -> Result<bool> {
        Ok(core.database().heartbeat(component)?.is_some())
    }

    fn enforcement_is_active(&self) -> Result<bool> {
        let core = self.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
        crate::rpc::enforcement_active_from_core(&core)
    }

    fn end_open_app_usage_sessions(&self) -> Result<()> {
        let core = self.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
        core.database()
            .end_open_app_usage_sessions(chrono::Utc::now())?;
        Ok(())
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

fn next_tier2_site_schedule_boundary(
    config: &focus_core::Config,
    now: DateTime<FixedOffset>,
) -> Option<Duration> {
    let schedule_ids: HashSet<&str> = config
        .rules
        .iter()
        .filter(|rule| rule.tier == RuleTier::ScheduledBlock)
        .flat_map(|rule| rule.schedule_ids.iter().map(String::as_str))
        .collect();
    let mut next: Option<DateTime<FixedOffset>> = None;

    for schedule in config
        .schedules
        .iter()
        .filter(|schedule| schedule_ids.contains(schedule.id.as_str()))
    {
        for day_offset in 0..=8 {
            let Some(date) = now.date_naive().checked_add_days(Days::new(day_offset)) else {
                continue;
            };
            let weekday = Weekday::from(date.weekday());
            for window in schedule
                .windows
                .iter()
                .filter(|window| window.weekday.includes(weekday))
            {
                let start = now
                    .offset()
                    .with_ymd_and_hms(
                        date.year(),
                        date.month(),
                        date.day(),
                        u32::from(window.start.hour()),
                        u32::from(window.start.minute()),
                        0,
                    )
                    .single()?;
                let end_date = if window.start < window.end {
                    date
                } else {
                    date.checked_add_days(Days::new(1))?
                };
                let end = now
                    .offset()
                    .with_ymd_and_hms(
                        end_date.year(),
                        end_date.month(),
                        end_date.day(),
                        u32::from(window.end.hour()),
                        u32::from(window.end.minute()),
                        0,
                    )
                    .single()?;

                for boundary in [start, end] {
                    if boundary > now && next.is_none_or(|current| boundary < current) {
                        next = Some(boundary);
                    }
                }
            }
        }
    }

    next.and_then(|boundary| (boundary - now).to_std().ok())
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

fn strict_browser_kill_details(
    processes: &[ProcessInfo],
    strict_mode: StrictModeConfig,
    database: &Database,
    now: DateTime<Utc>,
) -> Result<HashMap<u32, String>> {
    let mut kill_details = HashMap::new();

    if !strict_mode.kill_supported_browser_if_extension_stale {
        return Ok(kill_details);
    }

    for browser in [SupportedBrowser::Firefox, SupportedBrowser::Chrome] {
        if !browser_required_by_strict_mode(browser, &strict_mode) {
            continue;
        }

        let browser_pids = processes
            .iter()
            .filter(|process| supported_browser_for_process(&process.identity()) == Some(browser))
            .map(|process| process.pid)
            .collect::<Vec<_>>();

        if browser_pids.is_empty() {
            database.set_service_state(browser.missing_since_key(), "inactive", now)?;
            database.set_service_state(browser.session_started_at_key(), "inactive", now)?;
            continue;
        }

        if let Some(reason) = strict_browser_unhealthy_reason(browser, &strict_mode, database, now)?
        {
            for pid in browser_pids {
                kill_details.insert(pid, reason.clone());
            }
        }
    }

    Ok(kill_details)
}

fn strict_browser_unhealthy_reason(
    browser: SupportedBrowser,
    strict_mode: &StrictModeConfig,
    database: &Database,
    now: DateTime<Utc>,
) -> Result<Option<String>> {
    let session_started_at = browser_session_started_at(browser, database, now)?;
    let startup_grace_seconds = browser_startup_grace_seconds(strict_mode);
    let startup_elapsed_seconds = now
        .signed_duration_since(session_started_at)
        .num_seconds()
        .max(0);
    let heartbeat = database.heartbeat(browser.heartbeat_component())?;
    let heartbeat_is_for_current_session = heartbeat
        .as_ref()
        .is_some_and(|heartbeat| heartbeat.last_seen_at >= session_started_at);

    // A heartbeat from a previous browser launch is deliberately not accepted
    // here. A newly observed browser gets a bounded opportunity to start its
    // extension and Native Messaging host before strict enforcement begins.
    if !heartbeat_is_for_current_session {
        database.set_service_state(
            browser.missing_since_key(),
            &session_started_at.to_rfc3339(),
            now,
        )?;

        if startup_elapsed_seconds <= startup_grace_seconds {
            return Ok(None);
        }

        return Ok(Some(format!(
            "browser={};component={};heartbeat_missing_since_launch_seconds={};startup_grace_seconds={}",
            browser.label(),
            browser.heartbeat_component(),
            startup_elapsed_seconds,
            startup_grace_seconds
        )));
    }

    let heartbeat = heartbeat.expect("a current-session heartbeat was checked above");
    let grace_seconds = i64::from(strict_mode.grace_seconds);
    database.set_service_state(browser.missing_since_key(), "healthy", now)?;
    let age_seconds = now
        .signed_duration_since(heartbeat.last_seen_at)
        .num_seconds()
        .max(0);

    if age_seconds <= grace_seconds {
        return Ok(None);
    }

    Ok(Some(format!(
        "browser={};component={};heartbeat_age_seconds={};grace_seconds={}",
        browser.label(),
        browser.heartbeat_component(),
        age_seconds,
        strict_mode.grace_seconds
    )))
}

fn browser_session_started_at(
    browser: SupportedBrowser,
    database: &Database,
    now: DateTime<Utc>,
) -> Result<DateTime<Utc>> {
    if let Some(started_at) = strict_browser_session_started_at(database, browser)? {
        return Ok(started_at);
    }

    database.set_service_state(browser.session_started_at_key(), &now.to_rfc3339(), now)?;
    Ok(now)
}

pub(crate) fn strict_browser_session_started_at(
    database: &Database,
    browser: SupportedBrowser,
) -> Result<Option<DateTime<Utc>>> {
    Ok(database
        .service_state(browser.session_started_at_key())?
        .as_deref()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc)))
}

pub(crate) fn browser_startup_grace_seconds(strict_mode: &StrictModeConfig) -> i64 {
    i64::from(strict_mode.grace_seconds).max(MIN_BROWSER_STARTUP_HEARTBEAT_GRACE_SECONDS)
}

impl SupportedBrowser {
    fn label(self) -> &'static str {
        match self {
            Self::Firefox => "firefox",
            Self::Chrome => "chrome",
        }
    }

    fn heartbeat_component(self) -> &'static str {
        match self {
            Self::Firefox => FIREFOX_EXTENSION_HEARTBEAT_COMPONENT,
            Self::Chrome => CHROME_EXTENSION_HEARTBEAT_COMPONENT,
        }
    }

    fn missing_since_key(self) -> &'static str {
        match self {
            Self::Firefox => STRICT_FIREFOX_MISSING_SINCE_KEY,
            Self::Chrome => STRICT_CHROME_MISSING_SINCE_KEY,
        }
    }

    fn session_started_at_key(self) -> &'static str {
        match self {
            Self::Firefox => STRICT_FIREFOX_BROWSER_SESSION_STARTED_AT_KEY,
            Self::Chrome => STRICT_CHROME_BROWSER_SESSION_STARTED_AT_KEY,
        }
    }
}

fn browser_required_by_strict_mode(
    browser: SupportedBrowser,
    strict_mode: &StrictModeConfig,
) -> bool {
    match browser {
        SupportedBrowser::Firefox => strict_mode.require_firefox_extension,
        SupportedBrowser::Chrome => strict_mode.require_chrome_extension,
    }
}

fn strict_supported_browser_enforcement_enabled(strict_mode: &StrictModeConfig) -> bool {
    strict_mode.kill_supported_browser_if_extension_stale
        && (strict_mode.require_firefox_extension || strict_mode.require_chrome_extension)
}

pub(crate) fn is_blockuntu_process(process: &ProcessIdentity) -> bool {
    if process
        .executable_path
        .as_deref()
        .and_then(|path| Path::new(path).file_name())
        .and_then(|name| name.to_str())
        .is_some_and(is_blockuntu_name)
    {
        return true;
    }

    [
        process.executable_basename.as_deref(),
        process.command_name.as_deref(),
        process.desktop_id.as_deref(),
    ]
    .iter()
    .flatten()
    .any(|value| is_blockuntu_name(value))
}

fn is_blockuntu_name(value: &str) -> bool {
    let value = value.trim().to_ascii_lowercase();
    value.starts_with("blockuntu")
}

fn sync_metered_app_usage_sessions(
    core: &FocusCore,
    processes: &[ProcessInfo],
    now: DateTime<FixedOffset>,
) -> Result<Vec<String>> {
    let context = EvaluationContext::new(core.config(), core.database(), now);
    let mut rule_ids = HashSet::new();

    for process in processes {
        if process.pid <= 1
            || process.pid == std::process::id()
            || is_blockuntu_process(&process.identity())
        {
            continue;
        }

        for rule_id in focus_core::metered_app_rule_ids_for_process(&process.identity(), &context)?
        {
            rule_ids.insert(rule_id);
        }
    }

    let mut rule_ids = rule_ids.into_iter().collect::<Vec<_>>();
    rule_ids.sort();
    core.database()
        .sync_app_usage_sessions(&rule_ids, now.with_timezone(&Utc))?;
    Ok(rule_ids)
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

fn policy_recovery_immutable_enabled(args: &Args) -> bool {
    if args.policy_recovery_immutable {
        return true;
    }
    if args.no_policy_recovery_immutable {
        return false;
    }
    args.policy_recovery == Path::new(DEFAULT_POLICY_RECOVERY_PATH)
}

fn load_startup_policy(
    database: &Database,
    config_path: &Path,
    policy_recovery: &PolicyRecoveryManager,
    database_preexisting: bool,
) -> Result<(focus_core::Config, bool)> {
    if database.has_policy_config()? {
        return Ok((database.load_policy_config()?, false));
    }

    if let Some(config) = policy_recovery.load()? {
        database.replace_policy_config(&config)?;
        return Ok((config, true));
    }

    if database_preexisting {
        return Err(DaemonError::MissingPolicyRecovery {
            recovery_path: policy_recovery.path().to_path_buf(),
        });
    }

    let config = focus_core::load_config(config_path)?;
    database.replace_policy_config(&config)?;
    Ok((config, false))
}

pub(crate) fn ensure_mandatory_app_rules(config: &mut focus_core::Config) -> bool {
    if !config.strict_mode.block_unsupported_browsers {
        if let Some(index) = config
            .app_rules
            .iter()
            .position(|candidate| candidate.id == UNSUPPORTED_BROWSER_RULE_ID)
        {
            config.app_rules.remove(index);
            return true;
        }
        return false;
    }

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

fn sync_schedule_notifications(core: &FocusCore, now: DateTime<FixedOffset>) -> Result<()> {
    let now_utc = now.with_timezone(&Utc);
    let preferences = core.database().notification_preferences()?;
    let previous = core
        .database()
        .notification_lifecycle_states("schedule")?
        .into_iter()
        .collect::<HashMap<_, _>>();
    let mut current_ids = HashSet::new();

    for schedule in &core.config().schedules {
        current_ids.insert(schedule.id.clone());
        let active = schedule_ids_are_active_at(
            std::slice::from_ref(&schedule.id),
            core.config(),
            now,
            false,
        );
        if let Some(was_active) = previous.get(&schedule.id).copied() {
            if was_active != active {
                let name = schedule.name.as_deref().unwrap_or(&schedule.id);
                let (kind, title, detail, enabled) = if active {
                    (
                        "schedule_started",
                        "Schedule started",
                        format!("\"{name}\" is now active."),
                        preferences.schedule_started,
                    )
                } else {
                    (
                        "schedule_ended",
                        "Schedule ended",
                        format!("\"{name}\" is no longer active."),
                        preferences.schedule_ended,
                    )
                };
                core.database()
                    .record_event(kind, Some(&schedule.id), Some(&detail), now_utc)?;
                if preferences.enabled && enabled {
                    core.database().enqueue_notification(
                        kind,
                        title,
                        &detail,
                        &format!("{kind}:{}", schedule.id),
                        now_utc,
                        ChronoDuration::minutes(1),
                        ChronoDuration::minutes(LIFECYCLE_NOTIFICATION_TTL_MINUTES),
                    )?;
                }
            }
        }
        core.database().set_notification_lifecycle_state(
            "schedule",
            &schedule.id,
            active,
            now_utc,
        )?;
    }

    for schedule_id in previous.keys() {
        if !current_ids.contains(schedule_id) {
            core.database()
                .delete_notification_lifecycle_state("schedule", schedule_id)?;
        }
    }
    Ok(())
}

fn sync_detox_notifications(core: &FocusCore, now: DateTime<FixedOffset>) -> Result<()> {
    let now_utc = now.with_timezone(&Utc);
    let previous = core
        .database()
        .notification_lifecycle_states("detox")?
        .into_iter()
        .collect::<HashMap<_, _>>();
    let sessions = core.database().detox_sessions(200)?;
    let mut current_ids = HashSet::new();

    for session in &sessions {
        current_ids.insert(session.id.clone());
        let active = session.cancelled_at.is_none()
            && session.starts_at <= now_utc
            && session.ends_at > now_utc;
        if let Some(was_active) = previous.get(&session.id).copied() {
            if was_active != active {
                let kind = if active {
                    "detox_started"
                } else {
                    "detox_ended"
                };
                core.database().record_event(
                    kind,
                    Some(&session.id),
                    Some(if active {
                        "Detox became active"
                    } else {
                        "Detox reached its end time"
                    }),
                    now_utc,
                )?;
                crate::rpc::enqueue_detox_notification(
                    core,
                    session,
                    active,
                    session.cancelled_at.is_some(),
                    now_utc,
                )?;
            }
        }
        core.database()
            .set_notification_lifecycle_state("detox", &session.id, active, now_utc)?;
    }

    for session_id in previous.keys() {
        if !current_ids.contains(session_id) {
            core.database()
                .delete_notification_lifecycle_state("detox", session_id)?;
        }
    }
    Ok(())
}

fn sync_allowance_notifications(core: &FocusCore, now: DateTime<FixedOffset>) -> Result<()> {
    let now_utc = now.with_timezone(&Utc);
    let local_day = now.date_naive().to_string();
    let preferences = core.database().notification_preferences()?;
    let context = EvaluationContext::new(core.config(), core.database(), now);

    for status in allowance_statuses(&context)? {
        let previous = core
            .database()
            .allowance_notification_state(&status.rule_id)?;
        let crossed_threshold = previous
            .filter(|(previous_day, _)| previous_day == &local_day)
            .and_then(|(_, previous_remaining)| {
                preferences
                    .allowance_warning_minutes
                    .iter()
                    .copied()
                    .filter(|minutes| {
                        let threshold_seconds = i64::from(*minutes) * 60;
                        previous_remaining > threshold_seconds
                            && status.remaining_seconds <= threshold_seconds
                    })
                    .min()
            });

        if preferences.enabled && preferences.allowance_warnings {
            if let Some(threshold_minutes) = crossed_threshold {
                let body = allowance_notification_body(&status.rule_name, status.remaining_seconds);
                let inserted = core.database().enqueue_notification(
                    "allowance_warning",
                    "Allowance running low",
                    &body,
                    &format!(
                        "allowance_warning:{}:{local_day}:{threshold_minutes}",
                        status.rule_id
                    ),
                    now_utc,
                    ChronoDuration::days(2),
                    ChronoDuration::minutes(ALLOWANCE_NOTIFICATION_TTL_MINUTES),
                )?;
                if inserted.is_some() {
                    core.database().record_event(
                        "allowance_warning",
                        Some(&status.rule_id),
                        Some(&format!(
                            "threshold_minutes={threshold_minutes};remaining_seconds={}",
                            status.remaining_seconds
                        )),
                        now_utc,
                    )?;
                }
            }
        }

        core.database().set_allowance_notification_state(
            &status.rule_id,
            &local_day,
            status.remaining_seconds,
            now_utc,
        )?;
    }
    Ok(())
}

fn allowance_notification_body(rule_name: &str, remaining_seconds: i64) -> String {
    if remaining_seconds < 60 {
        format!("Less than 1 minute remains for \"{rule_name}\".")
    } else {
        let remaining_minutes = (remaining_seconds + 59) / 60;
        format!("{remaining_minutes} minutes remain for \"{rule_name}\".")
    }
}

fn blocked_rule_id(reason: &BlockReason) -> Option<&str> {
    match reason {
        BlockReason::Detox { rule_id, .. }
        | BlockReason::HardBlock { rule_id, .. }
        | BlockReason::ScheduledBlock { rule_id, .. }
        | BlockReason::ControlledAccess { rule_id, .. } => Some(rule_id.as_str()),
        BlockReason::InvalidUrl { .. } | BlockReason::RuntimeError { .. } => None,
    }
}

pub(crate) fn hosts_detox_sessions_for_clock(
    core: &FocusCore,
    now: DateTime<Utc>,
    clock_tampered: bool,
) -> Result<Vec<DetoxSession>> {
    if clock_tampered {
        Ok(core.database().uncancelled_detox_sessions()?)
    } else {
        Ok(core.database().active_detox_sessions(now)?)
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone, Utc};
    use focus_core::{
        BlockReason, Config, ControlledBlockReason, Database, Decision, ProcessIdentity,
        StrictModeConfig,
    };

    use super::{
        ensure_mandatory_app_rules, is_blockuntu_process, load_startup_policy,
        next_tier2_site_schedule_boundary, strict_browser_kill_details,
        sync_allowance_notifications, sync_metered_app_usage_sessions, sync_schedule_notifications,
        UNSUPPORTED_BROWSER_RULE_ID,
    };
    use crate::error::DaemonError;
    use crate::policy_recovery::PolicyRecoveryManager;
    use crate::process_scan::{supported_browser_for_process, ProcessInfo, SupportedBrowser};

    #[test]
    fn tier2_hosts_repair_wakes_at_the_next_schedule_boundary() {
        let config = Config::from_toml_str(
            r#"
            [[schedules]]
            id = "work"

            [[schedules.windows]]
            weekday = "mon"
            start = "09:00"
            end = "17:00"

            [[rules]]
            id = "strict"
            name = "Strict"
            tier = "scheduled_block"
            schedule_ids = ["work"]
            patterns = [{ kind = "domain", value = "strict.example" }]
            "#,
        )
        .expect("config should parse");
        let now = chrono::DateTime::parse_from_rfc3339("2026-05-18T08:59:00+00:00")
            .expect("timestamp should parse");

        assert_eq!(
            next_tier2_site_schedule_boundary(&config, now),
            Some(std::time::Duration::from_secs(60))
        );
    }

    #[test]
    fn schedule_notifications_fire_once_for_each_clock_boundary() {
        let config = Config::from_toml_str(
            r#"
            [[schedules]]
            id = "work"
            name = "Work"

            [[schedules.windows]]
            weekday = "mon"
            start = "09:00"
            end = "10:00"
            "#,
        )
        .expect("config should parse");
        let database = Database::in_memory().expect("database should initialize");
        let core = focus_core::FocusCore::new(config, database).expect("core should initialize");
        let before =
            chrono::DateTime::parse_from_rfc3339("2026-05-18T08:59:00+00:00").expect("timestamp");
        let started =
            chrono::DateTime::parse_from_rfc3339("2026-05-18T09:00:00+00:00").expect("timestamp");
        let ended =
            chrono::DateTime::parse_from_rfc3339("2026-05-18T10:00:00+00:00").expect("timestamp");

        sync_schedule_notifications(&core, before).expect("baseline should sync");
        assert!(core
            .database()
            .pending_notifications(before.with_timezone(&Utc), 20)
            .expect("pending notifications")
            .is_empty());

        sync_schedule_notifications(&core, started).expect("start should sync");
        let pending = core
            .database()
            .pending_notifications(started.with_timezone(&Utc), 20)
            .expect("start notification");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].kind, "schedule_started");
        core.database()
            .acknowledge_notifications(&[pending[0].id], started.with_timezone(&Utc))
            .expect("start should acknowledge");

        sync_schedule_notifications(&core, started + Duration::minutes(1))
            .expect("unchanged state should sync");
        assert!(core
            .database()
            .pending_notifications((started + Duration::minutes(1)).with_timezone(&Utc), 20)
            .expect("no duplicate")
            .is_empty());

        sync_schedule_notifications(&core, ended).expect("end should sync");
        let pending = core
            .database()
            .pending_notifications(ended.with_timezone(&Utc), 20)
            .expect("end notification");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].kind, "schedule_ended");
    }

    #[test]
    fn allowance_notifications_fire_only_when_thresholds_are_crossed() {
        let config = Config::from_toml_str(
            r#"
            [[allowances]]
            id = "app-daily"
            daily_minutes = 6

            [[app_rules]]
            id = "app-controlled"
            name = "Example App"
            tier = "controlled_access"
            allowance_id = "app-daily"
            matchers = [{ kind = "command_name", value = "example" }]
            "#,
        )
        .expect("config should parse");
        let database = Database::in_memory().expect("database should initialize");
        let core = focus_core::FocusCore::new(config, database).expect("core should initialize");
        let baseline =
            chrono::DateTime::parse_from_rfc3339("2026-05-18T09:00:00+00:00").expect("timestamp");
        sync_allowance_notifications(&core, baseline).expect("baseline should sync");

        core.database()
            .insert_app_usage_interval(
                "app-controlled",
                baseline.with_timezone(&Utc),
                (baseline + Duration::seconds(61)).with_timezone(&Utc),
            )
            .expect("usage should insert");
        let below_five = baseline + Duration::minutes(2);
        sync_allowance_notifications(&core, below_five).expect("five-minute crossing should sync");
        let pending = core
            .database()
            .pending_notifications(below_five.with_timezone(&Utc), 20)
            .expect("five-minute notification");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].kind, "allowance_warning");
        core.database()
            .acknowledge_notifications(&[pending[0].id], below_five.with_timezone(&Utc))
            .expect("warning should acknowledge");

        sync_allowance_notifications(&core, below_five + Duration::seconds(5))
            .expect("unchanged usage should sync");
        assert!(core
            .database()
            .pending_notifications((below_five + Duration::seconds(5)).with_timezone(&Utc), 20)
            .expect("no duplicate")
            .is_empty());

        core.database()
            .insert_app_usage_interval(
                "app-controlled",
                (baseline + Duration::minutes(2)).with_timezone(&Utc),
                (baseline + Duration::minutes(6)).with_timezone(&Utc),
            )
            .expect("more usage should insert");
        let below_one = baseline + Duration::minutes(7);
        sync_allowance_notifications(&core, below_one).expect("one-minute crossing should sync");
        let pending = core
            .database()
            .pending_notifications(below_one.with_timezone(&Utc), 20)
            .expect("one-minute notification");
        assert_eq!(pending.len(), 1);
        assert!(pending[0].body.contains("Less than 1 minute"));
    }

    #[test]
    fn restores_policy_snapshot_when_database_has_no_policy() {
        let temp = tempfile::tempdir().expect("tempdir should exist");
        let config_path = temp.path().join("config.toml");
        std::fs::write(&config_path, "").expect("baseline config should write");
        let recovery = PolicyRecoveryManager::new(temp.path().join("policy-recovery.toml"), false);
        let expected = Config::from_toml_str(
            r#"
            [[rules]]
            id = "recovered"
            name = "Recovered"
            tier = "hard"
            patterns = [
              { kind = "domain", value = "recovered.example", match_subdomains = true }
            ]
            "#,
        )
        .expect("recovery config should parse");
        recovery
            .write(&expected)
            .expect("recovery snapshot should write");
        let database =
            Database::open(temp.path().join("blockuntu.sqlite3")).expect("database should open");

        let (loaded, recovered) = load_startup_policy(&database, &config_path, &recovery, false)
            .expect("startup policy should restore");

        assert!(recovered);
        assert_eq!(loaded, expected);
        assert_eq!(
            database
                .load_policy_config()
                .expect("restored database policy should load"),
            expected
        );
    }

    #[test]
    fn existing_empty_database_without_recovery_fails_closed() {
        let temp = tempfile::tempdir().expect("tempdir should exist");
        let config_path = temp.path().join("config.toml");
        std::fs::write(&config_path, "").expect("baseline config should write");
        let recovery = PolicyRecoveryManager::new(temp.path().join("policy-recovery.toml"), false);
        let database =
            Database::open(temp.path().join("blockuntu.sqlite3")).expect("database should open");

        let error = load_startup_policy(&database, &config_path, &recovery, true)
            .expect_err("existing empty database should fail closed");

        assert!(matches!(error, DaemonError::MissingPolicyRecovery { .. }));
    }

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

    #[test]
    fn supported_browser_detection_is_limited_to_managed_browsers() {
        assert_eq!(
            supported_browser_for_process(&process("firefox")),
            Some(SupportedBrowser::Firefox)
        );
        assert_eq!(
            supported_browser_for_process(&process("google-chrome")),
            Some(SupportedBrowser::Chrome)
        );
        assert_eq!(supported_browser_for_process(&process("chromium")), None);
    }

    #[test]
    fn blockuntu_processes_are_protected_from_app_blocking() {
        assert!(is_blockuntu_process(&process("blockuntu-gui")));
        assert!(is_blockuntu_process(&process("blockuntud")));
        assert!(is_blockuntu_process(&process("blockuntu-native")));
        assert!(!is_blockuntu_process(&process("vlc")));
    }

    #[test]
    fn process_scan_sync_records_metered_app_usage() {
        let config = Config::from_toml_str(
            r#"
            [[allowances]]
            id = "kmines-daily"
            daily_minutes = 1

            [[app_rules]]
            id = "kmines-controlled"
            name = "KMines"
            tier = "controlled_access"
            allowance_id = "kmines-daily"
            schedule_ids = ["always"]
            matchers = [
              { kind = "command_name", value = "kmines" }
            ]

            [[schedules]]
            id = "always"

            [[schedules.windows]]
            weekday = "everyday"
            start = "00:00"
            end = "23:59"
            "#,
        )
        .expect("config should parse");
        let database = Database::in_memory().expect("database should initialize");
        let core = focus_core::FocusCore::new(config, database).expect("core should initialize");
        let process = process_info(1234, "kmines");
        let started_at = Utc
            .with_ymd_and_hms(2026, 5, 28, 10, 0, 0)
            .single()
            .expect("timestamp should be valid")
            .fixed_offset();
        let exhausted_at = started_at + Duration::minutes(2);

        let started_rules =
            sync_metered_app_usage_sessions(&core, std::slice::from_ref(&process), started_at)
                .expect("usage sync should start a session");
        assert_eq!(started_rules, vec!["kmines-controlled".to_string()]);

        sync_metered_app_usage_sessions(&core, std::slice::from_ref(&process), exhausted_at)
            .expect("usage sync should update the session");
        let context =
            focus_core::EvaluationContext::new(core.config(), core.database(), exhausted_at);
        assert_eq!(
            focus_core::evaluate_app(&process.identity(), &context),
            Decision::Block(BlockReason::ControlledAccess {
                rule_id: "kmines-controlled".to_string(),
                rule_name: "KMines".to_string(),
                reason: ControlledBlockReason::AllowanceExhausted,
            })
        );

        sync_metered_app_usage_sessions(&core, &[], exhausted_at + Duration::minutes(1))
            .expect("usage sync should end the session");
    }

    #[test]
    fn process_scan_sync_ignores_blockuntu_processes() {
        let config = Config::from_toml_str(
            r#"
            [[allowances]]
            id = "gui-daily"
            daily_minutes = 1

            [[app_rules]]
            id = "gui-controlled"
            name = "GUI"
            tier = "controlled_access"
            allowance_id = "gui-daily"
            schedule_ids = ["always"]
            matchers = [
              { kind = "command_name", value = "blockuntu-gui" }
            ]

            [[schedules]]
            id = "always"

            [[schedules.windows]]
            weekday = "everyday"
            start = "00:00"
            end = "23:59"
            "#,
        )
        .expect("config should parse");
        let database = Database::in_memory().expect("database should initialize");
        let core = focus_core::FocusCore::new(config, database).expect("core should initialize");

        let rule_ids = sync_metered_app_usage_sessions(
            &core,
            &[process_info(1234, "blockuntu-gui")],
            Utc.with_ymd_and_hms(2026, 5, 28, 10, 0, 0)
                .single()
                .expect("timestamp should be valid")
                .fixed_offset(),
        )
        .expect("usage sync should pass");

        assert!(rule_ids.is_empty());
    }

    #[test]
    fn strict_mode_kills_supported_browser_after_startup_heartbeat_grace() {
        let database = Database::in_memory().expect("database should initialize");
        let strict_mode = StrictModeConfig::default();
        let first_seen = Utc
            .with_ymd_and_hms(2026, 5, 28, 10, 0, 0)
            .single()
            .expect("timestamp should be valid");
        let processes = vec![process_info(1234, "firefox")];

        let first =
            strict_browser_kill_details(&processes, strict_mode.clone(), &database, first_seen)
                .expect("strict check should pass");
        assert!(first.is_empty());

        let after_grace = strict_browser_kill_details(
            &processes,
            strict_mode,
            &database,
            first_seen + Duration::seconds(61),
        )
        .expect("strict check should pass");
        assert!(after_grace
            .get(&1234)
            .expect("firefox should be selected")
            .contains("heartbeat_missing_since_launch_seconds=61"));
    }

    #[test]
    fn strict_mode_does_not_kill_new_browser_for_a_stale_previous_session_heartbeat() {
        let database = Database::in_memory().expect("database should initialize");
        let strict_mode = StrictModeConfig::default();
        let started_at = Utc
            .with_ymd_and_hms(2026, 5, 28, 10, 0, 0)
            .single()
            .expect("timestamp should be valid");
        database
            .upsert_heartbeat(
                "firefox_extension",
                Some("{}"),
                started_at - Duration::hours(1),
            )
            .expect("heartbeat should write");

        let processes = [process_info(1234, "firefox")];
        let during_startup =
            strict_browser_kill_details(&processes, strict_mode.clone(), &database, started_at)
                .expect("strict check should pass");
        assert!(during_startup.is_empty());

        let after_startup_grace = strict_browser_kill_details(
            &processes,
            strict_mode,
            &database,
            started_at + Duration::seconds(61),
        )
        .expect("strict check should pass");
        assert!(after_startup_grace
            .get(&1234)
            .expect("firefox should be selected")
            .contains("heartbeat_missing_since_launch_seconds=61"));
    }

    #[test]
    fn strict_mode_starts_a_new_heartbeat_grace_after_browser_closes_and_reopens() {
        let database = Database::in_memory().expect("database should initialize");
        let strict_mode = StrictModeConfig::default();
        let first_started_at = Utc
            .with_ymd_and_hms(2026, 5, 28, 10, 0, 0)
            .single()
            .expect("timestamp should be valid");
        let processes = [process_info(1234, "firefox")];

        strict_browser_kill_details(&processes, strict_mode.clone(), &database, first_started_at)
            .expect("initial strict check should pass");
        database
            .upsert_heartbeat(
                "firefox_extension",
                Some("{}"),
                first_started_at + Duration::seconds(5),
            )
            .expect("heartbeat should write");

        strict_browser_kill_details(
            &[],
            strict_mode.clone(),
            &database,
            first_started_at + Duration::seconds(10),
        )
        .expect("browser close should reset the session state");

        let reopened_at = first_started_at + Duration::seconds(20);
        let reopened = strict_browser_kill_details(&processes, strict_mode, &database, reopened_at)
            .expect("reopened browser should receive startup grace");
        assert!(reopened.is_empty());
        assert_eq!(
            database
                .service_state("strict_mode.firefox_browser_session_started_at")
                .expect("session start state should read"),
            Some(reopened_at.to_rfc3339())
        );
    }

    #[test]
    fn strict_mode_keeps_supported_browser_when_extension_heartbeat_is_from_current_session() {
        let database = Database::in_memory().expect("database should initialize");
        let strict_mode = StrictModeConfig::default();
        let started_at = Utc
            .with_ymd_and_hms(2026, 5, 28, 10, 0, 0)
            .single()
            .expect("timestamp should be valid");
        let processes = [process_info(1234, "firefox")];

        strict_browser_kill_details(&processes, strict_mode.clone(), &database, started_at)
            .expect("initial strict check should pass");
        database
            .upsert_heartbeat(
                "firefox_extension",
                Some("{}"),
                started_at + Duration::seconds(20),
            )
            .expect("heartbeat should write");

        let result = strict_browser_kill_details(
            &processes,
            strict_mode,
            &database,
            started_at + Duration::seconds(30),
        )
        .expect("strict check should pass");
        assert!(result.is_empty());
    }

    #[test]
    fn strict_mode_still_kills_browser_when_current_session_heartbeat_becomes_stale() {
        let database = Database::in_memory().expect("database should initialize");
        let strict_mode = StrictModeConfig::default();
        let started_at = Utc
            .with_ymd_and_hms(2026, 5, 28, 10, 0, 0)
            .single()
            .expect("timestamp should be valid");
        let processes = [process_info(1234, "firefox")];

        strict_browser_kill_details(&processes, strict_mode.clone(), &database, started_at)
            .expect("initial strict check should pass");
        database
            .upsert_heartbeat(
                "firefox_extension",
                Some("{}"),
                started_at + Duration::seconds(5),
            )
            .expect("heartbeat should write");

        let result = strict_browser_kill_details(
            &processes,
            strict_mode,
            &database,
            started_at + Duration::seconds(36),
        )
        .expect("strict check should pass");
        assert!(result
            .get(&1234)
            .expect("firefox should be selected")
            .contains("heartbeat_age_seconds=31"));
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

    fn process_info(pid: u32, name: &str) -> ProcessInfo {
        ProcessInfo {
            pid,
            executable_path: None,
            executable_basename: Some(name.to_string()),
            command_name: Some(name.to_string()),
            desktop_id: None,
            window_titles: Vec::new(),
        }
    }
}
