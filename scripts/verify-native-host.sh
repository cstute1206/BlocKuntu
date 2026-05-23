#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." >/dev/null 2>&1 && pwd)"
CRATE_DIR="${REPO_ROOT}/native-host"

echo "[native-host] crate: ${CRATE_DIR}"
cd "${CRATE_DIR}"

echo "[native-host] cleaning previous build artifacts"
cargo clean

echo "[native-host] checking formatting"
cargo fmt --check

echo "[native-host] compiling and running full test suite"
cargo test --all-targets

echo "[native-host] verification complete"
