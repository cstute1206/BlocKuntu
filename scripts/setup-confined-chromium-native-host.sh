#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"

TARGETS="auto"
TARGET_USER="${SUDO_USER:-${USER:-}}"
TARGET_HOME=""
NATIVE_HOST=""
BRIDGE_ADDRESS="127.0.0.1:35173"
BRIDGE_TOKEN_FILE="/etc/blockuntu/snap-native-bridge-token"
BRIDGE_TOKEN=""
CHROME_EXTENSION_ID="opfljaancedgklbpnbpjfhdbbhbfpnoc"

usage() {
  cat <<'USAGE'
Usage: scripts/setup-confined-chromium-native-host.sh [options]

Install BlocKuntu Native Messaging support for strict Chromium-family Snaps.
The helper copies blockuntu-native and a small launcher into each Snap's
user-common directory. The launcher uses BlocKuntu's authenticated loopback
bridge because strict Snaps cannot access /run/blockuntu directly. Browser
policies remain deferred until the store extension sends a verified heartbeat.

Options:
  --targets LIST              Comma-separated list: auto, chromium, brave,
                              opera, vivaldi, all. Default: auto.
  --native-host PATH          Source blockuntu-native binary to copy into the
                              Snap's writable user-common area.
  --bridge-address ADDRESS    Authenticated daemon bridge address. Default:
                              127.0.0.1:35173.
  --bridge-token-file PATH    Read the bridge token from this file. Default:
                              /etc/blockuntu/snap-native-bridge-token.
  --user USER                 Desktop user whose Snap profiles are configured.
                              Default: current sudo/user context.
  --home DIR                  Home directory for --user, if auto-detection is
                              not correct.
  -h, --help                  Show this help.

Run this again after a Snap browser refresh if the browser was updated while
BlocKuntu was not running, then restart that browser.
USAGE
}

log() {
  printf '[blockuntu-confined-chromium] %s\n' "$*"
}

die() {
  printf '[blockuntu-confined-chromium] error: %s\n' "$*" >&2
  exit 1
}

has_cmd() {
  command -v "$1" >/dev/null 2>&1
}

run_as_target_user() {
  if [[ "${EUID}" -eq 0 && "${TARGET_USER}" != "root" ]]; then
    sudo -u "${TARGET_USER}" "$@"
  else
    "$@"
  fi
}

user_group() {
  id -gn "${TARGET_USER}" 2>/dev/null || printf '%s\n' "${TARGET_USER}"
}

own_path_for_target_user() {
  local path="$1"
  if [[ "${EUID}" -eq 0 && "${TARGET_USER}" != "root" ]]; then
    chown "${TARGET_USER}:$(user_group)" "${path}"
  fi
}

install_user_dir() {
  local path="$1"
  install -d -m 0755 "${path}"
  own_path_for_target_user "${path}"
}

copy_native_host() {
  local destination="$1"

  install_user_dir "$(dirname -- "${destination}")"
  install -m 0755 "${NATIVE_HOST}" "${destination}"
  own_path_for_target_user "${destination}"
}

shell_quote() {
  local value="$1"
  value="${value//\'/\'\"\'\"\'}"
  printf "'%s'" "${value}"
}

write_launcher() {
  local launcher_path="$1"
  local host_path="$2"
  local temp_file
  local quoted_host
  local quoted_address
  local quoted_token

  install_user_dir "$(dirname -- "${launcher_path}")"
  quoted_host="$(shell_quote "${host_path}")"
  quoted_address="$(shell_quote "${BRIDGE_ADDRESS}")"
  quoted_token="$(shell_quote "${BRIDGE_TOKEN}")"
  temp_file="$(mktemp)"
  cat >"${temp_file}" <<EOF
#!/bin/sh
exec ${quoted_host} --tcp-address ${quoted_address} --access-token ${quoted_token} "\$@"
EOF
  install -m 0700 "${temp_file}" "${launcher_path}"
  own_path_for_target_user "${launcher_path}"
  rm -f "${temp_file}"
}

write_manifest() {
  local manifest_path="$1"
  local host_path="$2"
  local browser="$3"
  local temp_file

  install_user_dir "$(dirname -- "${manifest_path}")"
  temp_file="$(mktemp)"
  cat >"${temp_file}" <<EOF
{
  "name": "blockuntu_native",
  "description": "BlocKuntu Chromium Native Messaging bridge for ${browser} Snap",
  "path": "${host_path}",
  "type": "stdio",
  "allowed_origins": ["chrome-extension://${CHROME_EXTENSION_ID}/"]
}
EOF
  install -m 0644 "${temp_file}" "${manifest_path}"
  own_path_for_target_user "${manifest_path}"
  rm -f "${temp_file}"
}

resolve_target_home() {
  if [[ -n "${TARGET_HOME}" ]]; then
    return 0
  fi

  if [[ -n "${TARGET_USER}" ]]; then
    TARGET_HOME="$(getent passwd "${TARGET_USER}" | cut -d: -f6 || true)"
  fi
  if [[ -z "${TARGET_HOME}" ]]; then
    TARGET_HOME="${HOME}"
  fi
}

resolve_native_host() {
  if [[ -n "${NATIVE_HOST}" ]]; then
    [[ -x "${NATIVE_HOST}" ]] || die "native host is not executable: ${NATIVE_HOST}"
    return 0
  fi

  local candidates=(
    "/usr/bin/blockuntu-native"
    "/usr/local/bin/blockuntu-native"
    "${REPO_ROOT}/native-host/target/release/blockuntu-native"
    "${REPO_ROOT}/native-host/target/debug/blockuntu-native"
  )

  local candidate
  for candidate in "${candidates[@]}"; do
    if [[ -x "${candidate}" ]]; then
      NATIVE_HOST="${candidate}"
      return 0
    fi
  done

  die "could not find blockuntu-native; pass --native-host PATH"
}

resolve_bridge_token() {
  [[ -r "${BRIDGE_TOKEN_FILE}" ]] || die "Snap native bridge token is not readable: ${BRIDGE_TOKEN_FILE}; sign out and back in after installing BlocKuntu, then retry"
  BRIDGE_TOKEN="$(tr -d '[:space:]' <"${BRIDGE_TOKEN_FILE}")"
  [[ "${BRIDGE_TOKEN}" =~ ^[0-9A-Fa-f]{64}$ ]] || die "Snap native bridge token must be exactly 64 hexadecimal characters: ${BRIDGE_TOKEN_FILE}"
}

target_requested() {
  local target="$1"
  case ",${TARGETS}," in
    *,all,*|*,${target},*)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

snap_browser_available() {
  local snap_name="$1"
  [[ -d "${TARGET_HOME}/snap/${snap_name}" ]] && return 0
  has_cmd snap && has_cmd timeout && run_as_target_user timeout 5 snap list "${snap_name}" >/dev/null 2>&1
}

should_install() {
  local target="$1"
  local snap_name="$2"
  if target_requested "${target}"; then
    return 0
  fi
  [[ "${TARGETS}" == "auto" ]] && snap_browser_available "${snap_name}"
}

install_snap_browser() {
  local snap_name="$1"
  local profile_manifest="$2"
  local browser="$3"
  local common_root="${TARGET_HOME}/snap/${snap_name}/common"
  local profile_root
  local host_path="${common_root}/.local/share/blockuntu/blockuntu-native"
  local launcher_path="${common_root}/.local/share/blockuntu/blockuntu-native-launcher"

  if [[ "${snap_name}" == "chromium" ]]; then
    profile_root="${common_root}"
  else
    profile_root="${TARGET_HOME}/snap/${snap_name}/current"
  fi
  local manifest_path="${profile_root}/${profile_manifest}"

  if [[ "${snap_name}" != "chromium" && ! -e "${profile_root}" ]]; then
    log "warning: ${browser} Snap has no current user profile; launch it once, then rerun this helper"
    return 1
  fi

  log "installing ${browser} Snap native host copy"
  copy_native_host "${host_path}"
  write_launcher "${launcher_path}" "${host_path}"
  write_manifest "${manifest_path}" "${launcher_path}" "${browser}"
  log "installed ${manifest_path}"
}

validate_targets() {
  [[ "${TARGETS}" == "auto" || "${TARGETS}" == "all" ]] && return 0

  local target
  local entries=()
  IFS=',' read -r -a entries <<<"${TARGETS}"
  [[ "${#entries[@]}" -gt 0 ]] || die "unsupported --targets value: ${TARGETS}"
  for target in "${entries[@]}"; do
    case "${target}" in
      chromium|brave|opera|vivaldi)
        ;;
      *)
        die "unsupported --targets value: ${TARGETS}"
        ;;
    esac
  done
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --targets)
      [[ $# -ge 2 ]] || die "--targets requires a value"
      TARGETS="$2"
      shift 2
      ;;
    --native-host)
      [[ $# -ge 2 ]] || die "--native-host requires a value"
      NATIVE_HOST="$2"
      shift 2
      ;;
    --bridge-address)
      [[ $# -ge 2 ]] || die "--bridge-address requires a value"
      BRIDGE_ADDRESS="$2"
      shift 2
      ;;
    --bridge-token-file)
      [[ $# -ge 2 ]] || die "--bridge-token-file requires a value"
      BRIDGE_TOKEN_FILE="$2"
      shift 2
      ;;
    --user)
      [[ $# -ge 2 ]] || die "--user requires a value"
      TARGET_USER="$2"
      shift 2
      ;;
    --home)
      [[ $# -ge 2 ]] || die "--home requires a value"
      TARGET_HOME="$2"
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

[[ -n "${TARGET_USER}" ]] || die "could not determine target user; pass --user USER"
resolve_target_home
resolve_native_host
resolve_bridge_token
validate_targets

log "target user: ${TARGET_USER}"
log "target home: ${TARGET_HOME}"
log "native host source: ${NATIVE_HOST}"
log "Snap native bridge: ${BRIDGE_ADDRESS}"

installed_any=0
for target_spec in \
  "chromium:chromium:chromium/NativeMessagingHosts/blockuntu_native.json:Chromium" \
  "brave:brave:.config/BraveSoftware/Brave-Browser/NativeMessagingHosts/blockuntu_native.json:Brave" \
  "opera:opera:.config/google-chrome/NativeMessagingHosts/blockuntu_native.json:Opera" \
  "vivaldi:vivaldi:.config/vivaldi/NativeMessagingHosts/blockuntu_native.json:Vivaldi"; do
  IFS=':' read -r target snap_name profile_manifest browser <<<"${target_spec}"
  if should_install "${target}" "${snap_name}"; then
    if install_snap_browser "${snap_name}" "${profile_manifest}" "${browser}"; then
      installed_any=1
    fi
  fi
done

if [[ "${installed_any}" -eq 0 ]]; then
  log "no Chromium, Brave, Opera, or Vivaldi Snap profile found for ${TARGET_USER}"
fi

log "restart affected Snap browsers after installing or updating these manifests"
