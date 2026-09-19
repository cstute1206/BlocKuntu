#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
TESTING_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
REPO_ROOT="$(cd -- "${TESTING_ROOT}/.." && pwd)"
RUNTIME_DIR=""
SITE_PID=""
APP_PID=""
PARENT_PID=""

cleanup() {
  for pid in "${APP_PID}" "${PARENT_PID}" "${SITE_PID}"; do
    if [[ -n "${pid}" ]] && kill -0 "${pid}" >/dev/null 2>&1; then
      kill "${pid}" >/dev/null 2>&1 || true
      wait "${pid}" >/dev/null 2>&1 || true
    fi
  done
  if [[ -n "${RUNTIME_DIR}" && -d "${RUNTIME_DIR}" ]]; then
    rm -rf -- "${RUNTIME_DIR}"
  fi
}
trap cleanup EXIT INT TERM

wait_for_file() {
  local path="$1"
  for _attempt in $(seq 1 100); do
    [[ -s "${path}" ]] && return 0
    sleep 0.05
  done
  printf 'timed out waiting for %s\n' "${path}" >&2
  return 1
}

assert_process_identity() {
  local pid="$1"
  local expected_comm="$2"
  local expected_executable="$3"
  local actual_comm
  local actual_executable

  actual_comm="$(tr -d '\n' <"/proc/${pid}/comm")"
  actual_executable="$(basename -- "$(readlink -- "/proc/${pid}/exe")")"
  [[ "${actual_comm}" == "${expected_comm}" ]] || {
    printf 'PID %s command name: expected %s, got %s\n' \
      "${pid}" "${expected_comm}" "${actual_comm}" >&2
    return 1
  }
  [[ "${actual_executable}" == "${expected_executable}" ]] || {
    printf 'PID %s executable: expected %s, got %s\n' \
      "${pid}" "${expected_executable}" "${actual_executable}" >&2
    return 1
  }
}

command -v python3 >/dev/null 2>&1 || {
  printf 'python3 is required\n' >&2
  exit 1
}
command -v cargo >/dev/null 2>&1 || {
  printf 'cargo is required for full policy-schema validation\n' >&2
  exit 1
}

RUNTIME_DIR="$(mktemp -d "${TESTING_ROOT}/runtime/phase0.XXXXXX")"
BIN_DIR="${RUNTIME_DIR}/bin"
"${SCRIPT_DIR}/build-test-processes.sh" --output-dir "${BIN_DIR}"

SITE_READY="${RUNTIME_DIR}/site-ready.json"
python3 "${TESTING_ROOT}/fixtures/test-site/server.py" \
  --port 0 \
  --ready-file "${SITE_READY}" \
  >"${RUNTIME_DIR}/site.log" 2>&1 &
SITE_PID=$!
wait_for_file "${SITE_READY}"
SITE_PORT="$(python3 -c 'import json, sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["port"])' "${SITE_READY}")"
python3 -c 'import sys, urllib.request; assert urllib.request.urlopen(f"http://127.0.0.1:{sys.argv[1]}/healthz", timeout=2).status == 200' "${SITE_PORT}"

RUN_DIR="$(
  python3 "${SCRIPT_DIR}/prepare-run.py" \
    --run-id phase0-verification \
    --results-root "${RUNTIME_DIR}/results" \
    --site-port "${SITE_PORT}" \
    --vm-template ubuntu
)"
POLICY_PATH="${RUN_DIR}/policy/blockuntu-policy.toml"

CHECK_DIR="${RUNTIME_DIR}/policy-check"
mkdir -p "${CHECK_DIR}"
cargo run \
  --quiet \
  --locked \
  --target-dir "${RUNTIME_DIR}/cargo-target" \
  --manifest-path "${REPO_ROOT}/focusd/Cargo.toml" \
  -- \
  --config "${POLICY_PATH}" \
  --database "${CHECK_DIR}/blockuntu.sqlite3" \
  --event-log "${CHECK_DIR}/blockuntu.log" \
  --policy-recovery "${CHECK_DIR}/policy-recovery.toml" \
  --socket "${CHECK_DIR}/blockuntud.sock" \
  --hosts "${CHECK_DIR}/hosts" \
  --no-hosts-immutable \
  --no-policy-recovery-immutable \
  --no-browser-policy-repair \
  check

APP_READY="${RUNTIME_DIR}/app-a.json"
"${BIN_DIR}/blockuntu-test-app-a" \
  --ready-file "${APP_READY}" \
  --lifetime 30 \
  >"${RUNTIME_DIR}/app-a.log" 2>&1 &
APP_PID=$!
wait_for_file "${APP_READY}"
assert_process_identity "${APP_PID}" bk-test-app-a blockuntu-test-app-a
kill "${APP_PID}"
wait "${APP_PID}"
APP_PID=""

PARENT_READY="${RUNTIME_DIR}/parent.json"
"${BIN_DIR}/blockuntu-test-parent" \
  --spawn-helper "${BIN_DIR}/blockuntu-test-helper" \
  --ready-file "${PARENT_READY}" \
  --lifetime 30 \
  >"${RUNTIME_DIR}/parent.log" 2>&1 &
PARENT_PID=$!
wait_for_file "${PARENT_READY}"
HELPER_PID="$(python3 -c 'import json, sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["child_pid"])' "${PARENT_READY}")"
assert_process_identity "${PARENT_PID}" bk-test-parent blockuntu-test-parent
for _attempt in $(seq 1 100); do
  if [[ -r "/proc/${HELPER_PID}/comm" ]] && \
    [[ "$(tr -d '\n' <"/proc/${HELPER_PID}/comm")" == "bk-test-helper" ]]; then
    break
  fi
  sleep 0.05
done
assert_process_identity "${HELPER_PID}" bk-test-helper blockuntu-test-helper
kill "${PARENT_PID}"
wait "${PARENT_PID}"
PARENT_PID=""

printf 'Phase 0 fixture verification passed.\n'
