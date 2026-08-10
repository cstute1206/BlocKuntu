#!/usr/bin/env bash
set -euo pipefail

BROWSER=""
OUTPUT_JSON=0
PROBE_TIMEOUT_SECONDS=12
SNAP_NAME=""
SNAP_COMMAND=""
POLICY_PATH=""
POLICY_PAGE=""

usage() {
  cat <<'USAGE'
Usage: blockuntu-diagnose-snap-policy --browser BROWSER [--json]

Run a short, isolated policy-loader probe for a Chromium-family Snap browser.
The probe launches the browser with a temporary profile and reads only its
platform policy-loader output. It does not touch the real browser profile or
change any BlocKuntu policy.

Supported browsers: chromium, opera, vivaldi

Options:
  --browser BROWSER       Browser to inspect (required).
  --json                  Emit one JSON object for GUI/automation use.
  --timeout-seconds N     Probe timeout (default: 12; maximum: 30).
  -h, --help              Show this help.
USAGE
}

die() {
  printf '[blockuntu-snap-policy] error: %s\n' "$*" >&2
  exit 2
}

json_escape() {
  local value="$1"
  value="${value//\\/\\\\}"
  value="${value//\"/\\\"}"
  value="${value//$'\n'/\\n}"
  value="${value//$'\r'/\\r}"
  value="${value//$'\t'/\\t}"
  printf '%s' "${value}"
}

json_string() {
  printf '"%s"' "$(json_escape "$1")"
}

json_optional_string() {
  if [[ -z "$1" ]]; then
    printf 'null'
  else
    json_string "$1"
  fi
}

configure_browser() {
  case "${BROWSER}" in
    chromium)
      SNAP_NAME="chromium"
      SNAP_COMMAND="chromium"
      POLICY_PATH="/var/snap/chromium/current/policies/managed/blockuntu.json"
      POLICY_PAGE="chrome://policy"
      ;;
    opera)
      SNAP_NAME="opera"
      SNAP_COMMAND="opera"
      POLICY_PATH="/etc/opt/opera/policies/managed/blockuntu.json"
      POLICY_PAGE="opera://policy"
      ;;
    vivaldi)
      SNAP_NAME="vivaldi"
      SNAP_COMMAND="vivaldi.vivaldi-stable"
      POLICY_PATH="/etc/vivaldi/policies/managed/blockuntu.json"
      POLICY_PAGE="vivaldi://policy"
      ;;
    *)
      die "unsupported browser '${BROWSER}'; use chromium, opera, or vivaldi"
      ;;
  esac
}

policy_file_state() {
  if [[ ! -e "${POLICY_PATH}" ]]; then
    printf 'missing'
  elif [[ ! -r "${POLICY_PATH}" ]]; then
    printf 'unreadable'
  elif grep -Fq '"ExtensionInstallForcelist"' "${POLICY_PATH}" \
    && grep -Fq '"ExtensionSettings"' "${POLICY_PATH}"; then
    printf 'readable_expected_keys_present'
  else
    printf 'readable_expected_keys_missing'
  fi
}

emit_json() {
  local policy_state="$1"
  local loader_state="$2"
  local loader_path="$3"
  local loader_line="$4"
  local exit_status="$5"
  local detail="$6"

  printf '{'
  printf '"browser":'; json_string "${BROWSER}"
  printf ',"snap_name":'; json_string "${SNAP_NAME}"
  printf ',"snap_command":'; json_string "${SNAP_COMMAND}"
  printf ',"policy_path":'; json_string "${POLICY_PATH}"
  printf ',"policy_file_state":'; json_string "${policy_state}"
  printf ',"loader_state":'; json_string "${loader_state}"
  printf ',"loader_path":'; json_optional_string "${loader_path}"
  printf ',"loader_line":'; json_optional_string "${loader_line}"
  printf ',"probe_exit_status":%s' "${exit_status}"
  printf ',"policy_page":'; json_string "${POLICY_PAGE}"
  printf ',"detail":'; json_string "${detail}"
  printf '}\n'
}

emit_human() {
  local policy_state="$1"
  local loader_state="$2"
  local loader_path="$3"
  local loader_line="$4"
  local exit_status="$5"
  local detail="$6"

  printf 'BlocKuntu %s Snap policy diagnostic\n' "${BROWSER}"
  printf 'Policy file: %s (%s)\n' "${POLICY_PATH}" "${policy_state}"
  printf 'Policy loader: %s' "${loader_state}"
  [[ -n "${loader_path}" ]] && printf ' (%s)' "${loader_path}"
  printf '\n'
  [[ -n "${loader_line}" ]] && printf 'Loader evidence: %s\n' "${loader_line}"
  printf 'Probe exit status: %s\n' "${exit_status}"
  printf 'Result: %s\n' "${detail}"
  printf 'Browser policy page: %s\n' "${POLICY_PAGE}"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --browser)
      [[ $# -ge 2 ]] || die "--browser requires a value"
      BROWSER="$2"
      shift 2
      ;;
    --json)
      OUTPUT_JSON=1
      shift
      ;;
    --timeout-seconds)
      [[ $# -ge 2 ]] || die "--timeout-seconds requires a value"
      PROBE_TIMEOUT_SECONDS="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      die "unknown option: $1"
      ;;
  esac
done

[[ -n "${BROWSER}" ]] || die "--browser is required"
[[ "${PROBE_TIMEOUT_SECONDS}" =~ ^[1-9][0-9]*$ ]] \
  && [[ "${PROBE_TIMEOUT_SECONDS}" -le 30 ]] \
  || die "--timeout-seconds must be between 1 and 30"
configure_browser

policy_state="$(policy_file_state)"
loader_state="not_run"
loader_path=""
loader_line=""
probe_exit_status=0
detail=""

if ! command -v snap >/dev/null 2>&1; then
  loader_state="unavailable"
  detail="snapd is not installed or the snap command is unavailable"
elif ! timeout 5 snap list "${SNAP_NAME}" >/dev/null 2>&1; then
  loader_state="unavailable"
  detail="the ${BROWSER} Snap is not installed for this system"
else
  profile_dir="$(mktemp -d "${TMPDIR:-/tmp}/blockuntu-snap-policy.XXXXXX")"
  cleanup() {
    rm -rf "${profile_dir}"
  }
  trap cleanup EXIT

  set +e
  loader_output="$(timeout --signal=TERM --kill-after=2s "${PROBE_TIMEOUT_SECONDS}s" \
    snap run "${SNAP_COMMAND}" \
      --headless \
      --disable-gpu \
      --no-first-run \
      --no-default-browser-check \
      --user-data-dir="${profile_dir}" \
      --enable-logging=stderr \
      --v=1 \
      --vmodule=config_dir_policy_loader=2 \
      about:blank 2>&1)"
  probe_exit_status=$?
  set -e

  loader_line="$(printf '%s\n' "${loader_output}" | grep -m 1 -E \
    'Found mandatory policy file:|Skipping mandatory platform policies because no policy file was found at:' \
    || true)"
  if [[ "${loader_line}" == *"Found mandatory policy file:"* ]]; then
    loader_state="found"
    loader_path="${loader_line##*: }"
    detail="the Snap browser found a mandatory platform policy file"
  elif [[ "${loader_line}" == *"Skipping mandatory platform policies because no policy file was found at:"* ]]; then
    loader_state="missing"
    loader_path="${loader_line##*: }"
    detail="the Snap browser did not find a mandatory platform policy file"
  elif [[ "${probe_exit_status}" -eq 0 || "${probe_exit_status}" -eq 124 || "${probe_exit_status}" -eq 137 ]]; then
    loader_state="not_reported"
    detail="the probe ran, but this browser emitted no platform policy-loader evidence"
  else
    loader_state="probe_failed"
    detail="the browser probe failed before it reported platform policy loading"
  fi
fi

if [[ "${OUTPUT_JSON}" -eq 1 ]]; then
  emit_json "${policy_state}" "${loader_state}" "${loader_path}" "${loader_line}" "${probe_exit_status}" "${detail}"
else
  emit_human "${policy_state}" "${loader_state}" "${loader_path}" "${loader_line}" "${probe_exit_status}" "${detail}"
fi
