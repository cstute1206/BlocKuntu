#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." >/dev/null 2>&1 && pwd)"
RUNTIME_DIR="/tmp/blockuntu"

mkdir -p "${RUNTIME_DIR}"

LOCK_PATH="${RUNTIME_DIR}/dev-daemon.lock"
exec 9>"${LOCK_PATH}"
if ! flock -n 9; then
  echo "[blockuntud-dev] another dev daemon starter is already running"
  exit 0
fi

if [ ! -f "${RUNTIME_DIR}/config.toml" ]; then
  cp "${REPO_ROOT}/examples/blockuntu.toml" "${RUNTIME_DIR}/config.toml"
fi

echo "[blockuntud-dev] config: ${RUNTIME_DIR}/config.toml"
echo "[blockuntud-dev] database: ${RUNTIME_DIR}/blockuntu.sqlite3"
echo "[blockuntud-dev] event log: ${RUNTIME_DIR}/blockuntu.log"
echo "[blockuntud-dev] policy recovery: ${RUNTIME_DIR}/policy-recovery.toml"
echo "[blockuntud-dev] socket: ${RUNTIME_DIR}/blockuntud.sock"
echo "[blockuntud-dev] firefox policy sandbox: ${RUNTIME_DIR}/firefox/policies.json"
echo "[blockuntud-dev] chrome policy sandbox: ${RUNTIME_DIR}/chrome/policies/managed/blockuntu.json"
echo "[blockuntud-dev] chrome update manifest sandbox: ${RUNTIME_DIR}/chrome/updates.xml"
echo "[blockuntud-dev] hosts sandbox: ${RUNTIME_DIR}/hosts"

exec cargo run --manifest-path "${REPO_ROOT}/focusd/Cargo.toml" -- \
  --config "${RUNTIME_DIR}/config.toml" \
  --database "${RUNTIME_DIR}/blockuntu.sqlite3" \
  --event-log "${RUNTIME_DIR}/blockuntu.log" \
  --policy-recovery "${RUNTIME_DIR}/policy-recovery.toml" \
  --no-policy-recovery-immutable \
  --socket "${RUNTIME_DIR}/blockuntud.sock" \
  --firefox-policy "${RUNTIME_DIR}/firefox/policies.json" \
  --chrome-policy "${RUNTIME_DIR}/chrome/policies/managed/blockuntu.json" \
  --chrome-update-manifest "${RUNTIME_DIR}/chrome/updates.xml" \
  --hosts "${RUNTIME_DIR}/hosts" \
  --dev-bind-socket \
  serve
