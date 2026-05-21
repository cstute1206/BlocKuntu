use serde_json::json;
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

const SOCKET_PATH: &str = "/run/blockuntu/blockuntud.sock";
const MAX_MESSAGE_BYTES: usize = 1024 * 1024;

fn main() {
    if let Err(err) = run() {
        eprintln!("fatal native host error: {err}");
    }
}

fn run() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut input = stdin.lock();
    let mut output = stdout.lock();

    loop {
        let payload = match read_native_message(&mut input) {
            Ok(Some(payload)) => payload,
            Ok(None) => return Ok(()),
            Err(err) => {
                eprintln!("native messaging stdin protocol error: {err}");
                write_native_message(
                    &mut output,
                    &fallback_response("native messaging stdin protocol error"),
                )?;
                return Ok(());
            }
        };

        let response = match forward_to_daemon(&payload) {
            Ok(response) => response,
            Err(err) => {
                eprintln!("failed to evaluate URL through {SOCKET_PATH}: {err}");
                fallback_response_for_payload(&payload, "backend unavailable")
            }
        };

        if let Err(err) = write_native_message(&mut output, &response) {
            eprintln!("failed to write native messaging response to stdout: {err}");
            return Err(err);
        }
    }
}

fn read_native_message<R: Read>(input: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut length_bytes = [0_u8; 4];
    let mut bytes_read = 0;

    while bytes_read < length_bytes.len() {
        let read = input.read(&mut length_bytes[bytes_read..])?;
        if read == 0 {
            if bytes_read == 0 {
                return Ok(None);
            }

            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "partial native messaging length header",
            ));
        }

        bytes_read += read;
    }

    let length = u32::from_ne_bytes(length_bytes) as usize;
    if length > MAX_MESSAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("native message length {length} exceeds {MAX_MESSAGE_BYTES} bytes"),
        ));
    }

    let mut payload = vec![0_u8; length];
    input.read_exact(&mut payload)?;
    Ok(Some(payload))
}

fn write_native_message<W: Write>(output: &mut W, payload: &[u8]) -> io::Result<()> {
    if payload.len() > u32::MAX as usize {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "native response is too large",
        ));
    }

    output.write_all(&(payload.len() as u32).to_ne_bytes())?;
    output.write_all(payload)?;
    output.flush()
}

fn forward_to_daemon(payload: &[u8]) -> io::Result<Vec<u8>> {
    let mut stream = UnixStream::connect(SOCKET_PATH)?;
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;

    stream.write_all(payload)?;
    stream.shutdown(std::net::Shutdown::Write)?;

    let response = read_daemon_response(&mut stream)?;
    if response.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "daemon returned an empty response",
        ));
    }

    serde_json::from_slice::<serde_json::Value>(&response)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;

    Ok(response)
}

fn read_daemon_response(stream: &mut UnixStream) -> io::Result<Vec<u8>> {
    let mut response = Vec::new();
    let mut buffer = [0_u8; 8192];

    loop {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            return Ok(response);
        }

        if response.len() + read > MAX_MESSAGE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("daemon response exceeds {MAX_MESSAGE_BYTES} bytes"),
            ));
        }

        response.extend_from_slice(&buffer[..read]);
    }
}

fn fallback_response(reason: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({ "action": "allow", "error": reason }))
        .expect("fallback response must serialize")
}

fn fallback_response_for_payload(payload: &[u8], reason: &str) -> Vec<u8> {
    let is_heartbeat = serde_json::from_slice::<serde_json::Value>(payload)
        .ok()
        .and_then(|value| {
            value
                .get("type")
                .and_then(|message_type| message_type.as_str())
                .map(|message_type| message_type == "extension_heartbeat")
        })
        .unwrap_or(false);

    if is_heartbeat {
        serde_json::to_vec(&json!({
            "type": "extension_heartbeat",
            "status": "error",
            "error": reason
        }))
        .expect("fallback response must serialize")
    } else {
        fallback_response(reason)
    }
}

#[cfg(test)]
mod tests {
    use super::{fallback_response_for_payload, read_native_message, write_native_message};
    use std::io::Cursor;

    #[test]
    fn reads_native_message_with_native_length_prefix() {
        let payload = br#"{"url":"https://example.com"}"#;
        let mut framed = Vec::new();
        framed.extend_from_slice(&(payload.len() as u32).to_ne_bytes());
        framed.extend_from_slice(payload);

        let mut input = Cursor::new(framed);
        let parsed = read_native_message(&mut input)
            .expect("message should parse")
            .expect("message should be present");

        assert_eq!(parsed, payload);
    }

    #[test]
    fn writes_native_message_with_native_length_prefix() {
        let payload = br#"{"action":"allow"}"#;
        let mut output = Vec::new();

        write_native_message(&mut output, payload).expect("message should serialize");

        assert_eq!(&output[..4], &(payload.len() as u32).to_ne_bytes());
        assert_eq!(&output[4..], payload);
    }

    #[test]
    fn preserves_heartbeat_shape_for_fallbacks() {
        let response = fallback_response_for_payload(
            br#"{"type":"extension_heartbeat"}"#,
            "backend unavailable",
        );
        let response: serde_json::Value =
            serde_json::from_slice(&response).expect("fallback should be valid JSON");

        assert_eq!(response["type"], "extension_heartbeat");
        assert_eq!(response["status"], "error");
    }
}
