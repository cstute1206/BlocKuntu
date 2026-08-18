use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use crate::daemon_client::DaemonTransport;
use crate::error::Result;
use crate::native_messaging::{read_native_message, write_native_message};
use crate::protocol::{
    fallback_response_for_payload_with_diagnostic, prepare_browser_response, prepare_daemon_request,
};

const SLOW_REQUEST_THRESHOLD_MS: u128 = 500;
static DIAGNOSTIC_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub fn run_bridge<R: Read, W: Write>(
    input: &mut R,
    output: &mut W,
    daemon_client: &impl DaemonTransport,
) -> Result<()> {
    loop {
        let Some(browser_payload) = read_native_message(input)? else {
            return Ok(());
        };

        let started_at = Instant::now();
        let (method, request_id) = request_metadata(&browser_payload);
        let response = match process_browser_payload(daemon_client, &browser_payload) {
            Ok(response) => {
                let elapsed_ms = started_at.elapsed().as_millis();
                if elapsed_ms >= SLOW_REQUEST_THRESHOLD_MS {
                    eprintln!(
                        "blockuntu-native: slow RPC pid={} id={} method={} elapsed_ms={} request_bytes={} response_bytes={}",
                        std::process::id(),
                        request_id,
                        method,
                        elapsed_ms,
                        browser_payload.len(),
                        response.len()
                    );
                }
                response
            }
            Err(err) => {
                let elapsed_ms = started_at.elapsed().as_millis();
                eprintln!(
                    "blockuntu-native: RPC failed pid={} id={} method={} elapsed_ms={} request_bytes={} error={err}",
                    std::process::id(),
                    request_id,
                    method,
                    elapsed_ms,
                    browser_payload.len()
                );
                let diagnostic = json!({
                    "id": format!(
                        "native:{}:{}:{}",
                        std::process::id(),
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map(|duration| duration.as_nanos())
                            .unwrap_or_default(),
                        DIAGNOSTIC_SEQUENCE.fetch_add(1, Ordering::Relaxed)
                    ),
                    "component": "native_host",
                    "severity": "error",
                    "kind": "rpc_failed",
                    "message": format!(
                        "pid={} id={} method={} elapsed_ms={} request_bytes={} error={err}",
                        std::process::id(),
                        request_id,
                        method,
                        elapsed_ms,
                        browser_payload.len()
                    ),
                    "request_id": request_id,
                    "method": method,
                });
                fallback_response_for_payload_with_diagnostic(
                    &browser_payload,
                    "backend unavailable",
                    Some(diagnostic),
                )
            }
        };

        write_native_message(output, &response)?;
    }
}

pub fn process_browser_payload(
    daemon_client: &impl DaemonTransport,
    browser_payload: &[u8],
) -> Result<Vec<u8>> {
    let prepared = prepare_daemon_request(browser_payload);
    let daemon_response = daemon_client.send(&prepared.daemon_payload)?;
    Ok(prepare_browser_response(
        &prepared.browser_kind,
        &daemon_response,
    ))
}

fn request_metadata(payload: &[u8]) -> (String, String) {
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

#[cfg(test)]
mod tests {
    use super::request_metadata;

    #[test]
    fn extracts_rpc_metadata_without_logging_params() {
        let (method, id) = request_metadata(
            br#"{"jsonrpc":"2.0","id":17,"method":"evaluate_url","params":{"url":"https://private.example/"}}"#,
        );
        assert_eq!(method, "evaluate_url");
        assert_eq!(id, "17");
    }
}
