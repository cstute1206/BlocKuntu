use std::sync::{Arc, Mutex};

use chrono::{DateTime, Datelike, Duration, FixedOffset, Local, TimeZone, Timelike, Utc};
use focus_core::{
    evaluate_url, record_visit_end, record_visit_heartbeat, record_visit_start, request_unlock,
    BlockReason, Config, ControlledBlockReason, Decision, EvaluationContext, FocusCore,
    HeartbeatState, RuleConfig, ScheduleConfig, UnlockState, VisitState, Weekday,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::{DaemonError, Result};

const FIREFOX_EXTENSION_HEARTBEAT_COMPONENT: &str = "firefox_extension";
const DEFAULT_EXTENSION_HEARTBEAT_TIMEOUT_SECONDS: u64 = 15;

#[derive(Clone)]
pub struct RpcContext {
    core: Arc<Mutex<FocusCore>>,
    extension_heartbeat_timeout_seconds: u64,
}

impl RpcContext {
    pub fn new(core: Arc<Mutex<FocusCore>>) -> Self {
        Self {
            core,
            extension_heartbeat_timeout_seconds: DEFAULT_EXTENSION_HEARTBEAT_TIMEOUT_SECONDS,
        }
    }

    pub fn with_extension_heartbeat_timeout_seconds(mut self, seconds: u64) -> Self {
        self.extension_heartbeat_timeout_seconds = seconds;
        self
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
    extension_id: Option<String>,
    #[serde(default)]
    extension_version: Option<String>,
    #[serde(default)]
    now: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ExtensionStatusParams {
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
        "config_snapshot" => config_snapshot(context),
        "upsert_site_list" => {
            let params = parse_params::<UpsertSiteListParams>(params)?;
            upsert_site_list_method(context, params)
        }
        "delete_site_list" => {
            let params = parse_params::<DeleteSiteListParams>(params)?;
            delete_site_list_method(context, params)
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
    Ok(json!({
        "status": "ok",
        "rules": core.config().rules.len(),
        "schedules": core.config().schedules.len(),
        "allowances": core.config().allowances.len()
    }))
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

    focus_core::validate_config(&next)?;
    core.database().replace_policy_config(&next)?;
    core.replace_config(next)?;
    core.database().record_event(
        "site_list_saved",
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

    focus_core::validate_config(&next)?;
    core.database().replace_policy_config(&next)?;
    core.replace_config(next)?;
    core.database().record_event(
        "site_list_deleted",
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
    let details = json!({
        "extension_id": params.extension_id,
        "extension_version": params.extension_version
    });
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    core.database().upsert_heartbeat(
        FIREFOX_EXTENSION_HEARTBEAT_COMPONENT,
        Some(&details.to_string()),
        now.with_timezone(&Utc),
    )?;
    Ok(json!({ "status": "ok" }))
}

fn extension_status_method(context: &RpcContext, params: ExtensionStatusParams) -> Result<Value> {
    let now = parse_optional_now(params.now)?.with_timezone(&Utc);
    let core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    firefox_extension_status_from_core(&core, context.extension_heartbeat_timeout_seconds, now)
}

fn firefox_extension_status_from_core(
    core: &FocusCore,
    heartbeat_timeout_seconds: u64,
    now: DateTime<Utc>,
) -> Result<Value> {
    let heartbeat = core
        .database()
        .heartbeat(FIREFOX_EXTENSION_HEARTBEAT_COMPONENT)?;
    Ok(firefox_extension_status_json(
        heartbeat.as_ref(),
        heartbeat_timeout_seconds,
        now,
    ))
}

fn firefox_extension_status_json(
    heartbeat: Option<&HeartbeatState>,
    heartbeat_timeout_seconds: u64,
    now: DateTime<Utc>,
) -> Value {
    let Some(heartbeat) = heartbeat else {
        return json!({
            "state": "missing",
            "component": FIREFOX_EXTENSION_HEARTBEAT_COMPONENT,
            "installed_enabled": "unconfirmed",
            "last_seen_at": Value::Null,
            "age_seconds": Value::Null,
            "heartbeat_timeout_seconds": heartbeat_timeout_seconds,
            "detail": "no heartbeat has been recorded; Firefox may be closed, or the extension is not installed/enabled"
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
        "extension_id": extension_id,
        "extension_version": extension_version,
        "detail": detail
    })
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

fn active_site_list_edit_error(rule_id: &str) -> DaemonError {
    DaemonError::InvalidRequest(format!(
        "site list '{rule_id}' is currently active and cannot be edited"
    ))
}

fn active_schedule_edit_error(schedule_id: &str) -> DaemonError {
    DaemonError::InvalidRequest(format!(
        "schedule '{schedule_id}' is currently active and cannot be edited"
    ))
}

fn rule_is_active_at(rule: &RuleConfig, config: &Config, now: DateTime<FixedOffset>) -> bool {
    if !rule.enabled {
        return false;
    }

    if rule.schedule_ids.is_empty() {
        return true;
    }

    rule.schedule_ids.iter().any(|schedule_id| {
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
}
