#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." >/dev/null 2>&1 && pwd)"
CRATE_DIR="${REPO_ROOT}/focusd"

echo "[focusd] crate: ${CRATE_DIR}"
cd "${CRATE_DIR}"

echo "[focusd] cleaning previous build artifacts"
cargo clean

echo "[focusd] checking formatting"
cargo fmt --check

echo "[focusd] compiling and running full test suite"
cargo test --all-targets

echo "[focusd] verification complete"
