use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use ed25519_dalek::{Signature, VerifyingKey};

pub(crate) const EMERGENCY_UNINSTALL_CODE_PREFIX: &str = "BLOCKUNTU-EU2-";

const EMERGENCY_UNINSTALL_PUBLIC_KEY: [u8; 32] = [
    0xe5, 0xb0, 0xa4, 0x38, 0x37, 0xf8, 0xbb, 0x39, 0x51, 0x65, 0x7e, 0x77, 0x77, 0xb3, 0x1f, 0x37,
    0xd5, 0x89, 0x34, 0x06, 0x15, 0xf7, 0xda, 0x9c, 0x11, 0x7a, 0x3f, 0xf1, 0x0b, 0x41, 0x56, 0x5c,
];

pub fn installation_serial_is_valid(serial: &str) -> bool {
    let Some(body) = serial.trim().strip_prefix("BKI-") else {
        return false;
    };
    let mut groups = body.split('-');
    let valid_group = |group: &str| {
        group.len() == 8
            && group
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'A'..=b'F').contains(&byte))
    };

    (0..4).all(|_| groups.next().is_some_and(valid_group)) && groups.next().is_none()
}

pub(crate) fn emergency_uninstall_message(installation_serial: &str) -> Vec<u8> {
    format!(
        "blockuntu:emergency-uninstall:v2:{}",
        installation_serial.trim()
    )
    .into_bytes()
}

pub fn emergency_uninstall_code_is_valid(candidate: &str, installation_serial: &str) -> bool {
    emergency_uninstall_code_is_valid_for(
        candidate,
        installation_serial,
        &EMERGENCY_UNINSTALL_PUBLIC_KEY,
    )
}

fn emergency_uninstall_code_is_valid_for(
    candidate: &str,
    installation_serial: &str,
    public_key_bytes: &[u8; 32],
) -> bool {
    if !installation_serial_is_valid(installation_serial) {
        return false;
    }
    let Some(encoded_signature) = candidate
        .trim()
        .strip_prefix(EMERGENCY_UNINSTALL_CODE_PREFIX)
    else {
        return false;
    };
    let Ok(signature_bytes) = URL_SAFE_NO_PAD.decode(encoded_signature) else {
        return false;
    };
    let Ok(signature) = Signature::from_slice(&signature_bytes) else {
        return false;
    };
    let Ok(public_key) = VerifyingKey::from_bytes(public_key_bytes) else {
        return false;
    };

    public_key
        .verify_strict(
            &emergency_uninstall_message(installation_serial),
            &signature,
        )
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    const SERIAL_A: &str = "BKI-7A91C246-398AF072-5E70DA11-D9B4C83F";
    const SERIAL_B: &str = "BKI-00000000-00000000-00000000-00000001";
    const PRODUCTION_TEST_CODE: &str = "BLOCKUNTU-EU2-spbREObNCff7ly9mJiKTfl25QchAAOKMtknbC-6r9EWdgxGnrNsMrrp0hNgsfEENlbkzhHJYkwTnIpwtYixnAg";

    fn signed_code(signing_key: &SigningKey, installation_serial: &str) -> String {
        let signature = signing_key.sign(&emergency_uninstall_message(installation_serial));
        format!(
            "{EMERGENCY_UNINSTALL_CODE_PREFIX}{}",
            URL_SAFE_NO_PAD.encode(signature.to_bytes())
        )
    }

    #[test]
    fn accepts_a_signature_for_the_exact_installation() {
        let signing_key = SigningKey::from_bytes(&[7; 32]);
        let code = signed_code(&signing_key, SERIAL_A);

        assert!(emergency_uninstall_code_is_valid_for(
            &code,
            SERIAL_A,
            signing_key.verifying_key().as_bytes()
        ));
    }

    #[test]
    fn production_public_key_accepts_the_operator_test_vector() {
        assert!(emergency_uninstall_code_is_valid(
            PRODUCTION_TEST_CODE,
            SERIAL_B
        ));
        assert!(!emergency_uninstall_code_is_valid(
            PRODUCTION_TEST_CODE,
            SERIAL_A
        ));
    }

    #[test]
    fn rejects_a_signature_for_another_installation() {
        let signing_key = SigningKey::from_bytes(&[7; 32]);
        let code = signed_code(&signing_key, SERIAL_A);

        assert!(!emergency_uninstall_code_is_valid_for(
            &code,
            SERIAL_B,
            signing_key.verifying_key().as_bytes()
        ));
    }

    #[test]
    fn validates_the_serial_format() {
        assert!(installation_serial_is_valid(SERIAL_A));
        assert!(installation_serial_is_valid(&format!(" {SERIAL_A}\n")));
        assert!(!installation_serial_is_valid(
            "BKI-7a91c246-398AF072-5E70DA11-D9B4C83F"
        ));
        assert!(!installation_serial_is_valid(
            "BKI-7A91C246-398AF072-5E70DA11"
        ));
        assert!(!installation_serial_is_valid(
            "BKI-7A91C246-398AF072-5E70DA11-D9B4C83F-EXTRA"
        ));
    }

    #[test]
    fn rejects_malformed_and_legacy_codes() {
        let signing_key = SigningKey::from_bytes(&[7; 32]);

        assert!(!emergency_uninstall_code_is_valid_for(
            "BLOCKUNTU-EU2-not-a-signature",
            SERIAL_A,
            signing_key.verifying_key().as_bytes()
        ));
        assert!(!emergency_uninstall_code_is_valid_for(
            "BLOCKUNTU-EU1-not-a-signature",
            SERIAL_A,
            signing_key.verifying_key().as_bytes()
        ));
        assert!(!emergency_uninstall_code_is_valid_for(
            "",
            SERIAL_A,
            signing_key.verifying_key().as_bytes()
        ));
    }
}
