use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use ed25519_dalek::{Signature, VerifyingKey};

pub const BUILD_NUMBER: &str = match option_env!("BLOCKUNTU_BUILD_NUMBER") {
    Some(value) => value,
    None => env!("CARGO_PKG_VERSION"),
};
pub(crate) const EMERGENCY_UNINSTALL_CODE_PREFIX: &str = "BLOCKUNTU-EU1-";

const EMERGENCY_UNINSTALL_PUBLIC_KEY: [u8; 32] = [
    0xe5, 0xb0, 0xa4, 0x38, 0x37, 0xf8, 0xbb, 0x39, 0x51, 0x65, 0x7e, 0x77, 0x77, 0xb3, 0x1f, 0x37,
    0xd5, 0x89, 0x34, 0x06, 0x15, 0xf7, 0xda, 0x9c, 0x11, 0x7a, 0x3f, 0xf1, 0x0b, 0x41, 0x56, 0x5c,
];

pub(crate) fn emergency_uninstall_message(build_number: &str) -> Vec<u8> {
    format!("blockuntu:emergency-uninstall:v1:{build_number}").into_bytes()
}

pub fn emergency_uninstall_code_is_valid(candidate: &str) -> bool {
    emergency_uninstall_code_is_valid_for(candidate, BUILD_NUMBER, &EMERGENCY_UNINSTALL_PUBLIC_KEY)
}

fn emergency_uninstall_code_is_valid_for(
    candidate: &str,
    build_number: &str,
    public_key_bytes: &[u8; 32],
) -> bool {
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
        .verify_strict(&emergency_uninstall_message(build_number), &signature)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn signed_code(signing_key: &SigningKey, build_number: &str) -> String {
        let signature = signing_key.sign(&emergency_uninstall_message(build_number));
        format!(
            "{EMERGENCY_UNINSTALL_CODE_PREFIX}{}",
            URL_SAFE_NO_PAD.encode(signature.to_bytes())
        )
    }

    #[test]
    fn accepts_a_signature_for_the_exact_build() {
        let signing_key = SigningKey::from_bytes(&[7; 32]);
        let code = signed_code(&signing_key, "0.1.0-11");

        assert!(emergency_uninstall_code_is_valid_for(
            &code,
            "0.1.0-11",
            signing_key.verifying_key().as_bytes()
        ));
    }

    #[test]
    fn rejects_a_signature_for_another_build() {
        let signing_key = SigningKey::from_bytes(&[7; 32]);
        let code = signed_code(&signing_key, "0.1.0-11");

        assert!(!emergency_uninstall_code_is_valid_for(
            &code,
            "0.1.0-12",
            signing_key.verifying_key().as_bytes()
        ));
    }

    #[test]
    fn rejects_malformed_codes() {
        let signing_key = SigningKey::from_bytes(&[7; 32]);

        assert!(!emergency_uninstall_code_is_valid_for(
            "BLOCKUNTU-EU1-not-a-signature",
            "0.1.0-11",
            signing_key.verifying_key().as_bytes()
        ));
        assert!(!emergency_uninstall_code_is_valid_for(
            "",
            "0.1.0-11",
            signing_key.verifying_key().as_bytes()
        ));
    }
}
