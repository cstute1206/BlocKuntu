#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"

TARGETS="auto"
TARGET_USER="${SUDO_USER:-${USER:-}}"
TARGET_HOME=""
NATIVE_HOST=""
FIREFOX_XPI=""
APPLY_FLATPAK_OVERRIDE=1

usage() {
  cat <<'USAGE'
Usage: scripts/setup-confined-firefox-native-host.sh [options]

Install BlocKuntu Native Messaging support for confined Firefox builds.
For Firefox Flatpak this also installs the systemconfig policy extension that
force-installs and locks the BlocKuntu extension.

Options:
  --targets LIST              Comma-separated list: auto, flatpak, snap, all.
                              Default: auto.
  --native-host PATH          Source blockuntu-native binary to copy into the
                              confined browser's writable app area.
  --firefox-xpi PATH          Source signed Firefox XPI for Flatpak managed
                              policy. Auto-detected when omitted.
  --user USER                 Desktop user whose confined Firefox profiles are
                              configured. Default: current sudo/user context.
  --home DIR                  Home directory for --user, if auto-detection is
                              not correct.
  --no-flatpak-override       Do not run flatpak override for /run/blockuntu.
  -h, --help                  Show this help.

The helper copies the native host into each confined browser profile area
because strict Snap/Flatpak Firefox builds cannot execute the system
/usr/bin or /usr/local/bin host directly.
USAGE
}

log() {
  printf '[blockuntu-confined-firefox] %s\n' "$*"
}

die() {
  printf '[blockuntu-confined-firefox] error: %s\n' "$*" >&2
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

write_manifest() {
  local manifest_path="$1"
  local host_path="$2"
  local description="$3"
  local temp_file

  install_user_dir "$(dirname -- "${manifest_path}")"
  temp_file="$(mktemp)"
  cat >"${temp_file}" <<EOF
{
  "name": "blockuntu_native",
  "description": "${description}",
  "path": "${host_path}",
  "type": "stdio",
  "allowed_extensions": ["blockuntu@example.local", "{a7c3f3c4-6b1e-4c6f-9f2a-8d4e5b7c1a90}"]
}
EOF
  install -m 0644 "${temp_file}" "${manifest_path}"
  own_path_for_target_user "${manifest_path}"
  rm -f "${temp_file}"
}

write_firefox_policy() {
  local policy_path="$1"
  local xpi_path="$2"
  local install_url="file://${xpi_path}"
  local temp_file

  install_user_dir "$(dirname -- "${policy_path}")"
  temp_file="$(mktemp)"
  cat >"${temp_file}" <<EOF
{
  "policies": {
    "BlockAboutConfig": true,
    "BlockAboutProfiles": true,
    "BlockAboutSupport": true,
    "DisableDeveloperTools": true,
    "DisableSafeMode": true,
    "PrivateBrowsingModeAvailability": 0,
    "ExtensionSettings": {
      "{a7c3f3c4-6b1e-4c6f-9f2a-8d4e5b7c1a90}": {
        "installation_mode": "force_installed",
        "install_url": "${install_url}",
        "default_area": "navbar",
        "private_browsing": true
      }
    },
    "Preferences": {
      "extensions.quarantinedDomains.enabled": {
        "Value": false,
        "Status": "locked"
      }
    }
  }
}
EOF
  install -m 0644 "${temp_file}" "${policy_path}"
  own_path_for_target_user "${policy_path}"
  rm -f "${temp_file}"
}

copy_native_host() {
  local destination="$1"

  install_user_dir "$(dirname -- "${destination}")"
  install -m 0755 "${NATIVE_HOST}" "${destination}"
  own_path_for_target_user "${destination}"
}

copy_firefox_xpi() {
  local destination="$1"

  install_user_dir "$(dirname -- "${destination}")"
  install -m 0644 "${FIREFOX_XPI}" "${destination}"
  own_path_for_target_user "${destination}"
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

resolve_firefox_xpi() {
  if [[ -n "${FIREFOX_XPI}" ]]; then
    [[ -f "${FIREFOX_XPI}" ]] || die "Firefox XPI does not exist: ${FIREFOX_XPI}"
    return 0
  fi

  local candidates=(
    "/usr/share/blockuntu/BlocKuntu-Signed.xpi"
    "/usr/local/share/blockuntu/BlocKuntu-Signed.xpi"
    "${REPO_ROOT}/browser-extension-firefox/BlocKuntu-Signed.xpi"
  )

  local candidate
  for candidate in "${candidates[@]}"; do
    if [[ -f "${candidate}" ]]; then
      FIREFOX_XPI="${candidate}"
      return 0
    fi
  done

  die "could not find BlocKuntu-Signed.xpi; pass --firefox-xpi PATH"
}

flatpak_arch() {
  if has_cmd flatpak; then
    flatpak --default-arch 2>/dev/null && return 0
  fi
  uname -m
}

target_xdg_data_home() {
  if [[ "${EUID}" -eq 0 && "${TARGET_USER}" != "${USER:-}" ]]; then
    printf '%s\n' "${TARGET_HOME}/.local/share"
    return 0
  fi

  printf '%s\n' "${XDG_DATA_HOME:-${TARGET_HOME}/.local/share}"
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

flatpak_firefox_available() {
  if [[ -d "${TARGET_HOME}/.var/app/org.mozilla.firefox" ]]; then
    return 0
  fi
  has_cmd flatpak && run_as_target_user flatpak info org.mozilla.firefox >/dev/null 2>&1
}

snap_firefox_available() {
  if [[ -d "${TARGET_HOME}/snap/firefox/common" ]]; then
    return 0
  fi
  has_cmd snap && has_cmd timeout && run_as_target_user timeout 5 snap list firefox >/dev/null 2>&1
}

should_install_flatpak() {
  if target_requested flatpak; then
    return 0
  fi
  [[ "${TARGETS}" == "auto" ]] && flatpak_firefox_available
}

should_install_snap() {
  if target_requested snap; then
    return 0
  fi
  [[ "${TARGETS}" == "auto" ]] && snap_firefox_available
}

install_flatpak_firefox() {
  local app_root="${TARGET_HOME}/.var/app/org.mozilla.firefox"
  local host_path="${app_root}/data/blockuntu/blockuntu-native"
  local xpi_path="${app_root}/data/blockuntu/BlocKuntu-Signed.xpi"
  local manifest_path="${app_root}/.mozilla/native-messaging-hosts/blockuntu_native.json"
  local systemconfig_root="$(target_xdg_data_home)/flatpak/extension/org.mozilla.firefox.systemconfig/$(flatpak_arch)/stable"
  local policy_path="${systemconfig_root}/policies/policies.json"

  [[ -d "${app_root}" ]] || install_user_dir "${app_root}"

  log "installing Firefox Flatpak native host copy"
  copy_native_host "${host_path}"
  copy_firefox_xpi "${xpi_path}"
  write_manifest \
    "${manifest_path}" \
    "${host_path}" \
    "BlocKuntu Firefox Native Messaging bridge for Flatpak Firefox"
  write_firefox_policy "${policy_path}" "${xpi_path}"

  if [[ "${APPLY_FLATPAK_OVERRIDE}" -eq 1 ]]; then
    if has_cmd flatpak; then
      log "allowing org.mozilla.firefox to access /run/blockuntu"
      run_as_target_user flatpak override --user --filesystem=/run/blockuntu org.mozilla.firefox \
        || log "warning: could not apply Flatpak override; run: flatpak override --user --filesystem=/run/blockuntu org.mozilla.firefox"
    else
      log "warning: flatpak command not found; cannot apply /run/blockuntu override"
    fi
  fi

  log "installed ${manifest_path}"
  log "installed ${policy_path}"
}

install_snap_firefox() {
  local app_root="${TARGET_HOME}/snap/firefox/common"
  local host_path="${app_root}/.local/share/blockuntu/blockuntu-native"
  local manifest_path="${app_root}/.mozilla/native-messaging-hosts/blockuntu_native.json"

  [[ -d "${app_root}" ]] || install_user_dir "${app_root}"

  log "installing Firefox Snap native host copy"
  copy_native_host "${host_path}"
  write_manifest \
    "${manifest_path}" \
    "${host_path}" \
    "BlocKuntu Firefox Native Messaging bridge for Snap Firefox"

  log "installed ${manifest_path}"
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
    --firefox-xpi)
      [[ $# -ge 2 ]] || die "--firefox-xpi requires a value"
      FIREFOX_XPI="$2"
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
    --no-flatpak-override)
      APPLY_FLATPAK_OVERRIDE=0
      shift
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
if should_install_flatpak; then
  resolve_firefox_xpi
fi

case "${TARGETS}" in
  auto|all|flatpak|snap|flatpak,snap|snap,flatpak)
    ;;
  *)
    die "unsupported --targets value: ${TARGETS}"
    ;;
esac

log "target user: ${TARGET_USER}"
log "target home: ${TARGET_HOME}"
log "native host source: ${NATIVE_HOST}"
if should_install_flatpak; then
  log "Firefox XPI source: ${FIREFOX_XPI}"
fi

installed_any=0
if should_install_flatpak; then
  install_flatpak_firefox
  installed_any=1
fi
if should_install_snap; then
  install_snap_firefox
  installed_any=1
fi

if [[ "${installed_any}" -eq 0 ]]; then
  log "no Firefox Snap or Flatpak install found for ${TARGET_USER}"
fi

log "restart confined Firefox after installing or updating these manifests"
