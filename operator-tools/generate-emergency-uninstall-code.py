#!/usr/bin/env python3
import argparse
import base64
from pathlib import Path

from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey


CODE_PREFIX = "BLOCKUNTU-EU1-"
PRIVATE_KEY_PATH = Path.home() / ".local/share/blockuntu-emergency/private-key.pem"


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("build_number")
    parser.add_argument("--private-key", type=Path, default=PRIVATE_KEY_PATH)
    args = parser.parse_args()

    build_number = args.build_number.strip()
    if not build_number:
        parser.error("build_number must not be empty")

    private_key = serialization.load_pem_private_key(
        args.private_key.expanduser().read_bytes(),
        password=None,
    )
    if not isinstance(private_key, Ed25519PrivateKey):
        parser.error("private key must be an Ed25519 key")

    message = f"blockuntu:emergency-uninstall:v1:{build_number}".encode()
    signature = private_key.sign(message)
    encoded_signature = base64.urlsafe_b64encode(signature).decode().rstrip("=")
    print(f"{CODE_PREFIX}{encoded_signature}")


if __name__ == "__main__":
    main()
