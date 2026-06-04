use std::sync::{Arc, Mutex};

use chrono::{DateTime, Datelike, Duration, FixedOffset, Local, TimeZone, Timelike, Utc};
use focus_core::{
    evaluate_url, record_visit_end, record_visit_heartbeat, record_visit_start, request_unlock,
    AllowanceConfig, AppRuleConfig, BlockReason, Config, ControlledBlockReason, Decision,
    EvaluationContext, FocusCore, HeartbeatState, RuleConfig, ScheduleConfig, UnlockState,
    VisitState, Weekday,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::chrome_policy::{ChromePolicyManager, ChromePolicyRepairStatus};
use crate::error::{DaemonError, Result};
use crate::firefox_policy::{FirefoxPolicyManager, RepairStatus};
use crate::hosts::{HostsManager, HostsRepairStatus};

const FIREFOX_EXTENSION_HEARTBEAT_COMPONENT: &str = "firefox_extension";
const CHROME_EXTENSION_HEARTBEAT_COMPONENT: &str = "chrome_extension";
const DEFAULT_EXTENSION_HEARTBEAT_TIMEOUT_SECONDS: u64 = 15;
const CHROME_EXTENSION_HEARTBEAT_TIMEOUT_SECONDS: u64 = 75;
const ENFORCEMENT_STATE_KEY: &str = "enforcement_state";
const ENFORCEMENT_ACTIVE: &str = "active";
const ENFORCEMENT_STOPPED: &str = "stopped";

#[derive(Clone)]
pub struct RpcContext {
    core: Arc<Mutex<FocusCore>>,
    extension_heartbeat_timeout_seconds: u64,
    firefox_policy: Option<FirefoxPolicyManager>,
    chrome_policy: Option<ChromePolicyManager>,
    hosts: Option<HostsManager>,
    manage_firefox_policy: bool,
    manage_chrome_policy: bool,
    defer_firefox_policy_repair_until_heartbeat: bool,
    defer_chrome_policy_repair_until_heartbeat: bool,
}

impl RpcContext {
    pub fn new(core: Arc<Mutex<FocusCore>>) -> Self {
        Self {
            core,
            extension_heartbeat_timeout_seconds: DEFAULT_EXTENSION_HEARTBEAT_TIMEOUT_SECONDS,
            firefox_policy: None,
            chrome_policy: None,
            hosts: None,
            manage_firefox_policy: true,
            manage_chrome_policy: true,
            defer_firefox_policy_repair_until_heartbeat: false,
            defer_chrome_policy_repair_until_heartbeat: false,
        }
    }

    pub fn with_extension_heartbeat_timeout_seconds(mut self, seconds: u64) -> Self {
        self.extension_heartbeat_timeout_seconds = seconds;
        self
    }

    pub fn with_enforcement_managers(
        mut self,
        firefox_policy: FirefoxPolicyManager,
        chrome_policy: ChromePolicyManager,
        hosts: HostsManager,
    ) -> Self {
        self.firefox_policy = Some(firefox_policy);
        self.chrome_policy = Some(chrome_policy);
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

    fn firefox_policy(&self) -> Result<&FirefoxPolicyManager> {
        self.firefox_policy.as_ref().ok_or_else(|| {
            DaemonError::InvalidRequest("Firefox policy manager is not configured".to_string())
        })
    }

    fn hosts(&self) -> Result<&HostsManager> {
        self.hosts.as_ref().ok_or_else(|| {
            DaemonError::InvalidRequest("hosts manager is not configured".to_string())
        })
    }

    fn chrome_policy(&self) -> Result<&ChromePolicyManager> {
        self.chrome_policy.as_ref().ok_or_else(|| {
            DaemonError::InvalidRequest("Chrome policy manager is not configured".to_string())
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
    now: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RequestUnlockParams {
    target: String,
    minutes: u32,
    reason: String,
    #[serde(default)]
    now: Option<String>,
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
struct RecentEventsParams {
    #[serde(default = "default_recent_events_limit")]
    limit: u32,
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

#[derive(Debug, Deserialize)]
struct UpsertScheduleParams {
    schedule: ScheduleConfig,
    #[serde(default)]
    now: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeleteScheduleParams {
    id: String,
    #[serde(default)]
    now: Option<String>,
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
        "start_enforcement" => start_enforcement(context),
        "stop_enforcement" => stop_enforcement(context),
        "config_snapshot" => config_snapshot(context),
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
        "recent_events" => {
            let params = parse_params::<RecentEventsParams>(params)?;
            recent_events(context, params)
        }
        "evaluate_url" => {
            let params = parse_params::<EvaluateUrlParams>(params)?;
            evaluate_url_method(context, params)
        }
        "request_unlock" => {
            let params = parse_params::<RequestUnlockParams>(params)?;
            request_unlock_method(context, params)
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
    Ok(json!({
        "status": "ok",
        "enforcement_state": enforcement_state,
        "rules": core.config().rules.len(),
        "app_rules": core.config().app_rules.len(),
        "schedules": core.config().schedules.len(),
        "allowances": core.config().allowances.len()
    }))
}

fn enforcement_status(context: &RpcContext) -> Result<Value> {
    let hosts = context.hosts()?;
    let (enforcement_state, config) = {
        let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
        (enforcement_state_from_core(&core)?, core.config().clone())
    };

    Ok(json!({
        "status": "ok",
        "enforcement_state": enforcement_state,
        "firefox_policy": firefox_policy_status_json(context)?,
        "chrome_policy": chrome_policy_status_json(context)?,
        "hosts_file": hosts.status(&config)
    }))
}

fn start_enforcement(context: &RpcContext) -> Result<Value> {
    let hosts = context.hosts()?;
    let now = Utc::now();

    {
        let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
        core.database()
            .set_service_state(ENFORCEMENT_STATE_KEY, ENFORCEMENT_ACTIVE, now)?;
    }

    let firefox_policy_repair = repair_firefox_policy_from_context(context)?;
    let chrome_policy_repair = repair_chrome_policy_from_context(context)?;
    let hosts_repair = {
        let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
        hosts.verify_and_repair(core.config())?
    };

    let status = enforcement_status(context)?;
    {
        let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
        core.database().record_event(
            "enforcement_started",
            Some("system"),
            Some(&format!(
                "firefox_policy={firefox_policy_repair:?};chrome_policy={chrome_policy_repair:?};hosts={hosts_repair:?}"
            )),
            now,
        )?;
    }

    Ok(status)
}

fn stop_enforcement(context: &RpcContext) -> Result<Value> {
    let hosts = context.hosts()?;
    let now = Utc::now();

    {
        let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
        core.database()
            .set_service_state(ENFORCEMENT_STATE_KEY, ENFORCEMENT_STOPPED, now)?;
    }

    let firefox_policy_repair = remove_firefox_policy_from_context(context)?;
    let chrome_policy_repair = remove_chrome_policy_from_context(context)?;
    let hosts_repair = hosts.remove_managed_block()?;
    let status = enforcement_status(context)?;

    {
        let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
        core.database().record_event(
            "enforcement_stopped",
            Some("system"),
            Some(&format!(
                "firefox_policy={firefox_policy_repair:?};chrome_policy={chrome_policy_repair:?};hosts={hosts_repair:?}"
            )),
            now,
        )?;
    }

    Ok(status)
}

fn firefox_policy_status_json(context: &RpcContext) -> Result<Value> {
    let mut status = serde_json::to_value(context.firefox_policy()?.status())?;
    let heartbeat_seen = has_extension_heartbeat(context, FIREFOX_EXTENSION_HEARTBEAT_COMPONENT)?;
    let deferred = context.defer_firefox_policy_repair_until_heartbeat && !heartbeat_seen;
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
                json!("Firefox policy management is disabled; install and enable the extension manually"),
            );
        } else if deferred {
            object.insert("compliant".to_string(), json!(true));
            object.insert(
                "detail".to_string(),
                json!("Firefox policy repair is deferred until the first extension heartbeat"),
            );
        }
    }
    Ok(status)
}

fn chrome_policy_status_json(context: &RpcContext) -> Result<Value> {
    let mut status = serde_json::to_value(context.chrome_policy()?.status())?;
    let heartbeat_seen = has_extension_heartbeat(context, CHROME_EXTENSION_HEARTBEAT_COMPONENT)?;
    let deferred = context.defer_chrome_policy_repair_until_heartbeat && !heartbeat_seen;
    if let Some(object) = status.as_object_mut() {
        object.insert("managed".to_string(), json!(context.manage_chrome_policy));
        object.insert(
            "deferred_until_heartbeat".to_string(),
            json!(context.defer_chrome_policy_repair_until_heartbeat),
        );
        object.insert("active_after_heartbeat".to_string(), json!(heartbeat_seen));
        if !context.manage_chrome_policy {
            object.insert("compliant".to_string(), json!(true));
            object.insert("force_install_configured".to_string(), json!(true));
            object.insert("update_manifest_compliant".to_string(), json!(true));
            object.insert("override_update_url".to_string(), json!(true));
            object.insert(
                "detail".to_string(),
                json!("Chrome policy management is disabled; install and enable the extension manually"),
            );
        } else if deferred {
            object.insert("compliant".to_string(), json!(true));
            object.insert("force_install_configured".to_string(), json!(true));
            object.insert("update_manifest_compliant".to_string(), json!(true));
            object.insert("override_update_url".to_string(), json!(true));
            object.insert(
                "detail".to_string(),
                json!("Chrome policy repair is deferred until the first extension heartbeat"),
            );
        }
    }
    Ok(status)
}

fn repair_firefox_policy_from_context(context: &RpcContext) -> Result<RepairStatus> {
    if !context.manage_firefox_policy {
        return Ok(RepairStatus::SkippedDisabled);
    }
    if context.defer_firefox_policy_repair_until_heartbeat
        && !has_extension_heartbeat(context, FIREFOX_EXTENSION_HEARTBEAT_COMPONENT)?
    {
        return Ok(RepairStatus::SkippedDeferred);
    }
    context.firefox_policy()?.verify_and_repair()
}

fn repair_chrome_policy_from_context(context: &RpcContext) -> Result<ChromePolicyRepairStatus> {
    if !context.manage_chrome_policy {
        return Ok(ChromePolicyRepairStatus::SkippedDisabled);
    }
    if context.defer_chrome_policy_repair_until_heartbeat
        && !has_extension_heartbeat(context, CHROME_EXTENSION_HEARTBEAT_COMPONENT)?
    {
        return Ok(ChromePolicyRepairStatus::SkippedDeferred);
    }
    context.chrome_policy()?.verify_and_repair()
}

fn remove_firefox_policy_from_context(context: &RpcContext) -> Result<RepairStatus> {
    if !context.manage_firefox_policy {
        return Ok(RepairStatus::SkippedDisabled);
    }
    context.firefox_policy()?.remove_policy()
}

fn remove_chrome_policy_from_context(context: &RpcContext) -> Result<ChromePolicyRepairStatus> {
    if !context.manage_chrome_policy {
        return Ok(ChromePolicyRepairStatus::SkippedDisabled);
    }
    context.chrome_policy()?.remove_policy()
}

fn has_extension_heartbeat(context: &RpcContext, component: &str) -> Result<bool> {
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    Ok(core.database().heartbeat(component)?.is_some())
}

fn repair_deferred_policy_after_heartbeat(context: &RpcContext, component: &str) -> Result<Value> {
    {
        let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
        if !enforcement_active_from_core(&core)? {
            return Ok(json!({ "skipped": "enforcement_stopped" }));
        }
    }

    match component {
        FIREFOX_EXTENSION_HEARTBEAT_COMPONENT
            if context.manage_firefox_policy
                && context.defer_firefox_policy_repair_until_heartbeat =>
        {
            let status = context.firefox_policy()?.verify_and_repair()?;
            Ok(json!({ "firefox_policy": format!("{status:?}") }))
        }
        CHROME_EXTENSION_HEARTBEAT_COMPONENT
            if context.manage_chrome_policy
                && context.defer_chrome_policy_repair_until_heartbeat =>
        {
            let status = context.chrome_policy()?.verify_and_repair()?;
            Ok(json!({ "chrome_policy": format!("{status:?}") }))
        }
        _ => Ok(json!({})),
    }
}

fn config_snapshot(context: &RpcContext) -> Result<Value> {
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    serde_json::to_value(core.config()).map_err(DaemonError::from)
}

fn upsert_site_list_method(context: &RpcContext, params: UpsertSiteListParams) -> Result<Value> {
    let now = parse_optional_now(params.now)?;
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
            if current_rule != &params.rule && rule_is_active_at(current_rule, core.config(), now) {
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
    core.database().replace_policy_config(&next)?;
    core.replace_config(next)?;
    let hosts_repair = repair_hosts_after_policy_change(context, &core)?;
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
    let now = parse_optional_now(params.now)?;
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
    if rule_is_active_at(current_rule, core.config(), now) {
        return Err(active_site_list_edit_error(&current_rule.id));
    }
    let removed = next.rules.remove(index);
    remove_unreferenced_allowance(&mut next, removed.allowance_id.as_deref());

    focus_core::validate_config(&next)?;
    core.database().replace_policy_config(&next)?;
    core.replace_config(next)?;
    let hosts_repair = repair_hosts_after_policy_change(context, &core)?;
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
    let now = parse_optional_now(params.now)?;
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
                && allowance_is_active_at(&current_allowance.id, core.config(), now)
            {
                return Err(active_allowance_edit_error(&current_allowance.id));
            }
            next.allowances[index] = params.allowance;
        }
        None => next.allowances.push(params.allowance),
    }

    focus_core::validate_config(&next)?;
    core.database().replace_policy_config(&next)?;
    core.replace_config(next)?;
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
    let now = parse_optional_now(params.now)?;
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

    if allowance_is_active_at(&params.id, core.config(), now) {
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
    core.database().replace_policy_config(&next)?;
    core.replace_config(next)?;
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
    let now = parse_optional_now(params.now)?;
    let updated_at = Utc::now();
    let rule_id = params.rule.id.clone();
    let mut core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    let mut next = core.config().clone();

    match next
        .app_rules
        .iter()
        .position(|candidate| candidate.id == params.rule.id)
    {
        Some(index) => {
            let current_rule = &next.app_rules[index];
            if current_rule != &params.rule
                && app_rule_is_active_at(current_rule, core.config(), now)
            {
                return Err(active_app_rule_edit_error(&current_rule.id));
            }
            next.app_rules[index] = params.rule;
        }
        None => next.app_rules.push(params.rule),
    }

    focus_core::validate_config(&next)?;
    core.database().replace_policy_config(&next)?;
    core.replace_config(next)?;
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
    let now = parse_optional_now(params.now)?;
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
    if app_rule_is_active_at(current_rule, core.config(), now) {
        return Err(active_app_rule_edit_error(&current_rule.id));
    }
    let removed = next.app_rules.remove(index);

    focus_core::validate_config(&next)?;
    core.database().replace_policy_config(&next)?;
    core.replace_config(next)?;
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
    let now = parse_optional_now(params.now)?;
    let updated_at = Utc::now();
    let schedule_id = params.schedule.id.clone();
    let mut core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    let mut next = core.config().clone();

    match next
        .schedules
        .iter()
        .position(|candidate| candidate.id == params.schedule.id)
    {
        Some(index) => {
            let current_schedule = &next.schedules[index];
            if current_schedule != &params.schedule && schedule_is_active_at(current_schedule, now)
            {
                return Err(active_schedule_edit_error(&current_schedule.id));
            }
            next.schedules[index] = params.schedule;
        }
        None => next.schedules.push(params.schedule),
    }

    focus_core::validate_config(&next)?;
    core.database().replace_policy_config(&next)?;
    core.replace_config(next)?;
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
    let now = parse_optional_now(params.now)?;
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
    core.database().replace_policy_config(&next)?;
    core.replace_config(next)?;
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

fn recent_events(context: &RpcContext, params: RecentEventsParams) -> Result<Value> {
    let limit = params.limit.clamp(1, 200);
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    let mut statement = core
        .database()
        .connection()
        .prepare(
            r#"
            SELECT id, kind, target, details, created_at
            FROM events
            ORDER BY id DESC
            LIMIT ?1
            "#,
        )
        .map_err(focus_core::Error::from)?;

    let rows = statement
        .query_map([i64::from(limit)], |row| {
            Ok(json!({
                "id": row.get::<_, i64>(0)?,
                "kind": row.get::<_, String>(1)?,
                "target": row.get::<_, Option<String>>(2)?,
                "details": row.get::<_, Option<String>>(3)?,
                "created_at": row.get::<_, String>(4)?
            }))
        })
        .map_err(focus_core::Error::from)?;

    let mut events = Vec::new();
    for row in rows {
        events.push(row.map_err(focus_core::Error::from)?);
    }

    Ok(json!({ "events": events }))
}

fn evaluate_url_method(context: &RpcContext, params: EvaluateUrlParams) -> Result<Value> {
    let now = parse_optional_now(params.now)?;
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    if !enforcement_active_from_core(&core)? {
        return Ok(json!({
            "decision": "allow",
            "enforcement_state": ENFORCEMENT_STOPPED
        }));
    }
    let eval_context = EvaluationContext::new(core.config(), core.database(), now);
    let decision = evaluate_url(&params.url, &eval_context);

    if decision.is_block() {
        core.database().record_event(
            "url_blocked",
            Some(&params.url),
            Some(&format!("{decision:?}")),
            now.with_timezone(&Utc),
        )?;
    }

    Ok(decision_to_json(&decision, core.config(), now))
}

fn request_unlock_method(context: &RpcContext, params: RequestUnlockParams) -> Result<Value> {
    let now = parse_optional_now(params.now)?;
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    let eval_context = EvaluationContext::new(core.config(), core.database(), now);
    let unlock = request_unlock(&params.target, params.minutes, params.reason, &eval_context)?;
    Ok(unlock_to_json(&unlock))
}

fn record_visit_start_method(
    context: &RpcContext,
    params: RecordVisitStartParams,
) -> Result<Value> {
    let now = parse_optional_now(params.now)?;
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    let eval_context = EvaluationContext::new(core.config(), core.database(), now);
    let visit = record_visit_start(&params.url, &params.tab_id, &eval_context)?;
    Ok(visit_to_json(&visit))
}

fn record_visit_heartbeat_method(context: &RpcContext, params: VisitIdParams) -> Result<Value> {
    let now = parse_optional_now(params.now)?;
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    let eval_context = EvaluationContext::new(core.config(), core.database(), now);
    record_visit_heartbeat(params.visit_id, &eval_context)?;
    Ok(json!({ "status": "ok" }))
}

fn record_visit_end_method(context: &RpcContext, params: VisitIdParams) -> Result<Value> {
    let now = parse_optional_now(params.now)?;
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    let eval_context = EvaluationContext::new(core.config(), core.database(), now);
    record_visit_end(params.visit_id, &eval_context)?;
    Ok(json!({ "status": "ok" }))
}

fn extension_heartbeat_method(
    context: &RpcContext,
    params: ExtensionHeartbeatParams,
) -> Result<Value> {
    let now = parse_optional_now(params.now)?;
    let component = extension_component(
        params.component.as_deref(),
        params.browser.as_deref(),
        params.extension_id.as_deref(),
    );
    let details = json!({
        "browser": params.browser,
        "extension_id": params.extension_id,
        "extension_version": params.extension_version
    });
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    core.database().upsert_heartbeat(
        component,
        Some(&details.to_string()),
        now.with_timezone(&Utc),
    )?;
    drop(core);

    let policy_repair = repair_deferred_policy_after_heartbeat(context, component)?;
    Ok(json!({ "status": "ok", "policy_repair": policy_repair }))
}

fn extension_status_method(context: &RpcContext, params: ExtensionStatusParams) -> Result<Value> {
    let now = parse_optional_now(params.now)?.with_timezone(&Utc);
    let component = extension_component(params.component.as_deref(), None, None);
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    browser_extension_status_from_core(
        &core,
        component,
        extension_heartbeat_timeout_seconds(context, component),
        now,
    )
}

fn browser_extension_status_from_core(
    core: &FocusCore,
    component: &str,
    heartbeat_timeout_seconds: u64,
    now: DateTime<Utc>,
) -> Result<Value> {
    let heartbeat = core.database().heartbeat(component)?;
    Ok(browser_extension_status_json(
        heartbeat.as_ref(),
        component,
        heartbeat_timeout_seconds,
        now,
    ))
}

fn browser_extension_status_json(
    heartbeat: Option<&HeartbeatState>,
    component: &str,
    heartbeat_timeout_seconds: u64,
    now: DateTime<Utc>,
) -> Value {
    let Some(heartbeat) = heartbeat else {
        let browser = browser_name_for_component(component);
        return json!({
            "state": "missing",
            "component": component,
            "browser": browser,
            "installed_enabled": "unconfirmed",
            "last_seen_at": Value::Null,
            "age_seconds": Value::Null,
            "heartbeat_timeout_seconds": heartbeat_timeout_seconds,
            "detail": format!(
                "no heartbeat has been recorded; {browser} may be closed, or the extension is not installed/enabled"
            )
        });
    };

    let age_seconds = now
        .signed_duration_since(heartbeat.last_seen_at)
        .num_seconds()
        .max(0);
    let state = if age_seconds <= heartbeat_timeout_seconds as i64 {
        "active"
    } else {
        "stale"
    };
    let installed_enabled = if state == "active" {
        "confirmed"
    } else {
        "unconfirmed"
    };
    let details = heartbeat_details(heartbeat.details.as_deref());
    let extension_id = details
        .as_ref()
        .and_then(|details| details.get("extension_id"))
        .and_then(Value::as_str);
    let extension_version = details
        .as_ref()
        .and_then(|details| details.get("extension_version"))
        .and_then(Value::as_str);
    let browser = details
        .as_ref()
        .and_then(|details| details.get("browser"))
        .and_then(Value::as_str);
    let detail = match state {
        "active" => format!(
            "recent heartbeat received {} second(s) ago; extension installation and enabled state are confirmed",
            age_seconds
        ),
        _ => format!(
            "last heartbeat was {} second(s) ago; extension installation and enabled state are no longer confirmed",
            age_seconds
        ),
    };

    json!({
        "state": state,
        "component": heartbeat.component,
        "installed_enabled": installed_enabled,
        "last_seen_at": heartbeat.last_seen_at,
        "age_seconds": age_seconds,
        "heartbeat_timeout_seconds": heartbeat_timeout_seconds,
        "browser": browser,
        "extension_id": extension_id,
        "extension_version": extension_version,
        "detail": detail
    })
}

fn extension_component<'a>(
    component: Option<&'a str>,
    browser: Option<&str>,
    extension_id: Option<&str>,
) -> &'a str {
    if let Some(
        component @ (FIREFOX_EXTENSION_HEARTBEAT_COMPONENT | CHROME_EXTENSION_HEARTBEAT_COMPONENT),
    ) = component
    {
        return component;
    }

    if browser
        .map(|browser| browser.eq_ignore_ascii_case("chrome"))
        .unwrap_or(false)
    {
        return CHROME_EXTENSION_HEARTBEAT_COMPONENT;
    }

    if extension_id
        .map(|extension_id| extension_id == "odedgejjcdilkoibeljkeohekonmdfea")
        .unwrap_or(false)
    {
        return CHROME_EXTENSION_HEARTBEAT_COMPONENT;
    }

    FIREFOX_EXTENSION_HEARTBEAT_COMPONENT
}

fn extension_heartbeat_timeout_seconds(context: &RpcContext, component: &str) -> u64 {
    if component == CHROME_EXTENSION_HEARTBEAT_COMPONENT {
        context
            .extension_heartbeat_timeout_seconds
            .max(CHROME_EXTENSION_HEARTBEAT_TIMEOUT_SECONDS)
    } else {
        context.extension_heartbeat_timeout_seconds
    }
}

fn browser_name_for_component(component: &str) -> &'static str {
    match component {
        CHROME_EXTENSION_HEARTBEAT_COMPONENT => "Chrome",
        _ => "Firefox",
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
    match evaluate_url_method(context, EvaluateUrlParams { url, now: None }) {
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

fn parse_optional_now(now: Option<String>) -> Result<DateTime<FixedOffset>> {
    match now {
        Some(now) => DateTime::parse_from_rfc3339(&now)
            .map(|parsed| parsed.with_timezone(&Local).fixed_offset())
            .map_err(|err| DaemonError::InvalidRequest(format!("invalid RFC3339 now: {err}"))),
        None => Ok(Local::now().fixed_offset()),
    }
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
        BlockReason::HardBlock { rule_id, rule_name } => json!({
            "kind": "hard_block",
            "tier": "tier_1",
            "rule_id": rule_id,
            "rule_name": rule_name,
            "summary": "This site is on a Tier 1 hard-block list.",
            "detail": "Tier 1 sites are always blocked and are also eligible for the hosts-file fallback.",
            "free_at": Value::Null
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
        "tier": "tier_2",
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
        ControlledBlockReason::NoAllowance => "This Tier 2 site needs an explicit unlock.",
        ControlledBlockReason::AllowanceExhausted => {
            "This Tier 2 site used up its daily allowance."
        }
        ControlledBlockReason::UnlockRequired => "This Tier 2 site requires an unlock.",
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
        if window.weekday == current_weekday && current_minute >= start && current_minute < end {
            return Some(datetime_at_minute(now, 0, end));
        }
        return None;
    }

    if window.weekday == current_weekday && current_minute >= start {
        return Some(datetime_at_minute(now, 1, end));
    }

    if window.weekday == current_weekday.previous() && current_minute < end {
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
    let now_utc = now.with_timezone(&Utc);
    let next_utc_midnight = (now_utc.date_naive() + Duration::days(1))
        .and_hms_opt(0, 0, 0)
        .expect("midnight is valid")
        .and_utc();
    next_utc_midnight.with_timezone(now.offset())
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

fn default_recent_events_limit() -> u32 {
    50
}

fn enforcement_state_from_core(core: &FocusCore) -> Result<String> {
    Ok(
        match core.database().service_state(ENFORCEMENT_STATE_KEY)? {
            Some(state) if state == ENFORCEMENT_STOPPED => ENFORCEMENT_STOPPED.to_string(),
            _ => ENFORCEMENT_ACTIVE.to_string(),
        },
    )
}

fn enforcement_active_from_core(core: &FocusCore) -> Result<bool> {
    Ok(enforcement_state_from_core(core)? == ENFORCEMENT_ACTIVE)
}

fn repair_hosts_after_policy_change(
    context: &RpcContext,
    core: &FocusCore,
) -> Result<Option<HostsRepairStatus>> {
    let Some(hosts) = context.hosts.as_ref() else {
        return Ok(None);
    };

    if !enforcement_active_from_core(core)? {
        return Ok(Some(HostsRepairStatus::SkippedStopped));
    }

    hosts.verify_and_repair(core.config()).map(Some)
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

fn active_schedule_edit_error(schedule_id: &str) -> DaemonError {
    DaemonError::InvalidRequest(format!(
        "schedule '{schedule_id}' is currently active and cannot be edited"
    ))
}

fn active_allowance_edit_error(allowance_id: &str) -> DaemonError {
    DaemonError::InvalidRequest(format!(
        "allowance '{allowance_id}' is currently used by an active site list and cannot be edited"
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

fn allowance_is_active_at(allowance_id: &str, config: &Config, now: DateTime<FixedOffset>) -> bool {
    config.rules.iter().any(|rule| {
        rule.allowance_id.as_deref() == Some(allowance_id) && rule_is_active_at(rule, config, now)
    })
}

fn rule_is_active_at(rule: &RuleConfig, config: &Config, now: DateTime<FixedOffset>) -> bool {
    if !rule.enabled {
        return false;
    }

    schedule_ids_are_active_at(&rule.schedule_ids, config, now)
}

fn app_rule_is_active_at(
    rule: &AppRuleConfig,
    config: &Config,
    now: DateTime<FixedOffset>,
) -> bool {
    if !rule.enabled {
        return false;
    }

    schedule_ids_are_active_at(&rule.schedule_ids, config, now)
}

fn schedule_ids_are_active_at(
    schedule_ids: &[String],
    config: &Config,
    now: DateTime<FixedOffset>,
) -> bool {
    if schedule_ids.is_empty() {
        return true;
    }

    schedule_ids.iter().any(|schedule_id| {
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
            window.weekday == current_weekday && current_minute >= start && current_minute < end
        } else {
            (window.weekday == current_weekday && current_minute >= start)
                || (window.weekday == current_weekday.previous() && current_minute < end)
        }
    })
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use chrono::{Local, TimeZone, Utc};
    use focus_core::{Config, Database, FocusCore};
    use serde_json::{json, Value};

    use crate::chrome_policy::ChromePolicyManager;
    use crate::firefox_policy::FirefoxPolicyManager;
    use crate::hosts::HostsManager;

    use super::{handle_payload, parse_optional_now, RpcContext};

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

    fn rpc_context_with_config_toml(toml: &str) -> RpcContext {
        let config = Config::from_toml_str(toml).expect("config should parse");
        let database = Database::in_memory().expect("database should initialize");
        let core = FocusCore::new(config, database).expect("core should initialize");
        RpcContext::new(Arc::new(Mutex::new(core)))
    }

    fn rpc_context_with_enforcement_managers(
        manage_browser_policies: bool,
    ) -> (tempfile::TempDir, RpcContext) {
        let temp = tempfile::tempdir().expect("temp dir should be created");
        let firefox_policy = FirefoxPolicyManager::new(
            temp.path().join("firefox/policies.json"),
            "blockuntu-poc@example.local",
            temp.path().join("BlocKuntu-Signed.xpi"),
        );
        let chrome_policy = ChromePolicyManager::new(
            temp.path().join("chrome/policies/managed/blockuntu.json"),
            temp.path().join("chrome-extension-updates.xml"),
            "odedgejjcdilkoibeljkeohekonmdfea",
            "0.2.1",
            "https://example.invalid/blockuntu.crx",
        );
        let hosts = HostsManager::new(temp.path().join("hosts"));
        let context = rpc_context()
            .with_enforcement_managers(firefox_policy, chrome_policy, hosts)
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
                "now": "2026-05-22T10:00:00Z"
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
    fn stopped_enforcement_allows_url_evaluation() {
        let context = rpc_context();
        {
            let core = context.core.lock().expect("core lock should work");
            core.database()
                .set_service_state("enforcement_state", "stopped", Utc::now())
                .expect("service state should write");
        }
        let request = json!({
            "jsonrpc": "2.0",
            "id": 8,
            "method": "evaluate_url",
            "params": {
                "url": "https://blocked.example/",
                "now": "2026-05-22T10:00:00Z"
            }
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&request).unwrap(),
        ))
        .expect("response should parse");

        assert_eq!(response["id"], 8);
        assert_eq!(response["result"]["decision"], "allow");
        assert_eq!(response["result"]["enforcement_state"], "stopped");
    }

    #[test]
    fn manual_browser_extension_mode_does_not_write_browser_policies() {
        let (temp, context) = rpc_context_with_enforcement_managers(false);
        let request = json!({
            "jsonrpc": "2.0",
            "id": 71,
            "method": "start_enforcement",
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
        assert_eq!(
            response["result"]["chrome_policy"]["deferred_until_heartbeat"],
            true
        );
        assert_eq!(
            response["result"]["chrome_policy"]["active_after_heartbeat"],
            false
        );
        assert!(!temp.path().join("firefox/policies.json").exists());
        assert!(!temp
            .path()
            .join("chrome/policies/managed/blockuntu.json")
            .exists());

        let firefox_heartbeat = json!({
            "jsonrpc": "2.0",
            "id": 73,
            "method": "extension_heartbeat",
            "params": {
                "component": "firefox_extension",
                "browser": "firefox",
                "extension_id": "blockuntu-poc@example.local",
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
                "extension_id": "odedgejjcdilkoibeljkeohekonmdfea",
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
        assert!(temp.path().join("chrome-extension-updates.xml").exists());
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
        assert_eq!(reason["tier"], "tier_2");
        assert_eq!(reason["blocked_by"], "schedule");
        assert_eq!(reason["active_schedules"][0]["id"], "work-hours");
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
                "now": "2026-05-22T10:00:00Z"
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
    fn reports_missing_extension_heartbeat() {
        let context = rpc_context();
        let request = json!({
            "jsonrpc": "2.0",
            "id": 12,
            "method": "extension_status",
            "params": {
                "now": "2026-05-22T10:00:00Z"
            }
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&request).unwrap(),
        ))
        .expect("response should parse");

        assert_eq!(response["result"]["state"], "missing");
        assert_eq!(response["result"]["installed_enabled"], "unconfirmed");
    }

    #[test]
    fn reports_recent_extension_heartbeat_as_active() {
        let context = rpc_context();
        let heartbeat = json!({
            "jsonrpc": "2.0",
            "id": 13,
            "method": "extension_heartbeat",
            "params": {
                "extension_id": "blockuntu@example.local",
                "extension_version": "0.2.0",
                "now": "2026-05-22T10:00:00Z"
            }
        });
        let heartbeat_response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&heartbeat).unwrap(),
        ))
        .expect("response should parse");
        assert_eq!(heartbeat_response["result"]["status"], "ok");

        let status = json!({
            "jsonrpc": "2.0",
            "id": 14,
            "method": "extension_status",
            "params": {
                "now": "2026-05-22T10:00:05Z"
            }
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&status).unwrap(),
        ))
        .expect("response should parse");

        assert_eq!(response["result"]["state"], "active");
        assert_eq!(response["result"]["installed_enabled"], "confirmed");
        assert_eq!(
            response["result"]["extension_id"],
            "blockuntu@example.local"
        );
        assert_eq!(response["result"]["extension_version"], "0.2.0");
        assert_eq!(response["result"]["age_seconds"], 5);
    }

    #[test]
    fn reports_stale_extension_heartbeat() {
        let context = rpc_context();
        let heartbeat = json!({
            "jsonrpc": "2.0",
            "id": 15,
            "method": "extension_heartbeat",
            "params": {
                "extension_id": "blockuntu@example.local",
                "extension_version": "0.2.0",
                "now": "2026-05-22T10:00:00Z"
            }
        });
        let _ = handle_payload(&context, &serde_json::to_vec(&heartbeat).unwrap());

        let status = json!({
            "jsonrpc": "2.0",
            "id": 16,
            "method": "extension_status",
            "params": {
                "now": "2026-05-22T10:00:20Z"
            }
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&status).unwrap(),
        ))
        .expect("response should parse");

        assert_eq!(response["result"]["state"], "stale");
        assert_eq!(response["result"]["installed_enabled"], "unconfirmed");
        assert_eq!(response["result"]["age_seconds"], 20);
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
                "extension_id": "odedgejjcdilkoibeljkeohekonmdfea",
                "extension_version": "0.2.1",
                "now": "2026-05-22T10:00:00Z"
            }
        });
        let _ = handle_payload(&context, &serde_json::to_vec(&heartbeat).unwrap());

        let chrome_status = json!({
            "jsonrpc": "2.0",
            "id": 19,
            "method": "extension_status",
            "params": {
                "component": "chrome_extension",
                "now": "2026-05-22T10:01:00Z"
            }
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&chrome_status).unwrap(),
        ))
        .expect("response should parse");

        assert_eq!(response["result"]["component"], "chrome_extension");
        assert_eq!(response["result"]["state"], "active");
        assert_eq!(response["result"]["browser"], "chrome");
        assert_eq!(
            response["result"]["extension_id"],
            "odedgejjcdilkoibeljkeohekonmdfea"
        );

        let firefox_status = json!({
            "jsonrpc": "2.0",
            "id": 20,
            "method": "extension_status",
            "params": {
                "component": "firefox_extension",
                "now": "2026-05-22T10:01:00Z"
            }
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&firefox_status).unwrap(),
        ))
        .expect("response should parse");

        assert_eq!(response["result"]["state"], "missing");
        assert_eq!(response["result"]["component"], "firefox_extension");
    }
}
