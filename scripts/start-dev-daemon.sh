#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." >/dev/null 2>&1 && pwd)"
RUNTIME_DIR="/tmp/blockuntu"

mkdir -p "${RUNTIME_DIR}"

if [ ! -f "${RUNTIME_DIR}/config.toml" ]; then
  cp "${REPO_ROOT}/examples/blockuntu.toml" "${RUNTIME_DIR}/config.toml"
fi

echo "[blockuntud-dev] config: ${RUNTIME_DIR}/config.toml"
echo "[blockuntud-dev] database: ${RUNTIME_DIR}/blockuntu.sqlite3"
echo "[blockuntud-dev] socket: ${RUNTIME_DIR}/blockuntud.sock"
echo "[blockuntud-dev] firefox policy sandbox: ${RUNTIME_DIR}/firefox/policies.json"
echo "[blockuntud-dev] hosts sandbox: ${RUNTIME_DIR}/hosts"

exec cargo run --manifest-path "${REPO_ROOT}/focusd/Cargo.toml" -- \
  --config "${RUNTIME_DIR}/config.toml" \
  --database "${RUNTIME_DIR}/blockuntu.sqlite3" \
  --socket "${RUNTIME_DIR}/blockuntud.sock" \
  --firefox-policy "${RUNTIME_DIR}/firefox/policies.json" \
  --hosts "${RUNTIME_DIR}/hosts" \
  --dev-bind-socket \
  serve
