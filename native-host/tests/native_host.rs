use std::cell::RefCell;

use native_host::daemon_client::DaemonTransport;
use native_host::error::Result;
use native_host::native_messaging::{read_native_message, write_native_message};
use serde_json::{json, Value};

struct FakeDaemon {
    requests: RefCell<Vec<Value>>,
    response: Value,
}

impl DaemonTransport for FakeDaemon {
    fn send(&self, payload: &[u8]) -> Result<Vec<u8>> {
        self.requests
            .borrow_mut()
            .push(serde_json::from_slice(payload)?);
        Ok(serde_json::to_vec(&self.response)?)
    }
}

#[test]
fn bridges_legacy_native_message_to_daemon_jsonrpc_and_back() {
    let daemon = FakeDaemon {
        requests: RefCell::new(Vec::new()),
        response: json!({
            "jsonrpc": "2.0",
            "id": null,
            "result": {
                "decision": "block",
                "reason": {
                    "kind": "hard_block",
                    "rule_id": "hard"
                }
            }
        }),
    };

    let mut input = Vec::new();
    write_native_message(
        &mut input,
        br#"{"url":"https://blocked.example/","tabId":1}"#,
    )
    .expect("input should frame");
    let mut output = Vec::new();

    native_host::bridge::run_bridge(&mut input.as_slice(), &mut output, &daemon)
        .expect("bridge should run");

    let requests = daemon.requests.borrow();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["method"], "evaluate_url");
    assert_eq!(requests[0]["params"]["url"], "https://blocked.example/");

    let response = read_native_message(&mut output.as_slice())
        .expect("response should parse")
        .expect("response should exist");
    let response: Value = serde_json::from_slice(&response).expect("response should parse");

    assert_eq!(response["action"], "block");
    assert_eq!(response["reason"]["kind"], "hard_block");
}
