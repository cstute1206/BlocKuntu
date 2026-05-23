#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." >/dev/null 2>&1 && pwd)"
CRATE_DIR="${REPO_ROOT}/focus-core"

echo "[focus-core] crate: ${CRATE_DIR}"
cd "${CRATE_DIR}"

echo "[focus-core] cleaning previous build artifacts"
cargo clean

echo "[focus-core] checking formatting"
cargo fmt --check

echo "[focus-core] compiling and running full test suite"
cargo test --all-targets

echo "[focus-core] verification complete"
