use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Datelike, Duration, FixedOffset, Local, TimeZone, Timelike, Utc};
use focus_core::{
    emergency_uninstall_code_is_valid, evaluate_app, evaluate_url, installation_serial_is_valid,
    record_visit_end, record_visit_heartbeat, record_visit_start, request_unlock,
    schedule_ids_are_active_at as schedule_ids_are_active_at_with_clock, site_usage_is_metered,
    AllowanceConfig, AppRuleConfig, BlockReason, Config, ControlledBlockReason, Decision,
    DetoxSession, DetoxTargetKind, EvaluationContext, FocusCore, HeartbeatState,
    NotificationPreferences, RuleConfig, RulePatternKind, RuleTier, ScheduleConfig, UnlockState,
    VisitState, Weekday,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::app::{
    browser_startup_grace_seconds, hosts_detox_sessions_for_clock, is_blockuntu_process,
    strict_browser_session_started_at,
};
use crate::chrome_policy::{ChromePolicyManager, ChromePolicyRepairStatus, ChromiumIncognitoMode};
use crate::cli::DEFAULT_EVENT_LOG_PATH;
use crate::clock_guard;
use crate::error::{DaemonError, Result};
use crate::firefox_policy::{FirefoxPolicyManager, RepairStatus};
use crate::hosts::{HostsManager, HostsRepairStatus};
use crate::policy_recovery::PolicyRecoveryManager;
use crate::process_scan::{
    attach_detected_window_titles, scan_procfs, supported_browser_for_process, ProcessInfo,
    SupportedBrowser, WindowTitleSupport,
};

const FIREFOX_EXTENSION_HEARTBEAT_COMPONENT: &str = "firefox_extension";
const LIBREWOLF_EXTENSION_HEARTBEAT_COMPONENT: &str = "librewolf_extension";
const WATERFOX_EXTENSION_HEARTBEAT_COMPONENT: &str = "waterfox_extension";
const CHROME_EXTENSION_HEARTBEAT_COMPONENT: &str = "chrome_extension";
const CHROMIUM_EXTENSION_HEARTBEAT_COMPONENT: &str = "chromium_extension";
const BRAVE_EXTENSION_HEARTBEAT_COMPONENT: &str = "brave_extension";
const OPERA_EXTENSION_HEARTBEAT_COMPONENT: &str = "opera_extension";
const EDGE_EXTENSION_HEARTBEAT_COMPONENT: &str = "edge_extension";
const VIVALDI_EXTENSION_HEARTBEAT_COMPONENT: &str = "vivaldi_extension";
const DEFAULT_EXTENSION_HEARTBEAT_TIMEOUT_SECONDS: u64 = 15;
const CHROME_EXTENSION_HEARTBEAT_TIMEOUT_SECONDS: u64 = 75;
const ENFORCEMENT_ACTIVE: &str = "active";
const ENFORCEMENT_UNINSTALLING: &str = "uninstalling";
const BROWSER_EXTENSION_MODE_KEY: &str = "browser_extension_mode";
const BROWSER_EXTENSION_UNINSTALLING_UNTIL_KEY: &str = "browser_extension_uninstalling_until";
const BROWSER_EXTENSION_MODE_ACTIVE: &str = "active";
const BROWSER_EXTENSION_MODE_UNINSTALLING: &str = "uninstalling";
const UNINSTALL_HANDOFF_SECONDS: i64 = 30;
const PACKAGE_REMOVAL_LEASE_PATH: &str = "/run/blockuntu/package-removal-lease";
const INSTALLATION_SERIAL_PATH: &str = "/etc/blockuntu/installation-id";
const TIER1_EDIT_KEY_PATH: &str = "/etc/blockuntu/tier1-edit-key.txt";
const SYSTEM_UNINSTALL_RECOVERY_PHRASE_PATH: &str = "/etc/blockuntu/uninstall-recovery.txt";
const RECOVERY_CREDENTIALS_HIDDEN_MARKER_PATH: &str =
    "/var/lib/blockuntu/recovery-credentials-hidden";
const TIER1_EDIT_UNLOCK_UNTIL_KEY: &str = "tier1_edit_unlocked_until";
const TIER1_EDIT_CREDENTIAL_SALT_KEY: &str = "tier1_edit_credential_salt";
const TIER1_EDIT_CREDENTIAL_HASH_KEY: &str = "tier1_edit_credential_hash";
const PROTECTED_ACCESS_MODE_KEY: &str = "protected_access_mode";
const UNSUPPORTED_BROWSER_BLOCK_MODE_KEY: &str = "unsupported_browser_block_mode";
const CHROMIUM_INCOGNITO_MODE_KEY: &str = "chromium_incognito_mode";
const CHROMIUM_INCOGNITO_DISABLE_SCOPE_KEY: &str = "chromium_incognito_disable_scope";
const CHROMIUM_INCOGNITO_CHANGE_ACCESS_MODE_KEY: &str = "chromium_incognito_change_access_mode";
const OPERATOR_WINDOW_RESTRICTION_KEY: &str = "operator_window_restriction_enabled";
const UNSUPPORTED_BROWSER_RULE_ID: &str = "unsupported-browsers-hard";
const TIER1_EDIT_UNLOCK_MINUTES: i64 = 5;
const OPERATOR_WINDOW_START_MINUTE: u16 = 20 * 60;
const OPERATOR_WINDOW_END_MINUTE: u16 = 23 * 60 + 59;
const MAX_DETOX_DURATION_MINUTES: u32 = 12 * 7 * 24 * 60;
const MAX_NOTIFICATION_THRESHOLD_MINUTES: u32 = 24 * 60;
const MAX_NOTIFICATION_THRESHOLDS: usize = 10;
const MAX_PENDING_NOTIFICATIONS: u32 = 50;
const MAX_NOTIFICATION_DELIVERY_DETAIL_LENGTH: usize = 2_000;
const BLOCK_NOTIFICATION_COOLDOWN_SECONDS: i64 = 60;
const BLOCK_NOTIFICATION_TTL_MINUTES: i64 = 2;
const LIFECYCLE_NOTIFICATION_TTL_MINUTES: i64 = 10;

#[derive(Clone)]
pub struct ChromiumPolicyBinding {
    browser: SupportedBrowser,
    policy: ChromePolicyManager,
}

impl ChromiumPolicyBinding {
    pub fn new(browser: SupportedBrowser, policy: ChromePolicyManager) -> Self {
        debug_assert!(browser.is_chromium_based());
        Self { browser, policy }
    }

    pub(crate) fn browser(&self) -> SupportedBrowser {
        self.browser
    }

    pub(crate) fn policy(&self) -> &ChromePolicyManager {
        &self.policy
    }
}

#[derive(Clone)]
pub struct GeckoPolicyBinding {
    browser: SupportedBrowser,
    policy: FirefoxPolicyManager,
}

impl GeckoPolicyBinding {
    pub fn new(browser: SupportedBrowser, policy: FirefoxPolicyManager) -> Self {
        debug_assert!(browser.is_firefox_based());
        Self { browser, policy }
    }

    pub(crate) fn browser(&self) -> SupportedBrowser {
        self.browser
    }

    pub(crate) fn policy(&self) -> &FirefoxPolicyManager {
        &self.policy
    }
}

#[derive(Clone)]
pub struct RpcContext {
    core: Arc<Mutex<FocusCore>>,
    extension_heartbeat_timeout_seconds: u64,
    gecko_policies: Vec<GeckoPolicyBinding>,
    chromium_policies: Vec<ChromiumPolicyBinding>,
    hosts: Option<HostsManager>,
    manage_firefox_policy: bool,
    manage_chrome_policy: bool,
    defer_firefox_policy_repair_until_heartbeat: bool,
    defer_chrome_policy_repair_until_heartbeat: bool,
    installation_serial_path: PathBuf,
    package_removal_lease_path: PathBuf,
    event_log_path: PathBuf,
    policy_recovery: Option<PolicyRecoveryManager>,
    trust_client_time: bool,
}

impl RpcContext {
    pub fn new(core: Arc<Mutex<FocusCore>>) -> Self {
        Self {
            core,
            extension_heartbeat_timeout_seconds: DEFAULT_EXTENSION_HEARTBEAT_TIMEOUT_SECONDS,
            gecko_policies: Vec::new(),
            chromium_policies: Vec::new(),
            hosts: None,
            manage_firefox_policy: true,
            manage_chrome_policy: true,
            defer_firefox_policy_repair_until_heartbeat: false,
            defer_chrome_policy_repair_until_heartbeat: false,
            installation_serial_path: PathBuf::from(INSTALLATION_SERIAL_PATH),
            package_removal_lease_path: PathBuf::from(PACKAGE_REMOVAL_LEASE_PATH),
            event_log_path: PathBuf::from(DEFAULT_EVENT_LOG_PATH),
            policy_recovery: None,
            trust_client_time: false,
        }
    }

    pub fn with_extension_heartbeat_timeout_seconds(mut self, seconds: u64) -> Self {
        self.extension_heartbeat_timeout_seconds = seconds;
        self
    }

    pub fn with_enforcement_managers(
        mut self,
        gecko_policies: Vec<GeckoPolicyBinding>,
        chromium_policies: Vec<ChromiumPolicyBinding>,
        hosts: HostsManager,
    ) -> Self {
        self.gecko_policies = gecko_policies;
        self.chromium_policies = chromium_policies;
        self.hosts = Some(hosts);
        self
    }

    pub fn with_browser_policy_management(
        mut self,
        manage_firefox_policy: bool,
        manage_chrome_policy: bool,
    ) -> Self {
        self.manage_firefox_policy = manage_firefox_policy;
        self.manage_chrome_policy = manage_chrome_policy;
        self
    }

    pub fn with_deferred_browser_policy_repair(
        mut self,
        defer_firefox_policy_repair_until_heartbeat: bool,
        defer_chrome_policy_repair_until_heartbeat: bool,
    ) -> Self {
        self.defer_firefox_policy_repair_until_heartbeat =
            defer_firefox_policy_repair_until_heartbeat;
        self.defer_chrome_policy_repair_until_heartbeat =
            defer_chrome_policy_repair_until_heartbeat;
        self
    }

    pub fn with_installation_serial_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.installation_serial_path = path.into();
        self
    }

    #[cfg(test)]
    pub fn with_package_removal_lease_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.package_removal_lease_path = path.into();
        self
    }

    pub fn with_event_log_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.event_log_path = path.into();
        self
    }

    pub fn with_policy_recovery(mut self, policy_recovery: PolicyRecoveryManager) -> Self {
        self.policy_recovery = Some(policy_recovery);
        self
    }

    #[cfg(test)]
    pub fn with_trusted_client_time(mut self) -> Self {
        self.trust_client_time = true;
        self
    }

    fn gecko_policy(&self, browser: SupportedBrowser) -> Result<&FirefoxPolicyManager> {
        self.gecko_policies
            .iter()
            .find(|binding| binding.browser == browser)
            .map(|binding| &binding.policy)
            .ok_or_else(|| {
                DaemonError::InvalidRequest(format!(
                    "{} policy manager is not configured",
                    browser.label()
                ))
            })
    }

    fn hosts(&self) -> Result<&HostsManager> {
        self.hosts.as_ref().ok_or_else(|| {
            DaemonError::InvalidRequest("hosts manager is not configured".to_string())
        })
    }

    fn chromium_policy(&self, browser: SupportedBrowser) -> Result<&ChromePolicyManager> {
        self.chromium_policies
            .iter()
            .find(|binding| binding.browser == browser)
            .map(|binding| &binding.policy)
            .ok_or_else(|| {
                DaemonError::InvalidRequest(format!(
                    "{} policy manager is not configured",
                    browser.label()
                ))
            })
    }
}

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[allow(dead_code)]
    jsonrpc: Option<String>,
    id: Option<Value>,
    method: Option<String>,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Deserialize)]
struct LegacyRequest {
    #[serde(rename = "type")]
    message_type: Option<String>,
    url: Option<String>,
    #[serde(rename = "extensionId")]
    extension_id: Option<String>,
    #[serde(rename = "extensionVersion")]
    extension_version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EvaluateUrlParams {
    url: String,
    #[serde(default)]
    probe: bool,
    #[serde(default)]
    now: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RequestUnlockParams {
    target: String,
    reason: String,
    #[serde(default)]
    now: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Tier1EditUnlockParams {
    phrase: String,
    #[serde(default)]
    now: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ConfigureTier1EditCredentialParams {
    phrase: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ProtectedAccessMode {
    Sunday,
    NoActiveScheduleOrDetox,
    AllTime,
}

#[derive(Debug, Deserialize)]
struct SetProtectedAccessModeParams {
    mode: ProtectedAccessMode,
}

#[derive(Debug, Deserialize)]
struct SetUnsupportedBrowserBlockModeParams {
    mode: ProtectedAccessMode,
}

#[derive(Debug, Deserialize)]
struct SetChromiumIncognitoModeParams {
    mode: ChromiumIncognitoMode,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ChromiumIncognitoDisableScope {
    AllTime,
    ActiveScheduleOrDetox,
}

#[derive(Debug, Deserialize)]
struct SetChromiumIncognitoDisableScopeParams {
    scope: ChromiumIncognitoDisableScope,
}

#[derive(Debug, Deserialize)]
struct SetChromiumIncognitoChangeAccessModeParams {
    mode: ProtectedAccessMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChromiumIncognitoPolicySettings {
    /// The mode selected in Settings. `mode` below is the effective policy mode.
    pub configured_mode: ChromiumIncognitoMode,
    pub mode: ChromiumIncognitoMode,
    pub disable_scope: ChromiumIncognitoDisableScope,
    pub private_browsing_disabled: bool,
    pub url_blocklist: Vec<String>,
    pub unsupported_pattern_count: usize,
}

impl ChromiumIncognitoPolicySettings {
    fn url_block_limit_exceeded(&self) -> bool {
        self.url_blocklist.len() > 1_000
    }
}

#[derive(Debug, Deserialize)]
struct SetOperatorWindowRestrictionParams {
    enabled: bool,
}

#[derive(Debug, Deserialize)]
struct HideRecoveryCredentialsParams {}

#[derive(Debug, Deserialize)]
struct Tier1EditStatusParams {
    #[serde(default)]
    now: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PrepareUninstallParams {
    #[serde(default)]
    now: Option<String>,
    #[serde(default)]
    emergency_code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RecordVisitStartParams {
    url: String,
    tab_id: String,
    #[serde(default)]
    now: Option<String>,
}

#[derive(Debug, Deserialize)]
struct VisitIdParams {
    visit_id: i64,
    #[serde(default)]
    now: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ExtensionHeartbeatParams {
    #[serde(default)]
    component: Option<String>,
    #[serde(default)]
    browser: Option<String>,
    #[serde(default)]
    extension_id: Option<String>,
    #[serde(default)]
    extension_version: Option<String>,
    #[serde(default)]
    now: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ExtensionStatusParams {
    #[serde(default)]
    component: Option<String>,
    #[serde(default)]
    now: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ListRunningAppsParams {
    #[serde(default)]
    now: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpsertSiteListParams {
    rule: RuleConfig,
    #[serde(default)]
    now: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeleteSiteListParams {
    id: String,
    #[serde(default)]
    now: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpsertAllowanceParams {
    allowance: AllowanceConfig,
    #[serde(default)]
    now: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeleteAllowanceParams {
    id: String,
    #[serde(default)]
    now: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpsertAppRuleParams {
    rule: AppRuleConfig,
    #[serde(default)]
    now: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeleteAppRuleParams {
    id: String,
    #[serde(default)]
    now: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct RunningAppSnapshot {
    pid: u32,
    display_name: String,
    executable_path: Option<String>,
    executable_basename: Option<String>,
    command_name: Option<String>,
    desktop_id: Option<String>,
    window_titles: Vec<String>,
    decision: &'static str,
    blocking_rule_id: Option<String>,
    blocking_rule_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct RunningAppsResponse {
    apps: Vec<RunningAppSnapshot>,
    window_detection: WindowTitleSupport,
}

#[derive(Debug, Deserialize)]
struct UpsertScheduleParams {
    schedule: ScheduleConfig,
    #[serde(default)]
    site_rule_ids: Option<Vec<String>>,
    #[serde(default)]
    app_rule_ids: Option<Vec<String>>,
    #[serde(default)]
    now: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeleteScheduleParams {
    id: String,
    #[serde(default)]
    now: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StartDetoxParams {
    #[serde(default)]
    name: Option<String>,
    duration_minutes: u32,
    #[serde(default)]
    site_rule_ids: Vec<String>,
    #[serde(default)]
    app_rule_ids: Vec<String>,
    #[serde(default)]
    now: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CancelDetoxParams {
    id: String,
    #[serde(default)]
    now: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DetoxSessionsParams {
    #[serde(default = "default_detox_sessions_limit")]
    limit: u32,
    #[serde(default)]
    active_only: bool,
    #[serde(default)]
    now: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ScheduleActivitySummaryParams {
    #[serde(default)]
    now: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SetNotificationPreferencesParams {
    preferences: NotificationPreferences,
}

#[derive(Debug, Deserialize)]
struct PendingNotificationsParams {
    #[serde(default = "default_pending_notifications_limit")]
    limit: u32,
}

#[derive(Debug, Deserialize)]
struct AcknowledgeNotificationsParams {
    ids: Vec<i64>,
}

#[derive(Debug, Deserialize)]
struct RecordNotificationDeliveryParams {
    id: i64,
    delivered: bool,
    detail: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ImportPolicyTomlParams {
    toml: String,
    #[serde(default)]
    now: Option<String>,
}

#[derive(Debug, Default)]
struct PolicyAppendSummary {
    allowances: usize,
    schedules: usize,
    rules: usize,
    app_rules: usize,
}

impl PolicyAppendSummary {
    fn total(&self) -> usize {
        self.allowances + self.schedules + self.rules + self.app_rules
    }
}

pub fn handle_payload(context: &RpcContext, payload: &[u8]) -> Vec<u8> {
    let response = match serde_json::from_slice::<Value>(payload) {
        Ok(value) => handle_json_value(context, value),
        Err(err) => jsonrpc_error(None, -32700, "parse error", Some(err.to_string())),
    };

    serde_json::to_vec(&response).expect("JSON-RPC response must serialize")
}

fn handle_json_value(context: &RpcContext, value: Value) -> Value {
    if is_legacy_request(&value) {
        return handle_legacy_request(context, value);
    }

    let request = match serde_json::from_value::<JsonRpcRequest>(value) {
        Ok(request) => request,
        Err(err) => return jsonrpc_error(None, -32600, "invalid request", Some(err.to_string())),
    };
    let id = request.id.clone();
    let Some(method) = request.method.as_deref() else {
        return jsonrpc_error(id, -32600, "missing method", None);
    };

    match handle_method(context, method, request.params) {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(DaemonError::UnsupportedMethod(method)) => {
            jsonrpc_error(id, -32601, "method not found", Some(method))
        }
        Err(DaemonError::InvalidRequest(message)) => {
            jsonrpc_error(id, -32602, "invalid params", Some(message))
        }
        Err(err) => jsonrpc_error(id, -32603, "internal error", Some(err.to_string())),
    }
}

fn handle_method(context: &RpcContext, method: &str, params: Value) -> Result<Value> {
    match method {
        "status" => status(context),
        "enforcement_status" => enforcement_status(context),
        "clock_integrity_status" => clock_integrity_status_method(context),
        "prepare_uninstall" => {
            let params = parse_params::<PrepareUninstallParams>(params)?;
            prepare_uninstall(context, params)
        }
        "config_snapshot" => config_snapshot(context),
        "export_policy_toml" => export_policy_toml_method(context),
        "import_policy_toml" => {
            let params = parse_params::<ImportPolicyTomlParams>(params)?;
            import_policy_toml_method(context, params)
        }
        "upsert_site_list" => {
            let params = parse_params::<UpsertSiteListParams>(params)?;
            upsert_site_list_method(context, params)
        }
        "delete_site_list" => {
            let params = parse_params::<DeleteSiteListParams>(params)?;
            delete_site_list_method(context, params)
        }
        "upsert_allowance" => {
            let params = parse_params::<UpsertAllowanceParams>(params)?;
            upsert_allowance_method(context, params)
        }
        "delete_allowance" => {
            let params = parse_params::<DeleteAllowanceParams>(params)?;
            delete_allowance_method(context, params)
        }
        "upsert_app_rule" => {
            let params = parse_params::<UpsertAppRuleParams>(params)?;
            upsert_app_rule_method(context, params)
        }
        "delete_app_rule" => {
            let params = parse_params::<DeleteAppRuleParams>(params)?;
            delete_app_rule_method(context, params)
        }
        "upsert_schedule" => {
            let params = parse_params::<UpsertScheduleParams>(params)?;
            upsert_schedule_method(context, params)
        }
        "delete_schedule" => {
            let params = parse_params::<DeleteScheduleParams>(params)?;
            delete_schedule_method(context, params)
        }
        "start_detox" => {
            let params = parse_params::<StartDetoxParams>(params)?;
            start_detox_method(context, params)
        }
        "cancel_detox" => {
            let params = parse_params::<CancelDetoxParams>(params)?;
            cancel_detox_method(context, params)
        }
        "detox_sessions" => {
            let params = parse_params::<DetoxSessionsParams>(params)?;
            detox_sessions_method(context, params)
        }
        "log_summary" => log_summary(context),
        "schedule_activity_summary" => {
            let params = parse_params::<ScheduleActivitySummaryParams>(params)?;
            schedule_activity_summary(context, params)
        }
        "notification_preferences" => notification_preferences_method(context),
        "set_notification_preferences" => {
            let params = parse_params::<SetNotificationPreferencesParams>(params)?;
            set_notification_preferences_method(context, params)
        }
        "pending_notifications" => {
            let params = parse_params::<PendingNotificationsParams>(params)?;
            pending_notifications_method(context, params)
        }
        "acknowledge_notifications" => {
            let params = parse_params::<AcknowledgeNotificationsParams>(params)?;
            acknowledge_notifications_method(context, params)
        }
        "record_notification_delivery" => {
            let params = parse_params::<RecordNotificationDeliveryParams>(params)?;
            record_notification_delivery_method(context, params)
        }
        "running_apps" => {
            let params = parse_params::<ListRunningAppsParams>(params)?;
            running_apps_method(context, params)
        }
        "evaluate_url" => {
            let params = parse_params::<EvaluateUrlParams>(params)?;
            evaluate_url_method(context, params)
        }
        "request_unlock" => {
            let params = parse_params::<RequestUnlockParams>(params)?;
            request_unlock_method(context, params)
        }
        "unlock_tier1_edit" => {
            let params = parse_params::<Tier1EditUnlockParams>(params)?;
            unlock_tier1_edit_method(context, params)
        }
        "configure_tier1_edit_credential" => {
            let params = parse_params::<ConfigureTier1EditCredentialParams>(params)?;
            configure_tier1_edit_credential_method(context, params)
        }
        "set_operator_window_restriction" => {
            let params = parse_params::<SetOperatorWindowRestrictionParams>(params)?;
            set_operator_window_restriction_method(context, params)
        }
        "set_protected_access_mode" => {
            let params = parse_params::<SetProtectedAccessModeParams>(params)?;
            set_protected_access_mode_method(context, params)
        }
        "set_unsupported_browser_block_mode" => {
            let params = parse_params::<SetUnsupportedBrowserBlockModeParams>(params)?;
            set_unsupported_browser_block_mode_method(context, params)
        }
        "set_chromium_incognito_mode" => {
            let params = parse_params::<SetChromiumIncognitoModeParams>(params)?;
            set_chromium_incognito_mode_method(context, params)
        }
        "set_chromium_incognito_disable_scope" => {
            let params = parse_params::<SetChromiumIncognitoDisableScopeParams>(params)?;
            set_chromium_incognito_disable_scope_method(context, params)
        }
        "set_chromium_incognito_change_access_mode" => {
            let params = parse_params::<SetChromiumIncognitoChangeAccessModeParams>(params)?;
            set_chromium_incognito_change_access_mode_method(context, params)
        }
        "hide_recovery_credentials" => {
            let params = parse_params::<HideRecoveryCredentialsParams>(params)?;
            hide_recovery_credentials_method(context, params)
        }
        "tier1_edit_status" => {
            let params = parse_params::<Tier1EditStatusParams>(params)?;
            tier1_edit_status_method(context, params)
        }
        "record_visit_start" => {
            let params = parse_params::<RecordVisitStartParams>(params)?;
            record_visit_start_method(context, params)
        }
        "record_visit_heartbeat" => {
            let params = parse_params::<VisitIdParams>(params)?;
            record_visit_heartbeat_method(context, params)
        }
        "record_visit_end" => {
            let params = parse_params::<VisitIdParams>(params)?;
            record_visit_end_method(context, params)
        }
        "extension_heartbeat" => {
            let params = parse_params::<ExtensionHeartbeatParams>(params)?;
            extension_heartbeat_method(context, params)
        }
        "extension_status" => {
            let params = parse_params::<ExtensionStatusParams>(params)?;
            extension_status_method(context, params)
        }
        _ => Err(DaemonError::UnsupportedMethod(method.to_string())),
    }
}

fn status(context: &RpcContext) -> Result<Value> {
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    let enforcement_state = enforcement_state_from_core(&core)?;
    let clock_integrity =
        clock_guard::guarded_now(core.database(), None, context.trust_client_time)?.integrity;
    Ok(json!({
        "status": "ok",
        "enforcement_state": enforcement_state,
        "clock_integrity": clock_integrity,
        "rules": core.config().rules.len(),
        "app_rules": core.config().app_rules.len(),
        "schedules": core.config().schedules.len(),
        "allowances": core.config().allowances.len()
    }))
}

fn enforcement_status(context: &RpcContext) -> Result<Value> {
    let hosts = context.hosts()?;
    let (enforcement_state, config, active_detox_sessions, now, clock_tampered) = {
        let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
        let guarded = clock_guard::guarded_now(core.database(), None, context.trust_client_time)?;
        let active_detox_sessions = hosts_detox_sessions_for_clock(
            &core,
            guarded.now.with_timezone(&Utc),
            guarded.integrity.state == "tampered",
        )?;
        (
            enforcement_state_from_core(&core)?,
            core.config().clone(),
            active_detox_sessions,
            guarded.now,
            guarded.integrity.state == "tampered",
        )
    };

    Ok(json!({
        "status": "ok",
        "enforcement_state": enforcement_state,
        "clock_integrity": clock_integrity_status_method(context)?,
        "firefox_policy": firefox_policy_status_json(context)?,
        "firefox_family_policies": gecko_policy_statuses_json(context)?,
        "chrome_policy": chrome_policy_status_json(context)?,
        "chromium_policies": chromium_policy_statuses_json(context)?,
        "hosts_file": hosts.status_with_active_detox(
            &config,
            &active_detox_sessions,
            now,
            clock_tampered,
        )
    }))
}

fn clock_integrity_status_method(context: &RpcContext) -> Result<Value> {
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    serde_json::to_value(
        clock_guard::guarded_now(core.database(), None, context.trust_client_time)?.integrity,
    )
    .map_err(DaemonError::from)
}

fn prepare_uninstall(context: &RpcContext, params: PrepareUninstallParams) -> Result<Value> {
    let hosts = context.hosts()?;
    let operator_now = guarded_now(context, params.now.as_deref())?;
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    let protected_access_mode = protected_access_mode(&core)?;
    let protected_access_open = protected_access_is_open(&core, operator_now, false)?;
    drop(core);
    let emergency_authorized = params
        .emergency_code
        .as_deref()
        .is_some_and(|code| emergency_uninstall_code_matches(context, code));
    if !emergency_authorized {
        reject_if_clock_tampered(context)?;
        if !protected_access_open {
            return Err(protected_access_closed_error(
                "GUI uninstall",
                protected_access_mode,
            ));
        }
    }
    let now = Utc::now();
    let uninstalling_until = now + Duration::seconds(UNINSTALL_HANDOFF_SECONDS);
    let package_removal_lease =
        write_package_removal_lease(&context.package_removal_lease_path, uninstalling_until)?;

    {
        let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
        core.database().set_service_state(
            BROWSER_EXTENSION_MODE_KEY,
            BROWSER_EXTENSION_MODE_UNINSTALLING,
            now,
        )?;
        core.database().set_service_state(
            BROWSER_EXTENSION_UNINSTALLING_UNTIL_KEY,
            &uninstalling_until.to_rfc3339(),
            now,
        )?;
    }

    let gecko_policy_repairs = remove_gecko_policies_from_context(context)?;
    let chromium_policy_repairs = remove_chromium_policies_from_context(context)?;
    let hosts_repair = hosts.remove_managed_block()?;
    let enforcement = enforcement_status(context)?;

    {
        let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
        core.database().record_event(
            "uninstall_prepared",
            Some("system"),
            Some(&format!(
                "authorization={};browser_extension_mode={BROWSER_EXTENSION_MODE_UNINSTALLING};until={};gecko_policies={gecko_policy_repairs:?};chromium_policies={chromium_policy_repairs:?};hosts={hosts_repair:?}",
                if emergency_authorized {
                    "emergency".to_string()
                } else {
                    format!("protected_access:{}", protected_access_mode_name(protected_access_mode))
                },
                uninstalling_until.to_rfc3339()
            )),
            now,
        )?;
    }

    Ok(json!({
        "status": "ok",
        "browser_extension_mode": BROWSER_EXTENSION_MODE_UNINSTALLING,
        "uninstalling_until": uninstalling_until,
        "package_removal_lease": package_removal_lease,
        "enforcement": enforcement
    }))
}

fn write_package_removal_lease(path: &Path, expires_at: DateTime<Utc>) -> Result<String> {
    let mut bytes = [0_u8; 32];
    fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    let token = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let contents = format!("{token} {}\n", expires_at.timestamp());
    let temporary_path = path.with_extension("tmp");

    let mut temporary_file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&temporary_path)?;
    temporary_file.write_all(contents.as_bytes())?;
    temporary_file.sync_all()?;
    fs::set_permissions(&temporary_path, fs::Permissions::from_mode(0o600))?;
    fs::rename(&temporary_path, path)?;
    Ok(token)
}

fn emergency_uninstall_code_matches(context: &RpcContext, candidate: &str) -> bool {
    load_installation_serial(&context.installation_serial_path)
        .ok()
        .is_some_and(|serial| emergency_uninstall_code_is_valid(candidate, &serial))
}

fn load_installation_serial(path: &Path) -> Result<String> {
    let serial = fs::read_to_string(path)?.trim().to_string();
    if !installation_serial_is_valid(&serial) {
        return Err(DaemonError::InvalidRequest(
            "BlocKuntu installation serial is missing or invalid".to_string(),
        ));
    }
    Ok(serial)
}

fn firefox_policy_status_json(context: &RpcContext) -> Result<Value> {
    gecko_policy_status_json(context, SupportedBrowser::Firefox)
}

fn gecko_policy_statuses_json(context: &RpcContext) -> Result<Value> {
    let mut statuses = BTreeMap::new();
    for browser in [SupportedBrowser::LibreWolf, SupportedBrowser::Waterfox] {
        statuses.insert(browser.label(), gecko_policy_status_json(context, browser)?);
    }
    serde_json::to_value(statuses).map_err(DaemonError::from)
}

fn gecko_policy_status_json(context: &RpcContext, browser: SupportedBrowser) -> Result<Value> {
    let mut status = serde_json::to_value(context.gecko_policy(browser)?.status())?;
    let heartbeat_seen = has_extension_heartbeat(context, browser.extension_component())?;
    let deferred = context.defer_firefox_policy_repair_until_heartbeat && !heartbeat_seen;
    let browser_label = browser_display_name(browser);
    if let Some(object) = status.as_object_mut() {
        object.insert("managed".to_string(), json!(context.manage_firefox_policy));
        object.insert(
            "deferred_until_heartbeat".to_string(),
            json!(context.defer_firefox_policy_repair_until_heartbeat),
        );
        object.insert("active_after_heartbeat".to_string(), json!(heartbeat_seen));
        if !context.manage_firefox_policy {
            object.insert("compliant".to_string(), json!(true));
            object.insert(
                "detail".to_string(),
                json!(format!(
                    "{browser_label} policy management is disabled; install and enable the extension manually"
                )),
            );
        } else if deferred {
            object.insert("compliant".to_string(), json!(true));
            object.insert(
                "detail".to_string(),
                json!(format!(
                    "{browser_label} policy repair is deferred until the first extension heartbeat"
                )),
            );
        }
    }
    Ok(status)
}

fn chrome_policy_status_json(context: &RpcContext) -> Result<Value> {
    chromium_policy_status_json(context, SupportedBrowser::Chrome)
}

fn chromium_policy_statuses_json(context: &RpcContext) -> Result<Value> {
    let mut statuses = BTreeMap::new();
    for browser in [
        SupportedBrowser::Chromium,
        SupportedBrowser::Brave,
        SupportedBrowser::Opera,
        SupportedBrowser::Edge,
        SupportedBrowser::Vivaldi,
    ] {
        statuses.insert(
            browser.label(),
            chromium_policy_status_json(context, browser)?,
        );
    }
    serde_json::to_value(statuses).map_err(DaemonError::from)
}

fn chromium_policy_status_json(context: &RpcContext, browser: SupportedBrowser) -> Result<Value> {
    let settings = current_chromium_incognito_policy_settings_from_context(context)?;
    let mut status = serde_json::to_value(
        context
            .chromium_policy(browser)?
            .status_with(settings.mode, &settings.url_blocklist),
    )?;
    let heartbeat_seen = has_extension_heartbeat(context, browser.extension_component())?;
    let deferred = context.defer_chrome_policy_repair_until_heartbeat && !heartbeat_seen;
    let browser_label = browser_display_name(browser);
    if let Some(object) = status.as_object_mut() {
        object.insert("managed".to_string(), json!(context.manage_chrome_policy));
        object.insert(
            "deferred_until_heartbeat".to_string(),
            json!(context.defer_chrome_policy_repair_until_heartbeat),
        );
        object.insert("active_after_heartbeat".to_string(), json!(heartbeat_seen));
        object.insert(
            "incognito_unsupported_pattern_count".to_string(),
            json!(settings.unsupported_pattern_count),
        );
        object.insert(
            "incognito_url_block_limit_exceeded".to_string(),
            json!(settings.url_block_limit_exceeded()),
        );
        if settings.url_block_limit_exceeded() {
            object.insert("compliant".to_string(), json!(false));
            object.insert(
                "detail".to_string(),
                json!(format!(
                    "{browser_label} private URL policy has {} active patterns, but browsers apply at most 1000",
                    settings.url_blocklist.len()
                )),
            );
        }
        if !context.manage_chrome_policy {
            object.insert("compliant".to_string(), json!(true));
            object.insert("force_install_configured".to_string(), json!(true));
            object.insert(
                "detail".to_string(),
                json!(format!(
                    "{browser_label} policy management is disabled; install and enable the extension manually"
                )),
            );
        } else if deferred {
            object.insert("compliant".to_string(), json!(true));
            object.insert("force_install_configured".to_string(), json!(true));
            object.insert(
                "detail".to_string(),
                json!(format!(
                    "{browser_label} policy repair is deferred until the first extension heartbeat"
                )),
            );
        }
    }
    Ok(status)
}

fn remove_gecko_policies_from_context(
    context: &RpcContext,
) -> Result<BTreeMap<&'static str, RepairStatus>> {
    if !context.manage_firefox_policy {
        return Ok(SupportedBrowser::MANAGED
            .into_iter()
            .filter(|browser| browser.is_firefox_based())
            .map(|browser| (browser.label(), RepairStatus::SkippedDisabled))
            .collect());
    }
    SupportedBrowser::MANAGED
        .into_iter()
        .filter(|browser| browser.is_firefox_based())
        .map(|browser| {
            Ok((
                browser.label(),
                context.gecko_policy(browser)?.remove_policy()?,
            ))
        })
        .collect()
}

fn remove_chromium_policies_from_context(
    context: &RpcContext,
) -> Result<BTreeMap<&'static str, ChromePolicyRepairStatus>> {
    if !context.manage_chrome_policy {
        return Ok(SupportedBrowser::MANAGED
            .into_iter()
            .filter(|browser| browser.is_chromium_based())
            .map(|browser| (browser.label(), ChromePolicyRepairStatus::SkippedDisabled))
            .collect());
    }
    SupportedBrowser::MANAGED
        .into_iter()
        .filter(|browser| browser.is_chromium_based())
        .map(|browser| {
            Ok((
                browser.label(),
                context.chromium_policy(browser)?.remove_policy()?,
            ))
        })
        .collect()
}

fn has_extension_heartbeat(context: &RpcContext, component: &str) -> Result<bool> {
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    Ok(core.database().heartbeat(component)?.is_some())
}

fn current_chromium_incognito_policy_settings_from_context(
    context: &RpcContext,
) -> Result<ChromiumIncognitoPolicySettings> {
    let guarded = guarded_now_with_status(context, None)?;
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    current_chromium_incognito_policy_settings(
        &core,
        guarded.now,
        guarded.integrity.state == "tampered",
    )
}

fn repair_chromium_policies_for_settings_change(
    context: &RpcContext,
    settings: &ChromiumIncognitoPolicySettings,
) -> Result<BTreeMap<&'static str, ChromePolicyRepairStatus>> {
    if context.chromium_policies.is_empty() {
        return Ok(BTreeMap::new());
    }
    ensure_chromium_incognito_url_blocklist_within_limit(settings)?;
    {
        let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
        if !enforcement_active_from_core(&core)? {
            return Ok(SupportedBrowser::MANAGED
                .into_iter()
                .filter(|browser| browser.is_chromium_based())
                .map(|browser| (browser.label(), ChromePolicyRepairStatus::SkippedInactive))
                .collect());
        }
    }

    SupportedBrowser::MANAGED
        .into_iter()
        .filter(|browser| browser.is_chromium_based())
        .map(|browser| {
            if !context.manage_chrome_policy {
                return Ok((browser.label(), ChromePolicyRepairStatus::SkippedDisabled));
            }
            if context.defer_chrome_policy_repair_until_heartbeat
                && !has_extension_heartbeat(context, browser.extension_component())?
            {
                return Ok((browser.label(), ChromePolicyRepairStatus::SkippedDeferred));
            }
            Ok((
                browser.label(),
                context
                    .chromium_policy(browser)?
                    .verify_and_repair_with(settings.mode, &settings.url_blocklist)?,
            ))
        })
        .collect()
}

fn repair_chromium_policies_for_current_settings(
    context: &RpcContext,
) -> Result<BTreeMap<&'static str, ChromePolicyRepairStatus>> {
    let settings = current_chromium_incognito_policy_settings_from_context(context)?;
    repair_chromium_policies_for_settings_change(context, &settings)
}

fn repair_deferred_policy_after_heartbeat(context: &RpcContext, component: &str) -> Result<Value> {
    {
        let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
        if !enforcement_active_from_core(&core)? {
            return Ok(json!({ "skipped": "uninstall_handoff_active" }));
        }
    }

    match component {
        component
            if context.manage_firefox_policy
                && context.defer_firefox_policy_repair_until_heartbeat
                && supported_browser_for_extension_component(component)
                    .is_some_and(SupportedBrowser::is_firefox_based) =>
        {
            let browser = supported_browser_for_extension_component(component)
                .expect("Firefox-family browser was checked above");
            let status = context.gecko_policy(browser)?.verify_and_repair()?;
            Ok(json!({ (format!("{}_policy", browser.label())): format!("{status:?}") }))
        }
        component
            if context.manage_chrome_policy
                && context.defer_chrome_policy_repair_until_heartbeat
                && supported_browser_for_extension_component(component)
                    .is_some_and(SupportedBrowser::is_chromium_based) =>
        {
            let browser = supported_browser_for_extension_component(component)
                .expect("chromium browser was checked above");
            let settings = current_chromium_incognito_policy_settings_from_context(context)?;
            ensure_chromium_incognito_url_blocklist_within_limit(&settings)?;
            let status = context
                .chromium_policy(browser)?
                .verify_and_repair_with(settings.mode, &settings.url_blocklist)?;
            Ok(json!({ (format!("{}_policy", browser.label())): format!("{status:?}") }))
        }
        _ => Ok(json!({})),
    }
}

fn config_snapshot(context: &RpcContext) -> Result<Value> {
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    serde_json::to_value(core.config()).map_err(DaemonError::from)
}

fn schedule_activity_summary(
    context: &RpcContext,
    params: ScheduleActivitySummaryParams,
) -> Result<Value> {
    let now = guarded_now(context, params.now.as_deref())?;
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    let totals = core
        .database()
        .sync_schedule_activity_totals(&core.config().schedules, now)?;

    Ok(json!({
        "tracked_at": now.with_timezone(&Utc),
        "schedules": core
            .config()
            .schedules
            .iter()
            .zip(totals)
            .map(|(schedule, total)| json!({
                "id": schedule.id,
                "name": schedule.name,
                "total_active_seconds": total.total_active_seconds,
            }))
            .collect::<Vec<_>>(),
    }))
}

fn notification_preferences_method(context: &RpcContext) -> Result<Value> {
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    Ok(serde_json::to_value(
        core.database().notification_preferences()?,
    )?)
}

fn set_notification_preferences_method(
    context: &RpcContext,
    params: SetNotificationPreferencesParams,
) -> Result<Value> {
    let preferences = normalize_notification_preferences(params.preferences)?;
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    core.database().set_notification_preferences(&preferences)?;
    core.database()
        .discard_disabled_notifications(&preferences, Utc::now())?;
    core.database().record_event(
        "notification_preferences_updated",
        Some("notifications"),
        None,
        Utc::now(),
    )?;
    Ok(serde_json::to_value(preferences)?)
}

fn pending_notifications_method(
    context: &RpcContext,
    params: PendingNotificationsParams,
) -> Result<Value> {
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    let notifications = core
        .database()
        .pending_notifications(Utc::now(), params.limit.clamp(1, MAX_PENDING_NOTIFICATIONS))?;
    Ok(json!({ "notifications": notifications }))
}

fn acknowledge_notifications_method(
    context: &RpcContext,
    params: AcknowledgeNotificationsParams,
) -> Result<Value> {
    if params.ids.len() > MAX_PENDING_NOTIFICATIONS as usize {
        return Err(DaemonError::InvalidRequest(format!(
            "cannot acknowledge more than {MAX_PENDING_NOTIFICATIONS} notifications at once"
        )));
    }
    if params.ids.iter().any(|id| *id <= 0) {
        return Err(DaemonError::InvalidRequest(
            "notification ids must be positive".to_string(),
        ));
    }
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    core.database()
        .acknowledge_notifications(&params.ids, Utc::now())?;
    Ok(json!({ "status": "ok", "acknowledged": params.ids.len() }))
}

fn record_notification_delivery_method(
    context: &RpcContext,
    params: RecordNotificationDeliveryParams,
) -> Result<Value> {
    if params.id <= 0 {
        return Err(DaemonError::InvalidRequest(
            "notification id must be positive".to_string(),
        ));
    }
    if params
        .detail
        .as_ref()
        .is_some_and(|detail| detail.len() > MAX_NOTIFICATION_DELIVERY_DETAIL_LENGTH)
    {
        return Err(DaemonError::InvalidRequest(format!(
            "notification delivery detail cannot exceed {MAX_NOTIFICATION_DELIVERY_DETAIL_LENGTH} bytes"
        )));
    }

    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    let recorded = core.database().record_notification_delivery_result(
        params.id,
        params.delivered,
        params.detail.as_deref(),
        Utc::now(),
    )?;
    if !recorded {
        return Err(DaemonError::InvalidRequest(format!(
            "notification {} does not exist",
            params.id
        )));
    }
    Ok(json!({
        "status": if params.delivered { "accepted" } else { "failed" },
        "id": params.id
    }))
}

fn normalize_notification_preferences(
    mut preferences: NotificationPreferences,
) -> Result<NotificationPreferences> {
    if preferences.allowance_warning_minutes.len() > MAX_NOTIFICATION_THRESHOLDS {
        return Err(DaemonError::InvalidRequest(format!(
            "no more than {MAX_NOTIFICATION_THRESHOLDS} allowance warning thresholds are allowed"
        )));
    }
    if preferences
        .allowance_warning_minutes
        .iter()
        .any(|minutes| *minutes == 0 || *minutes > MAX_NOTIFICATION_THRESHOLD_MINUTES)
    {
        return Err(DaemonError::InvalidRequest(format!(
            "allowance warning thresholds must be between 1 and {MAX_NOTIFICATION_THRESHOLD_MINUTES} minutes"
        )));
    }
    preferences
        .allowance_warning_minutes
        .sort_unstable_by(|left, right| right.cmp(left));
    preferences.allowance_warning_minutes.dedup();
    Ok(preferences)
}

fn enqueue_website_block_notification(
    core: &FocusCore,
    url: &str,
    decision: &Decision,
    now: DateTime<Utc>,
) -> Result<()> {
    let preferences = core.database().notification_preferences()?;
    if !preferences.enabled || !preferences.website_blocked {
        return Ok(());
    }
    let Decision::Block(reason) = decision else {
        return Ok(());
    };
    let Some((rule_id, rule_name)) = notification_rule(reason) else {
        return Ok(());
    };
    let target = url::Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_string))
        .unwrap_or_else(|| url.to_string());
    core.database().enqueue_notification(
        "website_blocked",
        "Website blocked",
        &format!("{target} was blocked by \"{rule_name}\"."),
        &format!("website_blocked:{rule_id}:{}", target.to_ascii_lowercase()),
        now,
        Duration::seconds(BLOCK_NOTIFICATION_COOLDOWN_SECONDS),
        Duration::minutes(BLOCK_NOTIFICATION_TTL_MINUTES),
    )?;
    Ok(())
}

pub(crate) fn enqueue_application_block_notification(
    core: &FocusCore,
    rule_id: &str,
    application_name: &str,
    now: DateTime<Utc>,
) -> Result<()> {
    let preferences = core.database().notification_preferences()?;
    if !preferences.enabled || !preferences.application_blocked {
        return Ok(());
    }
    let rule_name = core
        .config()
        .app_rules
        .iter()
        .find(|rule| rule.id == rule_id)
        .map(|rule| rule.name.as_str())
        .unwrap_or(rule_id);
    core.database().enqueue_notification(
        "application_blocked",
        "Application blocked",
        &format!("{application_name} was blocked by \"{rule_name}\"."),
        &format!(
            "application_blocked:{rule_id}:{}",
            application_name.to_ascii_lowercase()
        ),
        now,
        Duration::seconds(BLOCK_NOTIFICATION_COOLDOWN_SECONDS),
        Duration::minutes(BLOCK_NOTIFICATION_TTL_MINUTES),
    )?;
    Ok(())
}

pub(crate) fn enqueue_detox_notification(
    core: &FocusCore,
    session: &DetoxSession,
    active: bool,
    ended_early: bool,
    now: DateTime<Utc>,
) -> Result<()> {
    core.database()
        .set_notification_lifecycle_state("detox", &session.id, active, now)?;
    let preferences = core.database().notification_preferences()?;
    if !preferences.enabled
        || (active && !preferences.detox_started)
        || (!active && !preferences.detox_ended)
    {
        return Ok(());
    }
    let name = session.name.as_deref().unwrap_or(&session.id);
    let (kind, title, body) = if active {
        (
            "detox_started",
            "Detox started",
            format!("\"{name}\" is now active."),
        )
    } else if ended_early {
        (
            "detox_ended",
            "Detox ended",
            format!("\"{name}\" was ended early."),
        )
    } else {
        (
            "detox_ended",
            "Detox ended",
            format!("\"{name}\" has finished."),
        )
    };
    core.database().enqueue_notification(
        kind,
        title,
        &body,
        &format!("{kind}:{}", session.id),
        now,
        Duration::minutes(1),
        Duration::minutes(LIFECYCLE_NOTIFICATION_TTL_MINUTES),
    )?;
    Ok(())
}

fn notification_rule(reason: &BlockReason) -> Option<(&str, &str)> {
    match reason {
        BlockReason::Detox {
            rule_id, rule_name, ..
        }
        | BlockReason::HardBlock { rule_id, rule_name }
        | BlockReason::ScheduledBlock { rule_id, rule_name }
        | BlockReason::ControlledAccess {
            rule_id, rule_name, ..
        } => Some((rule_id, rule_name)),
        BlockReason::InvalidUrl { .. } | BlockReason::RuntimeError { .. } => None,
    }
}

fn export_policy_toml_method(context: &RpcContext) -> Result<Value> {
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    let toml = core.config().to_toml_string()?;
    Ok(json!({
        "status": "ok",
        "toml": toml,
        "exported_at": Utc::now()
    }))
}

fn import_policy_toml_method(
    context: &RpcContext,
    params: ImportPolicyTomlParams,
) -> Result<Value> {
    let now = guarded_now(context, params.now.as_deref())?;
    reject_if_clock_tampered(context)?;
    let now_utc = now.with_timezone(&Utc);
    let imported = Config::from_toml_str(&params.toml).map_err(DaemonError::from)?;

    let mut core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    let mut next = core.config().clone();
    let summary = append_policy_config(&mut next, &imported)?;
    crate::app::ensure_mandatory_app_rules(&mut next);
    focus_core::validate_config(&next)?;

    persist_policy_config(context, &mut core, next)?;
    let hosts_repair = repair_hosts_after_policy_change(context, &core, now)?;
    core.database().record_event(
        "policy_appended",
        Some("policy"),
        Some(&format!(
            "TOML append;added={};rules={};app_rules={};schedules={};allowances={}{}",
            summary.total(),
            summary.rules,
            summary.app_rules,
            summary.schedules,
            summary.allowances,
            hosts_repair_detail(hosts_repair)
        )),
        now_utc,
    )?;

    Ok(json!({
        "status": "ok",
        "config": core.config(),
        "added": {
            "total": summary.total(),
            "rules": summary.rules,
            "app_rules": summary.app_rules,
            "schedules": summary.schedules,
            "allowances": summary.allowances
        },
        "updated_at": now_utc
    }))
}

fn persist_policy_config(context: &RpcContext, core: &mut FocusCore, next: Config) -> Result<()> {
    focus_core::validate_config(&next)?;
    let previous = core.config().clone();
    let now = Local::now().fixed_offset();
    core.database()
        .sync_schedule_activity_totals(&previous.schedules, now)?;
    if let Some(policy_recovery) = &context.policy_recovery {
        policy_recovery.write(&next)?;
    }
    if let Err(database_error) = core.database().replace_policy_config(&next) {
        if let Some(policy_recovery) = &context.policy_recovery {
            if let Err(recovery_error) = policy_recovery.write(&previous) {
                return Err(DaemonError::PolicyPersistenceRollback {
                    database_error: database_error.to_string(),
                    recovery_error: recovery_error.to_string(),
                });
            }
        }
        return Err(database_error.into());
    }
    core.replace_config(next)?;
    core.database()
        .sync_schedule_activity_totals(&core.config().schedules, now)?;
    rebaseline_schedule_notification_states(core, now)?;
    Ok(())
}

fn rebaseline_schedule_notification_states(
    core: &FocusCore,
    now: DateTime<FixedOffset>,
) -> Result<()> {
    let now_utc = now.with_timezone(&Utc);
    let existing = core.database().notification_lifecycle_states("schedule")?;
    for schedule in &core.config().schedules {
        let active =
            schedule_ids_are_active_at(std::slice::from_ref(&schedule.id), core.config(), now);
        core.database().set_notification_lifecycle_state(
            "schedule",
            &schedule.id,
            active,
            now_utc,
        )?;
    }
    for (schedule_id, _) in existing {
        if !core
            .config()
            .schedules
            .iter()
            .any(|schedule| schedule.id == schedule_id)
        {
            core.database()
                .delete_notification_lifecycle_state("schedule", &schedule_id)?;
        }
    }
    Ok(())
}

fn append_policy_config(current: &mut Config, imported: &Config) -> Result<PolicyAppendSummary> {
    let mut summary = PolicyAppendSummary::default();
    summary.allowances = append_unique_by_id(
        &mut current.allowances,
        &imported.allowances,
        "allowance",
        |allowance| allowance.id.as_str(),
    )?;
    summary.schedules = append_unique_by_id(
        &mut current.schedules,
        &imported.schedules,
        "schedule",
        |schedule| schedule.id.as_str(),
    )?;
    summary.rules =
        append_unique_by_id(&mut current.rules, &imported.rules, "site list", |rule| {
            rule.id.as_str()
        })?;
    summary.app_rules = append_unique_by_id(
        &mut current.app_rules,
        &imported.app_rules,
        "app rule",
        |rule| rule.id.as_str(),
    )?;
    Ok(summary)
}

fn append_unique_by_id<T, F>(
    current: &mut Vec<T>,
    imported: &[T],
    kind: &str,
    id: F,
) -> Result<usize>
where
    T: Clone + PartialEq,
    F: Fn(&T) -> &str,
{
    let mut added = 0;
    for item in imported {
        let item_id = id(item);
        match current.iter().find(|candidate| id(candidate) == item_id) {
            Some(existing) if existing == item => {}
            Some(_) => {
                return Err(DaemonError::InvalidRequest(format!(
                    "imported {kind} '{item_id}' conflicts with an existing {kind}"
                )));
            }
            None => {
                current.push(item.clone());
                added += 1;
            }
        }
    }
    Ok(added)
}

fn upsert_site_list_method(context: &RpcContext, params: UpsertSiteListParams) -> Result<Value> {
    let now = guarded_now(context, params.now.as_deref())?;
    reject_if_clock_tampered(context)?;
    let updated_at = Utc::now();
    let rule_id = params.rule.id.clone();
    let mut core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    let mut next = core.config().clone();
    let old_allowance_id = next
        .rules
        .iter()
        .find(|candidate| candidate.id == params.rule.id)
        .and_then(|rule| rule.allowance_id.clone());

    match next
        .rules
        .iter()
        .position(|candidate| candidate.id == params.rule.id)
    {
        Some(index) => {
            let current_rule = &next.rules[index];
            if current_rule != &params.rule
                && site_rule_in_active_detox(&core, &current_rule.id, now)?
                && !site_list_edit_is_additive(current_rule, &params.rule)
            {
                return Err(active_detox_site_list_edit_error(&current_rule.id));
            }
            if current_rule != &params.rule
                && rule_is_active_at(current_rule, core.config(), now)
                && !site_list_edit_is_additive(current_rule, &params.rule)
                && !active_tier1_site_list_edit_allowed(&core, current_rule, now)?
            {
                return Err(active_site_list_edit_error(&current_rule.id));
            }
            next.rules[index] = params.rule;
        }
        None => next.rules.push(params.rule),
    }
    let new_allowance_id = next
        .rules
        .iter()
        .find(|candidate| candidate.id == rule_id)
        .and_then(|rule| rule.allowance_id.clone());
    if old_allowance_id != new_allowance_id {
        remove_unreferenced_allowance(&mut next, old_allowance_id.as_deref());
    }

    focus_core::validate_config(&next)?;
    persist_policy_config(context, &mut core, next)?;
    let hosts_repair = repair_hosts_after_policy_change(context, &core, now)?;
    core.database().record_event(
        "site_list_saved",
        Some(&rule_id),
        Some(&format!(
            "GUI structured edit{}",
            hosts_repair_detail(hosts_repair)
        )),
        updated_at,
    )?;

    Ok(json!({
        "status": "ok",
        "config": core.config(),
        "updated_at": updated_at
    }))
}

fn delete_site_list_method(context: &RpcContext, params: DeleteSiteListParams) -> Result<Value> {
    let now = guarded_now(context, params.now.as_deref())?;
    reject_if_clock_tampered(context)?;
    let updated_at = Utc::now();
    let mut core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    let mut next = core.config().clone();
    let Some(index) = next
        .rules
        .iter()
        .position(|candidate| candidate.id == params.id)
    else {
        return Err(DaemonError::InvalidRequest(format!(
            "site list '{}' does not exist",
            params.id
        )));
    };
    let current_rule = &next.rules[index];
    if site_rule_in_active_detox(&core, &current_rule.id, now)? {
        return Err(active_detox_site_list_edit_error(&current_rule.id));
    }
    if rule_is_active_at(current_rule, core.config(), now) {
        return Err(active_site_list_edit_error(&current_rule.id));
    }
    let removed = next.rules.remove(index);
    remove_unreferenced_allowance(&mut next, removed.allowance_id.as_deref());

    focus_core::validate_config(&next)?;
    persist_policy_config(context, &mut core, next)?;
    let hosts_repair = repair_hosts_after_policy_change(context, &core, now)?;
    core.database().record_event(
        "site_list_deleted",
        Some(&removed.id),
        Some(&format!(
            "GUI structured edit{}",
            hosts_repair_detail(hosts_repair)
        )),
        updated_at,
    )?;

    Ok(json!({
        "status": "ok",
        "config": core.config(),
        "updated_at": updated_at
    }))
}

fn upsert_allowance_method(context: &RpcContext, params: UpsertAllowanceParams) -> Result<Value> {
    let now = guarded_now(context, params.now.as_deref())?;
    reject_if_clock_tampered(context)?;
    let updated_at = Utc::now();
    let allowance_id = params.allowance.id.clone();
    let mut core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    let mut next = core.config().clone();

    match next
        .allowances
        .iter()
        .position(|candidate| candidate.id == params.allowance.id)
    {
        Some(index) => {
            let current_allowance = &next.allowances[index];
            if current_allowance != &params.allowance
                && allowance_is_active_at(&current_allowance.id, &core, now)?
            {
                return Err(active_allowance_edit_error(&current_allowance.id));
            }
            next.allowances[index] = params.allowance;
        }
        None => next.allowances.push(params.allowance),
    }

    focus_core::validate_config(&next)?;
    persist_policy_config(context, &mut core, next)?;
    core.database().record_event(
        "allowance_saved",
        Some(&allowance_id),
        Some("GUI structured edit"),
        updated_at,
    )?;

    Ok(json!({
        "status": "ok",
        "config": core.config(),
        "updated_at": updated_at
    }))
}

fn delete_allowance_method(context: &RpcContext, params: DeleteAllowanceParams) -> Result<Value> {
    let now = guarded_now(context, params.now.as_deref())?;
    reject_if_clock_tampered(context)?;
    let updated_at = Utc::now();
    let mut core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    let mut next = core.config().clone();
    let Some(index) = next
        .allowances
        .iter()
        .position(|candidate| candidate.id == params.id)
    else {
        return Err(DaemonError::InvalidRequest(format!(
            "allowance '{}' does not exist",
            params.id
        )));
    };

    if allowance_is_active_at(&params.id, &core, now)? {
        return Err(active_allowance_edit_error(&params.id));
    }

    if let Some(rule) = next
        .rules
        .iter()
        .find(|rule| rule.allowance_id.as_deref() == Some(params.id.as_str()))
    {
        return Err(DaemonError::InvalidRequest(format!(
            "allowance '{}' is still used by site list '{}'",
            params.id, rule.id
        )));
    }

    if let Some(rule) = next
        .app_rules
        .iter()
        .find(|rule| rule.allowance_id.as_deref() == Some(params.id.as_str()))
    {
        return Err(DaemonError::InvalidRequest(format!(
            "allowance '{}' is still used by app rule '{}'",
            params.id, rule.id
        )));
    }

    let removed = next.allowances.remove(index);

    focus_core::validate_config(&next)?;
    persist_policy_config(context, &mut core, next)?;
    core.database().record_event(
        "allowance_deleted",
        Some(&removed.id),
        Some("GUI structured edit"),
        updated_at,
    )?;

    Ok(json!({
        "status": "ok",
        "config": core.config(),
        "updated_at": updated_at
    }))
}

fn upsert_app_rule_method(context: &RpcContext, params: UpsertAppRuleParams) -> Result<Value> {
    let now = guarded_now(context, params.now.as_deref())?;
    reject_if_clock_tampered(context)?;
    let updated_at = Utc::now();
    let rule_id = params.rule.id.clone();
    let mut core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    let mut next = core.config().clone();
    let old_allowance_id = next
        .app_rules
        .iter()
        .find(|candidate| candidate.id == params.rule.id)
        .and_then(|rule| rule.allowance_id.clone());

    match next
        .app_rules
        .iter()
        .position(|candidate| candidate.id == params.rule.id)
    {
        Some(index) => {
            let current_rule = &next.app_rules[index];
            if current_rule != &params.rule
                && app_rule_in_active_detox(&core, &current_rule.id, now)?
                && !app_rule_edit_is_additive(current_rule, &params.rule)
            {
                return Err(active_detox_app_rule_edit_error(&current_rule.id));
            }
            if current_rule != &params.rule
                && app_rule_is_active_at(current_rule, core.config(), now)
                && !app_rule_edit_is_additive(current_rule, &params.rule)
            {
                return Err(active_app_rule_edit_error(&current_rule.id));
            }
            next.app_rules[index] = params.rule;
        }
        None => next.app_rules.push(params.rule),
    }
    let new_allowance_id = next
        .app_rules
        .iter()
        .find(|candidate| candidate.id == rule_id)
        .and_then(|rule| rule.allowance_id.clone());
    if old_allowance_id != new_allowance_id {
        remove_unreferenced_allowance(&mut next, old_allowance_id.as_deref());
    }

    focus_core::validate_config(&next)?;
    persist_policy_config(context, &mut core, next)?;
    core.database().record_event(
        "app_rule_saved",
        Some(&rule_id),
        Some("GUI structured edit"),
        updated_at,
    )?;

    Ok(json!({
        "status": "ok",
        "config": core.config(),
        "updated_at": updated_at
    }))
}

fn delete_app_rule_method(context: &RpcContext, params: DeleteAppRuleParams) -> Result<Value> {
    let now = guarded_now(context, params.now.as_deref())?;
    reject_if_clock_tampered(context)?;
    let updated_at = Utc::now();
    let mut core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    let mut next = core.config().clone();
    let Some(index) = next
        .app_rules
        .iter()
        .position(|candidate| candidate.id == params.id)
    else {
        return Err(DaemonError::InvalidRequest(format!(
            "app rule '{}' does not exist",
            params.id
        )));
    };
    let current_rule = &next.app_rules[index];
    if app_rule_in_active_detox(&core, &current_rule.id, now)? {
        return Err(active_detox_app_rule_edit_error(&current_rule.id));
    }
    if app_rule_is_active_at(current_rule, core.config(), now) {
        return Err(active_app_rule_edit_error(&current_rule.id));
    }
    let removed = next.app_rules.remove(index);
    remove_unreferenced_allowance(&mut next, removed.allowance_id.as_deref());

    focus_core::validate_config(&next)?;
    persist_policy_config(context, &mut core, next)?;
    core.database().record_event(
        "app_rule_deleted",
        Some(&removed.id),
        Some("GUI structured edit"),
        updated_at,
    )?;

    Ok(json!({
        "status": "ok",
        "config": core.config(),
        "updated_at": updated_at
    }))
}

fn upsert_schedule_method(context: &RpcContext, params: UpsertScheduleParams) -> Result<Value> {
    let now = guarded_now(context, params.now.as_deref())?;
    reject_if_clock_tampered(context)?;
    let updated_at = Utc::now();
    let schedule_id = params.schedule.id.clone();
    let mut core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    let mut next = core.config().clone();
    let site_rule_ids = params.site_rule_ids.map(normalized_unique_ids);
    let app_rule_ids = params.app_rule_ids.map(normalized_unique_ids);

    if let Some(rule_ids) = site_rule_ids.as_deref() {
        validate_detox_targets(&next, rule_ids, &[])?;
    }
    if let Some(rule_ids) = app_rule_ids.as_deref() {
        validate_detox_targets(&next, &[], rule_ids)?;
    }

    let target_updates_change = schedule_target_updates_change(
        &next,
        &schedule_id,
        site_rule_ids.as_deref(),
        app_rule_ids.as_deref(),
    );

    match next
        .schedules
        .iter()
        .position(|candidate| candidate.id == params.schedule.id)
    {
        Some(index) => {
            let current_schedule = &next.schedules[index];
            if schedule_is_active_at(current_schedule, now)
                && ((current_schedule != &params.schedule
                    && !schedule_edit_is_additive(current_schedule, &params.schedule))
                    || target_updates_change)
            {
                return Err(active_schedule_edit_error(&current_schedule.id));
            }
            next.schedules[index] = params.schedule;
        }
        None => next.schedules.push(params.schedule),
    }

    apply_schedule_target_updates(
        &core,
        core.config(),
        &mut next,
        &schedule_id,
        site_rule_ids.as_deref(),
        app_rule_ids.as_deref(),
        now,
    )?;

    focus_core::validate_config(&next)?;
    persist_policy_config(context, &mut core, next)?;
    core.database().record_event(
        "schedule_saved",
        Some(&schedule_id),
        Some("GUI structured edit"),
        updated_at,
    )?;

    Ok(json!({
        "status": "ok",
        "config": core.config(),
        "updated_at": updated_at
    }))
}

fn delete_schedule_method(context: &RpcContext, params: DeleteScheduleParams) -> Result<Value> {
    let now = guarded_now(context, params.now.as_deref())?;
    reject_if_clock_tampered(context)?;
    let updated_at = Utc::now();
    let mut core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    let mut next = core.config().clone();
    let Some(index) = next
        .schedules
        .iter()
        .position(|candidate| candidate.id == params.id)
    else {
        return Err(DaemonError::InvalidRequest(format!(
            "schedule '{}' does not exist",
            params.id
        )));
    };
    let current_schedule = &next.schedules[index];
    if schedule_is_active_at(current_schedule, now) {
        return Err(active_schedule_edit_error(&current_schedule.id));
    }
    let removed = next.schedules.remove(index);

    focus_core::validate_config(&next)?;
    persist_policy_config(context, &mut core, next)?;
    core.database().record_event(
        "schedule_deleted",
        Some(&removed.id),
        Some("GUI structured edit"),
        updated_at,
    )?;

    Ok(json!({
        "status": "ok",
        "config": core.config(),
        "updated_at": updated_at
    }))
}

fn start_detox_method(context: &RpcContext, params: StartDetoxParams) -> Result<Value> {
    let now = guarded_now(context, params.now.as_deref())?;
    reject_if_clock_tampered(context)?;
    let now_utc = now.with_timezone(&Utc);
    if params.duration_minutes == 0 {
        return Err(DaemonError::InvalidRequest(
            "detox duration must be at least one minute".to_string(),
        ));
    }
    if params.duration_minutes > MAX_DETOX_DURATION_MINUTES {
        return Err(DaemonError::InvalidRequest(format!(
            "detox duration cannot exceed {MAX_DETOX_DURATION_MINUTES} minutes"
        )));
    }

    let site_rule_ids = normalized_unique_ids(params.site_rule_ids);
    let app_rule_ids = normalized_unique_ids(params.app_rule_ids);
    if site_rule_ids.is_empty() && app_rule_ids.is_empty() {
        return Err(DaemonError::InvalidRequest(
            "detox needs at least one site list or app rule".to_string(),
        ));
    }

    let name = params
        .name
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty());
    let ends_at = now_utc + Duration::minutes(i64::from(params.duration_minutes));
    let (session, hosts_repair) = {
        let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;

        validate_detox_targets(core.config(), &site_rule_ids, &app_rule_ids)?;
        let session = DetoxSession {
            id: next_detox_session_id(&core, now_utc)?,
            name,
            starts_at: now_utc,
            ends_at,
            cancelled_at: None,
            site_rule_ids,
            app_rule_ids,
        };
        let session = core.database().insert_detox_session(&session)?;
        core.database().record_event(
            "detox_started",
            Some(&session.id),
            Some(&format!(
                "duration_minutes={};site_rules={};app_rules={}",
                params.duration_minutes,
                session.site_rule_ids.join(","),
                session.app_rule_ids.join(",")
            )),
            now_utc,
        )?;
        if let Err(error) = enqueue_detox_notification(&core, &session, true, false, now_utc) {
            eprintln!("could not queue Detox start notification: {error}");
        }
        let hosts_repair = repair_hosts_after_policy_change(context, &core, now)?;
        (session, hosts_repair)
    };
    let chromium_policy_repairs = repair_chromium_policies_for_current_settings(context)?;

    Ok(json!({
        "status": "ok",
        "session": detox_session_to_json(&session, now_utc),
        "hosts_repair": hosts_repair.map(|status| format!("{status:?}")),
        "chromium_policy_repairs": chromium_policy_repairs
            .into_iter()
            .map(|(browser, status)| (browser, format!("{status:?}")))
            .collect::<BTreeMap<_, _>>(),
    }))
}

fn cancel_detox_method(context: &RpcContext, params: CancelDetoxParams) -> Result<Value> {
    let now = guarded_now(context, params.now.as_deref())?;
    reject_if_clock_tampered(context)?;
    let now_utc = now.with_timezone(&Utc);
    let (session, hosts_repair) = {
        let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
        let Some(session) = core.database().detox_session(&params.id)? else {
            return Err(DaemonError::InvalidRequest(format!(
                "detox session '{}' does not exist",
                params.id
            )));
        };
        if session.cancelled_at.is_some() {
            return Err(DaemonError::InvalidRequest(format!(
                "detox session '{}' is already cancelled",
                params.id
            )));
        }
        if session.ends_at <= now_utc {
            return Err(DaemonError::InvalidRequest(format!(
                "detox session '{}' has already ended",
                params.id
            )));
        }
        if !tier1_edit_window_active(&core, now)? {
            return Err(DaemonError::InvalidRequest(
                "Tier 1 edit unlock is required to cancel detox".to_string(),
            ));
        }

        let Some(session) = core.database().cancel_detox_session(&params.id, now_utc)? else {
            return Err(DaemonError::InvalidRequest(format!(
                "detox session '{}' does not exist",
                params.id
            )));
        };
        core.database().record_event(
            "detox_cancelled",
            Some(&session.id),
            Some("cancelled through privileged Tier 1 edit unlock"),
            now_utc,
        )?;
        if let Err(error) = enqueue_detox_notification(&core, &session, false, true, now_utc) {
            eprintln!("could not queue Detox end notification: {error}");
        }
        let hosts_repair = repair_hosts_after_policy_change(context, &core, now)?;
        (session, hosts_repair)
    };
    let chromium_policy_repairs = repair_chromium_policies_for_current_settings(context)?;

    Ok(json!({
        "status": "ok",
        "session": detox_session_to_json(&session, now_utc),
        "hosts_repair": hosts_repair.map(|status| format!("{status:?}")),
        "chromium_policy_repairs": chromium_policy_repairs
            .into_iter()
            .map(|(browser, status)| (browser, format!("{status:?}")))
            .collect::<BTreeMap<_, _>>(),
    }))
}

fn detox_sessions_method(context: &RpcContext, params: DetoxSessionsParams) -> Result<Value> {
    let now = guarded_now(context, params.now.as_deref())?;
    let now_utc = now.with_timezone(&Utc);
    let limit = params.limit.clamp(1, 200);
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    let sessions = if params.active_only {
        core.database().active_detox_sessions(now_utc)?
    } else {
        core.database().detox_sessions(limit)?
    };
    Ok(json!({
        "sessions": sessions
            .iter()
            .map(|session| detox_session_to_json(session, now_utc))
            .collect::<Vec<_>>()
    }))
}

fn log_summary(context: &RpcContext) -> Result<Value> {
    let contents = match fs::read_to_string(&context.event_log_path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.into()),
    };

    let mut event_counts = BTreeMap::<String, u64>::new();
    for line in contents.lines() {
        if let Some(kind) = event_kind_from_log_line(line) {
            *event_counts.entry(kind).or_default() += 1;
        }
    }
    let total_events = event_counts.values().sum::<u64>();

    Ok(json!({
        "path": context.event_log_path.display().to_string(),
        "total_events": total_events,
        "event_counts": event_counts,
    }))
}

fn event_kind_from_log_line(line: &str) -> Option<String> {
    let kind = line
        .split_whitespace()
        .find_map(|field| field.strip_prefix("kind="))?
        .strip_prefix('"')?
        .strip_suffix('"')?;
    (!kind.is_empty()).then(|| kind.to_string())
}

fn running_apps_method(context: &RpcContext, params: ListRunningAppsParams) -> Result<Value> {
    let guarded = guarded_now_with_status(context, params.now.as_deref())?;
    let mut processes = scan_procfs(Path::new("/proc"))?;
    let window_snapshot = attach_detected_window_titles(&mut processes);

    Ok(serde_json::to_value(RunningAppsResponse {
        apps: running_app_snapshots_from_processes(
            context,
            &processes,
            guarded.now,
            guarded.integrity.state == "tampered",
        )?,
        window_detection: window_snapshot.support,
    })?)
}

fn running_app_snapshots_from_processes(
    context: &RpcContext,
    processes: &[ProcessInfo],
    now: DateTime<FixedOffset>,
    clock_tampered: bool,
) -> Result<Vec<RunningAppSnapshot>> {
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    let unsupported_browser_block_active =
        unsupported_browser_block_is_active(&core, now, clock_tampered)?;
    let config_without_inactive_browser_block = (!unsupported_browser_block_active).then(|| {
        let mut config = core.config().clone();
        config
            .app_rules
            .retain(|rule| rule.id != UNSUPPORTED_BROWSER_RULE_ID);
        config
    });
    let evaluation_config = config_without_inactive_browser_block
        .as_ref()
        .unwrap_or_else(|| core.config());
    let eval_context = EvaluationContext::new(evaluation_config, core.database(), now)
        .with_clock_tampered(clock_tampered);
    let mut apps = processes
        .iter()
        .filter(|process| process.pid > 1 && process.pid != std::process::id())
        .filter_map(|process| {
            let identity = process.identity();
            if is_blockuntu_process(&identity) {
                return None;
            }
            running_app_snapshot_from_identity(process.pid, identity, &eval_context)
        })
        .collect::<Vec<_>>();

    apps.sort_by(|left, right| {
        left.display_name
            .to_ascii_lowercase()
            .cmp(&right.display_name.to_ascii_lowercase())
            .then(left.pid.cmp(&right.pid))
    });

    Ok(apps)
}

fn running_app_snapshot_from_identity(
    pid: u32,
    identity: focus_core::ProcessIdentity,
    context: &EvaluationContext<'_>,
) -> Option<RunningAppSnapshot> {
    let display_name = running_app_display_name(&identity)?;
    let decision = evaluate_app(&identity, context);
    let (decision_label, blocking_rule_id, blocking_rule_name) =
        running_app_decision_details(&decision);

    Some(RunningAppSnapshot {
        pid,
        display_name,
        executable_path: identity.executable_path,
        executable_basename: identity.executable_basename,
        command_name: identity.command_name,
        desktop_id: identity.desktop_id,
        window_titles: identity.window_titles,
        decision: decision_label,
        blocking_rule_id,
        blocking_rule_name,
    })
}

fn running_app_display_name(process: &focus_core::ProcessIdentity) -> Option<String> {
    process
        .command_name
        .clone()
        .or_else(|| process.executable_basename.clone())
        .or_else(|| process.desktop_id.clone())
        .or_else(|| process.window_titles.first().cloned())
}

fn running_app_decision_details(
    decision: &Decision,
) -> (&'static str, Option<String>, Option<String>) {
    match decision {
        Decision::Allow => ("allow", None, None),
        Decision::Block(BlockReason::HardBlock { rule_id, rule_name }) => {
            ("block", Some(rule_id.clone()), Some(rule_name.clone()))
        }
        Decision::Block(BlockReason::ScheduledBlock { rule_id, rule_name }) => {
            ("block", Some(rule_id.clone()), Some(rule_name.clone()))
        }
        Decision::Block(BlockReason::ControlledAccess {
            rule_id, rule_name, ..
        }) => ("block", Some(rule_id.clone()), Some(rule_name.clone())),
        Decision::Block(BlockReason::Detox {
            rule_id, rule_name, ..
        }) => ("block", Some(rule_id.clone()), Some(rule_name.clone())),
        Decision::Block(BlockReason::InvalidUrl { .. } | BlockReason::RuntimeError { .. }) => {
            ("block", None, None)
        }
    }
}

fn evaluate_url_method(context: &RpcContext, params: EvaluateUrlParams) -> Result<Value> {
    let guarded = guarded_now_with_status(context, params.now.as_deref())?;
    let now = guarded.now;
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    if !enforcement_active_from_core(&core)? {
        return Ok(json!({
            "decision": "allow",
            "metering_active": false,
            "enforcement_state": ENFORCEMENT_UNINSTALLING
        }));
    }
    let eval_context = EvaluationContext::new(core.config(), core.database(), now)
        .with_clock_tampered(guarded.integrity.state == "tampered");
    let decision = evaluate_url(&params.url, &eval_context);

    if decision.is_block() && !params.probe {
        core.database().record_event(
            "url_blocked",
            Some(&params.url),
            Some(&format!("{decision:?}")),
            now.with_timezone(&Utc),
        )?;
        if let Err(error) = enqueue_website_block_notification(
            &core,
            &params.url,
            &decision,
            now.with_timezone(&Utc),
        ) {
            eprintln!("could not queue website block notification: {error}");
        }
    }

    let metering_active = decision.is_allow() && site_usage_is_metered(&params.url, &eval_context);
    let mut response = decision_to_json(&decision, core.config(), now);
    if let Decision::Block(BlockReason::ControlledAccess {
        rule_id, reason, ..
    }) = &decision
    {
        let detox_sessions = if guarded.integrity.state == "tampered" {
            core.database().uncancelled_detox_sessions()?
        } else {
            core.database()
                .active_detox_sessions(now.with_timezone(&Utc))?
        };
        if let Some(session) = detox_sessions
            .iter()
            .filter(|session| session.site_rule_ids.iter().any(|id| id == rule_id))
            .max_by_key(|session| session.ends_at.timestamp_micros())
        {
            decorate_controlled_detox_reason(
                &mut response,
                rule_id,
                reason,
                session,
                core.config(),
                now,
            );
        }
    }
    if let Some(response) = response.as_object_mut() {
        response.insert("metering_active".to_string(), json!(metering_active));
    }
    Ok(response)
}

fn decorate_controlled_detox_reason(
    response: &mut Value,
    rule_id: &str,
    controlled_reason: &ControlledBlockReason,
    session: &DetoxSession,
    config: &Config,
    now: DateTime<FixedOffset>,
) {
    let Some(reason) = response.get_mut("reason").and_then(Value::as_object_mut) else {
        return;
    };
    let schedule_end = config
        .rules
        .iter()
        .find(|rule| rule.id == rule_id)
        .and_then(|rule| rule_schedule_inactive_at(rule, config, now));
    let detox_end = session.ends_at.with_timezone(now.offset());
    let activation_end = schedule_end
        .map(|schedule_end| schedule_end.max(detox_end))
        .unwrap_or(detox_end);
    let free_at = if matches!(controlled_reason, ControlledBlockReason::AllowanceExhausted) {
        activation_end.min(next_allowance_reset_at(now))
    } else {
        activation_end
    };

    reason.insert(
        "blocked_by".to_string(),
        json!(if schedule_end.is_some() {
            "schedule_and_detox"
        } else {
            "detox"
        }),
    );
    reason.insert("session_id".to_string(), json!(session.id));
    reason.insert("session_name".to_string(), json!(session.name));
    reason.insert("detox_ends_at".to_string(), json!(session.ends_at));
    reason.insert("free_at".to_string(), json!(free_at.to_rfc3339()));
}

fn request_unlock_method(context: &RpcContext, params: RequestUnlockParams) -> Result<Value> {
    let now = guarded_now(context, params.now.as_deref())?;
    reject_if_clock_tampered(context)?;
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    let eval_context = EvaluationContext::new(core.config(), core.database(), now);
    let unlock = request_unlock(&params.target, params.reason, &eval_context)?;
    Ok(unlock_to_json(&unlock))
}

fn unlock_tier1_edit_method(context: &RpcContext, params: Tier1EditUnlockParams) -> Result<Value> {
    let now = guarded_now(context, params.now.as_deref())?;
    reject_if_clock_tampered(context)?;
    let now_utc = now.with_timezone(&Utc);
    let phrase = params.phrase.trim();
    if phrase.is_empty() {
        return Err(DaemonError::InvalidRequest(
            "Tier 1 credential is required".to_string(),
        ));
    }
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    if !tier1_edit_credential_matches(&core, phrase)? {
        return Err(DaemonError::InvalidRequest(
            "Tier 1 edit credential does not match".to_string(),
        ));
    }
    let protected_access_mode = protected_access_mode(&core)?;
    if !protected_access_is_open(&core, now, false)? {
        return Err(protected_access_closed_error(
            "Tier 1 edits",
            protected_access_mode,
        ));
    }

    let expires_at = now_utc + Duration::minutes(TIER1_EDIT_UNLOCK_MINUTES);
    core.database().set_service_state(
        TIER1_EDIT_UNLOCK_UNTIL_KEY,
        &expires_at.to_rfc3339(),
        now_utc,
    )?;
    core.database().record_event(
        "tier1_edit_unlocked",
        Some("site_lists"),
        Some("Tier 1 site-list edits unlocked for 5 minutes"),
        now_utc,
    )?;

    tier1_edit_status_json(&core, now)
}

fn configure_tier1_edit_credential_method(
    context: &RpcContext,
    params: ConfigureTier1EditCredentialParams,
) -> Result<Value> {
    let phrase = params.phrase.trim();
    if phrase.chars().count() < 16 {
        return Err(DaemonError::InvalidRequest(
            "Tier 1 credential must contain at least 16 characters".to_string(),
        ));
    }
    let now = Utc::now();
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    if tier1_edit_credential_configured(&core)? {
        return Err(DaemonError::InvalidRequest(
            "Tier 1 credential is already configured".to_string(),
        ));
    }
    let salt = random_credential_salt()?;
    core.database()
        .set_service_state(TIER1_EDIT_CREDENTIAL_SALT_KEY, &salt, now)?;
    core.database().set_service_state(
        TIER1_EDIT_CREDENTIAL_HASH_KEY,
        &tier1_edit_credential_hash(&salt, phrase),
        now,
    )?;
    core.database().record_event(
        "tier1_edit_credential_configured",
        Some("site_lists"),
        Some("Daemon-owned Tier 1 credential configured"),
        now,
    )?;
    Ok(json!({ "configured": true }))
}

fn set_operator_window_restriction_method(
    context: &RpcContext,
    params: SetOperatorWindowRestrictionParams,
) -> Result<Value> {
    let mode = if params.enabled {
        ProtectedAccessMode::Sunday
    } else {
        ProtectedAccessMode::AllTime
    };
    set_protected_access_mode(context, mode)?;
    Ok(json!({ "enabled": params.enabled }))
}

fn set_protected_access_mode_method(
    context: &RpcContext,
    params: SetProtectedAccessModeParams,
) -> Result<Value> {
    set_protected_access_mode(context, params.mode)?;
    Ok(json!({ "mode": params.mode }))
}

fn set_protected_access_mode(context: &RpcContext, mode: ProtectedAccessMode) -> Result<()> {
    let now = Utc::now();
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    for key in [
        PROTECTED_ACCESS_MODE_KEY,
        UNSUPPORTED_BROWSER_BLOCK_MODE_KEY,
        CHROMIUM_INCOGNITO_CHANGE_ACCESS_MODE_KEY,
    ] {
        core.database()
            .set_service_state(key, protected_access_mode_name(mode), now)?;
    }
    core.database().record_event(
        "protected_access_mode_updated",
        Some("protected_changes"),
        Some(&format!(
            "Tier 1 edits, uninstall, unsupported-browser blocking, and Chromium private-browsing settings are available {}",
            protected_access_mode_description(mode)
        )),
        now,
    )?;
    Ok(())
}

fn set_unsupported_browser_block_mode_method(
    context: &RpcContext,
    params: SetUnsupportedBrowserBlockModeParams,
) -> Result<Value> {
    set_protected_access_mode(context, params.mode)?;
    let now = Utc::now();
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    let active = unsupported_browser_block_is_active(&core, now.fixed_offset(), false)?;
    Ok(json!({ "mode": params.mode, "active": active }))
}

fn set_chromium_incognito_mode_method(
    context: &RpcContext,
    params: SetChromiumIncognitoModeParams,
) -> Result<Value> {
    let guarded = guarded_now_with_status(context, None)?;
    let now = guarded.now;
    let settings = {
        let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
        ensure_chromium_incognito_settings_change_allowed(
            &core,
            now,
            guarded.integrity.state == "tampered",
        )?;
        let settings = chromium_incognito_policy_settings(
            &core,
            now,
            guarded.integrity.state == "tampered",
            params.mode,
        )?;
        ensure_chromium_incognito_url_blocklist_within_limit(&settings)?;
        core.database().set_service_state(
            CHROMIUM_INCOGNITO_MODE_KEY,
            chromium_incognito_mode_name(params.mode),
            now.with_timezone(&Utc),
        )?;
        core.database().record_event(
            "chromium_incognito_mode_updated",
            Some("browser_policy"),
            Some(&format!(
                "Chromium private browsing mode set to {}; {} URL policy pattern(s), {} unsupported pattern(s)",
                chromium_incognito_mode_name(params.mode),
                settings.url_blocklist.len(),
                settings.unsupported_pattern_count
            )),
            now.with_timezone(&Utc),
        )?;
        settings
    };

    let policy_repair = repair_chromium_policies_for_settings_change(context, &settings)?;
    Ok(json!({
        "mode": params.mode,
        "url_block_count": settings.url_blocklist.len(),
        "unsupported_pattern_count": settings.unsupported_pattern_count,
        "policy_repair": policy_repair
            .into_iter()
            .map(|(browser, status)| (browser, format!("{status:?}")))
            .collect::<BTreeMap<_, _>>(),
    }))
}

fn set_chromium_incognito_disable_scope_method(
    context: &RpcContext,
    params: SetChromiumIncognitoDisableScopeParams,
) -> Result<Value> {
    let guarded = guarded_now_with_status(context, None)?;
    let now = guarded.now;
    let settings = {
        let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
        ensure_chromium_incognito_settings_change_allowed(
            &core,
            now,
            guarded.integrity.state == "tampered",
        )?;
        core.database().set_service_state(
            CHROMIUM_INCOGNITO_DISABLE_SCOPE_KEY,
            chromium_incognito_disable_scope_name(params.scope),
            now.with_timezone(&Utc),
        )?;
        let settings = current_chromium_incognito_policy_settings(
            &core,
            now,
            guarded.integrity.state == "tampered",
        )?;
        core.database().record_event(
            "chromium_incognito_disable_scope_updated",
            Some("browser_policy"),
            Some(&format!(
                "Chromium private browsing is disabled {}",
                chromium_incognito_disable_scope_description(params.scope)
            )),
            now.with_timezone(&Utc),
        )?;
        settings
    };

    let policy_repair = repair_chromium_policies_for_settings_change(context, &settings)?;
    Ok(json!({
        "scope": params.scope,
        "private_browsing_disabled": settings.private_browsing_disabled,
        "policy_repair": policy_repair
            .into_iter()
            .map(|(browser, status)| (browser, format!("{status:?}")))
            .collect::<BTreeMap<_, _>>(),
    }))
}

fn set_chromium_incognito_change_access_mode_method(
    context: &RpcContext,
    params: SetChromiumIncognitoChangeAccessModeParams,
) -> Result<Value> {
    set_protected_access_mode(context, params.mode)?;
    Ok(json!({ "mode": params.mode }))
}

fn hide_recovery_credentials_method(
    context: &RpcContext,
    _params: HideRecoveryCredentialsParams,
) -> Result<Value> {
    for path in [TIER1_EDIT_KEY_PATH, SYSTEM_UNINSTALL_RECOVERY_PHRASE_PATH] {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    fs::create_dir_all("/var/lib/blockuntu")?;
    fs::write(RECOVERY_CREDENTIALS_HIDDEN_MARKER_PATH, b"hidden\n")?;
    let now = Utc::now();
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    core.database()
        .set_service_state(TIER1_EDIT_CREDENTIAL_SALT_KEY, "", now)?;
    core.database()
        .set_service_state(TIER1_EDIT_CREDENTIAL_HASH_KEY, "", now)?;
    core.database().record_event(
        "recovery_credentials_hidden",
        Some("protected_changes"),
        Some("Removed /etc/blockuntu recovery credentials at the user's request"),
        now,
    )?;
    Ok(json!({ "hidden": true }))
}

fn tier1_edit_status_method(context: &RpcContext, params: Tier1EditStatusParams) -> Result<Value> {
    let now = guarded_now(context, params.now.as_deref())?;
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    tier1_edit_status_json(&core, now)
}

fn record_visit_start_method(
    context: &RpcContext,
    params: RecordVisitStartParams,
) -> Result<Value> {
    let guarded = guarded_now_with_status(context, params.now.as_deref())?;
    let now = guarded.now;
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    let eval_context = EvaluationContext::new(core.config(), core.database(), now)
        .with_clock_tampered(guarded.integrity.state == "tampered");
    let visit = record_visit_start(&params.url, &params.tab_id, &eval_context)?;
    Ok(visit_to_json(&visit))
}

fn record_visit_heartbeat_method(context: &RpcContext, params: VisitIdParams) -> Result<Value> {
    let guarded = guarded_now_with_status(context, params.now.as_deref())?;
    let now = guarded.now;
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    let eval_context = EvaluationContext::new(core.config(), core.database(), now)
        .with_clock_tampered(guarded.integrity.state == "tampered");
    record_visit_heartbeat(params.visit_id, &eval_context)?;
    Ok(json!({ "status": "ok" }))
}

fn record_visit_end_method(context: &RpcContext, params: VisitIdParams) -> Result<Value> {
    let guarded = guarded_now_with_status(context, params.now.as_deref())?;
    let now = guarded.now;
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    let eval_context = EvaluationContext::new(core.config(), core.database(), now)
        .with_clock_tampered(guarded.integrity.state == "tampered");
    record_visit_end(params.visit_id, &eval_context)?;
    Ok(json!({ "status": "ok" }))
}

fn extension_heartbeat_method(
    context: &RpcContext,
    params: ExtensionHeartbeatParams,
) -> Result<Value> {
    let now = guarded_now(context, params.now.as_deref())?;
    let reported_component = extension_component(
        params.component.as_deref(),
        params.browser.as_deref(),
        params.extension_id.as_deref(),
    );
    let component = resolve_extension_heartbeat_component(reported_component)?;
    let details = json!({
        "browser": params.browser,
        "extension_id": params.extension_id,
        "extension_version": params.extension_version,
        "reported_component": reported_component,
        "resolved_component": component,
        "identity_resolution": if component == reported_component { "reported" } else { "process_inferred" }
    });
    let (enforcement_state, browser_extension_mode) = {
        let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
        core.database().upsert_heartbeat(
            &component,
            Some(&details.to_string()),
            now.with_timezone(&Utc),
        )?;
        (
            enforcement_state_from_core(&core)?,
            browser_extension_mode_from_core(&core)?,
        )
    };

    let policy_repair = repair_deferred_policy_after_heartbeat(context, &component)?;
    Ok(json!({
        "status": "ok",
        "enforcement_state": enforcement_state,
        "browser_extension_mode": browser_extension_mode,
        "policy_repair": policy_repair
    }))
}

fn extension_status_method(context: &RpcContext, params: ExtensionStatusParams) -> Result<Value> {
    let now = guarded_now(context, params.now.as_deref())?.with_timezone(&Utc);
    let component = extension_component(params.component.as_deref(), None, None);
    let browser_running = extension_browser_running(component)?;
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    browser_extension_status_from_core(
        &core,
        component,
        extension_heartbeat_timeout_seconds(context, component),
        now,
        browser_running,
    )
}

fn browser_extension_status_from_core(
    core: &FocusCore,
    component: &str,
    heartbeat_timeout_seconds: u64,
    now: DateTime<Utc>,
    browser_running: bool,
) -> Result<Value> {
    let heartbeat = core.database().heartbeat(component)?;
    let browser = supported_browser_for_extension_component(component);
    let session_started_at = if browser_running {
        browser
            .map(|browser| strict_browser_session_started_at(core.database(), browser))
            .transpose()?
            .flatten()
    } else {
        None
    };
    Ok(browser_extension_status_json(
        heartbeat.as_ref(),
        component,
        heartbeat_timeout_seconds,
        now,
        browser_running,
        session_started_at,
        browser_startup_grace_seconds(&core.config().strict_mode),
    ))
}

fn browser_extension_status_json(
    heartbeat: Option<&HeartbeatState>,
    component: &str,
    heartbeat_timeout_seconds: u64,
    now: DateTime<Utc>,
    browser_running: bool,
    session_started_at: Option<DateTime<Utc>>,
    startup_grace_seconds: i64,
) -> Value {
    let browser_label = browser_name_for_component(component);
    let heartbeat_age_seconds = heartbeat.map(|heartbeat| {
        now.signed_duration_since(heartbeat.last_seen_at)
            .num_seconds()
            .max(0)
    });
    let details = heartbeat.and_then(|heartbeat| heartbeat_details(heartbeat.details.as_deref()));
    let extension_id = details
        .as_ref()
        .and_then(|details| details.get("extension_id"))
        .and_then(Value::as_str);
    let extension_version = details
        .as_ref()
        .and_then(|details| details.get("extension_version"))
        .and_then(Value::as_str);
    let browser_from_heartbeat = details
        .as_ref()
        .and_then(|details| details.get("browser"))
        .and_then(Value::as_str);
    let heartbeat_is_for_current_session = heartbeat
        .zip(session_started_at)
        .is_some_and(|(heartbeat, started_at)| heartbeat.last_seen_at >= started_at);
    let startup_elapsed_seconds = session_started_at
        .map(|started_at| now.signed_duration_since(started_at).num_seconds().max(0));
    let startup_grace_remaining_seconds =
        startup_elapsed_seconds.map(|elapsed| (startup_grace_seconds - elapsed).max(0));

    let (state, detail) = if !browser_running {
        (
            "inactive",
            format!("{browser_label} is not running; no extension heartbeat is expected"),
        )
    } else if !heartbeat_is_for_current_session {
        let within_startup_grace = startup_elapsed_seconds
            .map(|elapsed| elapsed <= startup_grace_seconds)
            .unwrap_or(true);
        if within_startup_grace {
            let remaining_seconds =
                startup_grace_remaining_seconds.unwrap_or(startup_grace_seconds);
            (
                "starting",
                format!(
                    "{browser_label} is starting; waiting for an extension heartbeat from this launch ({remaining_seconds} second(s) remaining)"
                ),
            )
        } else {
            (
                "missing",
                format!(
                    "{browser_label} is running, but no extension heartbeat from this launch arrived within the {startup_grace_seconds}-second startup grace period"
                ),
            )
        }
    } else {
        let age_seconds = heartbeat_age_seconds.expect("current-session heartbeat has an age");
        if age_seconds <= heartbeat_timeout_seconds as i64 {
            (
                "active",
                format!(
                    "current-session heartbeat received {age_seconds} second(s) ago; extension installation and enabled state are confirmed"
                ),
            )
        } else {
            (
                "stale",
                format!(
                    "{browser_label} is running, but its current-session heartbeat is {age_seconds} second(s) old (extension fail-closed timeout: {heartbeat_timeout_seconds} seconds)"
                ),
            )
        }
    };

    json!({
        "state": state,
        "component": component,
        "installed_enabled": if heartbeat.is_some() { "confirmed" } else { "unconfirmed" },
        "last_seen_at": heartbeat.map(|heartbeat| heartbeat.last_seen_at),
        "age_seconds": heartbeat_age_seconds,
        "heartbeat_timeout_seconds": heartbeat_timeout_seconds,
        "browser": browser_from_heartbeat.unwrap_or(browser_label),
        "browser_running": browser_running,
        "session_started_at": session_started_at,
        "current_session_heartbeat": heartbeat_is_for_current_session,
        "startup_grace_seconds": startup_grace_seconds,
        "startup_grace_remaining_seconds": if state == "starting" {
            startup_grace_remaining_seconds.unwrap_or(startup_grace_seconds)
        } else {
            0
        },
        "extension_id": extension_id,
        "extension_version": extension_version,
        "detail": detail
    })
}

fn extension_browser_running(component: &str) -> Result<bool> {
    let Some(browser) = supported_browser_for_extension_component(component) else {
        return Ok(false);
    };
    Ok(scan_procfs(Path::new("/proc"))?
        .iter()
        .any(|process| supported_browser_for_process(&process.identity()) == Some(browser)))
}

fn supported_browser_for_extension_component(component: &str) -> Option<SupportedBrowser> {
    match component {
        FIREFOX_EXTENSION_HEARTBEAT_COMPONENT => Some(SupportedBrowser::Firefox),
        LIBREWOLF_EXTENSION_HEARTBEAT_COMPONENT => Some(SupportedBrowser::LibreWolf),
        WATERFOX_EXTENSION_HEARTBEAT_COMPONENT => Some(SupportedBrowser::Waterfox),
        CHROME_EXTENSION_HEARTBEAT_COMPONENT => Some(SupportedBrowser::Chrome),
        CHROMIUM_EXTENSION_HEARTBEAT_COMPONENT => Some(SupportedBrowser::Chromium),
        BRAVE_EXTENSION_HEARTBEAT_COMPONENT => Some(SupportedBrowser::Brave),
        OPERA_EXTENSION_HEARTBEAT_COMPONENT => Some(SupportedBrowser::Opera),
        EDGE_EXTENSION_HEARTBEAT_COMPONENT => Some(SupportedBrowser::Edge),
        VIVALDI_EXTENSION_HEARTBEAT_COMPONENT => Some(SupportedBrowser::Vivaldi),
        _ => None,
    }
}

fn extension_component<'a>(
    component: Option<&'a str>,
    browser: Option<&str>,
    extension_id: Option<&str>,
) -> &'a str {
    if let Some(component) = component {
        if supported_browser_for_extension_component(component).is_some() {
            return component;
        }
    }

    if let Some(browser) = browser {
        let component = match browser.trim().to_ascii_lowercase().as_str() {
            "firefox" => Some(FIREFOX_EXTENSION_HEARTBEAT_COMPONENT),
            "librewolf" => Some(LIBREWOLF_EXTENSION_HEARTBEAT_COMPONENT),
            "waterfox" => Some(WATERFOX_EXTENSION_HEARTBEAT_COMPONENT),
            "chrome" => Some(CHROME_EXTENSION_HEARTBEAT_COMPONENT),
            "chromium" => Some(CHROMIUM_EXTENSION_HEARTBEAT_COMPONENT),
            "brave" => Some(BRAVE_EXTENSION_HEARTBEAT_COMPONENT),
            "opera" => Some(OPERA_EXTENSION_HEARTBEAT_COMPONENT),
            "edge" => Some(EDGE_EXTENSION_HEARTBEAT_COMPONENT),
            "vivaldi" => Some(VIVALDI_EXTENSION_HEARTBEAT_COMPONENT),
            _ => None,
        };
        if let Some(component) = component {
            return component;
        }
    }

    if extension_id
        .map(|extension_id| extension_id == "opfljaancedgklbpnbpjfhdbbhbfpnoc")
        .unwrap_or(false)
    {
        return CHROME_EXTENSION_HEARTBEAT_COMPONENT;
    }

    FIREFOX_EXTENSION_HEARTBEAT_COMPONENT
}

fn resolve_extension_heartbeat_component(reported_component: &str) -> Result<String> {
    let Some(reported_browser) = supported_browser_for_extension_component(reported_component)
    else {
        return Ok(reported_component.to_string());
    };
    if !matches!(
        reported_browser,
        SupportedBrowser::Chrome | SupportedBrowser::Firefox
    ) {
        return Ok(reported_component.to_string());
    }

    let running_browsers = scan_procfs(Path::new("/proc"))?
        .iter()
        .filter_map(|process| supported_browser_for_process(&process.identity()))
        .collect::<Vec<_>>();
    Ok(
        infer_browser_from_running_browsers(reported_browser, &running_browsers)
            .extension_component()
            .to_string(),
    )
}

fn infer_browser_from_running_browsers(
    reported_browser: SupportedBrowser,
    running_browsers: &[SupportedBrowser],
) -> SupportedBrowser {
    if !matches!(
        reported_browser,
        SupportedBrowser::Chrome | SupportedBrowser::Firefox
    ) {
        return reported_browser;
    }

    let mut candidates = Vec::new();
    for browser in running_browsers.iter().copied() {
        if browser.is_chromium_based() == reported_browser.is_chromium_based()
            && !candidates.contains(&browser)
        {
            candidates.push(browser);
        }
    }

    if candidates.len() == 1 {
        candidates[0]
    } else {
        reported_browser
    }
}

fn extension_heartbeat_timeout_seconds(context: &RpcContext, component: &str) -> u64 {
    if supported_browser_for_extension_component(component)
        .is_some_and(SupportedBrowser::is_chromium_based)
    {
        context
            .extension_heartbeat_timeout_seconds
            .max(CHROME_EXTENSION_HEARTBEAT_TIMEOUT_SECONDS)
    } else {
        context.extension_heartbeat_timeout_seconds
    }
}

fn browser_name_for_component(component: &str) -> &'static str {
    supported_browser_for_extension_component(component)
        .map(browser_display_name)
        .unwrap_or("Unknown browser")
}

fn browser_display_name(browser: SupportedBrowser) -> &'static str {
    match browser {
        SupportedBrowser::Firefox => "Firefox",
        SupportedBrowser::LibreWolf => "LibreWolf",
        SupportedBrowser::Waterfox => "Waterfox",
        SupportedBrowser::Chrome => "Chrome",
        SupportedBrowser::Chromium => "Chromium",
        SupportedBrowser::Brave => "Brave",
        SupportedBrowser::Opera => "Opera",
        SupportedBrowser::Edge => "Microsoft Edge",
        SupportedBrowser::Vivaldi => "Vivaldi",
    }
}

fn heartbeat_details(details: Option<&str>) -> Option<Value> {
    details.and_then(|details| serde_json::from_str(details).ok())
}

fn handle_legacy_request(context: &RpcContext, value: Value) -> Value {
    let request = match serde_json::from_value::<LegacyRequest>(value) {
        Ok(request) => request,
        Err(err) => {
            return json!({ "action": "allow", "error": format!("invalid legacy request: {err}") })
        }
    };

    if request.message_type.as_deref() == Some("extension_heartbeat") {
        let params = ExtensionHeartbeatParams {
            component: None,
            browser: None,
            extension_id: request.extension_id,
            extension_version: request.extension_version,
            now: None,
        };
        return match extension_heartbeat_method(context, params) {
            Ok(_) => json!({ "type": "extension_heartbeat", "status": "ok" }),
            Err(err) => {
                json!({ "type": "extension_heartbeat", "status": "error", "error": err.to_string() })
            }
        };
    }

    let Some(url) = request.url else {
        return json!({ "action": "allow", "error": "missing url" });
    };
    match evaluate_url_method(
        context,
        EvaluateUrlParams {
            url,
            probe: false,
            now: None,
        },
    ) {
        Ok(value) if value.get("decision").and_then(Value::as_str) == Some("block") => {
            json!({ "action": "block", "reason": value.get("reason").cloned().unwrap_or(Value::Null) })
        }
        Ok(_) => json!({ "action": "allow" }),
        Err(err) => json!({ "action": "allow", "error": err.to_string() }),
    }
}

fn is_legacy_request(value: &Value) -> bool {
    value.get("method").is_none()
        && (value.get("url").is_some()
            || value.get("type").and_then(Value::as_str) == Some("extension_heartbeat"))
}

fn parse_params<T: for<'de> Deserialize<'de>>(params: Value) -> Result<T> {
    serde_json::from_value(params).map_err(|err| DaemonError::InvalidRequest(err.to_string()))
}

fn guarded_now(context: &RpcContext, now: Option<&str>) -> Result<DateTime<FixedOffset>> {
    Ok(guarded_now_with_status(context, now)?.now)
}

fn guarded_now_with_status(
    context: &RpcContext,
    now: Option<&str>,
) -> Result<clock_guard::GuardedNow> {
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    clock_guard::guarded_now(core.database(), now, context.trust_client_time)
}

fn reject_if_clock_tampered(context: &RpcContext) -> Result<()> {
    if context.trust_client_time {
        return Ok(());
    }

    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    clock_guard::ensure_trusted(core.database())
}

#[cfg(test)]
fn parse_optional_now(now: Option<String>) -> Result<DateTime<FixedOffset>> {
    clock_guard::parse_optional_now(now.as_deref())
}

fn decision_to_json(decision: &Decision, config: &Config, now: DateTime<FixedOffset>) -> Value {
    match decision {
        Decision::Allow => json!({ "decision": "allow" }),
        Decision::Block(reason) => json!({
            "decision": "block",
            "reason": block_reason_to_json(reason, config, now)
        }),
    }
}

fn block_reason_to_json(
    reason: &BlockReason,
    config: &Config,
    now: DateTime<FixedOffset>,
) -> Value {
    match reason {
        BlockReason::InvalidUrl { url } => json!({
            "kind": "invalid_url",
            "url": url,
            "summary": "The URL could not be parsed safely."
        }),
        BlockReason::Detox {
            session_id,
            session_name,
            rule_id,
            rule_name,
            target_kind,
            ends_at,
        } => {
            let free_at = ends_at.with_timezone(now.offset());
            json!({
                "kind": "detox",
                "blocked_by": "detox",
                "session_id": session_id,
                "session_name": session_name,
                "rule_id": rule_id,
                "rule_name": rule_name,
                "target_kind": detox_target_kind_to_str(*target_kind),
                "summary": "Detox is active for this target.",
                "detail": "This temporary block stays active until the detox session ends or is cancelled from the privileged admin path.",
                "detox_ends_at": ends_at.to_rfc3339(),
                "free_at": free_at.to_rfc3339()
            })
        }
        BlockReason::HardBlock { rule_id, rule_name } => json!({
            "kind": "hard_block",
            "tier": "tier_1",
            "rule_id": rule_id,
            "rule_name": rule_name,
            "summary": "This site is on a Tier 1 hard-block list.",
            "detail": "Tier 1 sites are always blocked and are also eligible for the hosts-file fallback.",
            "free_at": Value::Null
        }),
        BlockReason::ScheduledBlock { rule_id, rule_name } => json!({
            "kind": "scheduled_block",
            "tier": "tier_2",
            "rule_id": rule_id,
            "rule_name": rule_name,
            "blocked_by": "schedule",
            "summary": "This target is on an active Tier 2 scheduled-block list.",
            "detail": "Tier 2 cannot be bypassed with an allowance or manual unlock and domain rules also use the hosts-file fallback while active.",
            "free_at": config.rules.iter()
                .find(|rule| rule.id == *rule_id)
                .and_then(|rule| rule_schedule_inactive_at(rule, config, now))
                .map(|free_at| free_at.to_rfc3339())
        }),
        BlockReason::ControlledAccess {
            rule_id,
            rule_name,
            reason,
        } => controlled_block_reason_to_json(rule_id, rule_name, reason, config, now),
        BlockReason::RuntimeError { message } => json!({
            "kind": "runtime_error",
            "message": message,
            "summary": "BlocKuntu hit a runtime error while evaluating this navigation."
        }),
    }
}

fn controlled_block_reason_to_json(
    rule_id: &str,
    rule_name: &str,
    reason: &ControlledBlockReason,
    config: &Config,
    now: DateTime<FixedOffset>,
) -> Value {
    let mut value = json!({
        "kind": "controlled_access",
        "tier": "tier_3",
        "rule_id": rule_id,
        "rule_name": rule_name,
        "controlled_reason": controlled_reason_to_str(reason),
        "summary": controlled_reason_summary(reason),
        "detail": controlled_reason_detail(reason)
    });

    let Some(rule) = config.rules.iter().find(|rule| rule.id == rule_id) else {
        return value;
    };

    let active_schedules = active_schedule_details(rule, config, now);
    let mut free_at = rule_schedule_inactive_at(rule, config, now);
    if let Some(object) = value.as_object_mut() {
        if !active_schedules.is_empty() {
            object.insert("blocked_by".to_string(), json!("schedule"));
            object.insert(
                "active_schedules".to_string(),
                Value::Array(active_schedules),
            );
        }

        if matches!(reason, ControlledBlockReason::AllowanceExhausted) {
            let allowance_reset_at = next_allowance_reset_at(now);
            free_at = Some(match free_at {
                Some(schedule_free_at) => schedule_free_at.min(allowance_reset_at),
                None => allowance_reset_at,
            });
            object.insert(
                "allowance_reset_at".to_string(),
                json!(allowance_reset_at.to_rfc3339()),
            );
        }

        if let Some(free_at) = free_at {
            object.insert("free_at".to_string(), json!(free_at.to_rfc3339()));
        }
    }

    value
}

fn rule_schedule_inactive_at(
    rule: &RuleConfig,
    config: &Config,
    now: DateTime<FixedOffset>,
) -> Option<DateTime<FixedOffset>> {
    rule.schedule_ids
        .iter()
        .filter_map(|schedule_id| {
            config
                .schedules
                .iter()
                .find(|schedule| schedule.id == *schedule_id)
        })
        .filter_map(|schedule| schedule_active_until(schedule, now))
        .max()
}

fn controlled_reason_to_str(reason: &ControlledBlockReason) -> &'static str {
    match reason {
        ControlledBlockReason::NoAllowance => "no_allowance",
        ControlledBlockReason::AllowanceExhausted => "allowance_exhausted",
        ControlledBlockReason::UnlockRequired => "unlock_required",
    }
}

fn controlled_reason_summary(reason: &ControlledBlockReason) -> &'static str {
    match reason {
        ControlledBlockReason::NoAllowance => "This Tier 3 target needs an explicit unlock.",
        ControlledBlockReason::AllowanceExhausted => {
            "This Tier 3 target used up its daily allowance."
        }
        ControlledBlockReason::UnlockRequired => "This Tier 3 target requires an unlock.",
    }
}

fn controlled_reason_detail(reason: &ControlledBlockReason) -> &'static str {
    match reason {
        ControlledBlockReason::NoAllowance => {
            "No allowance is configured for this list, so access is blocked unless an unlock is active."
        }
        ControlledBlockReason::AllowanceExhausted => {
            "The configured allowance has already been consumed for the current accounting day."
        }
        ControlledBlockReason::UnlockRequired => {
            "Use the BlocKuntu GUI to request a temporary unlock if policy allows it."
        }
    }
}

fn active_schedule_details(
    rule: &RuleConfig,
    config: &Config,
    now: DateTime<FixedOffset>,
) -> Vec<Value> {
    rule.schedule_ids
        .iter()
        .filter_map(|schedule_id| {
            let schedule = config
                .schedules
                .iter()
                .find(|schedule| schedule.id == *schedule_id)?;
            let active_until = schedule_active_until(schedule, now)?;
            Some(json!({
                "id": &schedule.id,
                "name": schedule.name.as_deref().unwrap_or(&schedule.id),
                "active_until": active_until.to_rfc3339()
            }))
        })
        .collect()
}

fn schedule_active_until(
    schedule: &ScheduleConfig,
    now: DateTime<FixedOffset>,
) -> Option<DateTime<FixedOffset>> {
    schedule
        .windows
        .iter()
        .filter_map(|window| window_active_until(window, now))
        .max()
}

fn window_active_until(
    window: &focus_core::ScheduleWindow,
    now: DateTime<FixedOffset>,
) -> Option<DateTime<FixedOffset>> {
    let current_weekday = Weekday::from(now.weekday());
    let current_minute = (now.hour() as u16) * 60 + now.minute() as u16;
    let start = window.start.minutes_after_midnight();
    let end = window.end.minutes_after_midnight();

    if start < end {
        if window.weekday.includes(current_weekday)
            && current_minute >= start
            && current_minute < end
        {
            return Some(datetime_at_minute(now, 0, end));
        }
        return None;
    }

    if window.weekday.includes(current_weekday) && current_minute >= start {
        return Some(datetime_at_minute(now, 1, end));
    }

    if window.weekday.includes(current_weekday.previous()) && current_minute < end {
        return Some(datetime_at_minute(now, 0, end));
    }

    None
}

fn datetime_at_minute(
    now: DateTime<FixedOffset>,
    day_offset: i64,
    minute: u16,
) -> DateTime<FixedOffset> {
    let date = now.date_naive() + Duration::days(day_offset);
    let hour = u32::from(minute / 60);
    let minute = u32::from(minute % 60);
    let naive = date
        .and_hms_opt(hour, minute, 0)
        .expect("schedule minute should form a valid time");
    now.offset()
        .from_local_datetime(&naive)
        .single()
        .expect("fixed offset local time should be unambiguous")
}

fn next_allowance_reset_at(now: DateTime<FixedOffset>) -> DateTime<FixedOffset> {
    let next_local_midnight = (now.date_naive() + Duration::days(1))
        .and_hms_opt(0, 0, 0)
        .expect("midnight is valid");
    now.offset()
        .from_local_datetime(&next_local_midnight)
        .single()
        .expect("fixed offset local time should be unambiguous")
}

fn detox_session_to_json(session: &DetoxSession, now: DateTime<Utc>) -> Value {
    let status = if session.cancelled_at.is_some() {
        "cancelled"
    } else if session.starts_at > now {
        "scheduled"
    } else if session.ends_at > now {
        "active"
    } else {
        "expired"
    };
    let remaining_seconds = if status == "active" {
        Some((session.ends_at - now).num_seconds())
    } else {
        None
    };

    json!({
        "id": session.id,
        "name": session.name,
        "starts_at": session.starts_at,
        "ends_at": session.ends_at,
        "cancelled_at": session.cancelled_at,
        "site_rule_ids": session.site_rule_ids,
        "app_rule_ids": session.app_rule_ids,
        "status": status,
        "remaining_seconds": remaining_seconds
    })
}

fn detox_target_kind_to_str(kind: DetoxTargetKind) -> &'static str {
    match kind {
        DetoxTargetKind::SiteRule => "site_rule",
        DetoxTargetKind::AppRule => "app_rule",
    }
}

fn unlock_to_json(unlock: &UnlockState) -> Value {
    json!({
        "id": unlock.id,
        "target": unlock.target,
        "rule_id": unlock.rule_id,
        "minutes": unlock.minutes,
        "reason": unlock.reason,
        "started_at": unlock.started_at,
        "expires_at": unlock.expires_at
    })
}

fn visit_to_json(visit: &VisitState) -> Value {
    json!({
        "id": visit.id,
        "target": visit.target,
        "rule_id": visit.rule_id,
        "url": visit.url,
        "tab_id": visit.tab_id,
        "started_at": visit.started_at,
        "last_heartbeat_at": visit.last_heartbeat_at,
        "ended_at": visit.ended_at
    })
}

fn jsonrpc_error(id: Option<Value>, code: i64, message: &str, data: Option<String>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message,
            "data": data
        }
    })
}

fn default_detox_sessions_limit() -> u32 {
    50
}

fn default_pending_notifications_limit() -> u32 {
    20
}

fn normalized_unique_ids(ids: Vec<String>) -> Vec<String> {
    let mut unique = Vec::new();
    for id in ids {
        let id = id.trim().to_string();
        if !id.is_empty() && !unique.contains(&id) {
            unique.push(id);
        }
    }
    unique
}

fn validate_detox_targets(
    config: &Config,
    site_rule_ids: &[String],
    app_rule_ids: &[String],
) -> Result<()> {
    for rule_id in site_rule_ids {
        let Some(rule) = config.rules.iter().find(|rule| rule.id == *rule_id) else {
            return Err(DaemonError::InvalidRequest(format!(
                "site list '{}' does not exist",
                rule_id
            )));
        };
        if rule.tier == RuleTier::Hard {
            return Err(DaemonError::InvalidRequest(format!(
                "site list '{}' is Tier 1 and cannot be used for detox",
                rule_id
            )));
        }
    }

    for rule_id in app_rule_ids {
        let Some(rule) = config.app_rules.iter().find(|rule| rule.id == *rule_id) else {
            return Err(DaemonError::InvalidRequest(format!(
                "app rule '{}' does not exist",
                rule_id
            )));
        };
        if rule.tier == RuleTier::Hard {
            return Err(DaemonError::InvalidRequest(format!(
                "app rule '{}' is Tier 1 and cannot be used for detox",
                rule_id
            )));
        }
    }

    Ok(())
}

fn next_detox_session_id(core: &FocusCore, now: DateTime<Utc>) -> Result<String> {
    let base = format!("detox-{}", now.timestamp_micros());
    let mut id = base.clone();
    let mut suffix = 2;
    while core.database().detox_session(&id)?.is_some() {
        id = format!("{base}-{suffix}");
        suffix += 1;
    }
    Ok(id)
}

fn enforcement_state_from_core(core: &FocusCore) -> Result<String> {
    if uninstall_handoff_active(core, Utc::now())? {
        Ok(ENFORCEMENT_UNINSTALLING.to_string())
    } else {
        Ok(ENFORCEMENT_ACTIVE.to_string())
    }
}

fn browser_extension_mode_from_core(core: &FocusCore) -> Result<String> {
    if uninstall_handoff_active(core, Utc::now())? {
        Ok(BROWSER_EXTENSION_MODE_UNINSTALLING.to_string())
    } else {
        Ok(BROWSER_EXTENSION_MODE_ACTIVE.to_string())
    }
}

pub(crate) fn enforcement_active_from_core(core: &FocusCore) -> Result<bool> {
    Ok(!uninstall_handoff_active(core, Utc::now())?)
}

fn uninstall_handoff_active(core: &FocusCore, now: DateTime<Utc>) -> Result<bool> {
    let mode = core.database().service_state(BROWSER_EXTENSION_MODE_KEY)?;
    if mode.as_deref() != Some(BROWSER_EXTENSION_MODE_UNINSTALLING) {
        return Ok(false);
    }

    let Some(until) = core
        .database()
        .service_state(BROWSER_EXTENSION_UNINSTALLING_UNTIL_KEY)?
    else {
        return Ok(false);
    };
    Ok(DateTime::parse_from_rfc3339(&until)
        .map(|until| until.with_timezone(&Utc) > now)
        .unwrap_or(false))
}

fn active_tier1_site_list_edit_allowed(
    core: &FocusCore,
    current_rule: &RuleConfig,
    now: DateTime<FixedOffset>,
) -> Result<bool> {
    Ok(current_rule.tier == RuleTier::Hard && tier1_edit_window_active(core, now)?)
}

fn tier1_edit_window_active(core: &FocusCore, now: DateTime<FixedOffset>) -> Result<bool> {
    Ok(tier1_edit_unlocked_until(core)?
        .map(|expires_at| expires_at > now.with_timezone(&Utc))
        .unwrap_or(false))
}

fn tier1_edit_unlocked_until(core: &FocusCore) -> Result<Option<DateTime<Utc>>> {
    let Some(value) = core.database().service_state(TIER1_EDIT_UNLOCK_UNTIL_KEY)? else {
        return Ok(None);
    };
    Ok(DateTime::parse_from_rfc3339(&value)
        .ok()
        .map(|expires_at| expires_at.with_timezone(&Utc)))
}

fn tier1_edit_status_json(core: &FocusCore, now: DateTime<FixedOffset>) -> Result<Value> {
    let now_utc = now.with_timezone(&Utc);
    let expires_at = tier1_edit_unlocked_until(core)?;
    let clock_integrity = clock_guard::status(core.database())?;
    let clock_tampered = clock_integrity.state == "tampered";
    let active = !clock_tampered
        && expires_at
            .map(|expires_at| expires_at > now_utc)
            .unwrap_or(false);
    let remaining_seconds = expires_at
        .filter(|_| !clock_tampered)
        .filter(|expires_at| *expires_at > now_utc)
        .map(|expires_at| (expires_at - now_utc).num_seconds());

    let protected_access_mode = protected_access_mode(core)?;
    let protected_access_open = protected_access_is_open(core, now, clock_tampered)?;
    let browser_block_mode = unsupported_browser_block_mode(core)?;
    let browser_block_active = unsupported_browser_block_is_active(core, now, clock_tampered)?;
    let chromium_incognito = current_chromium_incognito_policy_settings(core, now, clock_tampered)?;
    let chromium_incognito_change_access_mode = chromium_incognito_change_access_mode(core)?;
    let chromium_incognito_settings_change_allowed =
        chromium_incognito_settings_change_allowed(core, now, clock_tampered)?;
    Ok(json!({
        "active": active,
        "expires_at": expires_at,
        "remaining_seconds": remaining_seconds,
        "credential_configured": tier1_edit_credential_configured(core)?,
        "protected_access_mode": protected_access_mode,
        "protected_access_open": protected_access_open,
        "protected_access_label": protected_access_mode_label(protected_access_mode),
        "unsupported_browser_block_mode": browser_block_mode,
        "unsupported_browser_block_active": browser_block_active,
        "chromium_incognito_mode": chromium_incognito.configured_mode,
        "chromium_incognito_effective_mode": chromium_incognito.mode,
        "chromium_incognito_disable_scope": chromium_incognito.disable_scope,
        "chromium_incognito_private_browsing_disabled": chromium_incognito.private_browsing_disabled,
        "chromium_incognito_change_access_mode": chromium_incognito_change_access_mode,
        "chromium_incognito_settings_change_allowed": chromium_incognito_settings_change_allowed,
        "chromium_incognito_url_block_count": chromium_incognito.url_blocklist.len(),
        "chromium_incognito_unsupported_pattern_count": chromium_incognito.unsupported_pattern_count,
        "chromium_incognito_url_block_limit_exceeded": chromium_incognito.url_block_limit_exceeded(),
        "operator_window_restriction_enabled": protected_access_mode == ProtectedAccessMode::Sunday,
        "operator_window_open": protected_access_open,
        "operator_window_label": protected_access_mode_label(protected_access_mode),
        "clock_integrity": clock_integrity
    }))
}

fn operator_window_open(now: DateTime<FixedOffset>) -> bool {
    let current_minute = (now.hour() as u16) * 60 + now.minute() as u16;
    matches!(Weekday::from(now.weekday()), Weekday::Sun)
        && (OPERATOR_WINDOW_START_MINUTE..=OPERATOR_WINDOW_END_MINUTE).contains(&current_minute)
}

fn protected_access_mode(core: &FocusCore) -> Result<ProtectedAccessMode> {
    match core
        .database()
        .service_state(PROTECTED_ACCESS_MODE_KEY)?
        .as_deref()
    {
        Some("sunday") => Ok(ProtectedAccessMode::Sunday),
        Some("no_active_schedule_or_detox") => Ok(ProtectedAccessMode::NoActiveScheduleOrDetox),
        Some("all_time") => Ok(ProtectedAccessMode::AllTime),
        Some(mode) => Err(DaemonError::InvalidRequest(format!(
            "invalid protected access mode '{mode}'"
        ))),
        None => Ok(
            if core
                .database()
                .service_state(OPERATOR_WINDOW_RESTRICTION_KEY)?
                .as_deref()
                == Some("true")
            {
                ProtectedAccessMode::Sunday
            } else {
                ProtectedAccessMode::AllTime
            },
        ),
    }
}

fn unsupported_browser_block_mode(core: &FocusCore) -> Result<ProtectedAccessMode> {
    protected_access_mode(core)
}

pub(crate) fn chromium_incognito_policy_settings(
    core: &FocusCore,
    now: DateTime<FixedOffset>,
    clock_tampered: bool,
    configured_mode: ChromiumIncognitoMode,
) -> Result<ChromiumIncognitoPolicySettings> {
    let disable_scope = chromium_incognito_disable_scope(core)?;
    let private_browsing_disabled = configured_mode == ChromiumIncognitoMode::Disabled
        && chromium_incognito_private_browsing_disabled(core, disable_scope, now, clock_tampered)?;
    let mode = if configured_mode == ChromiumIncognitoMode::Disabled && !private_browsing_disabled {
        ChromiumIncognitoMode::ManualConsent
    } else {
        configured_mode
    };
    let (url_blocklist, unsupported_pattern_count) =
        if mode == ChromiumIncognitoMode::PolicyUrlBlocking {
            chromium_incognito_url_blocklist(core, now, clock_tampered)?
        } else {
            (Vec::new(), 0)
        };

    Ok(ChromiumIncognitoPolicySettings {
        configured_mode,
        mode,
        disable_scope,
        private_browsing_disabled,
        url_blocklist,
        unsupported_pattern_count,
    })
}

pub(crate) fn current_chromium_incognito_policy_settings(
    core: &FocusCore,
    now: DateTime<FixedOffset>,
    clock_tampered: bool,
) -> Result<ChromiumIncognitoPolicySettings> {
    chromium_incognito_policy_settings(core, now, clock_tampered, chromium_incognito_mode(core)?)
}

fn chromium_incognito_mode(core: &FocusCore) -> Result<ChromiumIncognitoMode> {
    match core
        .database()
        .service_state(CHROMIUM_INCOGNITO_MODE_KEY)?
        .as_deref()
    {
        Some("disabled") => Ok(ChromiumIncognitoMode::Disabled),
        Some("manual_consent") => Ok(ChromiumIncognitoMode::ManualConsent),
        None => Ok(ChromiumIncognitoMode::default()),
        Some("policy_url_blocking") => Ok(ChromiumIncognitoMode::PolicyUrlBlocking),
        Some(mode) => Err(DaemonError::InvalidRequest(format!(
            "invalid Chromium private browsing mode '{mode}'"
        ))),
    }
}

fn chromium_incognito_mode_name(mode: ChromiumIncognitoMode) -> &'static str {
    match mode {
        ChromiumIncognitoMode::Disabled => "disabled",
        ChromiumIncognitoMode::ManualConsent => "manual_consent",
        ChromiumIncognitoMode::PolicyUrlBlocking => "policy_url_blocking",
    }
}

fn chromium_incognito_url_blocklist(
    core: &FocusCore,
    now: DateTime<FixedOffset>,
    clock_tampered: bool,
) -> Result<(Vec<String>, usize)> {
    let config = core.config();
    let detox_rule_ids = core
        .database()
        .active_detox_sessions(now.with_timezone(&Utc))?
        .into_iter()
        .flat_map(|session| session.site_rule_ids)
        .collect::<BTreeSet<_>>();
    let mut entries = BTreeSet::new();
    let mut unsupported_pattern_count = 0;

    for rule in config.rules.iter().filter(|rule| rule.enabled) {
        let selected_by_detox = detox_rule_ids.contains(&rule.id);
        let active = match rule.tier {
            RuleTier::Hard => true,
            RuleTier::ScheduledBlock | RuleTier::ControlledAccess => {
                selected_by_detox
                    || schedule_ids_are_active_at_with_clock(
                        &rule.schedule_ids,
                        config,
                        now,
                        clock_tampered,
                    )
            }
        };
        if !active {
            continue;
        }

        for pattern in &rule.patterns {
            let entry = match pattern.kind {
                RulePatternKind::Domain => Some(if pattern.match_subdomains {
                    pattern.value.clone()
                } else {
                    format!(".{}", pattern.value.trim_start_matches('.'))
                }),
                // Chrome and Edge URL blocklist policies use the same URL-filter grammar
                // for exact URL and host/path patterns.
                RulePatternKind::ExactUrl | RulePatternKind::UrlPrefix => {
                    Some(pattern.value.clone())
                }
                RulePatternKind::UrlContains | RulePatternKind::PathPrefix => None,
            };
            if let Some(entry) = entry {
                entries.insert(entry);
            } else {
                unsupported_pattern_count += 1;
            }
        }
    }

    Ok((entries.into_iter().collect(), unsupported_pattern_count))
}

fn chromium_incognito_disable_scope(core: &FocusCore) -> Result<ChromiumIncognitoDisableScope> {
    match core
        .database()
        .service_state(CHROMIUM_INCOGNITO_DISABLE_SCOPE_KEY)?
        .as_deref()
    {
        Some("all_time") | None => Ok(ChromiumIncognitoDisableScope::AllTime),
        Some("active_schedule_or_detox") => {
            Ok(ChromiumIncognitoDisableScope::ActiveScheduleOrDetox)
        }
        Some(scope) => Err(DaemonError::InvalidRequest(format!(
            "invalid Chromium private browsing disable scope '{scope}'"
        ))),
    }
}

fn chromium_incognito_private_browsing_disabled(
    core: &FocusCore,
    scope: ChromiumIncognitoDisableScope,
    now: DateTime<FixedOffset>,
    clock_tampered: bool,
) -> Result<bool> {
    if clock_tampered {
        return Ok(true);
    }

    match scope {
        ChromiumIncognitoDisableScope::AllTime => Ok(true),
        ChromiumIncognitoDisableScope::ActiveScheduleOrDetox => {
            Ok(!no_active_schedule_or_detox(core, now)?)
        }
    }
}

fn chromium_incognito_disable_scope_name(scope: ChromiumIncognitoDisableScope) -> &'static str {
    match scope {
        ChromiumIncognitoDisableScope::AllTime => "all_time",
        ChromiumIncognitoDisableScope::ActiveScheduleOrDetox => "active_schedule_or_detox",
    }
}

fn chromium_incognito_disable_scope_description(
    scope: ChromiumIncognitoDisableScope,
) -> &'static str {
    match scope {
        ChromiumIncognitoDisableScope::AllTime => "at all times",
        ChromiumIncognitoDisableScope::ActiveScheduleOrDetox => {
            "only while a schedule or Detox is active"
        }
    }
}

fn chromium_incognito_change_access_mode(core: &FocusCore) -> Result<ProtectedAccessMode> {
    protected_access_mode(core)
}

fn chromium_incognito_settings_change_allowed(
    core: &FocusCore,
    now: DateTime<FixedOffset>,
    clock_tampered: bool,
) -> Result<bool> {
    if clock_tampered {
        return Ok(false);
    }

    match chromium_incognito_change_access_mode(core)? {
        ProtectedAccessMode::Sunday => Ok(operator_window_open(now)),
        ProtectedAccessMode::NoActiveScheduleOrDetox => no_active_schedule_or_detox(core, now),
        ProtectedAccessMode::AllTime => Ok(true),
    }
}

fn ensure_chromium_incognito_settings_change_allowed(
    core: &FocusCore,
    now: DateTime<FixedOffset>,
    clock_tampered: bool,
) -> Result<()> {
    let access_mode = chromium_incognito_change_access_mode(core)?;
    if chromium_incognito_settings_change_allowed(core, now, clock_tampered)? {
        return Ok(());
    }
    Err(protected_access_closed_error(
        "Chromium private browsing settings",
        access_mode,
    ))
}

pub(crate) fn ensure_chromium_incognito_url_blocklist_within_limit(
    settings: &ChromiumIncognitoPolicySettings,
) -> Result<()> {
    if settings.mode == ChromiumIncognitoMode::PolicyUrlBlocking
        && settings.url_block_limit_exceeded()
    {
        return Err(DaemonError::InvalidRequest(format!(
            "Chromium private URL policy supports at most 1000 active patterns; this configuration has {}",
            settings.url_blocklist.len()
        )));
    }
    Ok(())
}

fn protected_access_is_open(
    core: &FocusCore,
    now: DateTime<FixedOffset>,
    clock_tampered: bool,
) -> Result<bool> {
    if clock_tampered {
        return Ok(false);
    }

    match protected_access_mode(core)? {
        ProtectedAccessMode::Sunday => Ok(operator_window_open(now)),
        ProtectedAccessMode::NoActiveScheduleOrDetox => no_active_schedule_or_detox(core, now),
        ProtectedAccessMode::AllTime => Ok(true),
    }
}

pub(crate) fn unsupported_browser_block_is_active(
    core: &FocusCore,
    now: DateTime<FixedOffset>,
    clock_tampered: bool,
) -> Result<bool> {
    if !core.config().strict_mode.block_unsupported_browsers || clock_tampered {
        return Ok(core.config().strict_mode.block_unsupported_browsers);
    }

    match unsupported_browser_block_mode(core)? {
        ProtectedAccessMode::Sunday => Ok(operator_window_open(now)),
        ProtectedAccessMode::NoActiveScheduleOrDetox => no_active_schedule_or_detox(core, now),
        ProtectedAccessMode::AllTime => Ok(true),
    }
}

fn no_active_schedule_or_detox(core: &FocusCore, now: DateTime<FixedOffset>) -> Result<bool> {
    let schedule_active = core
        .config()
        .schedules
        .iter()
        .any(|schedule| schedule_is_active_at(schedule, now));
    let detox_active = !core
        .database()
        .active_detox_sessions(now.with_timezone(&Utc))?
        .is_empty();
    Ok(!schedule_active && !detox_active)
}

fn protected_access_mode_name(mode: ProtectedAccessMode) -> &'static str {
    match mode {
        ProtectedAccessMode::Sunday => "sunday",
        ProtectedAccessMode::NoActiveScheduleOrDetox => "no_active_schedule_or_detox",
        ProtectedAccessMode::AllTime => "all_time",
    }
}

fn protected_access_mode_label(mode: ProtectedAccessMode) -> &'static str {
    match mode {
        ProtectedAccessMode::Sunday => "Sunday 20:00-23:59",
        ProtectedAccessMode::NoActiveScheduleOrDetox => "No active schedule or Detox",
        ProtectedAccessMode::AllTime => "Any time",
    }
}

fn protected_access_mode_description(mode: ProtectedAccessMode) -> &'static str {
    match mode {
        ProtectedAccessMode::Sunday => "during Sunday 20:00-23:59",
        ProtectedAccessMode::NoActiveScheduleOrDetox => "while no schedule or Detox is active",
        ProtectedAccessMode::AllTime => "at any time",
    }
}

fn protected_access_closed_error(action: &str, mode: ProtectedAccessMode) -> DaemonError {
    DaemonError::InvalidRequest(format!(
        "{action} are only available {}",
        protected_access_mode_description(mode)
    ))
}

fn tier1_edit_credential_configured(core: &FocusCore) -> Result<bool> {
    if core
        .database()
        .service_state(TIER1_EDIT_CREDENTIAL_SALT_KEY)?
        .is_some_and(|value| !value.is_empty())
        && core
            .database()
            .service_state(TIER1_EDIT_CREDENTIAL_HASH_KEY)?
            .is_some_and(|value| !value.is_empty())
    {
        return Ok(true);
    }
    Ok(read_tier1_edit_key()?.is_some())
}

fn tier1_edit_credential_matches(core: &FocusCore, candidate: &str) -> Result<bool> {
    let Some(salt) = core
        .database()
        .service_state(TIER1_EDIT_CREDENTIAL_SALT_KEY)?
        .filter(|value| !value.is_empty())
    else {
        return Ok(read_tier1_edit_key()?.is_some_and(|key| key == candidate));
    };
    let Some(expected_hash) = core
        .database()
        .service_state(TIER1_EDIT_CREDENTIAL_HASH_KEY)?
        .filter(|value| !value.is_empty())
    else {
        return Ok(read_tier1_edit_key()?.is_some_and(|key| key == candidate));
    };
    Ok(tier1_edit_credential_hash(&salt, candidate) == expected_hash)
}

fn read_tier1_edit_key() -> Result<Option<String>> {
    match fs::read_to_string(TIER1_EDIT_KEY_PATH) {
        Ok(contents) => Ok(contents
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .map(str::to_owned)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn random_credential_salt() -> Result<String> {
    let mut bytes = [0_u8; 32];
    fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn tier1_edit_credential_hash(salt: &str, phrase: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(salt.as_bytes());
    hasher.update([0]);
    hasher.update(phrase.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn repair_hosts_after_policy_change(
    context: &RpcContext,
    core: &FocusCore,
    now: DateTime<FixedOffset>,
) -> Result<Option<HostsRepairStatus>> {
    let Some(hosts) = context.hosts.as_ref() else {
        return Ok(None);
    };

    if !enforcement_active_from_core(core)? {
        return Ok(Some(HostsRepairStatus::SkippedInactive));
    }

    let active_detox_sessions = core
        .database()
        .active_detox_sessions(now.with_timezone(&Utc))?;
    hosts
        .verify_and_repair_with_active_detox(core.config(), &active_detox_sessions, now, false)
        .map(Some)
}

fn hosts_repair_detail(status: Option<HostsRepairStatus>) -> String {
    status
        .map(|status| format!(";hosts={status:?}"))
        .unwrap_or_default()
}

fn active_site_list_edit_error(rule_id: &str) -> DaemonError {
    DaemonError::InvalidRequest(format!(
        "site list '{rule_id}' is currently active and cannot be edited"
    ))
}

fn active_app_rule_edit_error(rule_id: &str) -> DaemonError {
    DaemonError::InvalidRequest(format!(
        "app rule '{rule_id}' is currently active and cannot be edited"
    ))
}

fn active_detox_site_list_edit_error(rule_id: &str) -> DaemonError {
    DaemonError::InvalidRequest(format!(
        "site list '{rule_id}' is covered by an active detox session and cannot be edited"
    ))
}

fn active_detox_app_rule_edit_error(rule_id: &str) -> DaemonError {
    DaemonError::InvalidRequest(format!(
        "app rule '{rule_id}' is covered by an active detox session and cannot be edited"
    ))
}

fn active_schedule_edit_error(schedule_id: &str) -> DaemonError {
    DaemonError::InvalidRequest(format!(
        "schedule '{schedule_id}' is currently active and cannot be edited"
    ))
}

fn active_allowance_edit_error(allowance_id: &str) -> DaemonError {
    DaemonError::InvalidRequest(format!(
        "allowance '{allowance_id}' is currently used by an active rule and cannot be edited"
    ))
}

fn remove_unreferenced_allowance(config: &mut Config, allowance_id: Option<&str>) {
    let Some(allowance_id) = allowance_id else {
        return;
    };

    let used_by_site_list = config
        .rules
        .iter()
        .any(|rule| rule.allowance_id.as_deref() == Some(allowance_id));
    let used_by_app_rule = config
        .app_rules
        .iter()
        .any(|rule| rule.allowance_id.as_deref() == Some(allowance_id));

    if used_by_site_list || used_by_app_rule {
        return;
    }

    if let Some(index) = config
        .allowances
        .iter()
        .position(|allowance| allowance.id == allowance_id)
    {
        config.allowances.remove(index);
    }
}

fn allowance_is_active_at(
    allowance_id: &str,
    core: &FocusCore,
    now: DateTime<FixedOffset>,
) -> Result<bool> {
    let config = core.config();
    if config.rules.iter().any(|rule| {
        rule.allowance_id.as_deref() == Some(allowance_id) && rule_is_active_at(rule, config, now)
    }) || config.app_rules.iter().any(|rule| {
        rule.allowance_id.as_deref() == Some(allowance_id)
            && app_rule_is_active_at(rule, config, now)
    }) {
        return Ok(true);
    }

    let active_detox_sessions = core
        .database()
        .active_detox_sessions(now.with_timezone(&Utc))?;
    Ok(active_detox_sessions.iter().any(|session| {
        config.rules.iter().any(|rule| {
            rule.allowance_id.as_deref() == Some(allowance_id)
                && session.site_rule_ids.iter().any(|id| id == &rule.id)
        }) || config.app_rules.iter().any(|rule| {
            rule.allowance_id.as_deref() == Some(allowance_id)
                && session.app_rule_ids.iter().any(|id| id == &rule.id)
        })
    }))
}

fn site_list_edit_is_additive(current: &RuleConfig, proposed: &RuleConfig) -> bool {
    current.id == proposed.id
        && current.name == proposed.name
        && current.tier == proposed.tier
        && current.enabled == proposed.enabled
        && current.schedule_ids == proposed.schedule_ids
        && current.allowance_id == proposed.allowance_id
        && proposed.patterns.len() >= current.patterns.len()
        && current
            .patterns
            .iter()
            .zip(proposed.patterns.iter())
            .all(|(current, proposed)| current == proposed)
}

fn app_rule_edit_is_additive(current: &AppRuleConfig, proposed: &AppRuleConfig) -> bool {
    current.id == proposed.id
        && current.name == proposed.name
        && current.tier == proposed.tier
        && current.enabled == proposed.enabled
        && current.schedule_ids == proposed.schedule_ids
        && current.allowance_id == proposed.allowance_id
        && proposed.matchers.len() >= current.matchers.len()
        && current
            .matchers
            .iter()
            .zip(proposed.matchers.iter())
            .all(|(current, proposed)| current == proposed)
}

fn schedule_edit_is_additive(current: &ScheduleConfig, proposed: &ScheduleConfig) -> bool {
    current.id == proposed.id
        && current.name == proposed.name
        && proposed.windows.len() >= current.windows.len()
        && current
            .windows
            .iter()
            .zip(proposed.windows.iter())
            .all(|(current, proposed)| current == proposed)
}

fn schedule_target_updates_change(
    config: &Config,
    schedule_id: &str,
    site_rule_ids: Option<&[String]>,
    app_rule_ids: Option<&[String]>,
) -> bool {
    site_rule_ids.is_some_and(|requested| {
        !same_id_membership(
            requested,
            &config
                .rules
                .iter()
                .filter(|rule| {
                    rule.tier != RuleTier::Hard
                        && rule.schedule_ids.iter().any(|id| id == schedule_id)
                })
                .map(|rule| rule.id.clone())
                .collect::<Vec<_>>(),
        )
    }) || app_rule_ids.is_some_and(|requested| {
        !same_id_membership(
            requested,
            &config
                .app_rules
                .iter()
                .filter(|rule| {
                    rule.tier != RuleTier::Hard
                        && rule.schedule_ids.iter().any(|id| id == schedule_id)
                })
                .map(|rule| rule.id.clone())
                .collect::<Vec<_>>(),
        )
    })
}

fn same_id_membership(left: &[String], right: &[String]) -> bool {
    left.len() == right.len() && left.iter().all(|id| right.contains(id))
}

fn apply_schedule_target_updates(
    core: &FocusCore,
    current: &Config,
    next: &mut Config,
    schedule_id: &str,
    site_rule_ids: Option<&[String]>,
    app_rule_ids: Option<&[String]>,
    now: DateTime<FixedOffset>,
) -> Result<()> {
    if let Some(rule_ids) = site_rule_ids {
        let changed_rule_ids = current
            .rules
            .iter()
            .filter(|rule| {
                rule.tier != RuleTier::Hard
                    && (rule.schedule_ids.iter().any(|id| id == schedule_id)
                        != rule_ids.contains(&rule.id))
            })
            .map(|rule| rule.id.clone())
            .collect::<Vec<_>>();

        for rule_id in changed_rule_ids {
            let rule = current
                .rules
                .iter()
                .find(|rule| rule.id == rule_id)
                .expect("changed site rule must exist");
            if site_rule_in_active_detox(core, &rule.id, now)? {
                return Err(active_detox_site_list_edit_error(&rule.id));
            }
            if rule_is_active_at(rule, current, now) {
                return Err(active_site_list_edit_error(&rule.id));
            }
        }

        for rule in &mut next.rules {
            if rule.tier == RuleTier::Hard {
                continue;
            }
            if rule.schedule_ids.iter().any(|id| id == schedule_id) == rule_ids.contains(&rule.id) {
                continue;
            }
            rule.schedule_ids.retain(|id| id != schedule_id);
            if rule_ids.contains(&rule.id) {
                rule.schedule_ids.push(schedule_id.to_string());
            }
        }
    }

    if let Some(rule_ids) = app_rule_ids {
        let changed_rule_ids = current
            .app_rules
            .iter()
            .filter(|rule| {
                rule.tier != RuleTier::Hard
                    && (rule.schedule_ids.iter().any(|id| id == schedule_id)
                        != rule_ids.contains(&rule.id))
            })
            .map(|rule| rule.id.clone())
            .collect::<Vec<_>>();

        for rule_id in changed_rule_ids {
            let rule = current
                .app_rules
                .iter()
                .find(|rule| rule.id == rule_id)
                .expect("changed app rule must exist");
            if app_rule_in_active_detox(core, &rule.id, now)? {
                return Err(active_detox_app_rule_edit_error(&rule.id));
            }
            if app_rule_is_active_at(rule, current, now) {
                return Err(active_app_rule_edit_error(&rule.id));
            }
        }

        for rule in &mut next.app_rules {
            if rule.tier == RuleTier::Hard {
                continue;
            }
            if rule.schedule_ids.iter().any(|id| id == schedule_id) == rule_ids.contains(&rule.id) {
                continue;
            }
            rule.schedule_ids.retain(|id| id != schedule_id);
            if rule_ids.contains(&rule.id) {
                rule.schedule_ids.push(schedule_id.to_string());
            }
        }
    }

    Ok(())
}

fn site_rule_in_active_detox(
    core: &FocusCore,
    rule_id: &str,
    now: DateTime<FixedOffset>,
) -> Result<bool> {
    Ok(core
        .database()
        .active_detox_sessions(now.with_timezone(&Utc))?
        .iter()
        .any(|session| session.site_rule_ids.iter().any(|id| id == rule_id)))
}

fn app_rule_in_active_detox(
    core: &FocusCore,
    rule_id: &str,
    now: DateTime<FixedOffset>,
) -> Result<bool> {
    Ok(core
        .database()
        .active_detox_sessions(now.with_timezone(&Utc))?
        .iter()
        .any(|session| session.app_rule_ids.iter().any(|id| id == rule_id)))
}

fn rule_is_active_at(rule: &RuleConfig, config: &Config, now: DateTime<FixedOffset>) -> bool {
    match rule.tier {
        RuleTier::Hard => true,
        RuleTier::ScheduledBlock | RuleTier::ControlledAccess => {
            schedule_ids_are_active_at(&rule.schedule_ids, config, now)
        }
    }
}

fn app_rule_is_active_at(
    rule: &AppRuleConfig,
    config: &Config,
    now: DateTime<FixedOffset>,
) -> bool {
    match rule.tier {
        RuleTier::Hard => true,
        RuleTier::ScheduledBlock | RuleTier::ControlledAccess => {
            schedule_ids_are_active_at(&rule.schedule_ids, config, now)
        }
    }
}

fn schedule_ids_are_active_at(
    schedule_ids: &[String],
    config: &Config,
    now: DateTime<FixedOffset>,
) -> bool {
    !schedule_ids.is_empty()
        && schedule_ids.iter().any(|schedule_id| {
            config
                .schedules
                .iter()
                .find(|schedule| schedule.id == *schedule_id)
                .map(|schedule| schedule_is_active_at(schedule, now))
                .unwrap_or(true)
        })
}

fn schedule_is_active_at(schedule: &ScheduleConfig, now: DateTime<FixedOffset>) -> bool {
    let current_weekday = Weekday::from(now.weekday());
    let current_minute = (now.hour() as u16) * 60 + now.minute() as u16;

    schedule.windows.iter().any(|window| {
        let start = window.start.minutes_after_midnight();
        let end = window.end.minutes_after_midnight();

        if start < end {
            window.weekday.includes(current_weekday)
                && current_minute >= start
                && current_minute < end
        } else {
            (window.weekday.includes(current_weekday) && current_minute >= start)
                || (window.weekday.includes(current_weekday.previous()) && current_minute < end)
        }
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::{Arc, Mutex};

    use chrono::{Duration, Local, TimeZone, Utc};
    use focus_core::{Config, Database, FocusCore, HeartbeatState};
    use serde_json::{json, Value};

    use crate::chrome_policy::ChromePolicyManager;
    use crate::chrome_policy::ChromiumIncognitoMode;
    use crate::firefox_policy::FirefoxPolicyManager;
    use crate::hosts::HostsManager;
    use crate::policy_recovery::PolicyRecoveryManager;

    use super::{
        browser_extension_status_json, chromium_incognito_policy_settings,
        chromium_incognito_settings_change_allowed, chromium_incognito_url_blocklist,
        handle_payload, infer_browser_from_running_browsers, parse_optional_now,
        running_app_snapshots_from_processes, ChromiumPolicyBinding, GeckoPolicyBinding,
        RpcContext, CHROMIUM_INCOGNITO_DISABLE_SCOPE_KEY,
    };
    use crate::process_scan::{ProcessInfo, SupportedBrowser};

    fn rpc_context() -> RpcContext {
        rpc_context_with_config_toml(
            r#"
            [[rules]]
            id = "hard"
            name = "Hard"
            tier = "hard"
            patterns = [
              { kind = "domain", value = "blocked.example", match_subdomains = true }
            ]
            "#,
        )
    }

    #[test]
    fn resolves_generic_extension_identity_when_one_browser_family_member_is_running() {
        assert_eq!(
            infer_browser_from_running_browsers(
                SupportedBrowser::Chrome,
                &[SupportedBrowser::Chromium],
            ),
            SupportedBrowser::Chromium
        );
        assert_eq!(
            infer_browser_from_running_browsers(
                SupportedBrowser::Firefox,
                &[SupportedBrowser::LibreWolf],
            ),
            SupportedBrowser::LibreWolf
        );
        assert_eq!(
            infer_browser_from_running_browsers(
                SupportedBrowser::Chrome,
                &[SupportedBrowser::Chrome, SupportedBrowser::Vivaldi],
            ),
            SupportedBrowser::Chrome,
            "the daemon must not guess when multiple Chromium browsers are running"
        );
    }

    #[test]
    fn private_url_policy_converts_only_representable_active_rules() {
        let config = Config::from_toml_str(
            r#"
            [[schedules]]
            id = "monday"
            windows = [{ weekday = "mon", start = "10:00", end = "11:00" }]

            [[rules]]
            id = "hard"
            name = "Hard"
            tier = "hard"
            patterns = [
              { kind = "domain", value = "blocked.example", match_subdomains = true },
              { kind = "domain", value = "exact.example", match_subdomains = false },
              { kind = "exact_url", value = "https://blocked.example/path", match_subdomains = false },
              { kind = "url_contains", value = "watch", match_subdomains = false }
            ]

            [[rules]]
            id = "scheduled"
            name = "Scheduled"
            tier = "scheduled_block"
            schedule_ids = ["monday"]
            patterns = [{ kind = "url_prefix", value = "https://scheduled.example/path", match_subdomains = false }]

            [[rules]]
            id = "controlled"
            name = "Controlled"
            tier = "controlled_access"
            schedule_ids = ["monday"]
            patterns = [{ kind = "domain", value = "controlled.example", match_subdomains = true }]
            "#,
        )
        .expect("test policy should parse");
        let now = parse_optional_now(Some("2026-08-03T08:30:00+00:00".to_string()))
            .expect("test timestamp should parse");

        let temp = tempfile::tempdir().expect("temporary database directory should exist");
        let core = FocusCore::new(
            config,
            Database::open(temp.path().join("blockuntu.sqlite3"))
                .expect("temporary database should open"),
        )
        .expect("test core should initialize");
        let (entries, unsupported) = chromium_incognito_url_blocklist(&core, now, false)
            .expect("private URL policy should build");

        assert_eq!(
            entries,
            vec![
                ".exact.example".to_string(),
                "blocked.example".to_string(),
                "controlled.example".to_string(),
                "https://blocked.example/path".to_string(),
                "https://scheduled.example/path".to_string(),
            ]
        );
        assert_eq!(unsupported, 1);
    }

    #[test]
    fn private_browsing_disable_scope_and_settings_access_follow_schedule_activity() {
        let config = Config::from_toml_str(
            r#"
            [[schedules]]
            id = "monday"
            windows = [{ weekday = "mon", start = "10:00", end = "11:00" }]
            "#,
        )
        .expect("test policy should parse");
        let temp = tempfile::tempdir().expect("temporary database directory should exist");
        let core = FocusCore::new(
            config,
            Database::open(temp.path().join("blockuntu.sqlite3"))
                .expect("temporary database should open"),
        )
        .expect("test core should initialize");
        let active = parse_optional_now(Some("2026-08-03T08:30:00+00:00".to_string()))
            .expect("active timestamp should parse");
        let inactive = parse_optional_now(Some("2026-08-03T12:30:00+00:00".to_string()))
            .expect("inactive timestamp should parse");
        core.database()
            .set_service_state(
                CHROMIUM_INCOGNITO_DISABLE_SCOPE_KEY,
                "active_schedule_or_detox",
                active.with_timezone(&Utc),
            )
            .expect("disable scope should persist");
        core.database()
            .set_service_state(
                super::PROTECTED_ACCESS_MODE_KEY,
                "no_active_schedule_or_detox",
                active.with_timezone(&Utc),
            )
            .expect("protected access mode should persist");

        let active_settings = chromium_incognito_policy_settings(
            &core,
            active,
            false,
            ChromiumIncognitoMode::Disabled,
        )
        .expect("active settings should build");
        assert!(active_settings.private_browsing_disabled);
        assert_eq!(active_settings.mode, ChromiumIncognitoMode::Disabled);
        assert!(
            !chromium_incognito_settings_change_allowed(&core, active, false)
                .expect("change access should evaluate")
        );

        let inactive_settings = chromium_incognito_policy_settings(
            &core,
            inactive,
            false,
            ChromiumIncognitoMode::Disabled,
        )
        .expect("inactive settings should build");
        assert!(!inactive_settings.private_browsing_disabled);
        assert_eq!(inactive_settings.mode, ChromiumIncognitoMode::ManualConsent);
        assert!(
            chromium_incognito_settings_change_allowed(&core, inactive, false)
                .expect("change access should evaluate")
        );

        let tampered_settings = chromium_incognito_policy_settings(
            &core,
            inactive,
            true,
            ChromiumIncognitoMode::Disabled,
        )
        .expect("tampered-clock settings should build");
        assert!(tampered_settings.private_browsing_disabled);
        assert!(
            !chromium_incognito_settings_change_allowed(&core, inactive, true)
                .expect("tampered clock should lock settings")
        );
    }

    fn editable_rpc_context() -> RpcContext {
        rpc_context_with_config_toml(
            r#"
            [[schedules]]
            id = "work-hours"
            name = "Work hours"

            [[schedules.windows]]
            weekday = "mon"
            start = "09:00"
            end = "17:00"

            [[rules]]
            id = "controlled"
            name = "Controlled"
            tier = "controlled_access"
            schedule_ids = ["work-hours"]
            patterns = [
              { kind = "domain", value = "controlled.example", match_subdomains = true }
            ]
            "#,
        )
    }

    fn active_scheduled_rpc_context() -> RpcContext {
        rpc_context_with_config_toml(
            r#"
            [[schedules]]
            id = "work-hours"
            name = "Work hours"

            [[schedules.windows]]
            weekday = "fri"
            start = "09:00"
            end = "17:00"

            [[rules]]
            id = "controlled"
            name = "Controlled"
            tier = "controlled_access"
            schedule_ids = ["work-hours"]
            patterns = [
              { kind = "domain", value = "controlled.example", match_subdomains = true }
            ]
            "#,
        )
    }

    fn active_sunday_scheduled_rpc_context() -> RpcContext {
        rpc_context_with_config_toml(
            r#"
            [[schedules]]
            id = "work-hours"
            name = "Work hours"

            [[schedules.windows]]
            weekday = "sun"
            start = "20:00"
            end = "23:59"

            [[rules]]
            id = "controlled"
            name = "Controlled"
            tier = "controlled_access"
            schedule_ids = ["work-hours"]
            patterns = [
              { kind = "domain", value = "controlled.example", match_subdomains = true }
            ]
            "#,
        )
    }

    fn active_app_rule_rpc_context() -> RpcContext {
        rpc_context_with_config_toml(
            r#"
            [[schedules]]
            id = "work-hours"
            name = "Work hours"

            [[schedules.windows]]
            weekday = "fri"
            start = "09:00"
            end = "17:00"

            [[app_rules]]
            id = "kmines-controlled"
            name = "KMines"
            tier = "controlled_access"
            schedule_ids = ["work-hours"]
            matchers = [
              { kind = "command_name", value = "kmines" }
            ]
            "#,
        )
    }

    fn rpc_context_with_config_toml(toml: &str) -> RpcContext {
        let config = Config::from_toml_str(toml).expect("config should parse");
        let database = Database::in_memory().expect("database should initialize");
        let core = FocusCore::new(config, database).expect("core should initialize");
        RpcContext::new(Arc::new(Mutex::new(core))).with_trusted_client_time()
    }

    fn rpc_context_with_tier1_edit_credential(
        context: RpcContext,
    ) -> (tempfile::TempDir, RpcContext) {
        let temp = tempfile::tempdir().expect("temp dir should be created");
        let core = context.core.lock().expect("core should lock");
        let salt = "test-tier1-edit-salt";
        core.database()
            .set_service_state(super::TIER1_EDIT_CREDENTIAL_SALT_KEY, salt, Utc::now())
            .expect("tier 1 salt should persist");
        core.database()
            .set_service_state(
                super::TIER1_EDIT_CREDENTIAL_HASH_KEY,
                &super::tier1_edit_credential_hash(salt, "BLOCKUNTU-TIER1-EDIT-TEST"),
                Utc::now(),
            )
            .expect("tier 1 credential should persist");
        drop(core);
        (temp, context)
    }

    fn rpc_context_with_enforcement_managers(
        manage_browser_policies: bool,
    ) -> (tempfile::TempDir, RpcContext) {
        rpc_context_with_enforcement_managers_for(rpc_context(), manage_browser_policies)
    }

    fn rpc_context_with_enforcement_managers_for(
        context: RpcContext,
        manage_browser_policies: bool,
    ) -> (tempfile::TempDir, RpcContext) {
        let temp = tempfile::tempdir().expect("temp dir should be created");
        let gecko_policies = vec![
            GeckoPolicyBinding::new(
                SupportedBrowser::Firefox,
                FirefoxPolicyManager::new(
                    temp.path().join("firefox/policies.json"),
                    "{a7c3f3c4-6b1e-4c6f-9f2a-8d4e5b7c1a90}",
                    "https://addons.mozilla.org/firefox/downloads/latest/blockuntu/latest.xpi",
                ),
            ),
            GeckoPolicyBinding::new(
                SupportedBrowser::LibreWolf,
                FirefoxPolicyManager::merging_existing_policy(
                    temp.path().join("librewolf/distribution/policies.json"),
                    "{a7c3f3c4-6b1e-4c6f-9f2a-8d4e5b7c1a90}",
                    "https://addons.mozilla.org/firefox/downloads/latest/blockuntu/latest.xpi",
                    temp.path().join("backups/librewolf.json"),
                ),
            ),
            GeckoPolicyBinding::new(
                SupportedBrowser::Waterfox,
                FirefoxPolicyManager::merging_existing_policy(
                    temp.path().join("waterfox/distribution/policies.json"),
                    "{a7c3f3c4-6b1e-4c6f-9f2a-8d4e5b7c1a90}",
                    "https://addons.mozilla.org/firefox/downloads/latest/blockuntu/latest.xpi",
                    temp.path().join("backups/waterfox.json"),
                ),
            ),
        ];
        let chromium_policies = vec![
            ChromiumPolicyBinding::new(
                SupportedBrowser::Chrome,
                ChromePolicyManager::for_browser(
                    temp.path().join("chrome/policies/managed/blockuntu.json"),
                    "opfljaancedgklbpnbpjfhdbbhbfpnoc",
                    "Chrome",
                ),
            ),
            ChromiumPolicyBinding::new(
                SupportedBrowser::Chromium,
                ChromePolicyManager::for_browser(
                    temp.path().join("chromium/policies/managed/blockuntu.json"),
                    "opfljaancedgklbpnbpjfhdbbhbfpnoc",
                    "Chromium",
                ),
            ),
            ChromiumPolicyBinding::new(
                SupportedBrowser::Brave,
                ChromePolicyManager::for_browser(
                    temp.path().join("brave/policies/managed/blockuntu.json"),
                    "opfljaancedgklbpnbpjfhdbbhbfpnoc",
                    "Brave",
                ),
            ),
            ChromiumPolicyBinding::new(
                SupportedBrowser::Opera,
                ChromePolicyManager::for_browser(
                    temp.path().join("opera/policies/managed/blockuntu.json"),
                    "opfljaancedgklbpnbpjfhdbbhbfpnoc",
                    "Opera",
                ),
            ),
            ChromiumPolicyBinding::new(
                SupportedBrowser::Edge,
                ChromePolicyManager::for_browser(
                    temp.path().join("edge/policies/managed/blockuntu.json"),
                    "opfljaancedgklbpnbpjfhdbbhbfpnoc",
                    "Microsoft Edge",
                ),
            ),
            ChromiumPolicyBinding::new(
                SupportedBrowser::Vivaldi,
                ChromePolicyManager::for_browser(
                    temp.path().join("vivaldi/policies/managed/blockuntu.json"),
                    "opfljaancedgklbpnbpjfhdbbhbfpnoc",
                    "Vivaldi",
                ),
            ),
        ];
        let hosts = HostsManager::new(temp.path().join("hosts"));
        let context = context
            .with_enforcement_managers(gecko_policies, chromium_policies, hosts)
            .with_browser_policy_management(manage_browser_policies, manage_browser_policies);
        (temp, context)
    }

    #[test]
    fn handles_jsonrpc_evaluate_url() {
        let context = rpc_context();
        let request = json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "evaluate_url",
            "params": {
                "url": "https://sub.blocked.example/",
                "now": "2026-05-24T20:00:00+02:00"
            }
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&request).unwrap(),
        ))
        .expect("response should parse");

        assert_eq!(response["id"], 7);
        assert_eq!(response["result"]["decision"], "block");
        assert_eq!(response["result"]["reason"]["kind"], "hard_block");
    }

    #[test]
    fn notification_preferences_rpc_normalizes_and_roundtrips_thresholds() {
        let context = rpc_context();
        let update = json!({
            "jsonrpc": "2.0",
            "id": 701,
            "method": "set_notification_preferences",
            "params": {
                "preferences": {
                    "enabled": true,
                    "website_blocked": false,
                    "application_blocked": true,
                    "allowance_warnings": true,
                    "allowance_warning_minutes": [1, 10, 5, 10],
                    "schedule_started": true,
                    "schedule_ended": false,
                    "detox_started": true,
                    "detox_ended": false
                }
            }
        });
        let updated: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&update).unwrap(),
        ))
        .expect("response should parse");
        assert_eq!(
            updated["result"]["allowance_warning_minutes"],
            json!([10, 5, 1])
        );

        let get = json!({
            "jsonrpc": "2.0",
            "id": 702,
            "method": "notification_preferences",
            "params": {}
        });
        let loaded: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&get).unwrap(),
        ))
        .expect("response should parse");
        assert_eq!(loaded["result"], updated["result"]);
    }

    #[test]
    fn blocked_website_notifications_are_deduplicated_and_acknowledged() {
        let context = rpc_context();
        let now = Local::now().to_rfc3339();
        let evaluate = json!({
            "jsonrpc": "2.0",
            "id": 703,
            "method": "evaluate_url",
            "params": {
                "url": "https://blocked.example/watch",
                "now": now
            }
        });
        for _ in 0..2 {
            let response: Value = serde_json::from_slice(&handle_payload(
                &context,
                &serde_json::to_vec(&evaluate).unwrap(),
            ))
            .expect("response should parse");
            assert_eq!(response["result"]["decision"], "block");
        }

        let pending = json!({
            "jsonrpc": "2.0",
            "id": 704,
            "method": "pending_notifications",
            "params": { "limit": 20 }
        });
        let pending_response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&pending).unwrap(),
        ))
        .expect("response should parse");
        let notifications = pending_response["result"]["notifications"]
            .as_array()
            .expect("notifications should be an array");
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0]["kind"], "website_blocked");
        let id = notifications[0]["id"].as_i64().expect("id should exist");

        let acknowledge = json!({
            "jsonrpc": "2.0",
            "id": 705,
            "method": "acknowledge_notifications",
            "params": { "ids": [id] }
        });
        let acknowledged: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&acknowledge).unwrap(),
        ))
        .expect("response should parse");
        assert_eq!(acknowledged["result"]["acknowledged"], 1);

        let after_ack: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&pending).unwrap(),
        ))
        .expect("response should parse");
        assert!(after_ack["result"]["notifications"]
            .as_array()
            .expect("notifications should be an array")
            .is_empty());
    }

    #[test]
    fn blocked_url_probe_does_not_queue_a_notification() {
        let context = rpc_context();
        let evaluate = json!({
            "jsonrpc": "2.0",
            "id": 706,
            "method": "evaluate_url",
            "params": {
                "url": "https://blocked.example/watch",
                "probe": true,
                "now": Local::now().to_rfc3339()
            }
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&evaluate).unwrap(),
        ))
        .expect("response should parse");
        assert_eq!(response["result"]["decision"], "block");

        let pending = json!({
            "jsonrpc": "2.0",
            "id": 707,
            "method": "pending_notifications",
            "params": { "limit": 20 }
        });
        let pending_response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&pending).unwrap(),
        ))
        .expect("response should parse");
        assert!(pending_response["result"]["notifications"]
            .as_array()
            .expect("notifications should be an array")
            .is_empty());
    }

    #[test]
    fn notification_delivery_result_rpc_records_success() {
        let context = rpc_context();
        let now = Local::now().to_rfc3339();
        let evaluate = json!({
            "jsonrpc": "2.0",
            "id": 706,
            "method": "evaluate_url",
            "params": {
                "url": "https://blocked.example/delivery",
                "now": now
            }
        });
        let _: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&evaluate).unwrap(),
        ))
        .expect("evaluation response should parse");
        let pending = json!({
            "jsonrpc": "2.0",
            "id": 707,
            "method": "pending_notifications",
            "params": { "limit": 20 }
        });
        let pending_response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&pending).unwrap(),
        ))
        .expect("pending response should parse");
        let id = pending_response["result"]["notifications"][0]["id"]
            .as_i64()
            .expect("notification id");

        let delivery = json!({
            "jsonrpc": "2.0",
            "id": 708,
            "method": "record_notification_delivery",
            "params": {
                "id": id,
                "delivered": true,
                "detail": "accepted by desktop notification service"
            }
        });
        let delivery_response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&delivery).unwrap(),
        ))
        .expect("delivery response should parse");
        assert_eq!(delivery_response["result"]["status"], "accepted");

        let after_delivery: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&pending).unwrap(),
        ))
        .expect("pending response should parse");
        assert!(after_delivery["result"]["notifications"]
            .as_array()
            .expect("notifications should be an array")
            .is_empty());
    }

    #[test]
    fn summarizes_events_from_the_plain_log_file() {
        let temp = tempfile::tempdir().expect("temp dir should be created");
        let log_path = temp.path().join("blockuntu.log");
        std::fs::write(
            &log_path,
            concat!(
                "2026-07-15T10:00:00Z kind=\"website_blocked\" target=\"example.com\" details=None\n",
                "2026-07-15T10:01:00Z kind=\"app_blocked\" target=\"game\" details=None\n",
                "2026-07-15T10:02:00Z kind=\"website_blocked\" target=\"example.org\" details=None\n"
            ),
        )
        .expect("event log should write");
        let context = rpc_context().with_event_log_path(&log_path);
        let request = json!({
            "jsonrpc": "2.0",
            "id": 8,
            "method": "log_summary",
            "params": {}
        });

        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&request).unwrap(),
        ))
        .expect("response should parse");

        assert_eq!(response["result"]["path"], log_path.display().to_string());
        assert_eq!(response["result"]["total_events"], 3);
        assert_eq!(response["result"]["event_counts"]["website_blocked"], 2);
        assert_eq!(response["result"]["event_counts"]["app_blocked"], 1);
    }

    #[test]
    fn accumulates_schedule_activity_across_statistics_requests() {
        let context = active_scheduled_rpc_context();
        let initial_request = json!({
            "jsonrpc": "2.0",
            "id": 9,
            "method": "schedule_activity_summary",
            "params": { "now": "2026-05-22T08:00:00+02:00" }
        });
        let initial_response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&initial_request).unwrap(),
        ))
        .expect("initial response should parse");
        assert_eq!(
            initial_response["result"]["schedules"][0]["total_active_seconds"],
            0
        );

        let later_request = json!({
            "jsonrpc": "2.0",
            "id": 10,
            "method": "schedule_activity_summary",
            "params": { "now": "2026-05-22T11:00:00+02:00" }
        });
        let later_response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&later_request).unwrap(),
        ))
        .expect("later response should parse");

        assert_eq!(
            later_response["result"]["schedules"][0]["total_active_seconds"],
            2 * 60 * 60
        );
    }

    #[test]
    fn exports_policy_as_toml() {
        let context = rpc_context();
        let request = json!({
            "jsonrpc": "2.0",
            "id": 72,
            "method": "export_policy_toml",
            "params": {}
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&request).unwrap(),
        ))
        .expect("response should parse");

        assert!(response.get("error").is_none(), "{response}");
        let toml = response["result"]["toml"]
            .as_str()
            .expect("TOML export should be a string");
        assert!(toml.contains("[[rules]]"));
        assert!(toml.contains("blocked.example"));
    }

    #[test]
    fn appends_policy_import_without_tier1_unlock() {
        let context = rpc_context();
        let request = json!({
            "jsonrpc": "2.0",
            "id": 73,
            "method": "import_policy_toml",
            "params": {
                "now": "2026-05-24T20:00:00+02:00",
                "toml": r#"
                  [[rules]]
                  id = "replacement"
                  name = "Replacement"
                  tier = "hard"
                  patterns = [
                    { kind = "domain", value = "replacement.example", match_subdomains = true }
                  ]
                "#
            }
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&request).unwrap(),
        ))
        .expect("response should parse");

        assert!(response.get("error").is_none(), "{response}");
        assert_eq!(response["result"]["status"], "ok");
        assert_eq!(response["result"]["added"]["rules"], 1);
        let rules = response["result"]["config"]["rules"]
            .as_array()
            .expect("rules should be an array");
        assert!(rules.iter().any(|rule| rule["id"] == "hard"));
        assert!(rules.iter().any(|rule| rule["id"] == "replacement"));
    }

    #[test]
    fn policy_import_updates_recovery_snapshot() {
        let temp = tempfile::tempdir().expect("temp dir should be created");
        let recovery = PolicyRecoveryManager::new(temp.path().join("policy-recovery.toml"), false);
        let context = rpc_context().with_policy_recovery(recovery.clone());
        let request = json!({
            "jsonrpc": "2.0",
            "id": 731,
            "method": "import_policy_toml",
            "params": {
                "now": "2026-05-24T20:00:00+02:00",
                "toml": r#"
                  [[rules]]
                  id = "snapshot-rule"
                  name = "Snapshot"
                  tier = "hard"
                  patterns = [
                    { kind = "domain", value = "snapshot.example", match_subdomains = true }
                  ]
                "#
            }
        });

        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&request).unwrap(),
        ))
        .expect("response should parse");

        assert!(response.get("error").is_none(), "{response}");
        let snapshot = recovery
            .load()
            .expect("recovery snapshot should load")
            .expect("recovery snapshot should exist");
        assert!(snapshot.rules.iter().any(|rule| rule.id == "snapshot-rule"));
    }

    #[test]
    fn skips_identical_policy_import_items() {
        let context = rpc_context();
        let request = json!({
            "jsonrpc": "2.0",
            "id": 74,
            "method": "import_policy_toml",
            "params": {
                "now": "2026-05-24T20:30:00+02:00",
                "toml": r#"
                  [[rules]]
                  id = "hard"
                  name = "Hard"
                  tier = "hard"
                  patterns = [
                    { kind = "domain", value = "blocked.example", match_subdomains = true }
                  ]
                "#
            }
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&request).unwrap(),
        ))
        .expect("response should parse");

        assert!(response.get("error").is_none(), "{response}");
        assert_eq!(response["result"]["status"], "ok");
        assert_eq!(response["result"]["added"]["total"], 0);
        assert_eq!(
            response["result"]["config"]["rules"]
                .as_array()
                .expect("rules should be an array")
                .len(),
            1
        );
        assert!(response["result"]["config"]["app_rules"]
            .as_array()
            .expect("app rules should be an array")
            .iter()
            .any(|rule| rule["id"] == "unsupported-browsers-hard"));
    }

    #[test]
    fn rejects_conflicting_policy_import_ids() {
        let context = rpc_context();
        let request = json!({
            "jsonrpc": "2.0",
            "id": 75,
            "method": "import_policy_toml",
            "params": {
                "now": "2026-05-24T20:30:00+02:00",
                "toml": r#"
                  [[rules]]
                  id = "hard"
                  name = "Hard changed"
                  tier = "hard"
                  patterns = [
                    { kind = "domain", value = "changed.example", match_subdomains = true }
                  ]
                "#
            }
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&request).unwrap(),
        ))
        .expect("response should parse");

        assert_eq!(response["error"]["code"], -32602);
        assert!(response["error"]["data"]
            .as_str()
            .expect("error data should be a string")
            .contains("imported site list 'hard' conflicts"));
    }

    #[test]
    fn manual_browser_extension_mode_does_not_write_browser_policies() {
        let (temp, context) = rpc_context_with_enforcement_managers(false);
        let request = json!({
            "jsonrpc": "2.0",
            "id": 71,
            "method": "enforcement_status",
            "params": {}
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&request).unwrap(),
        ))
        .expect("response should parse");

        assert!(response.get("error").is_none(), "{response}");
        assert_eq!(response["result"]["firefox_policy"]["managed"], false);
        assert_eq!(response["result"]["chrome_policy"]["managed"], false);
        assert_eq!(response["result"]["firefox_policy"]["compliant"], true);
        assert_eq!(response["result"]["chrome_policy"]["compliant"], true);
        assert!(!temp.path().join("firefox/policies.json").exists());
        assert!(!temp
            .path()
            .join("chrome/policies/managed/blockuntu.json")
            .exists());
    }

    #[test]
    fn enforcement_start_and_stop_methods_are_not_available() {
        let context = rpc_context();
        for method in ["start_enforcement", "stop_enforcement"] {
            let request = json!({
                "jsonrpc": "2.0",
                "id": 711,
                "method": method,
                "params": {}
            });
            let response: Value = serde_json::from_slice(&handle_payload(
                &context,
                &serde_json::to_vec(&request).unwrap(),
            ))
            .expect("response should parse");

            assert_eq!(response["error"]["code"], -32601);
        }
    }

    #[test]
    fn deferred_browser_policy_repair_waits_for_first_heartbeat() {
        let (temp, context) = rpc_context_with_enforcement_managers(true);
        let context = context.with_deferred_browser_policy_repair(true, true);
        let status_request = json!({
            "jsonrpc": "2.0",
            "id": 72,
            "method": "enforcement_status",
            "params": {}
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&status_request).unwrap(),
        ))
        .expect("response should parse");

        assert!(response.get("error").is_none(), "{response}");
        assert_eq!(
            response["result"]["firefox_policy"]["deferred_until_heartbeat"],
            true
        );
        assert_eq!(
            response["result"]["firefox_policy"]["active_after_heartbeat"],
            false
        );
        for browser in ["librewolf", "waterfox"] {
            assert_eq!(
                response["result"]["firefox_family_policies"][browser]["deferred_until_heartbeat"],
                true
            );
            assert_eq!(
                response["result"]["firefox_family_policies"][browser]["active_after_heartbeat"],
                false
            );
        }
        assert_eq!(
            response["result"]["chrome_policy"]["deferred_until_heartbeat"],
            true
        );
        assert_eq!(
            response["result"]["chrome_policy"]["active_after_heartbeat"],
            false
        );
        for browser in ["chromium", "brave", "opera", "edge", "vivaldi"] {
            assert_eq!(
                response["result"]["chromium_policies"][browser]["deferred_until_heartbeat"],
                true
            );
            assert_eq!(
                response["result"]["chromium_policies"][browser]["active_after_heartbeat"],
                false
            );
        }
        assert!(!temp.path().join("firefox/policies.json").exists());
        assert!(!temp
            .path()
            .join("chrome/policies/managed/blockuntu.json")
            .exists());

        for (id, component, browser, policy_path) in [
            (
                741,
                "librewolf_extension",
                "librewolf",
                "librewolf/distribution/policies.json",
            ),
            (
                742,
                "waterfox_extension",
                "waterfox",
                "waterfox/distribution/policies.json",
            ),
        ] {
            let heartbeat = json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "extension_heartbeat",
                "params": {
                    "component": component,
                    "browser": browser,
                    "extension_id": "{a7c3f3c4-6b1e-4c6f-9f2a-8d4e5b7c1a90}",
                    "extension_version": "0.2.4"
                }
            });
            let response: Value = serde_json::from_slice(&handle_payload(
                &context,
                &serde_json::to_vec(&heartbeat).unwrap(),
            ))
            .expect("response should parse");

            assert!(response.get("error").is_none(), "{response}");
            assert!(temp.path().join(policy_path).exists());
        }

        let firefox_heartbeat = json!({
            "jsonrpc": "2.0",
            "id": 73,
            "method": "extension_heartbeat",
            "params": {
                "component": "firefox_extension",
                "browser": "firefox",
                "extension_id": "{a7c3f3c4-6b1e-4c6f-9f2a-8d4e5b7c1a90}",
                "extension_version": "0.1.0"
            }
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&firefox_heartbeat).unwrap(),
        ))
        .expect("response should parse");

        assert!(response.get("error").is_none(), "{response}");
        assert!(temp.path().join("firefox/policies.json").exists());
        let firefox_policy = std::fs::read_to_string(temp.path().join("firefox/policies.json"))
            .expect("Firefox policy should be readable");
        assert!(firefox_policy
            .contains("https://addons.mozilla.org/firefox/downloads/latest/blockuntu/latest.xpi"));
        assert!(!temp
            .path()
            .join("chrome/policies/managed/blockuntu.json")
            .exists());

        let chrome_heartbeat = json!({
            "jsonrpc": "2.0",
            "id": 74,
            "method": "extension_heartbeat",
            "params": {
                "component": "chrome_extension",
                "browser": "chrome",
                "extension_id": "opfljaancedgklbpnbpjfhdbbhbfpnoc",
                "extension_version": "0.2.1"
            }
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&chrome_heartbeat).unwrap(),
        ))
        .expect("response should parse");

        assert!(response.get("error").is_none(), "{response}");
        assert!(temp
            .path()
            .join("chrome/policies/managed/blockuntu.json")
            .exists());

        for (id, component, browser, policy_path) in [
            (
                75,
                "chromium_extension",
                "chromium",
                "chromium/policies/managed/blockuntu.json",
            ),
            (
                76,
                "brave_extension",
                "brave",
                "brave/policies/managed/blockuntu.json",
            ),
            (
                77,
                "opera_extension",
                "opera",
                "opera/policies/managed/blockuntu.json",
            ),
            (
                78,
                "edge_extension",
                "edge",
                "edge/policies/managed/blockuntu.json",
            ),
            (
                79,
                "vivaldi_extension",
                "vivaldi",
                "vivaldi/policies/managed/blockuntu.json",
            ),
        ] {
            let heartbeat = json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "extension_heartbeat",
                "params": {
                    "component": component,
                    "browser": browser,
                    "extension_id": "opfljaancedgklbpnbpjfhdbbhbfpnoc",
                    "extension_version": "0.2.1"
                }
            });
            let response: Value = serde_json::from_slice(&handle_payload(
                &context,
                &serde_json::to_vec(&heartbeat).unwrap(),
            ))
            .expect("response should parse");

            assert!(response.get("error").is_none(), "{response}");
            assert!(temp.path().join(policy_path).exists());
        }
    }

    #[test]
    fn parses_client_utc_now_as_daemon_local_wall_time() {
        let client_now = "2026-05-22T16:30:00Z";
        let parsed_client_now =
            chrono::DateTime::parse_from_rfc3339(client_now).expect("test timestamp should parse");
        let expected = parsed_client_now.with_timezone(&Local).fixed_offset();

        let parsed = parse_optional_now(Some(client_now.to_string())).expect("now should parse");

        assert_eq!(parsed, expected);
    }

    #[test]
    fn evaluates_browser_utc_timestamps_against_local_schedule_time() {
        let local_after_window = Local
            .with_ymd_and_hms(2026, 5, 22, 18, 30, 0)
            .single()
            .expect("local test timestamp should be unambiguous");
        let browser_now = local_after_window.with_timezone(&Utc).to_rfc3339();

        let context = active_scheduled_rpc_context();
        let request = json!({
            "jsonrpc": "2.0",
            "id": 8,
            "method": "evaluate_url",
            "params": {
                "url": "https://controlled.example/",
                "now": browser_now
            }
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&request).unwrap(),
        ))
        .expect("response should parse");

        assert_eq!(response["result"]["decision"], "allow");
    }

    #[test]
    fn evaluate_url_reports_whether_an_allowed_site_should_be_metered() {
        let context = rpc_context_with_config_toml(
            r#"
            [[allowances]]
            id = "daily"
            daily_minutes = 30

            [[schedules]]
            id = "work-hours"

            [[schedules.windows]]
            weekday = "fri"
            start = "09:00"
            end = "17:00"

            [[rules]]
            id = "controlled"
            name = "Controlled"
            tier = "controlled_access"
            schedule_ids = ["work-hours"]
            allowance_id = "daily"
            patterns = [
              { kind = "domain", value = "controlled.example", match_subdomains = true }
            ]
            "#,
        );

        for (id, now, expected_metering) in [
            (41, "2026-05-22T10:30:00+02:00", true),
            (42, "2026-05-22T18:30:00+02:00", false),
        ] {
            let request = json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "evaluate_url",
                "params": {
                    "url": "https://controlled.example/",
                    "now": now
                }
            });
            let response: Value = serde_json::from_slice(&handle_payload(
                &context,
                &serde_json::to_vec(&request).unwrap(),
            ))
            .expect("response should parse");

            assert_eq!(response["result"]["decision"], "allow");
            assert_eq!(response["result"]["metering_active"], expected_metering);
        }
    }

    #[test]
    fn allowance_reset_uses_local_midnight() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-05-18T23:30:00+02:00")
            .expect("timestamp should parse");

        assert_eq!(
            super::next_allowance_reset_at(now).to_rfc3339(),
            "2026-05-19T00:00:00+02:00"
        );
    }

    #[test]
    fn includes_schedule_details_for_controlled_blocks() {
        let local_inside_window = Local
            .with_ymd_and_hms(2026, 5, 22, 10, 30, 0)
            .single()
            .expect("local test timestamp should be unambiguous");
        let browser_now = local_inside_window.with_timezone(&Utc).to_rfc3339();
        let local_window_end = Local
            .with_ymd_and_hms(2026, 5, 22, 17, 0, 0)
            .single()
            .expect("local test timestamp should be unambiguous")
            .fixed_offset()
            .to_rfc3339();

        let context = active_scheduled_rpc_context();
        let request = json!({
            "jsonrpc": "2.0",
            "id": 18,
            "method": "evaluate_url",
            "params": {
                "url": "https://controlled.example/",
                "now": browser_now
            }
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&request).unwrap(),
        ))
        .expect("response should parse");

        let reason = &response["result"]["reason"];
        assert_eq!(response["result"]["decision"], "block");
        assert_eq!(reason["kind"], "controlled_access");
        assert_eq!(reason["tier"], "tier_3");
        assert_eq!(reason["blocked_by"], "schedule");
        assert_eq!(reason["active_schedules"][0]["id"], "work-hours");
        assert_eq!(reason["free_at"], local_window_end);
    }

    #[test]
    fn includes_grouped_schedule_details_for_controlled_blocks() {
        let local_inside_window = Local
            .with_ymd_and_hms(2026, 5, 22, 10, 30, 0)
            .single()
            .expect("local test timestamp should be unambiguous");
        let browser_now = local_inside_window.with_timezone(&Utc).to_rfc3339();
        let local_window_end = Local
            .with_ymd_and_hms(2026, 5, 22, 17, 0, 0)
            .single()
            .expect("local test timestamp should be unambiguous")
            .fixed_offset()
            .to_rfc3339();
        let context = rpc_context_with_config_toml(
            r#"
            [[schedules]]
            id = "workday-hours"
            name = "Workday hours"

            [[schedules.windows]]
            weekday = "workdays"
            start = "09:00"
            end = "17:00"

            [[rules]]
            id = "controlled"
            name = "Controlled"
            tier = "controlled_access"
            schedule_ids = ["workday-hours"]
            patterns = [
              { kind = "domain", value = "controlled.example", match_subdomains = true }
            ]
            "#,
        );
        let request = json!({
            "jsonrpc": "2.0",
            "id": 19,
            "method": "evaluate_url",
            "params": {
                "url": "https://controlled.example/",
                "now": browser_now
            }
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&request).unwrap(),
        ))
        .expect("response should parse");

        let reason = &response["result"]["reason"];
        assert_eq!(response["result"]["decision"], "block");
        assert_eq!(reason["blocked_by"], "schedule");
        assert_eq!(reason["active_schedules"][0]["id"], "workday-hours");
        assert_eq!(reason["free_at"], local_window_end);
    }

    #[test]
    fn handles_legacy_url_request_for_native_host_compatibility() {
        let context = rpc_context();
        let request = json!({ "url": "https://blocked.example/" });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&request).unwrap(),
        ))
        .expect("response should parse");

        assert_eq!(response["action"], "block");
    }

    #[test]
    fn upserts_site_list_after_validation_and_reloads_core() {
        let context = editable_rpc_context();
        let request = json!({
            "jsonrpc": "2.0",
            "id": 9,
            "method": "upsert_site_list",
            "params": {
                "now": "2026-05-22T10:00:00Z",
                "rule": {
                    "id": "new-hard",
                    "name": "New Hard",
                    "tier": "hard",
                    "enabled": true,
                    "patterns": [
                        { "kind": "domain", "value": "new.example", "match_subdomains": true }
                    ],
                    "schedule_ids": []
                }
            }
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&request).unwrap(),
        ))
        .expect("response should parse");

        assert!(response.get("error").is_none(), "{response}");
        assert!(response["result"]["config"]["rules"]
            .as_array()
            .expect("rules should be an array")
            .iter()
            .any(|rule| rule["id"] == "new-hard"));

        let eval_request = json!({
            "jsonrpc": "2.0",
            "id": 10,
            "method": "evaluate_url",
            "params": {
                "url": "https://new.example/",
                "now": "2026-05-24T20:00:00+02:00"
            }
        });
        let eval_response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&eval_request).unwrap(),
        ))
        .expect("response should parse");
        assert_eq!(eval_response["result"]["decision"], "block");
        assert_eq!(eval_response["result"]["reason"]["rule_id"], "new-hard");
    }

    #[test]
    fn upserts_allowance_for_site_list_edits() {
        let context = editable_rpc_context();
        let allowance_request = json!({
            "jsonrpc": "2.0",
            "id": 21,
            "method": "upsert_allowance",
            "params": {
                "now": "2026-05-22T10:00:00Z",
                "allowance": {
                    "id": "daily",
                    "name": "Daily",
                    "daily_minutes": 20
                }
            }
        });
        let allowance_response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&allowance_request).unwrap(),
        ))
        .expect("response should parse");

        assert!(
            allowance_response.get("error").is_none(),
            "{allowance_response}"
        );
        assert!(allowance_response["result"]["config"]["allowances"]
            .as_array()
            .expect("allowances should be an array")
            .iter()
            .any(|allowance| allowance["id"] == "daily"));

        let rule_request = json!({
            "jsonrpc": "2.0",
            "id": 22,
            "method": "upsert_site_list",
            "params": {
                "now": "2026-05-22T10:00:00Z",
                "rule": {
                    "id": "daily-list",
                    "name": "Daily List",
                    "tier": "controlled_access",
                    "enabled": true,
                    "allowance_id": "daily",
                    "patterns": [
                        { "kind": "domain", "value": "daily.example", "match_subdomains": true }
                    ],
                    "schedule_ids": ["work-hours"]
                }
            }
        });
        let rule_response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&rule_request).unwrap(),
        ))
        .expect("response should parse");

        assert!(rule_response.get("error").is_none(), "{rule_response}");
        assert!(rule_response["result"]["config"]["rules"]
            .as_array()
            .expect("rules should be an array")
            .iter()
            .any(|rule| rule["allowance_id"] == "daily"));

        let delete_request = json!({
            "jsonrpc": "2.0",
            "id": 23,
            "method": "delete_site_list",
            "params": {
                "now": "2026-05-22T10:00:00Z",
                "id": "daily-list"
            }
        });
        let delete_response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&delete_request).unwrap(),
        ))
        .expect("response should parse");

        assert!(delete_response.get("error").is_none(), "{delete_response}");
        assert!(!delete_response["result"]["config"]["allowances"]
            .as_array()
            .expect("allowances should be an array")
            .iter()
            .any(|allowance| allowance["id"] == "daily"));
    }

    #[test]
    fn upserts_app_rule_after_validation_and_reloads_core() {
        let context = editable_rpc_context();
        let request = json!({
            "jsonrpc": "2.0",
            "id": 19,
            "method": "upsert_app_rule",
            "params": {
                "now": "2026-05-22T10:00:00Z",
                "rule": {
                    "id": "kmines-hard",
                    "name": "KMines",
                    "tier": "hard",
                    "enabled": true,
                    "matchers": [
                        { "kind": "command_name", "value": "kmines" },
                        { "kind": "desktop_id", "value": "org.kde.kmines.desktop" }
                    ],
                    "schedule_ids": ["work-hours"]
                }
            }
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&request).unwrap(),
        ))
        .expect("response should parse");

        assert!(response.get("error").is_none(), "{response}");
        assert!(response["result"]["config"]["app_rules"]
            .as_array()
            .expect("app rules should be an array")
            .iter()
            .any(|rule| rule["id"] == "kmines-hard"));
    }

    #[test]
    fn rejects_active_app_rule_edits() {
        let context = active_app_rule_rpc_context();
        let request = json!({
            "jsonrpc": "2.0",
            "id": 118,
            "method": "upsert_app_rule",
            "params": {
                "now": "2026-05-22T10:00:00Z",
                "rule": {
                    "id": "kmines-controlled",
                    "name": "KMines edited",
                    "tier": "controlled_access",
                    "enabled": true,
                    "matchers": [
                        { "kind": "command_name", "value": "different" }
                    ],
                    "schedule_ids": ["work-hours"]
                }
            }
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&request).unwrap(),
        ))
        .expect("response should parse");

        assert_eq!(response["error"]["code"], -32602);
        assert!(response["error"]["data"]
            .as_str()
            .expect("error data should be a string")
            .contains("app rule 'kmines-controlled' is currently active"));
    }

    #[test]
    fn allows_additive_active_app_rule_edits() {
        let context = active_app_rule_rpc_context();
        let request = json!({
            "jsonrpc": "2.0",
            "id": 119,
            "method": "upsert_app_rule",
            "params": {
                "now": "2026-05-22T10:00:00Z",
                "rule": {
                    "id": "kmines-controlled",
                    "name": "KMines",
                    "tier": "controlled_access",
                    "enabled": true,
                    "matchers": [
                        { "kind": "command_name", "value": "kmines" },
                        { "kind": "window_title_contains", "value": "KMines" }
                    ],
                    "schedule_ids": ["work-hours"]
                }
            }
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&request).unwrap(),
        ))
        .expect("response should parse");

        assert!(response.get("error").is_none(), "{response}");
        let app_rule = response["result"]["config"]["app_rules"]
            .as_array()
            .expect("app rules should be an array")
            .iter()
            .find(|rule| rule["id"] == "kmines-controlled")
            .expect("app rule should exist");
        assert_eq!(
            app_rule["matchers"]
                .as_array()
                .expect("matchers should be an array")
                .len(),
            2
        );
    }

    #[test]
    fn running_apps_reports_detected_identity_and_block_state() {
        let context = rpc_context_with_config_toml(
            r#"
            [[app_rules]]
            id = "vlc-hard"
            name = "VLC"
            tier = "hard"
            matchers = [
              { kind = "command_name", value = "VLC" },
              { kind = "desktop_id", value = "ORG.VIDEOLAN.VLC.DESKTOP" }
            ]
            "#,
        );
        let processes = vec![
            ProcessInfo {
                pid: 7,
                executable_path: Some("/usr/bin/blockuntu-gui".into()),
                executable_basename: Some("blockuntu-gui".into()),
                command_name: Some("blockuntu-gui".into()),
                desktop_id: Some("blockuntu.desktop".into()),
                window_titles: vec!["BlocKuntu".into()],
            },
            ProcessInfo {
                pid: 4242,
                executable_path: Some("/usr/bin/vlc".into()),
                executable_basename: Some("vlc".into()),
                command_name: Some("vlc".into()),
                desktop_id: Some("org.videolan.vlc.desktop".into()),
                window_titles: vec!["VLC media player".into()],
            },
        ];

        let apps = running_app_snapshots_from_processes(
            &context,
            &processes,
            parse_optional_now(Some("2026-06-19T10:00:00Z".to_string())).unwrap(),
            false,
        )
        .expect("running app snapshots should build");

        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].display_name, "vlc");
        assert_eq!(apps[0].decision, "block");
        assert_eq!(apps[0].blocking_rule_id.as_deref(), Some("vlc-hard"));
        assert_eq!(apps[0].blocking_rule_name.as_deref(), Some("VLC"));
    }

    #[test]
    fn upserts_allowance_for_app_rule_edits() {
        let context = editable_rpc_context();
        let allowance_request = json!({
            "jsonrpc": "2.0",
            "id": 24,
            "method": "upsert_allowance",
            "params": {
                "now": "2026-05-22T10:00:00Z",
                "allowance": {
                    "id": "app-daily",
                    "name": "App Daily",
                    "daily_minutes": 20
                }
            }
        });
        let allowance_response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&allowance_request).unwrap(),
        ))
        .expect("response should parse");
        assert!(
            allowance_response.get("error").is_none(),
            "{allowance_response}"
        );

        let app_rule_request = json!({
            "jsonrpc": "2.0",
            "id": 25,
            "method": "upsert_app_rule",
            "params": {
                "now": "2026-05-22T10:00:00Z",
                "rule": {
                    "id": "kmines-controlled",
                    "name": "KMines",
                    "tier": "controlled_access",
                    "enabled": true,
                    "allowance_id": "app-daily",
                    "matchers": [
                        { "kind": "command_name", "value": "kmines" }
                    ],
                    "schedule_ids": ["work-hours"]
                }
            }
        });
        let app_rule_response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&app_rule_request).unwrap(),
        ))
        .expect("response should parse");
        assert!(
            app_rule_response.get("error").is_none(),
            "{app_rule_response}"
        );
        assert!(app_rule_response["result"]["config"]["app_rules"]
            .as_array()
            .expect("app rules should be an array")
            .iter()
            .any(|rule| rule["allowance_id"] == "app-daily"));

        let delete_request = json!({
            "jsonrpc": "2.0",
            "id": 26,
            "method": "delete_app_rule",
            "params": {
                "now": "2026-05-22T10:00:00Z",
                "id": "kmines-controlled"
            }
        });
        let delete_response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&delete_request).unwrap(),
        ))
        .expect("response should parse");

        assert!(delete_response.get("error").is_none(), "{delete_response}");
        assert!(!delete_response["result"]["config"]["allowances"]
            .as_array()
            .expect("allowances should be an array")
            .iter()
            .any(|allowance| allowance["id"] == "app-daily"));
    }

    #[test]
    fn rejects_active_site_list_edits() {
        let context = active_scheduled_rpc_context();
        let request = json!({
            "jsonrpc": "2.0",
            "id": 11,
            "method": "upsert_site_list",
            "params": {
                "now": "2026-05-22T10:00:00Z",
                "rule": {
                    "id": "controlled",
                    "name": "Controlled edited",
                    "tier": "controlled_access",
                    "enabled": true,
                    "patterns": [
                        { "kind": "domain", "value": "different.example", "match_subdomains": true }
                    ],
                    "schedule_ids": ["work-hours"]
                }
            }
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&request).unwrap(),
        ))
        .expect("response should parse");

        assert_eq!(response["error"]["code"], -32602);
        assert!(response["error"]["data"]
            .as_str()
            .expect("error data should be a string")
            .contains("site list 'controlled' is currently active"));
    }

    #[test]
    fn allows_additive_active_site_list_edits() {
        let context = active_scheduled_rpc_context();
        let request = json!({
            "jsonrpc": "2.0",
            "id": 116,
            "method": "upsert_site_list",
            "params": {
                "now": "2026-05-22T10:00:00Z",
                "rule": {
                    "id": "controlled",
                    "name": "Controlled",
                    "tier": "controlled_access",
                    "enabled": true,
                    "patterns": [
                        { "kind": "domain", "value": "controlled.example", "match_subdomains": true },
                        { "kind": "url_contains", "value": "watch?v=shorts", "match_subdomains": false }
                    ],
                    "schedule_ids": ["work-hours"]
                }
            }
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&request).unwrap(),
        ))
        .expect("response should parse");

        assert!(response.get("error").is_none(), "{response}");
        let controlled_rule = response["result"]["config"]["rules"]
            .as_array()
            .expect("rules should be an array")
            .iter()
            .find(|rule| rule["id"] == "controlled")
            .expect("controlled rule should exist");
        assert_eq!(
            controlled_rule["patterns"]
                .as_array()
                .expect("patterns should be an array")
                .len(),
            2
        );
    }

    #[test]
    fn rejects_active_tier1_site_list_edits_without_unlock() {
        let context = rpc_context();
        let request = json!({
            "jsonrpc": "2.0",
            "id": 111,
            "method": "upsert_site_list",
            "params": {
                "now": "2026-05-22T10:00:00Z",
                "rule": {
                    "id": "hard",
                    "name": "Hard edited",
                    "tier": "hard",
                    "enabled": true,
                    "patterns": [
                        { "kind": "domain", "value": "edited.example", "match_subdomains": true }
                    ],
                    "schedule_ids": []
                }
            }
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&request).unwrap(),
        ))
        .expect("response should parse");

        assert_eq!(response["error"]["code"], -32602);
        assert!(response["error"]["data"]
            .as_str()
            .expect("error data should be a string")
            .contains("site list 'hard' is currently active"));
    }

    #[test]
    fn allows_additive_active_tier1_site_list_edits_without_unlock() {
        let context = rpc_context();
        let request = json!({
            "jsonrpc": "2.0",
            "id": 117,
            "method": "upsert_site_list",
            "params": {
                "now": "2026-05-22T10:00:00Z",
                "rule": {
                    "id": "hard",
                    "name": "Hard",
                    "tier": "hard",
                    "enabled": true,
                    "patterns": [
                        { "kind": "domain", "value": "blocked.example", "match_subdomains": true },
                        { "kind": "domain", "value": "extra-blocked.example", "match_subdomains": true }
                    ],
                    "schedule_ids": []
                }
            }
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&request).unwrap(),
        ))
        .expect("response should parse");

        assert!(response.get("error").is_none(), "{response}");
        let hard_rule = response["result"]["config"]["rules"]
            .as_array()
            .expect("rules should be an array")
            .iter()
            .find(|rule| rule["id"] == "hard")
            .expect("hard rule should exist");
        assert_eq!(
            hard_rule["patterns"]
                .as_array()
                .expect("patterns should be an array")
                .len(),
            2
        );
    }

    #[test]
    fn tier1_edit_unlock_allows_active_tier1_site_list_edits() {
        let (_temp, context) = rpc_context_with_tier1_edit_credential(rpc_context());
        let unlock_request = json!({
            "jsonrpc": "2.0",
            "id": 112,
            "method": "unlock_tier1_edit",
            "params": {
                "phrase": "BLOCKUNTU-TIER1-EDIT-TEST",
                "now": "2026-05-24T20:00:00+02:00"
            }
        });
        let unlock_response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&unlock_request).unwrap(),
        ))
        .expect("response should parse");

        assert!(unlock_response.get("error").is_none(), "{unlock_response}");
        assert_eq!(unlock_response["result"]["active"], true);

        let edit_request = json!({
            "jsonrpc": "2.0",
            "id": 113,
            "method": "upsert_site_list",
            "params": {
                "now": "2026-05-24T20:04:00+02:00",
                "rule": {
                    "id": "hard",
                    "name": "Hard edited",
                    "tier": "hard",
                    "enabled": true,
                    "patterns": [
                        { "kind": "domain", "value": "edited.example", "match_subdomains": true }
                    ],
                    "schedule_ids": []
                }
            }
        });
        let edit_response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&edit_request).unwrap(),
        ))
        .expect("response should parse");

        assert!(edit_response.get("error").is_none(), "{edit_response}");
        assert!(edit_response["result"]["config"]["rules"]
            .as_array()
            .expect("rules should be an array")
            .iter()
            .any(|rule| rule["id"] == "hard" && rule["name"] == "Hard edited"));
    }

    #[test]
    fn tier1_edit_unlock_is_limited_to_sunday_evening_operator_window() {
        let (_temp, context) = rpc_context_with_tier1_edit_credential(rpc_context());
        let core = context.core.lock().expect("core should lock");
        core.database()
            .set_service_state(super::OPERATOR_WINDOW_RESTRICTION_KEY, "true", Utc::now())
            .expect("operator restriction should persist");
        drop(core);
        let unlock_request = json!({
            "jsonrpc": "2.0",
            "id": 127,
            "method": "unlock_tier1_edit",
            "params": {
                "phrase": "BLOCKUNTU-TIER1-EDIT-TEST",
                "now": "2026-05-24T19:59:00+02:00"
            }
        });
        let unlock_response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&unlock_request).unwrap(),
        ))
        .expect("response should parse");

        assert_eq!(unlock_response["error"]["code"], -32602);
        assert!(unlock_response["error"]["data"]
            .as_str()
            .expect("error data should be a string")
            .contains("Sunday 20:00-23:59"));

        let status_request = json!({
            "jsonrpc": "2.0",
            "id": 128,
            "method": "tier1_edit_status",
            "params": {
                "now": "2026-05-24T20:00:00+02:00"
            }
        });
        let status_response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&status_request).unwrap(),
        ))
        .expect("response should parse");

        assert_eq!(
            status_response["result"]["operator_window_label"],
            "Sunday 20:00-23:59"
        );
        assert_eq!(status_response["result"]["operator_window_open"], true);
    }

    #[test]
    fn protected_access_mode_can_require_an_idle_schedule_and_detox_state() {
        let (_temp, idle_context) = rpc_context_with_tier1_edit_credential(rpc_context());
        let set_idle_mode = json!({
            "jsonrpc": "2.0",
            "id": 129,
            "method": "set_protected_access_mode",
            "params": { "mode": "no_active_schedule_or_detox" }
        });
        let set_idle_response: Value = serde_json::from_slice(&handle_payload(
            &idle_context,
            &serde_json::to_vec(&set_idle_mode).unwrap(),
        ))
        .expect("response should parse");
        assert_eq!(
            set_idle_response["result"]["mode"],
            "no_active_schedule_or_detox"
        );
        let idle_core = idle_context.core.lock().expect("core should lock");
        for key in [
            super::PROTECTED_ACCESS_MODE_KEY,
            super::UNSUPPORTED_BROWSER_BLOCK_MODE_KEY,
            super::CHROMIUM_INCOGNITO_CHANGE_ACCESS_MODE_KEY,
        ] {
            assert_eq!(
                idle_core
                    .database()
                    .service_state(key)
                    .expect("protected access mode should read")
                    .as_deref(),
                Some("no_active_schedule_or_detox")
            );
        }
        drop(idle_core);

        let idle_unlock = json!({
            "jsonrpc": "2.0",
            "id": 130,
            "method": "unlock_tier1_edit",
            "params": {
                "phrase": "BLOCKUNTU-TIER1-EDIT-TEST",
                "now": "2026-05-22T10:00:00+02:00"
            }
        });
        let idle_unlock_response: Value = serde_json::from_slice(&handle_payload(
            &idle_context,
            &serde_json::to_vec(&idle_unlock).unwrap(),
        ))
        .expect("response should parse");
        assert!(
            idle_unlock_response.get("error").is_none(),
            "{idle_unlock_response}"
        );

        let (_temp, active_context) =
            rpc_context_with_tier1_edit_credential(active_scheduled_rpc_context());
        let active_core = active_context.core.lock().expect("core should lock");
        active_core
            .database()
            .set_service_state(
                super::PROTECTED_ACCESS_MODE_KEY,
                "no_active_schedule_or_detox",
                Utc::now(),
            )
            .expect("protected access mode should persist");
        drop(active_core);

        let active_unlock_response: Value = serde_json::from_slice(&handle_payload(
            &active_context,
            &serde_json::to_vec(&idle_unlock).unwrap(),
        ))
        .expect("response should parse");
        assert_eq!(active_unlock_response["error"]["code"], -32602);
        assert!(active_unlock_response["error"]["data"]
            .as_str()
            .expect("error data should be a string")
            .contains("while no schedule or Detox is active"));

        let (_temp, detox_context) = rpc_context_with_tier1_edit_credential(editable_rpc_context());
        let detox_core = detox_context.core.lock().expect("core should lock");
        detox_core
            .database()
            .set_service_state(
                super::PROTECTED_ACCESS_MODE_KEY,
                "no_active_schedule_or_detox",
                Utc::now(),
            )
            .expect("protected access mode should persist");
        drop(detox_core);
        let start_detox = json!({
            "jsonrpc": "2.0",
            "id": 131,
            "method": "start_detox",
            "params": {
                "duration_minutes": 90,
                "site_rule_ids": ["controlled"],
                "now": "2026-05-23T20:00:00+02:00"
            }
        });
        let detox_start_response: Value = serde_json::from_slice(&handle_payload(
            &detox_context,
            &serde_json::to_vec(&start_detox).unwrap(),
        ))
        .expect("response should parse");
        assert!(
            detox_start_response.get("error").is_none(),
            "{detox_start_response}"
        );

        let detox_unlock = json!({
            "jsonrpc": "2.0",
            "id": 132,
            "method": "unlock_tier1_edit",
            "params": {
                "phrase": "BLOCKUNTU-TIER1-EDIT-TEST",
                "now": "2026-05-23T20:00:00+02:00"
            }
        });
        let detox_unlock_response: Value = serde_json::from_slice(&handle_payload(
            &detox_context,
            &serde_json::to_vec(&detox_unlock).unwrap(),
        ))
        .expect("response should parse");
        assert_eq!(detox_unlock_response["error"]["code"], -32602);
        assert!(detox_unlock_response["error"]["data"]
            .as_str()
            .expect("error data should be a string")
            .contains("while no schedule or Detox is active"));
    }

    #[test]
    fn protected_access_mode_controls_unsupported_browser_block_activation() {
        let context = active_scheduled_rpc_context();
        let core = context.core.lock().expect("core should lock");
        let active_schedule_time =
            parse_optional_now(Some("2026-05-22T10:00:00+02:00".to_string()))
                .expect("time should parse");
        let sunday_window = parse_optional_now(Some("2026-05-24T20:00:00+02:00".to_string()))
            .expect("time should parse");

        core.database()
            .set_service_state(
                super::PROTECTED_ACCESS_MODE_KEY,
                "no_active_schedule_or_detox",
                Utc::now(),
            )
            .expect("protected access mode should persist");
        assert!(
            !super::unsupported_browser_block_is_active(&core, active_schedule_time, false)
                .expect("browser block should evaluate")
        );

        core.database()
            .set_service_state(super::PROTECTED_ACCESS_MODE_KEY, "all_time", Utc::now())
            .expect("protected access mode should persist");
        assert!(
            super::unsupported_browser_block_is_active(&core, active_schedule_time, false)
                .expect("browser block should evaluate")
        );

        core.database()
            .set_service_state(super::PROTECTED_ACCESS_MODE_KEY, "sunday", Utc::now())
            .expect("protected access mode should persist");
        assert!(
            !super::unsupported_browser_block_is_active(&core, active_schedule_time, false)
                .expect("browser block should evaluate")
        );
        assert!(
            super::unsupported_browser_block_is_active(&core, sunday_window, false)
                .expect("browser block should evaluate")
        );
    }

    #[test]
    fn tier1_edit_unlock_does_not_allow_active_tier2_site_list_edits() {
        let (_temp, context) =
            rpc_context_with_tier1_edit_credential(active_sunday_scheduled_rpc_context());
        let unlock_request = json!({
            "jsonrpc": "2.0",
            "id": 114,
            "method": "unlock_tier1_edit",
            "params": {
                "phrase": "BLOCKUNTU-TIER1-EDIT-TEST",
                "now": "2026-05-24T20:00:00+02:00"
            }
        });
        let unlock_response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&unlock_request).unwrap(),
        ))
        .expect("response should parse");
        assert!(unlock_response.get("error").is_none(), "{unlock_response}");

        let edit_request = json!({
            "jsonrpc": "2.0",
            "id": 115,
            "method": "upsert_site_list",
            "params": {
                "now": "2026-05-24T20:01:00+02:00",
                "rule": {
                    "id": "controlled",
                    "name": "Controlled edited",
                    "tier": "controlled_access",
                    "enabled": true,
                    "patterns": [
                        { "kind": "domain", "value": "different.example", "match_subdomains": true }
                    ],
                    "schedule_ids": ["work-hours"]
                }
            }
        });
        let edit_response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&edit_request).unwrap(),
        ))
        .expect("response should parse");

        assert_eq!(edit_response["error"]["code"], -32602);
        assert!(edit_response["error"]["data"]
            .as_str()
            .expect("error data should be a string")
            .contains("site list 'controlled' is currently active"));
    }

    #[test]
    fn detox_session_blocks_until_privileged_cancel() {
        let (_temp, context) = rpc_context_with_tier1_edit_credential(editable_rpc_context());
        let start_request = json!({
            "jsonrpc": "2.0",
            "id": 121,
            "method": "start_detox",
            "params": {
                "name": "Deep work",
                "duration_minutes": 90,
                "site_rule_ids": ["controlled"],
                "now": "2026-05-24T20:00:00+02:00"
            }
        });
        let start_response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&start_request).unwrap(),
        ))
        .expect("response should parse");

        assert!(start_response.get("error").is_none(), "{start_response}");
        assert_eq!(start_response["result"]["session"]["status"], "active");
        let session_id = start_response["result"]["session"]["id"]
            .as_str()
            .expect("session id should be a string")
            .to_string();

        let eval_request = json!({
            "jsonrpc": "2.0",
            "id": 122,
            "method": "evaluate_url",
            "params": {
                "url": "https://controlled.example/",
                "now": "2026-05-24T20:10:00+02:00"
            }
        });
        let eval_response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&eval_request).unwrap(),
        ))
        .expect("response should parse");

        assert_eq!(eval_response["result"]["decision"], "block");
        assert_eq!(
            eval_response["result"]["reason"]["kind"],
            "controlled_access"
        );
        assert_eq!(eval_response["result"]["reason"]["tier"], "tier_3");
        assert_eq!(eval_response["result"]["reason"]["blocked_by"], "detox");
        assert_eq!(eval_response["result"]["reason"]["session_id"], session_id);

        let edit_request = json!({
            "jsonrpc": "2.0",
            "id": 123,
            "method": "upsert_site_list",
            "params": {
                "now": "2026-05-24T20:11:00+02:00",
                "rule": {
                    "id": "controlled",
                    "name": "Controlled edited",
                    "tier": "controlled_access",
                    "enabled": true,
                    "patterns": [
                        { "kind": "domain", "value": "different.example", "match_subdomains": true }
                    ],
                    "schedule_ids": ["work-hours"]
                }
            }
        });
        let edit_response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&edit_request).unwrap(),
        ))
        .expect("response should parse");
        assert_eq!(edit_response["error"]["code"], -32602);
        assert!(edit_response["error"]["data"]
            .as_str()
            .expect("error data should be a string")
            .contains("covered by an active detox session"));

        let cancel_without_unlock = json!({
            "jsonrpc": "2.0",
            "id": 124,
            "method": "cancel_detox",
            "params": {
                "id": session_id.clone(),
                "now": "2026-05-24T20:12:00+02:00"
            }
        });
        let cancel_response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&cancel_without_unlock).unwrap(),
        ))
        .expect("response should parse");
        assert_eq!(cancel_response["error"]["code"], -32602);
        assert!(cancel_response["error"]["data"]
            .as_str()
            .expect("error data should be a string")
            .contains("Tier 1 edit unlock is required"));

        let unlock_request = json!({
            "jsonrpc": "2.0",
            "id": 125,
            "method": "unlock_tier1_edit",
            "params": {
                "phrase": "BLOCKUNTU-TIER1-EDIT-TEST",
                "now": "2026-05-24T20:13:00+02:00"
            }
        });
        let unlock_response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&unlock_request).unwrap(),
        ))
        .expect("response should parse");
        assert!(unlock_response.get("error").is_none(), "{unlock_response}");

        let cancel_request = json!({
            "jsonrpc": "2.0",
            "id": 126,
            "method": "cancel_detox",
            "params": {
                "id": session_id.clone(),
                "now": "2026-05-24T20:14:00+02:00"
            }
        });
        let cancel_response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&cancel_request).unwrap(),
        ))
        .expect("response should parse");
        assert!(cancel_response.get("error").is_none(), "{cancel_response}");
        assert_eq!(cancel_response["result"]["session"]["status"], "cancelled");

        let eval_after_cancel: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&eval_request).unwrap(),
        ))
        .expect("response should parse");
        assert_eq!(eval_after_cancel["result"]["decision"], "allow");
    }

    #[test]
    fn allows_additive_site_list_edits_during_active_detox() {
        let context = editable_rpc_context();
        let start_request = json!({
            "jsonrpc": "2.0",
            "id": 131,
            "method": "start_detox",
            "params": {
                "duration_minutes": 90,
                "site_rule_ids": ["controlled"],
                "now": "2026-05-24T20:00:00+02:00"
            }
        });
        let start_response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&start_request).unwrap(),
        ))
        .expect("response should parse");
        assert!(start_response.get("error").is_none(), "{start_response}");

        let edit_request = json!({
            "jsonrpc": "2.0",
            "id": 132,
            "method": "upsert_site_list",
            "params": {
                "now": "2026-05-24T20:10:00+02:00",
                "rule": {
                    "id": "controlled",
                    "name": "Controlled",
                    "tier": "controlled_access",
                    "enabled": true,
                    "patterns": [
                        { "kind": "domain", "value": "controlled.example", "match_subdomains": true },
                        { "kind": "domain", "value": "extra-controlled.example", "match_subdomains": true }
                    ],
                    "schedule_ids": ["work-hours"]
                }
            }
        });
        let edit_response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&edit_request).unwrap(),
        ))
        .expect("response should parse");

        assert!(edit_response.get("error").is_none(), "{edit_response}");
        let controlled_rule = edit_response["result"]["config"]["rules"]
            .as_array()
            .expect("rules should be an array")
            .iter()
            .find(|rule| rule["id"] == "controlled")
            .expect("controlled rule should exist");
        assert_eq!(
            controlled_rule["patterns"]
                .as_array()
                .expect("patterns should be an array")
                .len(),
            2
        );
    }

    #[test]
    fn allows_additive_app_rule_edits_during_active_detox() {
        let context = active_app_rule_rpc_context();
        let start_request = json!({
            "jsonrpc": "2.0",
            "id": 133,
            "method": "start_detox",
            "params": {
                "duration_minutes": 90,
                "app_rule_ids": ["kmines-controlled"],
                "now": "2026-05-22T10:00:00Z"
            }
        });
        let start_response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&start_request).unwrap(),
        ))
        .expect("response should parse");
        assert!(start_response.get("error").is_none(), "{start_response}");

        let edit_request = json!({
            "jsonrpc": "2.0",
            "id": 134,
            "method": "upsert_app_rule",
            "params": {
                "now": "2026-05-22T10:10:00Z",
                "rule": {
                    "id": "kmines-controlled",
                    "name": "KMines",
                    "tier": "controlled_access",
                    "enabled": true,
                    "matchers": [
                        { "kind": "command_name", "value": "kmines" },
                        { "kind": "window_title_contains", "value": "KMines" }
                    ],
                    "schedule_ids": ["work-hours"]
                }
            }
        });
        let edit_response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&edit_request).unwrap(),
        ))
        .expect("response should parse");

        assert!(edit_response.get("error").is_none(), "{edit_response}");
        let app_rule = edit_response["result"]["config"]["app_rules"]
            .as_array()
            .expect("app rules should be an array")
            .iter()
            .find(|rule| rule["id"] == "kmines-controlled")
            .expect("app rule should exist");
        assert_eq!(
            app_rule["matchers"]
                .as_array()
                .expect("matchers should be an array")
                .len(),
            2
        );
    }

    #[test]
    fn start_and_cancel_detox_repairs_hosts_file_domains() {
        let strict_context = rpc_context_with_config_toml(
            r#"
            [[rules]]
            id = "controlled"
            name = "Strict scheduled"
            tier = "scheduled_block"
            patterns = [
              { kind = "domain", value = "controlled.example", match_subdomains = true }
            ]
            "#,
        );
        let (temp, context) = rpc_context_with_enforcement_managers_for(strict_context, false);
        let (_credential_temp, context) = rpc_context_with_tier1_edit_credential(context);
        let hosts_path = temp.path().join("hosts");
        let start_request = json!({
            "jsonrpc": "2.0",
            "id": 135,
            "method": "start_detox",
            "params": {
                "duration_minutes": 90,
                "site_rule_ids": ["controlled"],
                "now": "2026-05-24T20:00:00+02:00"
            }
        });
        let start_response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&start_request).unwrap(),
        ))
        .expect("response should parse");
        assert!(start_response.get("error").is_none(), "{start_response}");
        assert_eq!(start_response["result"]["hosts_repair"], "Repaired");
        let session_id = start_response["result"]["session"]["id"]
            .as_str()
            .expect("session id should be a string")
            .to_string();

        let hosts_contents = std::fs::read_to_string(&hosts_path).expect("hosts should exist");
        assert!(hosts_contents.contains("0.0.0.0 controlled.example"));

        let unlock_request = json!({
            "jsonrpc": "2.0",
            "id": 136,
            "method": "unlock_tier1_edit",
            "params": {
                "phrase": "BLOCKUNTU-TIER1-EDIT-TEST",
                "now": "2026-05-24T20:01:00+02:00"
            }
        });
        let unlock_response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&unlock_request).unwrap(),
        ))
        .expect("response should parse");
        assert!(unlock_response.get("error").is_none(), "{unlock_response}");

        let cancel_request = json!({
            "jsonrpc": "2.0",
            "id": 137,
            "method": "cancel_detox",
            "params": {
                "id": session_id,
                "now": "2026-05-24T20:02:00+02:00"
            }
        });
        let cancel_response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&cancel_request).unwrap(),
        ))
        .expect("response should parse");
        assert!(cancel_response.get("error").is_none(), "{cancel_response}");
        assert_eq!(cancel_response["result"]["hosts_repair"], "Repaired");

        let hosts_contents = std::fs::read_to_string(&hosts_path).expect("hosts should exist");
        assert!(!hosts_contents.contains("controlled.example"));
    }

    #[test]
    fn detox_sessions_rpc_lists_active_sessions() {
        let context = editable_rpc_context();
        let start_request = json!({
            "jsonrpc": "2.0",
            "id": 127,
            "method": "start_detox",
            "params": {
                "duration_minutes": 30,
                "site_rule_ids": ["controlled"],
                "now": "2026-05-22T18:00:00Z"
            }
        });
        let start_response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&start_request).unwrap(),
        ))
        .expect("response should parse");
        assert!(start_response.get("error").is_none(), "{start_response}");

        let list_request = json!({
            "jsonrpc": "2.0",
            "id": 128,
            "method": "detox_sessions",
            "params": {
                "active_only": true,
                "now": "2026-05-22T18:10:00Z"
            }
        });
        let list_response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&list_request).unwrap(),
        ))
        .expect("response should parse");
        assert_eq!(
            list_response["result"]["sessions"]
                .as_array()
                .expect("sessions should be an array")
                .len(),
            1
        );
    }

    #[test]
    fn detox_supports_multiweek_durations_and_caps_them_at_twelve_weeks() {
        let context = editable_rpc_context();
        let two_weeks = json!({
            "jsonrpc": "2.0",
            "id": 129,
            "method": "start_detox",
            "params": {
                "duration_minutes": 2 * 7 * 24 * 60,
                "site_rule_ids": ["controlled"],
                "now": "2026-05-22T18:00:00Z"
            }
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&two_weeks).unwrap(),
        ))
        .expect("response should parse");
        assert!(response.get("error").is_none(), "{response}");
        assert_eq!(
            response["result"]["session"]["ends_at"],
            "2026-06-05T18:00:00Z"
        );

        let too_long = json!({
            "jsonrpc": "2.0",
            "id": 130,
            "method": "start_detox",
            "params": {
                "duration_minutes": super::MAX_DETOX_DURATION_MINUTES + 1,
                "site_rule_ids": ["controlled"],
                "now": "2026-05-22T18:00:00Z"
            }
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&too_long).unwrap(),
        ))
        .expect("response should parse");
        assert_eq!(response["error"]["code"], -32602);
        assert!(response["error"]["data"]
            .as_str()
            .expect("error data should be a string")
            .contains("detox duration cannot exceed"));
    }

    #[test]
    fn rejects_active_schedule_edits() {
        let context = active_scheduled_rpc_context();
        let request = json!({
            "jsonrpc": "2.0",
            "id": 17,
            "method": "upsert_schedule",
            "params": {
                "now": "2026-05-22T10:00:00Z",
                "schedule": {
                    "id": "work-hours",
                    "name": "Edited work hours",
                    "windows": [
                        { "weekday": "fri", "start": "10:00", "end": "18:00" }
                    ]
                }
            }
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&request).unwrap(),
        ))
        .expect("response should parse");

        assert_eq!(response["error"]["code"], -32602);
        assert!(response["error"]["data"]
            .as_str()
            .expect("error data should be a string")
            .contains("schedule 'work-hours' is currently active"));
    }

    #[test]
    fn allows_appending_windows_to_an_active_schedule() {
        let context = active_scheduled_rpc_context();
        let request = json!({
            "jsonrpc": "2.0",
            "id": 18,
            "method": "upsert_schedule",
            "params": {
                "now": "2026-05-22T10:00:00Z",
                "schedule": {
                    "id": "work-hours",
                    "name": "Work hours",
                    "windows": [
                        { "weekday": "fri", "start": "09:00", "end": "17:00" },
                        { "weekday": "fri", "start": "18:00", "end": "19:00" }
                    ]
                }
            }
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&request).unwrap(),
        ))
        .expect("response should parse");

        assert!(response.get("error").is_none(), "{response}");
        assert_eq!(
            response["result"]["config"]["schedules"][0]["windows"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn schedule_save_attaches_selected_website_and_application_rules() {
        let context = rpc_context_with_config_toml(
            r#"
            [[schedules]]
            id = "work-hours"
            name = "Work hours"

            [[schedules.windows]]
            weekday = "mon"
            start = "09:00"
            end = "17:00"

            [[rules]]
            id = "controlled-site"
            name = "Controlled site"
            tier = "controlled_access"
            patterns = [
              { kind = "domain", value = "controlled.example", match_subdomains = true }
            ]

            [[app_rules]]
            id = "controlled-app"
            name = "Controlled app"
            tier = "controlled_access"
            matchers = [
              { kind = "command_name", value = "kmines" }
            ]
            "#,
        );
        let request = json!({
            "jsonrpc": "2.0",
            "id": 19,
            "method": "upsert_schedule",
            "params": {
                "now": "2026-05-22T10:00:00Z",
                "schedule": {
                    "id": "work-hours",
                    "name": "Work hours",
                    "windows": [{ "weekday": "mon", "start": "09:00", "end": "17:00" }]
                },
                "site_rule_ids": ["controlled-site"],
                "app_rule_ids": ["controlled-app"]
            }
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&request).unwrap(),
        ))
        .expect("response should parse");

        assert!(response.get("error").is_none(), "{response}");
        assert_eq!(
            response["result"]["config"]["rules"][0]["schedule_ids"],
            json!(["work-hours"])
        );
        assert_eq!(
            response["result"]["config"]["app_rules"][0]["schedule_ids"],
            json!(["work-hours"])
        );
    }

    #[test]
    fn reports_missing_extension_heartbeat() {
        let now = Utc
            .with_ymd_and_hms(2026, 5, 22, 10, 0, 0)
            .single()
            .expect("timestamp should be valid");
        let response = browser_extension_status_json(
            None,
            "firefox_extension",
            15,
            now,
            true,
            Some(now - Duration::seconds(61)),
            60,
        );

        assert_eq!(response["state"], "missing");
        assert_eq!(response["installed_enabled"], "unconfirmed");
    }

    #[test]
    fn reports_closed_and_starting_browser_states_without_an_error() {
        let now = Utc
            .with_ymd_and_hms(2026, 5, 22, 10, 0, 0)
            .single()
            .expect("timestamp should be valid");

        let closed =
            browser_extension_status_json(None, "firefox_extension", 15, now, false, None, 60);
        assert_eq!(closed["state"], "inactive");

        let starting = browser_extension_status_json(
            None,
            "firefox_extension",
            15,
            now,
            true,
            Some(now - Duration::seconds(20)),
            60,
        );
        assert_eq!(starting["state"], "starting");
        assert_eq!(starting["startup_grace_remaining_seconds"], 40);
    }

    #[test]
    fn reports_recent_extension_heartbeat_as_active() {
        let now = Utc
            .with_ymd_and_hms(2026, 5, 22, 10, 0, 5)
            .single()
            .expect("timestamp should be valid");
        let heartbeat = HeartbeatState {
            component: "firefox_extension".to_string(),
            last_seen_at: now - Duration::seconds(5),
            details: Some(
                json!({
                    "browser": "firefox",
                    "extension_id": "blockuntu@example.local",
                    "extension_version": "0.2.0"
                })
                .to_string(),
            ),
        };
        let response = browser_extension_status_json(
            Some(&heartbeat),
            "firefox_extension",
            15,
            now,
            true,
            Some(now - Duration::seconds(10)),
            60,
        );

        assert_eq!(response["state"], "active");
        assert_eq!(response["installed_enabled"], "confirmed");
        assert_eq!(response["extension_id"], "blockuntu@example.local");
        assert_eq!(response["extension_version"], "0.2.0");
        assert_eq!(response["age_seconds"], 5);
    }

    #[test]
    fn expired_uninstall_handoff_rearms_browser_extension_mode() {
        let context = rpc_context();
        {
            let core = context.core.lock().expect("core lock should work");
            core.database()
                .set_service_state("browser_extension_mode", "uninstalling", Utc::now())
                .expect("browser extension mode should write");
            core.database()
                .set_service_state(
                    "browser_extension_uninstalling_until",
                    &(Utc::now() - Duration::seconds(1)).to_rfc3339(),
                    Utc::now(),
                )
                .expect("uninstall handoff expiry should write");
        }

        let heartbeat = json!({
            "jsonrpc": "2.0",
            "id": 141,
            "method": "extension_heartbeat",
            "params": {
                "component": "firefox_extension",
                "browser": "firefox",
                "extension_id": "{a7c3f3c4-6b1e-4c6f-9f2a-8d4e5b7c1a90}",
                "extension_version": "0.2.1",
                "now": "2026-05-22T10:00:00Z"
            }
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&heartbeat).unwrap(),
        ))
        .expect("response should parse");

        assert_eq!(response["result"]["status"], "ok");
        assert_eq!(response["result"]["enforcement_state"], "active");
        assert_eq!(response["result"]["browser_extension_mode"], "active");
    }

    #[test]
    fn prepare_uninstall_marks_browser_extension_mode() {
        let (temp, context) = rpc_context_with_enforcement_managers(true);
        let lease_path = temp.path().join("package-removal-lease");
        let context = context.with_package_removal_lease_path(&lease_path);
        let prepare = json!({
            "jsonrpc": "2.0",
            "id": 142,
            "method": "prepare_uninstall",
            "params": {
                "now": "2026-05-24T20:00:00+02:00"
            }
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&prepare).unwrap(),
        ))
        .expect("response should parse");

        assert!(response.get("error").is_none(), "{response}");
        assert_eq!(response["result"]["status"], "ok");
        assert_eq!(response["result"]["browser_extension_mode"], "uninstalling");
        let lease = fs::read_to_string(&lease_path).expect("package removal lease should exist");
        let lease_token = response["result"]["package_removal_lease"]
            .as_str()
            .expect("package removal lease token should be returned");
        assert!(lease.starts_with(lease_token));
        assert_eq!(
            fs::metadata(&lease_path)
                .expect("lease metadata should exist")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );

        let heartbeat = json!({
            "jsonrpc": "2.0",
            "id": 143,
            "method": "extension_heartbeat",
            "params": {
                "component": "firefox_extension",
                "browser": "firefox",
                "extension_id": "{a7c3f3c4-6b1e-4c6f-9f2a-8d4e5b7c1a90}",
                "extension_version": "0.2.1",
                "now": "2026-05-22T10:00:05Z"
            }
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&heartbeat).unwrap(),
        ))
        .expect("response should parse");

        assert_eq!(response["result"]["enforcement_state"], "uninstalling");
        assert_eq!(response["result"]["browser_extension_mode"], "uninstalling");
    }

    #[test]
    fn prepare_uninstall_is_limited_to_operator_window() {
        let (_temp, context) = rpc_context_with_enforcement_managers(true);
        let core = context.core.lock().expect("core should lock");
        core.database()
            .set_service_state(super::OPERATOR_WINDOW_RESTRICTION_KEY, "true", Utc::now())
            .expect("operator restriction should persist");
        drop(core);
        let prepare = json!({
            "jsonrpc": "2.0",
            "id": 144,
            "method": "prepare_uninstall",
            "params": {
                "now": "2026-05-25T20:00:00+02:00"
            }
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&prepare).unwrap(),
        ))
        .expect("response should parse");

        assert_eq!(response["error"]["code"], -32602);
        assert!(response["error"]["data"]
            .as_str()
            .expect("error data should be a string")
            .contains("Sunday 20:00-23:59"));
    }

    #[test]
    fn prepare_uninstall_accepts_a_code_for_its_own_installation_serial() {
        let (_enforcement_temp, context) = rpc_context_with_enforcement_managers(true);
        let serial_temp = tempfile::tempdir().expect("serial temp dir should be created");
        let serial_path = serial_temp.path().join("installation-id");
        std::fs::write(&serial_path, "BKI-00000000-00000000-00000000-00000001\n")
            .expect("installation serial should write");
        let context = context
            .with_installation_serial_path(serial_path)
            .with_package_removal_lease_path(serial_temp.path().join("package-removal-lease"));
        let prepare = json!({
            "jsonrpc": "2.0",
            "id": 145,
            "method": "prepare_uninstall",
            "params": {
                "now": "2026-05-25T20:00:00+02:00",
                "emergency_code": "BLOCKUNTU-EU2-spbREObNCff7ly9mJiKTfl25QchAAOKMtknbC-6r9EWdgxGnrNsMrrp0hNgsfEENlbkzhHJYkwTnIpwtYixnAg"
            }
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&prepare).unwrap(),
        ))
        .expect("response should parse");

        assert!(response.get("error").is_none(), "{response}");
        assert_eq!(response["result"]["status"], "ok");
        assert_eq!(response["result"]["browser_extension_mode"], "uninstalling");
    }

    #[test]
    fn reports_stale_extension_heartbeat() {
        let now = Utc
            .with_ymd_and_hms(2026, 5, 22, 10, 0, 20)
            .single()
            .expect("timestamp should be valid");
        let heartbeat = HeartbeatState {
            component: "firefox_extension".to_string(),
            last_seen_at: now - Duration::seconds(20),
            details: None,
        };
        let response = browser_extension_status_json(
            Some(&heartbeat),
            "firefox_extension",
            15,
            now,
            true,
            Some(now - Duration::seconds(25)),
            60,
        );

        assert_eq!(response["state"], "stale");
        assert_eq!(response["installed_enabled"], "confirmed");
        assert_eq!(response["age_seconds"], 20);
    }

    #[test]
    fn reports_chrome_extension_heartbeat_separately() {
        let context = rpc_context();
        let heartbeat = json!({
            "jsonrpc": "2.0",
            "id": 18,
            "method": "extension_heartbeat",
            "params": {
                "browser": "chrome",
                "component": "chrome_extension",
                "extension_id": "opfljaancedgklbpnbpjfhdbbhbfpnoc",
                "extension_version": "0.2.1",
                "now": "2026-05-22T10:00:00Z"
            }
        });
        let _ = handle_payload(&context, &serde_json::to_vec(&heartbeat).unwrap());

        let now = Utc
            .with_ymd_and_hms(2026, 5, 22, 10, 1, 0)
            .single()
            .expect("timestamp should be valid");
        let chrome_heartbeat = {
            let core = context.core.lock().expect("core lock should work");
            core.database()
                .heartbeat("chrome_extension")
                .expect("chrome heartbeat should read")
                .expect("chrome heartbeat should exist")
        };
        let response = browser_extension_status_json(
            Some(&chrome_heartbeat),
            "chrome_extension",
            75,
            now,
            true,
            Some(now - Duration::seconds(70)),
            60,
        );

        assert_eq!(response["component"], "chrome_extension");
        assert_eq!(response["state"], "active");
        assert_eq!(response["browser"], "chrome");
        assert_eq!(response["extension_id"], "opfljaancedgklbpnbpjfhdbbhbfpnoc");

        let firefox_status = browser_extension_status_json(
            None,
            "firefox_extension",
            15,
            now,
            true,
            Some(now - Duration::seconds(61)),
            60,
        );
        assert_eq!(firefox_status["state"], "missing");
        assert_eq!(firefox_status["component"], "firefox_extension");
    }
}
