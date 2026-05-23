use std::io::{Read, Write};

use crate::error::{NativeHostError, Result};

pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;

pub fn read_native_message<R: Read>(input: &mut R) -> Result<Option<Vec<u8>>> {
    let mut length_bytes = [0_u8; 4];
    let mut bytes_read = 0;

    while bytes_read < length_bytes.len() {
        let read = input.read(&mut length_bytes[bytes_read..])?;
        if read == 0 {
            if bytes_read == 0 {
                return Ok(None);
            }

            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "partial native messaging length header",
            )
            .into());
        }

        bytes_read += read;
    }

    let length = u32::from_ne_bytes(length_bytes) as usize;
    if length > MAX_MESSAGE_BYTES {
        return Err(NativeHostError::MessageTooLarge {
            length,
            limit: MAX_MESSAGE_BYTES,
        });
    }

    let mut payload = vec![0_u8; length];
    input.read_exact(&mut payload)?;
    Ok(Some(payload))
}

pub fn write_native_message<W: Write>(output: &mut W, payload: &[u8]) -> Result<()> {
    if payload.len() > u32::MAX as usize {
        return Err(NativeHostError::ResponseTooLarge);
    }

    output.write_all(&(payload.len() as u32).to_ne_bytes())?;
    output.write_all(payload)?;
    output.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::{read_native_message, write_native_message};

    #[test]
    fn reads_native_message_with_native_length_prefix() {
        let payload = br#"{"url":"https://example.com"}"#;
        let mut framed = Vec::new();
        framed.extend_from_slice(&(payload.len() as u32).to_ne_bytes());
        framed.extend_from_slice(payload);

        let parsed = read_native_message(&mut Cursor::new(framed))
            .expect("message should parse")
            .expect("message should exist");
        assert_eq!(parsed, payload);
    }

    #[test]
    fn writes_native_message_with_native_length_prefix() {
        let payload = br#"{"action":"allow"}"#;
        let mut output = Vec::new();

        write_native_message(&mut output, payload).expect("message should write");

        assert_eq!(&output[..4], &(payload.len() as u32).to_ne_bytes());
        assert_eq!(&output[4..], payload);
    }
}
