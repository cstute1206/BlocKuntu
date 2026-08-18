use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::time::Instant;

use chrono::{
    DateTime, Datelike, Days, Duration as ChronoDuration, FixedOffset, Local, TimeZone, Utc,
};
use focus_core::{
    allowance_statuses, schedule_ids_are_active_at, AppMatcherConfig, AppMatcherKind,
    AppRuleConfig, BlockReason, Database, Decision, DetoxSession, EvaluationContext, FocusCore,
    ProcessIdentity, RuleTier, StrictModeConfig, Weekday, EVENT_DETAIL_RETENTION_DAYS,
};
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UnixListener, UnixStream};

use crate::chrome_policy::{ChromePolicyManager, ChromePolicyRepairStatus};
use crate::cli::{Args, DEFAULT_HOSTS_PATH, DEFAULT_POLICY_RECOVERY_PATH};
use crate::clock_guard;
use crate::error::{DaemonError, Result};
use crate::firefox_policy::{FirefoxPolicyManager, RepairStatus};
use crate::hosts::{HostsManager, HostsRepairStatus};
use crate::policy_recovery::PolicyRecoveryManager;
use crate::process_scan::{
    attach_detected_window_titles, kill_processes, scan_procfs, supported_browser_for_process,
    unsupported_browser_installation_for_process, LinuxSignalKiller, ProcessInfo, SupportedBrowser,
};
use crate::rpc::{
    current_chromium_incognito_policy_settings,
    ensure_chromium_incognito_url_blocklist_within_limit, handle_payload, record_daemon_diagnostic,
    unsupported_browser_block_is_active, ChromiumIncognitoPolicySettings, ChromiumPolicyBinding,
    GeckoPolicyBinding, RpcContext,
};
use crate::socket::listener_from_systemd_or_path;

const MAX_PAYLOAD_BYTES: usize = 1024 * 1024;
const SNAP_NATIVE_BRIDGE_HEADER_PREFIX: &[u8] = b"BLOCKUNTU-SNAP-BRIDGE ";
const CHROMIUM_SNAP_POLICY_CURRENT_DIR: &str = "/var/snap/chromium/current";
const UNSUPPORTED_BROWSER_RULE_ID: &str = "unsupported-browsers-hard";
const STRICT_FIREFOX_MISSING_SINCE_KEY: &str = "strict_mode.firefox_missing_since";
const STRICT_LIBREWOLF_MISSING_SINCE_KEY: &str = "strict_mode.librewolf_missing_since";
const STRICT_WATERFOX_MISSING_SINCE_KEY: &str = "strict_mode.waterfox_missing_since";
const STRICT_CHROME_MISSING_SINCE_KEY: &str = "strict_mode.chrome_missing_since";
const STRICT_CHROMIUM_MISSING_SINCE_KEY: &str = "strict_mode.chromium_missing_since";
const STRICT_BRAVE_MISSING_SINCE_KEY: &str = "strict_mode.brave_missing_since";
const STRICT_OPERA_MISSING_SINCE_KEY: &str = "strict_mode.opera_missing_since";
const STRICT_EDGE_MISSING_SINCE_KEY: &str = "strict_mode.edge_missing_since";
const STRICT_VIVALDI_MISSING_SINCE_KEY: &str = "strict_mode.vivaldi_missing_since";
const STRICT_FIREFOX_BROWSER_SESSION_STARTED_AT_KEY: &str =
    "strict_mode.firefox_browser_session_started_at";
const STRICT_LIBREWOLF_BROWSER_SESSION_STARTED_AT_KEY: &str =
    "strict_mode.librewolf_browser_session_started_at";
const STRICT_WATERFOX_BROWSER_SESSION_STARTED_AT_KEY: &str =
    "strict_mode.waterfox_browser_session_started_at";
const STRICT_CHROME_BROWSER_SESSION_STARTED_AT_KEY: &str =
    "strict_mode.chrome_browser_session_started_at";
const STRICT_CHROMIUM_BROWSER_SESSION_STARTED_AT_KEY: &str =
    "strict_mode.chromium_browser_session_started_at";
const STRICT_BRAVE_BROWSER_SESSION_STARTED_AT_KEY: &str =
    "strict_mode.brave_browser_session_started_at";
const STRICT_OPERA_BROWSER_SESSION_STARTED_AT_KEY: &str =
    "strict_mode.opera_browser_session_started_at";
const STRICT_EDGE_BROWSER_SESSION_STARTED_AT_KEY: &str =
    "strict_mode.edge_browser_session_started_at";
const STRICT_VIVALDI_BROWSER_SESSION_STARTED_AT_KEY: &str =
    "strict_mode.vivaldi_browser_session_started_at";
const MIN_BROWSER_STARTUP_HEARTBEAT_GRACE_SECONDS: i64 = 60;
const NOTIFICATION_STATE_INTERVAL_SECONDS: u64 = 5;
const ALLOWANCE_NOTIFICATION_TTL_MINUTES: i64 = 5;
const LIFECYCLE_NOTIFICATION_TTL_MINUTES: i64 = 10;
const SLOW_RPC_THRESHOLD_MS: u128 = 500;
const EVENT_RETENTION_MAINTENANCE_INTERVAL_SECONDS: u64 = 24 * 60 * 60;

#[derive(Clone)]
pub struct DaemonApp {
    core: Arc<Mutex<FocusCore>>,
    rpc_context: RpcContext,
    gecko_policies: Vec<GeckoPolicyBinding>,
    chromium_policies: Vec<ChromiumPolicyBinding>,
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
        let retention_clock = clock_guard::guarded_now(&database, None, false)?;
        if retention_clock.integrity.state != "tampered" {
            database.enforce_event_retention(
                retention_clock.now.with_timezone(&Utc),
                ChronoDuration::days(EVENT_DETAIL_RETENTION_DAYS),
            )?;
        }
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
        let gecko_policies = vec![
            GeckoPolicyBinding::new(
                SupportedBrowser::Firefox,
                FirefoxPolicyManager::for_browser(
                    &args.firefox_policy,
                    &args.extension_id,
                    &args.firefox_extension_install_url,
                ),
            ),
            GeckoPolicyBinding::new(
                SupportedBrowser::LibreWolf,
                FirefoxPolicyManager::merging_existing_policy(
                    &args.librewolf_policy,
                    &args.librewolf_extension_id,
                    &args.librewolf_extension_install_url,
                    &args.librewolf_policy_backup,
                ),
            ),
            GeckoPolicyBinding::new(
                SupportedBrowser::Waterfox,
                FirefoxPolicyManager::merging_existing_policy(
                    &args.waterfox_policy,
                    &args.waterfox_extension_id,
                    &args.waterfox_extension_install_url,
                    &args.waterfox_policy_backup,
                ),
            ),
        ];
        let chromium_policies = vec![
            ChromiumPolicyBinding::new(
                SupportedBrowser::Chrome,
                ChromePolicyManager::for_browser(
                    &args.chrome_policy,
                    &args.chrome_extension_id,
                    "Chrome",
                ),
            ),
            ChromiumPolicyBinding::new(
                SupportedBrowser::Chromium,
                ChromePolicyManager::for_browser(
                    &args.chromium_policy,
                    &args.chromium_extension_id,
                    "Chromium",
                )
                .with_snap_policy_current_dir(CHROMIUM_SNAP_POLICY_CURRENT_DIR),
            ),
            ChromiumPolicyBinding::new(
                SupportedBrowser::Brave,
                ChromePolicyManager::for_browser(
                    &args.brave_policy,
                    &args.brave_extension_id,
                    "Brave",
                ),
            ),
            ChromiumPolicyBinding::new(
                SupportedBrowser::Opera,
                ChromePolicyManager::for_browser(
                    &args.opera_policy,
                    &args.opera_extension_id,
                    "Opera",
                ),
            ),
            ChromiumPolicyBinding::new(
                SupportedBrowser::Edge,
                ChromePolicyManager::for_browser(
                    &args.edge_policy,
                    &args.edge_extension_id,
                    "Microsoft Edge",
                ),
            ),
            ChromiumPolicyBinding::new(
                SupportedBrowser::Vivaldi,
                ChromePolicyManager::for_browser(
                    &args.vivaldi_policy,
                    &args.vivaldi_extension_id,
                    "Vivaldi",
                ),
            ),
        ];
        let hosts = HostsManager::new_with_immutable(&args.hosts, hosts_immutable_enabled(args));
        let rpc_context = rpc_context
            .with_enforcement_managers(
                gecko_policies.clone(),
                chromium_policies.clone(),
                hosts.clone(),
            )
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
            gecko_policies,
            chromium_policies,
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
        self.repair_gecko_policy(SupportedBrowser::Firefox)
    }

    fn repair_gecko_policy(&self, browser: SupportedBrowser) -> Result<RepairStatus> {
        debug_assert!(browser.is_firefox_based());
        if !self.manage_firefox_policy {
            return Ok(RepairStatus::SkippedDisabled);
        }
        if self.defer_firefox_policy_repair_until_heartbeat
            && !self.has_extension_heartbeat(browser.extension_component())?
        {
            return Ok(RepairStatus::SkippedDeferred);
        }
        if !self.enforcement_is_active()? {
            return Ok(RepairStatus::SkippedInactive);
        }
        self.gecko_policies
            .iter()
            .find(|binding| binding.browser() == browser)
            .ok_or_else(|| {
                DaemonError::InvalidRequest(format!(
                    "{} policy manager is not configured",
                    browser.label()
                ))
            })?
            .policy()
            .verify_and_repair()
    }

    fn repair_gecko_policies(&self) -> Result<Vec<(SupportedBrowser, RepairStatus)>> {
        SupportedBrowser::MANAGED
            .into_iter()
            .filter(|browser| browser.is_firefox_based())
            .map(|browser| Ok((browser, self.repair_gecko_policy(browser)?)))
            .collect()
    }

    pub fn repair_chrome_policy(&self) -> Result<ChromePolicyRepairStatus> {
        self.repair_chromium_policy(SupportedBrowser::Chrome)
    }

    fn repair_chromium_policy(
        &self,
        browser: SupportedBrowser,
    ) -> Result<ChromePolicyRepairStatus> {
        let settings = self.chromium_incognito_policy_settings()?;
        self.repair_chromium_policy_with_settings(browser, &settings)
    }

    fn repair_chromium_policy_with_settings(
        &self,
        browser: SupportedBrowser,
        settings: &ChromiumIncognitoPolicySettings,
    ) -> Result<ChromePolicyRepairStatus> {
        debug_assert!(browser.is_chromium_based());
        if !self.manage_chrome_policy {
            return Ok(ChromePolicyRepairStatus::SkippedDisabled);
        }
        if self.defer_chrome_policy_repair_until_heartbeat
            && !self.has_extension_heartbeat(browser.extension_component())?
        {
            return Ok(ChromePolicyRepairStatus::SkippedDeferred);
        }
        if !self.enforcement_is_active()? {
            return Ok(ChromePolicyRepairStatus::SkippedInactive);
        }
        ensure_chromium_incognito_url_blocklist_within_limit(settings)?;
        self.chromium_policies
            .iter()
            .find(|binding| binding.browser() == browser)
            .ok_or_else(|| {
                DaemonError::InvalidRequest(format!(
                    "{} policy manager is not configured",
                    browser.label()
                ))
            })?
            .policy()
            .verify_and_repair_with(settings.mode, &settings.url_blocklist)
    }

    fn repair_chromium_policies(
        &self,
    ) -> Result<Vec<(SupportedBrowser, ChromePolicyRepairStatus)>> {
        let settings = self.chromium_incognito_policy_settings()?;
        SupportedBrowser::MANAGED
            .into_iter()
            .filter(|browser| browser.is_chromium_based())
            .map(|browser| {
                Ok((
                    browser,
                    self.repair_chromium_policy_with_settings(browser, &settings)?,
                ))
            })
            .collect()
    }

    fn chromium_incognito_policy_settings(&self) -> Result<ChromiumIncognitoPolicySettings> {
        let core = self.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
        let guarded = clock_guard::guarded_now(core.database(), None, false)?;
        current_chromium_incognito_policy_settings(
            &core,
            guarded.now,
            guarded.integrity.state == "tampered",
        )
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
        self.repair_gecko_policies()?;
        self.repair_chromium_policies()?;
        self.repair_hosts()?;
        self.spawn_repair_loop();
        self.spawn_process_scan_loop();
        self.spawn_notification_loop();
        self.spawn_event_retention_loop();

        if args.snap_native_bridge {
            let bridge_token = read_snap_native_bridge_token(&args.snap_native_bridge_token_file)?;
            let bridge_listener = TcpListener::bind(args.snap_native_bridge_address).await?;
            let bridge_address = bridge_listener.local_addr()?;
            eprintln!("BlocKuntu Snap native bridge listening on {bridge_address}");
            let bridge_app = self.clone();
            tokio::spawn(async move {
                bridge_app
                    .accept_snap_native_bridge_loop(bridge_listener, bridge_token)
                    .await;
            });
        }

        let listener = listener_from_systemd_or_path(&args.socket, args.dev_bind_socket)?;
        self.accept_loop(listener).await
    }

    fn spawn_repair_loop(&self) {
        let app = self.clone();
        tokio::spawn(async move {
            loop {
                if let Err(err) = app.repair_gecko_policies() {
                    let message = format!("Firefox-family policy repair failed: {err}");
                    eprintln!("{message}");
                    record_daemon_diagnostic(
                        &app.rpc_context,
                        "error",
                        "policy_repair_failed",
                        &message,
                    );
                }
                if let Err(err) = app.repair_chromium_policies() {
                    let message = format!("Chromium-family policy repair failed: {err}");
                    eprintln!("{message}");
                    record_daemon_diagnostic(
                        &app.rpc_context,
                        "error",
                        "policy_repair_failed",
                        &message,
                    );
                }
                if let Err(err) = app.repair_hosts() {
                    let message = format!("hosts repair failed: {err}");
                    eprintln!("{message}");
                    record_daemon_diagnostic(
                        &app.rpc_context,
                        "error",
                        "hosts_repair_failed",
                        &message,
                    );
                }
                tokio::time::sleep(app.next_policy_repair_delay()).await;
            }
        });
    }

    fn spawn_event_retention_loop(&self) {
        let app = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(
                    EVENT_RETENTION_MAINTENANCE_INTERVAL_SECONDS,
                ))
                .await;
                let result = app
                    .core
                    .lock()
                    .map_err(|_| DaemonError::LockPoisoned)
                    .and_then(|core| {
                        let guarded = clock_guard::guarded_now(core.database(), None, false)?;
                        if guarded.integrity.state == "tampered" {
                            return Ok(());
                        }
                        core.database()
                            .enforce_event_retention(
                                guarded.now.with_timezone(&Utc),
                                ChronoDuration::days(EVENT_DETAIL_RETENTION_DAYS),
                            )
                            .map(|_| ())
                            .map_err(DaemonError::from)
                    });
                if let Err(error) = result {
                    let message = format!("event retention maintenance failed: {error}");
                    eprintln!("{message}");
                    record_daemon_diagnostic(
                        &app.rpc_context,
                        "error",
                        "event_retention_failed",
                        &message,
                    );
                }
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

        let schedule_delay = next_tier2_site_schedule_boundary(core.config(), guarded.now)
            .map(|delay| delay.min(self.policy_repair_interval))
            .unwrap_or(self.policy_repair_interval);
        let detox_delay = core
            .database()
            .active_detox_sessions(guarded.now.with_timezone(&Utc))
            .ok()
            .and_then(|sessions| {
                sessions
                    .into_iter()
                    .filter_map(|session| {
                        (session.ends_at > guarded.now.with_timezone(&Utc))
                            .then(|| {
                                (session.ends_at - guarded.now.with_timezone(&Utc))
                                    .to_std()
                                    .ok()
                            })
                            .flatten()
                    })
                    .min()
            });

        detox_delay
            .map(|delay| delay.min(schedule_delay))
            .unwrap_or(schedule_delay)
    }

    fn spawn_process_scan_loop(&self) {
        let app = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(app.process_scan_interval);
            loop {
                interval.tick().await;
                if let Err(err) = app.scan_processes_once() {
                    let message = format!("process scan failed: {err}");
                    eprintln!("{message}");
                    record_daemon_diagnostic(
                        &app.rpc_context,
                        "error",
                        "process_scan_failed",
                        &message,
                    );
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
                    let message = format!("notification state sync failed: {err}");
                    eprintln!("{message}");
                    record_daemon_diagnostic(
                        &app.rpc_context,
                        "error",
                        "notification_sync_failed",
                        &message,
                    );
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
                && !core.config().strict_mode.block_unsupported_browsers
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
            let unsupported_browser_block_active =
                unsupported_browser_block_is_active(&core, now, clock_tampered)?;
            let config_without_inactive_browser_block =
                (!unsupported_browser_block_active).then(|| {
                    let mut config = core.config().clone();
                    config
                        .app_rules
                        .retain(|rule| rule.id != UNSUPPORTED_BROWSER_RULE_ID);
                    config
                });
            let evaluation_config = config_without_inactive_browser_block
                .as_ref()
                .unwrap_or_else(|| core.config());
            let context = EvaluationContext::new(evaluation_config, core.database(), now)
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

            if unsupported_browser_block_active {
                for process in &processes {
                    if process.pid <= 1
                        || process.pid == std::process::id()
                        || is_blockuntu_process(&process.identity())
                    {
                        continue;
                    }
                    let Some(installation) =
                        unsupported_browser_installation_for_process(&process.identity())
                    else {
                        continue;
                    };

                    if !blocked_pids.contains(&process.pid) {
                        blocked_pids.push(process.pid);
                    }
                    kill_details_by_pid.insert(
                        process.pid,
                        format!(
                            "rule_id={UNSUPPORTED_BROWSER_RULE_ID};browser_installation={installation}"
                        ),
                    );
                    kill_event_kind_by_pid
                        .insert(process.pid, "browser_killed_unsupported_installation");
                    blocked_rule_id_by_pid
                        .insert(process.pid, UNSUPPORTED_BROWSER_RULE_ID.to_string());
                }
            }

            let strict_mode = core.config().strict_mode.clone();
            let mut deferred_browsers = HashSet::new();
            if self.defer_firefox_policy_repair_until_heartbeat {
                for browser in SupportedBrowser::MANAGED
                    .into_iter()
                    .filter(|browser| browser.is_firefox_based())
                {
                    if !self.has_extension_heartbeat_locked(&core, browser.extension_component())? {
                        deferred_browsers.insert(browser);
                    }
                }
            }
            if self.defer_chrome_policy_repair_until_heartbeat {
                for browser in SupportedBrowser::MANAGED
                    .into_iter()
                    .filter(|browser| browser.is_chromium_based())
                {
                    if !self.has_extension_heartbeat_locked(&core, browser.extension_component())? {
                        deferred_browsers.insert(browser);
                    }
                }
            }

            for (pid, detail) in strict_browser_kill_details_except(
                &processes,
                strict_mode,
                core.database(),
                now.with_timezone(&Utc),
                &deferred_browsers,
            )? {
                if !blocked_pids.contains(&pid) {
                    blocked_pids.push(pid);
                    kill_details_by_pid.insert(pid, detail);
                    kill_event_kind_by_pid.insert(pid, "browser_killed_extension_stale");
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
                        let diagnostic_context = context.clone();
                        if let Err(err) = handle_client(stream, context).await {
                            let message = format!("client error: {err}");
                            eprintln!("{message}");
                            record_daemon_diagnostic(
                                &diagnostic_context,
                                "error",
                                "rpc_client_error",
                                &message,
                            );
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

    async fn accept_snap_native_bridge_loop(self, listener: TcpListener, token: String) {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let context = self.rpc_context.clone();
                    let token = token.clone();
                    tokio::spawn(async move {
                        let diagnostic_context = context.clone();
                        if let Err(err) =
                            handle_snap_native_bridge_client(stream, context, &token).await
                        {
                            let message = format!("Snap native bridge client error: {err}");
                            eprintln!("{message}");
                            record_daemon_diagnostic(
                                &diagnostic_context,
                                "error",
                                "snap_bridge_client_error",
                                &message,
                            );
                        }
                    });
                }
                Err(err) => {
                    let message = format!("Snap native bridge accept error: {err}");
                    eprintln!("{message}");
                    record_daemon_diagnostic(
                        &self.rpc_context,
                        "error",
                        "snap_bridge_accept_error",
                        &message,
                    );
                }
            }
        }
    }
}

fn next_tier2_site_schedule_boundary(
    config: &focus_core::Config,
    now: DateTime<FixedOffset>,
) -> Option<Duration> {
    let mut next: Option<DateTime<FixedOffset>> = None;

    // Chromium private-browsing disablement can be scoped to any active
    // schedule, including schedules used only by Controlled Access rules.
    for schedule in &config.schedules {
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
    let connection_started_at = Instant::now();
    let peer = unix_peer_label(&stream);
    let read_started_at = Instant::now();
    let request = read_limited(&mut stream).await?;
    let read_ms = read_started_at.elapsed().as_millis();
    let (method, request_id) = rpc_request_metadata(&request);
    let handler_started_at = Instant::now();
    let response = handle_payload(&context, &request);
    let handler_ms = handler_started_at.elapsed().as_millis();
    let write_started_at = Instant::now();
    if let Err(err) = stream.write_all(&response).await {
        let message = format!(
            "RPC response write failed transport=unix {peer} id={request_id} method={method} read_ms={read_ms} handler_ms={handler_ms} write_ms={} request_bytes={} response_bytes={} error={err}",
            write_started_at.elapsed().as_millis(),
            request.len(),
            response.len()
        );
        eprintln!("{message}");
        record_daemon_diagnostic(&context, "error", "rpc_response_write_failed", &message);
        return Err(err.into());
    }
    let write_ms = write_started_at.elapsed().as_millis();
    if let Err(err) = stream.shutdown().await {
        let message = format!(
            "RPC shutdown failed transport=unix {peer} id={request_id} method={method} read_ms={read_ms} handler_ms={handler_ms} write_ms={write_ms} error={err}"
        );
        eprintln!("{message}");
        record_daemon_diagnostic(&context, "error", "rpc_shutdown_failed", &message);
        return Err(err.into());
    }
    let total_ms = connection_started_at.elapsed().as_millis();
    if total_ms >= SLOW_RPC_THRESHOLD_MS {
        let message = format!(
            "RPC slow transport=unix {peer} id={request_id} method={method} total_ms={total_ms} read_ms={read_ms} handler_ms={handler_ms} write_ms={write_ms} request_bytes={} response_bytes={}",
            request.len(),
            response.len()
        );
        eprintln!("{message}");
        if method != "record_diagnostics" {
            record_daemon_diagnostic(&context, "warn", "rpc_slow", &message);
        }
    }
    Ok(())
}

async fn handle_snap_native_bridge_client(
    mut stream: TcpStream,
    context: RpcContext,
    token: &str,
) -> Result<()> {
    let connection_started_at = Instant::now();
    let read_started_at = Instant::now();
    let request = read_limited(&mut stream).await?;
    let read_ms = read_started_at.elapsed().as_millis();
    let payload = snap_native_bridge_payload(&request, token)?;
    let (method, request_id) = rpc_request_metadata(payload);
    let handler_started_at = Instant::now();
    let response = handle_payload(&context, payload);
    let handler_ms = handler_started_at.elapsed().as_millis();
    let write_started_at = Instant::now();
    if let Err(err) = stream.write_all(&response).await {
        let message = format!(
            "RPC response write failed transport=snap_bridge id={request_id} method={method} read_ms={read_ms} handler_ms={handler_ms} write_ms={} request_bytes={} response_bytes={} error={err}",
            write_started_at.elapsed().as_millis(),
            payload.len(),
            response.len()
        );
        eprintln!("{message}");
        record_daemon_diagnostic(&context, "error", "rpc_response_write_failed", &message);
        return Err(err.into());
    }
    let write_ms = write_started_at.elapsed().as_millis();
    if let Err(err) = stream.shutdown().await {
        let message = format!(
            "RPC shutdown failed transport=snap_bridge id={request_id} method={method} read_ms={read_ms} handler_ms={handler_ms} write_ms={write_ms} error={err}"
        );
        eprintln!("{message}");
        record_daemon_diagnostic(&context, "error", "rpc_shutdown_failed", &message);
        return Err(err.into());
    }
    let total_ms = connection_started_at.elapsed().as_millis();
    if total_ms >= SLOW_RPC_THRESHOLD_MS {
        let message = format!(
            "RPC slow transport=snap_bridge id={request_id} method={method} total_ms={total_ms} read_ms={read_ms} handler_ms={handler_ms} write_ms={write_ms} request_bytes={} response_bytes={}",
            payload.len(),
            response.len()
        );
        eprintln!("{message}");
        if method != "record_diagnostics" {
            record_daemon_diagnostic(&context, "warn", "rpc_slow", &message);
        }
    }
    Ok(())
}

fn unix_peer_label(stream: &UnixStream) -> String {
    match stream.peer_cred() {
        Ok(credentials) => format!(
            "peer_pid={} peer_uid={} peer_gid={}",
            credentials
                .pid()
                .map(|pid| pid.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            credentials.uid(),
            credentials.gid()
        ),
        Err(_) => "peer_pid=unknown peer_uid=unknown peer_gid=unknown".to_string(),
    }
}

fn rpc_request_metadata(payload: &[u8]) -> (String, String) {
    let Ok(value) = serde_json::from_slice::<Value>(payload) else {
        return ("invalid_json".to_string(), "null".to_string());
    };
    let method = value
        .get("method")
        .and_then(Value::as_str)
        .or_else(|| value.get("type").and_then(Value::as_str))
        .or_else(|| value.get("url").map(|_| "legacy_evaluate_url"))
        .unwrap_or("unknown")
        .to_string();
    let request_id = value
        .get("id")
        .map(Value::to_string)
        .unwrap_or_else(|| "null".to_string());
    (method, request_id)
}

async fn read_limited<R>(stream: &mut R) -> Result<Vec<u8>>
where
    R: AsyncRead + Unpin,
{
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

fn read_snap_native_bridge_token(path: &Path) -> Result<String> {
    let token = fs::read_to_string(path)?.trim().to_string();
    if token.len() != 64 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(DaemonError::InvalidRequest(format!(
            "Snap native bridge token at {} must be exactly 64 hexadecimal characters",
            path.display()
        )));
    }
    Ok(token)
}

fn snap_native_bridge_payload<'a>(request: &'a [u8], expected_token: &str) -> Result<&'a [u8]> {
    let header_end = request
        .iter()
        .position(|byte| *byte == b'\n')
        .ok_or_else(|| {
            DaemonError::InvalidRequest("Snap native bridge request has no header".to_string())
        })?;
    let header = &request[..header_end];
    let supplied_token = header
        .strip_prefix(SNAP_NATIVE_BRIDGE_HEADER_PREFIX)
        .ok_or_else(|| {
            DaemonError::InvalidRequest(
                "Snap native bridge request has an invalid header".to_string(),
            )
        })?;

    if !constant_time_equals(supplied_token, expected_token.as_bytes()) {
        return Err(DaemonError::InvalidRequest(
            "Snap native bridge authentication failed".to_string(),
        ));
    }

    let payload = &request[header_end + 1..];
    if payload.is_empty() {
        return Err(DaemonError::InvalidRequest(
            "Snap native bridge request has no JSON-RPC payload".to_string(),
        ));
    }
    Ok(payload)
}

fn constant_time_equals(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
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

#[cfg(test)]
fn strict_browser_kill_details(
    processes: &[ProcessInfo],
    strict_mode: StrictModeConfig,
    database: &Database,
    now: DateTime<Utc>,
) -> Result<HashMap<u32, String>> {
    strict_browser_kill_details_except(processes, strict_mode, database, now, &HashSet::new())
}

fn strict_browser_kill_details_except(
    processes: &[ProcessInfo],
    strict_mode: StrictModeConfig,
    database: &Database,
    now: DateTime<Utc>,
    deferred_browsers: &HashSet<SupportedBrowser>,
) -> Result<HashMap<u32, String>> {
    let mut kill_details = HashMap::new();

    if !strict_mode.kill_supported_browser_if_extension_stale {
        return Ok(kill_details);
    }

    for browser in SupportedBrowser::MANAGED {
        if deferred_browsers.contains(&browser)
            || !browser_required_by_strict_mode(browser, &strict_mode)
        {
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
    let heartbeat = database.heartbeat(browser.extension_component())?;
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
            browser.extension_component(),
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
        browser.extension_component(),
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
    fn missing_since_key(self) -> &'static str {
        match self {
            Self::Firefox => STRICT_FIREFOX_MISSING_SINCE_KEY,
            Self::LibreWolf => STRICT_LIBREWOLF_MISSING_SINCE_KEY,
            Self::Waterfox => STRICT_WATERFOX_MISSING_SINCE_KEY,
            Self::Chrome => STRICT_CHROME_MISSING_SINCE_KEY,
            Self::Chromium => STRICT_CHROMIUM_MISSING_SINCE_KEY,
            Self::Brave => STRICT_BRAVE_MISSING_SINCE_KEY,
            Self::Opera => STRICT_OPERA_MISSING_SINCE_KEY,
            Self::Edge => STRICT_EDGE_MISSING_SINCE_KEY,
            Self::Vivaldi => STRICT_VIVALDI_MISSING_SINCE_KEY,
        }
    }

    fn session_started_at_key(self) -> &'static str {
        match self {
            Self::Firefox => STRICT_FIREFOX_BROWSER_SESSION_STARTED_AT_KEY,
            Self::LibreWolf => STRICT_LIBREWOLF_BROWSER_SESSION_STARTED_AT_KEY,
            Self::Waterfox => STRICT_WATERFOX_BROWSER_SESSION_STARTED_AT_KEY,
            Self::Chrome => STRICT_CHROME_BROWSER_SESSION_STARTED_AT_KEY,
            Self::Chromium => STRICT_CHROMIUM_BROWSER_SESSION_STARTED_AT_KEY,
            Self::Brave => STRICT_BRAVE_BROWSER_SESSION_STARTED_AT_KEY,
            Self::Opera => STRICT_OPERA_BROWSER_SESSION_STARTED_AT_KEY,
            Self::Edge => STRICT_EDGE_BROWSER_SESSION_STARTED_AT_KEY,
            Self::Vivaldi => STRICT_VIVALDI_BROWSER_SESSION_STARTED_AT_KEY,
        }
    }
}

fn browser_required_by_strict_mode(
    browser: SupportedBrowser,
    strict_mode: &StrictModeConfig,
) -> bool {
    match browser {
        SupportedBrowser::Firefox | SupportedBrowser::LibreWolf | SupportedBrowser::Waterfox => {
            strict_mode.require_firefox_extension
        }
        SupportedBrowser::Chrome
        | SupportedBrowser::Chromium
        | SupportedBrowser::Brave
        | SupportedBrowser::Opera
        | SupportedBrowser::Edge
        | SupportedBrowser::Vivaldi => strict_mode.require_chrome_extension,
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
        // Package-specific identities for browser variants that cannot load
        // BlocKuntu's managed policy. The runtime check also uses package
        // paths, so terminal starts without a desktop ID are covered.
        (DesktopId, "org.chromium.Chromium.desktop"),
        (DesktopId, "brave_brave.desktop"),
        (DesktopId, "opera_opera.desktop"),
        (DesktopId, "vivaldi_vivaldi-stable.desktop"),
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
        constant_time_equals, ensure_mandatory_app_rules, is_blockuntu_process,
        load_startup_policy, next_tier2_site_schedule_boundary, rpc_request_metadata,
        snap_native_bridge_payload, strict_browser_kill_details, sync_allowance_notifications,
        sync_metered_app_usage_sessions, sync_schedule_notifications, UNSUPPORTED_BROWSER_RULE_ID,
    };
    use crate::error::DaemonError;
    use crate::policy_recovery::PolicyRecoveryManager;
    use crate::process_scan::{supported_browser_for_process, ProcessInfo, SupportedBrowser};

    #[test]
    fn snap_native_bridge_requires_the_expected_token_and_payload() {
        let token = "a".repeat(64);
        let request = format!("BLOCKUNTU-SNAP-BRIDGE {token}\n{{\"jsonrpc\":\"2.0\"}}");

        assert_eq!(
            snap_native_bridge_payload(request.as_bytes(), &token).expect("token should match"),
            br#"{"jsonrpc":"2.0"}"#
        );
        assert!(snap_native_bridge_payload(b"BLOCKUNTU-SNAP-BRIDGE bad\n{}", &token).is_err());
        assert!(snap_native_bridge_payload(b"{}", &token).is_err());
    }

    #[test]
    fn snap_native_bridge_token_comparison_is_exact() {
        assert!(constant_time_equals(b"same", b"same"));
        assert!(!constant_time_equals(b"same", b"different"));
        assert!(!constant_time_equals(b"same", b"sane"));
    }

    #[test]
    fn rpc_diagnostics_extract_method_and_id_without_params() {
        let (method, id) = rpc_request_metadata(
            br#"{"jsonrpc":"2.0","id":23,"method":"evaluate_url","params":{"url":"https://private.example/"}}"#,
        );
        assert_eq!(method, "evaluate_url");
        assert_eq!(id, "23");
    }

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
    fn unsupported_browser_rule_blocks_unmanaged_browsers_but_not_supported_browsers() {
        let mut config = Config::default();
        ensure_mandatory_app_rules(&mut config);
        let database = focus_core::Database::in_memory().expect("database should initialize");
        let core = focus_core::FocusCore::new(config, database).expect("core should initialize");
        let now = chrono::Local::now().fixed_offset();
        let context = focus_core::EvaluationContext::new(core.config(), core.database(), now);

        let epiphany = process("epiphany");
        let firefox = process("firefox");
        let librewolf = process("librewolf");
        let waterfox = process("waterfox");
        let chrome = process("google-chrome");
        let chromium = process("chromium");
        let brave = process("brave-browser");
        let opera = process("opera");
        let edge = process("microsoft-edge");
        let vivaldi = process("vivaldi");
        let chromium_flatpak = ProcessIdentity {
            desktop_id: Some("org.chromium.Chromium.desktop".to_string()),
            ..process("chromium")
        };
        let brave_snap = ProcessIdentity {
            desktop_id: Some("brave_brave.desktop".to_string()),
            ..process("brave-browser")
        };
        let opera_snap = ProcessIdentity {
            desktop_id: Some("opera_opera.desktop".to_string()),
            ..process("opera")
        };
        let vivaldi_snap = ProcessIdentity {
            desktop_id: Some("vivaldi_vivaldi-stable.desktop".to_string()),
            ..process("vivaldi")
        };
        let chromium_snap = ProcessIdentity {
            desktop_id: Some("chromium_chromium.desktop".to_string()),
            ..process("chromium")
        };

        assert!(focus_core::evaluate_app(&epiphany, &context).is_block());
        assert!(!focus_core::evaluate_app(&firefox, &context).is_block());
        assert!(!focus_core::evaluate_app(&librewolf, &context).is_block());
        assert!(!focus_core::evaluate_app(&waterfox, &context).is_block());
        assert!(!focus_core::evaluate_app(&chrome, &context).is_block());
        assert!(!focus_core::evaluate_app(&chromium, &context).is_block());
        assert!(!focus_core::evaluate_app(&brave, &context).is_block());
        assert!(!focus_core::evaluate_app(&opera, &context).is_block());
        assert!(!focus_core::evaluate_app(&edge, &context).is_block());
        assert!(!focus_core::evaluate_app(&vivaldi, &context).is_block());
        assert!(focus_core::evaluate_app(&chromium_flatpak, &context).is_block());
        assert!(focus_core::evaluate_app(&brave_snap, &context).is_block());
        assert!(focus_core::evaluate_app(&opera_snap, &context).is_block());
        assert!(focus_core::evaluate_app(&vivaldi_snap, &context).is_block());
        assert!(!focus_core::evaluate_app(&chromium_snap, &context).is_block());
    }

    #[test]
    fn supported_browser_detection_covers_each_managed_browser() {
        assert_eq!(
            supported_browser_for_process(&process("firefox")),
            Some(SupportedBrowser::Firefox)
        );
        assert_eq!(
            supported_browser_for_process(&process("librewolf")),
            Some(SupportedBrowser::LibreWolf)
        );
        assert_eq!(
            supported_browser_for_process(&process("waterfox")),
            Some(SupportedBrowser::Waterfox)
        );
        assert_eq!(
            supported_browser_for_process(&process("google-chrome")),
            Some(SupportedBrowser::Chrome)
        );
        assert_eq!(
            supported_browser_for_process(&process("chromium")),
            Some(SupportedBrowser::Chromium)
        );
        assert_eq!(
            supported_browser_for_process(&process("brave-browser")),
            Some(SupportedBrowser::Brave)
        );
        assert_eq!(
            supported_browser_for_process(&process("opera")),
            Some(SupportedBrowser::Opera)
        );
        assert_eq!(
            supported_browser_for_process(&process("microsoft-edge")),
            Some(SupportedBrowser::Edge)
        );
        assert_eq!(
            supported_browser_for_process(&process("msedge")),
            Some(SupportedBrowser::Edge)
        );
        assert_eq!(
            supported_browser_for_process(&process("vivaldi")),
            Some(SupportedBrowser::Vivaldi)
        );
        assert_eq!(
            supported_browser_for_process(&process("vivaldi-bin")),
            Some(SupportedBrowser::Vivaldi)
        );

        let chromium_snap = ProcessIdentity {
            executable_path: Some(
                "/snap/chromium/3499/usr/lib/chromium-browser/chrome".to_string(),
            ),
            executable_basename: Some("chrome".to_string()),
            command_name: Some("chrome".to_string()),
            ..process("chrome")
        };
        assert_eq!(
            supported_browser_for_process(&chromium_snap),
            Some(SupportedBrowser::Chromium)
        );
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
