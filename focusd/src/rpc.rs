use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Datelike, FixedOffset, Local, Timelike, Utc};
use focus_core::{
    evaluate_url, record_visit_end, record_visit_heartbeat, record_visit_start, request_unlock,
    BlockReason, Config, ControlledBlockReason, Decision, EvaluationContext, FocusCore, RuleConfig,
    RuleTier, ScheduleConfig, UnlockState, VisitState, Weekday,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::{DaemonError, Result};

#[derive(Clone)]
pub struct RpcContext {
    core: Arc<Mutex<FocusCore>>,
    config_path: Arc<PathBuf>,
}

impl RpcContext {
    pub fn new(core: Arc<Mutex<FocusCore>>, config_path: impl Into<PathBuf>) -> Self {
        Self {
            core,
            config_path: Arc::new(config_path.into()),
        }
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
struct RecentEventsParams {
    #[serde(default = "default_recent_events_limit")]
    limit: u32,
}

#[derive(Debug, Deserialize)]
struct WriteConfigFileParams {
    toml: String,
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
        "config_file" => config_file(context),
        "write_config_file" => {
            let params = parse_params::<WriteConfigFileParams>(params)?;
            write_config_file(context, params)
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

fn config_file(context: &RpcContext) -> Result<Value> {
    let toml = fs::read_to_string(context.config_path.as_ref())?;
    Ok(json!({
        "path": context.config_path.display().to_string(),
        "toml": toml
    }))
}

fn write_config_file(context: &RpcContext, params: WriteConfigFileParams) -> Result<Value> {
    let parsed = focus_core::Config::from_toml_str(&params.toml)?;
    let now = Local::now().fixed_offset();
    let updated_at = Utc::now();

    let mut core = context.core.lock().map_err(|_| DaemonError::LockPoisoned)?;
    ensure_gui_edit_preserves_active_hard_rules(core.config(), &parsed, now)?;
    write_config_atomically(context.config_path.as_ref(), &params.toml)?;
    core.replace_config(parsed)?;
    core.database().record_event(
        "config_updated",
        Some(&context.config_path.display().to_string()),
        Some("GUI TOML edit"),
        updated_at,
    )?;

    Ok(json!({
        "path": context.config_path.display().to_string(),
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

    Ok(decision_to_json(&decision))
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
        "firefox_extension",
        Some(&details.to_string()),
        now.with_timezone(&Utc),
    )?;
    Ok(json!({ "status": "ok" }))
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
            .map_err(|err| DaemonError::InvalidRequest(format!("invalid RFC3339 now: {err}"))),
        None => Ok(Local::now().fixed_offset()),
    }
}

fn decision_to_json(decision: &Decision) -> Value {
    match decision {
        Decision::Allow => json!({ "decision": "allow" }),
        Decision::Block(reason) => json!({
            "decision": "block",
            "reason": block_reason_to_json(reason)
        }),
    }
}

fn block_reason_to_json(reason: &BlockReason) -> Value {
    match reason {
        BlockReason::InvalidUrl { url } => json!({ "kind": "invalid_url", "url": url }),
        BlockReason::HardBlock { rule_id, rule_name } => {
            json!({ "kind": "hard_block", "rule_id": rule_id, "rule_name": rule_name })
        }
        BlockReason::ControlledAccess {
            rule_id,
            rule_name,
            reason,
        } => json!({
            "kind": "controlled_access",
            "rule_id": rule_id,
            "rule_name": rule_name,
            "controlled_reason": controlled_reason_to_str(reason)
        }),
        BlockReason::RuntimeError { message } => {
            json!({ "kind": "runtime_error", "message": message })
        }
    }
}

fn controlled_reason_to_str(reason: &ControlledBlockReason) -> &'static str {
    match reason {
        ControlledBlockReason::NoAllowance => "no_allowance",
        ControlledBlockReason::AllowanceExhausted => "allowance_exhausted",
        ControlledBlockReason::UnlockRequired => "unlock_required",
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

fn default_recent_events_limit() -> u32 {
    50
}

fn ensure_gui_edit_preserves_active_hard_rules(
    current: &Config,
    next: &Config,
    now: DateTime<FixedOffset>,
) -> Result<()> {
    for rule in current
        .rules
        .iter()
        .filter(|rule| rule.enabled && rule.tier == RuleTier::Hard)
    {
        if !hard_rule_is_active(rule, current, now) {
            continue;
        }

        let next_rule = next
            .rules
            .iter()
            .find(|candidate| candidate.id == rule.id)
            .ok_or_else(|| active_hard_rule_edit_error(&rule.id))?;
        if next_rule != rule {
            return Err(active_hard_rule_edit_error(&rule.id));
        }

        for schedule_id in &rule.schedule_ids {
            let Some(current_schedule) = current
                .schedules
                .iter()
                .find(|schedule| schedule.id == *schedule_id)
            else {
                continue;
            };
            let next_schedule = next
                .schedules
                .iter()
                .find(|schedule| schedule.id == *schedule_id)
                .ok_or_else(|| active_hard_rule_edit_error(&rule.id))?;
            if next_schedule != current_schedule {
                return Err(active_hard_rule_edit_error(&rule.id));
            }
        }
    }

    Ok(())
}

fn active_hard_rule_edit_error(rule_id: &str) -> DaemonError {
    DaemonError::InvalidRequest(format!(
        "active Tier 1 hard rule '{rule_id}' cannot be modified through the GUI TOML editor"
    ))
}

fn hard_rule_is_active(rule: &RuleConfig, config: &Config, now: DateTime<FixedOffset>) -> bool {
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

fn write_config_atomically(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        if parent.exists() {
            fs::create_dir_all(parent)?;
        } else {
            fs::create_dir_all(parent)?;
            fs::set_permissions(parent, fs::Permissions::from_mode(0o755))?;
        }
    }

    let temporary_path = temporary_config_path(path);
    let result = (|| {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temporary_path)?;
        file.set_permissions(fs::Permissions::from_mode(0o644))?;
        file.write_all(contents.as_bytes())?;
        if !contents.ends_with('\n') {
            file.write_all(b"\n")?;
        }
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary_path, path)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o644))?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }

    result
}

fn temporary_config_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("config.toml");
    path.with_file_name(format!(".{file_name}.blockuntu.{}.tmp", std::process::id()))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use focus_core::{Config, Database, FocusCore};
    use serde_json::{json, Value};

    use super::{handle_payload, RpcContext};

    fn rpc_context() -> RpcContext {
        rpc_context_with_config_path("/tmp/blockuntu-test-config.toml")
    }

    fn rpc_context_with_config_path(config_path: impl Into<std::path::PathBuf>) -> RpcContext {
        rpc_context_with_config_toml(
            config_path,
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

    fn writable_rpc_context(config_path: impl Into<std::path::PathBuf>) -> RpcContext {
        rpc_context_with_config_toml(
            config_path,
            r#"
            [[allowances]]
            id = "daily"
            daily_minutes = 30

            [[rules]]
            id = "controlled"
            name = "Controlled"
            tier = "controlled_access"
            allowance_id = "daily"
            patterns = [
              { kind = "domain", value = "controlled.example", match_subdomains = true }
            ]
            "#,
        )
    }

    fn rpc_context_with_config_toml(
        config_path: impl Into<std::path::PathBuf>,
        toml: &str,
    ) -> RpcContext {
        let config = Config::from_toml_str(toml).expect("config should parse");
        let database = Database::in_memory().expect("database should initialize");
        let core = FocusCore::new(config, database).expect("core should initialize");
        RpcContext::new(Arc::new(Mutex::new(core)), config_path)
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
    fn writes_config_file_after_validation_and_reloads_core() {
        let temp = tempfile::tempdir().expect("tempdir should exist");
        let config_path = temp.path().join("config.toml");
        let context = writable_rpc_context(&config_path);
        let new_toml = r#"
            [[rules]]
            id = "new-hard"
            name = "New Hard"
            tier = "hard"
            patterns = [
              { kind = "domain", value = "new.example", match_subdomains = true }
            ]
            "#;

        let request = json!({
            "jsonrpc": "2.0",
            "id": 9,
            "method": "write_config_file",
            "params": {
                "toml": new_toml
            }
        });
        let response: Value = serde_json::from_slice(&handle_payload(
            &context,
            &serde_json::to_vec(&request).unwrap(),
        ))
        .expect("response should parse");

        assert!(response.get("error").is_none(), "{response}");
        assert_eq!(response["result"]["config"]["rules"][0]["id"], "new-hard");
        assert!(std::fs::read_to_string(config_path)
            .expect("config should be written")
            .contains("new-hard"));

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
    fn rejects_gui_edits_that_modify_active_hard_rules() {
        let temp = tempfile::tempdir().expect("tempdir should exist");
        let config_path = temp.path().join("config.toml");
        let context = rpc_context_with_config_path(&config_path);
        let new_toml = r#"
            [[rules]]
            id = "hard"
            name = "Hard edited"
            tier = "hard"
            patterns = [
              { kind = "domain", value = "different.example", match_subdomains = true }
            ]
            "#;

        let request = json!({
            "jsonrpc": "2.0",
            "id": 11,
            "method": "write_config_file",
            "params": {
                "toml": new_toml
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
            .contains("active Tier 1 hard rule 'hard'"));
        assert!(!config_path.exists());
    }
}
