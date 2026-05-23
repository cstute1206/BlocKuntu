use std::io::{Read, Write};

use crate::daemon_client::DaemonTransport;
use crate::error::Result;
use crate::native_messaging::{read_native_message, write_native_message};
use crate::protocol::{
    fallback_response_for_payload, prepare_browser_response, prepare_daemon_request,
};

pub fn run_bridge<R: Read, W: Write>(
    input: &mut R,
    output: &mut W,
    daemon_client: &impl DaemonTransport,
) -> Result<()> {
    loop {
        let Some(browser_payload) = read_native_message(input)? else {
            return Ok(());
        };

        let response = match process_browser_payload(daemon_client, &browser_payload) {
            Ok(response) => response,
            Err(err) => {
                eprintln!("blockuntu-native: backend unavailable: {err}");
                fallback_response_for_payload(&browser_payload, "backend unavailable")
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
