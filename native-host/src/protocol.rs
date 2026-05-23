use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserMessageKind {
    LegacyUrl,
    LegacyHeartbeat,
    JsonRpc,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct PreparedRequest {
    pub daemon_payload: Vec<u8>,
    pub browser_kind: BrowserMessageKind,
}

pub fn prepare_daemon_request(browser_payload: &[u8]) -> PreparedRequest {
    let browser_value = match serde_json::from_slice::<Value>(browser_payload) {
        Ok(value) => value,
        Err(_) => {
            return PreparedRequest {
                daemon_payload: browser_payload.to_vec(),
                browser_kind: BrowserMessageKind::JsonRpc,
            }
        }
    };

    if browser_value.get("method").is_some() {
        return PreparedRequest {
            daemon_payload: browser_payload.to_vec(),
            browser_kind: BrowserMessageKind::JsonRpc,
        };
    }

    if browser_value.get("type").and_then(Value::as_str) == Some("extension_heartbeat") {
        let rpc = json!({
            "jsonrpc": "2.0",
            "id": null,
            "method": "extension_heartbeat",
            "params": {
                "extension_id": browser_value.get("extensionId").and_then(Value::as_str),
                "extension_version": browser_value.get("extensionVersion").and_then(Value::as_str)
            }
        });
        return PreparedRequest {
            daemon_payload: serialize(&rpc),
            browser_kind: BrowserMessageKind::LegacyHeartbeat,
        };
    }

    if let Some(url) = browser_value.get("url").and_then(Value::as_str) {
        let rpc = json!({
            "jsonrpc": "2.0",
            "id": null,
            "method": "evaluate_url",
            "params": {
                "url": url
            }
        });
        return PreparedRequest {
            daemon_payload: serialize(&rpc),
            browser_kind: BrowserMessageKind::LegacyUrl,
        };
    }

    PreparedRequest {
        daemon_payload: browser_payload.to_vec(),
        browser_kind: BrowserMessageKind::Unknown,
    }
}

pub fn prepare_browser_response(
    browser_kind: &BrowserMessageKind,
    daemon_response: &[u8],
) -> Vec<u8> {
    let Ok(response_value) = serde_json::from_slice::<Value>(daemon_response) else {
        return fallback_response_for_kind(browser_kind, "daemon returned invalid JSON");
    };

    match browser_kind {
        BrowserMessageKind::LegacyUrl => legacy_url_response(&response_value),
        BrowserMessageKind::LegacyHeartbeat => legacy_heartbeat_response(&response_value),
        BrowserMessageKind::JsonRpc | BrowserMessageKind::Unknown => serialize(&response_value),
    }
}

pub fn fallback_response_for_payload(browser_payload: &[u8], reason: &str) -> Vec<u8> {
    let kind = classify_browser_payload(browser_payload);
    fallback_response_for_kind(&kind, reason)
}

pub fn classify_browser_payload(browser_payload: &[u8]) -> BrowserMessageKind {
    let Ok(value) = serde_json::from_slice::<Value>(browser_payload) else {
        return BrowserMessageKind::JsonRpc;
    };

    if value.get("method").is_some() {
        BrowserMessageKind::JsonRpc
    } else if value.get("type").and_then(Value::as_str) == Some("extension_heartbeat") {
        BrowserMessageKind::LegacyHeartbeat
    } else if value.get("url").is_some() {
        BrowserMessageKind::LegacyUrl
    } else {
        BrowserMessageKind::Unknown
    }
}

fn legacy_url_response(response: &Value) -> Vec<u8> {
    if let Some(error) = response.get("error") {
        return serialize(&json!({
            "action": "allow",
            "error": error
        }));
    }

    let Some(result) = response.get("result") else {
        return serialize(&json!({
            "action": "allow",
            "error": "daemon response missing result"
        }));
    };

    if result.get("decision").and_then(Value::as_str) == Some("block") {
        serialize(&json!({
            "action": "block",
            "reason": result.get("reason").cloned().unwrap_or(Value::Null)
        }))
    } else {
        serialize(&json!({ "action": "allow" }))
    }
}

fn legacy_heartbeat_response(response: &Value) -> Vec<u8> {
    if let Some(error) = response.get("error") {
        return serialize(&json!({
            "type": "extension_heartbeat",
            "status": "error",
            "error": error
        }));
    }

    serialize(&json!({
        "type": "extension_heartbeat",
        "status": "ok"
    }))
}

fn fallback_response_for_kind(kind: &BrowserMessageKind, reason: &str) -> Vec<u8> {
    match kind {
        BrowserMessageKind::LegacyHeartbeat => serialize(&json!({
            "type": "extension_heartbeat",
            "status": "error",
            "error": reason
        })),
        BrowserMessageKind::JsonRpc => serialize(&json!({
            "jsonrpc": "2.0",
            "id": null,
            "error": {
                "code": -32000,
                "message": "native host error",
                "data": reason
            }
        })),
        BrowserMessageKind::LegacyUrl | BrowserMessageKind::Unknown => serialize(&json!({
            "action": "allow",
            "error": reason
        })),
    }
}

fn serialize(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).expect("native-host JSON value must serialize")
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::{
        fallback_response_for_payload, prepare_browser_response, prepare_daemon_request,
        BrowserMessageKind,
    };

    #[test]
    fn translates_legacy_url_to_evaluate_url_jsonrpc() {
        let prepared = prepare_daemon_request(br#"{"url":"https://blocked.example/"}"#);
        assert_eq!(prepared.browser_kind, BrowserMessageKind::LegacyUrl);

        let rpc: Value =
            serde_json::from_slice(&prepared.daemon_payload).expect("request should parse");
        assert_eq!(rpc["method"], "evaluate_url");
        assert_eq!(rpc["params"]["url"], "https://blocked.example/");
    }

    #[test]
    fn translates_block_result_back_to_legacy_action() {
        let daemon_response = json!({
            "jsonrpc": "2.0",
            "id": null,
            "result": {
                "decision": "block",
                "reason": { "kind": "hard_block", "rule_id": "hard" }
            }
        });

        let response = prepare_browser_response(
            &BrowserMessageKind::LegacyUrl,
            &serde_json::to_vec(&daemon_response).unwrap(),
        );
        let response: Value = serde_json::from_slice(&response).expect("response should parse");

        assert_eq!(response["action"], "block");
        assert_eq!(response["reason"]["kind"], "hard_block");
    }

    #[test]
    fn preserves_jsonrpc_passthrough() {
        let request = br#"{"jsonrpc":"2.0","id":1,"method":"status","params":{}}"#;
        let prepared = prepare_daemon_request(request);
        assert_eq!(prepared.browser_kind, BrowserMessageKind::JsonRpc);
        assert_eq!(prepared.daemon_payload, request);

        let response = br#"{"jsonrpc":"2.0","id":1,"result":{"status":"ok"}}"#;
        let bridged: Value = serde_json::from_slice(&prepare_browser_response(
            &BrowserMessageKind::JsonRpc,
            response,
        ))
        .expect("bridged response should parse");
        let expected: Value =
            serde_json::from_slice(response).expect("expected response should parse");
        assert_eq!(bridged, expected);
    }

    #[test]
    fn preserves_heartbeat_shape_for_backend_failures() {
        let response = fallback_response_for_payload(
            br#"{"type":"extension_heartbeat"}"#,
            "backend unavailable",
        );
        let response: Value = serde_json::from_slice(&response).expect("response should parse");
        assert_eq!(response["type"], "extension_heartbeat");
        assert_eq!(response["status"], "error");
    }
}
