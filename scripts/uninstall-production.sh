#!/usr/bin/env bash
set -euo pipefail

PURGE_DATA=0
REMOVE_BROWSER_POLICIES=0
REMOVE_GROUP=0
ASSUME_YES=0

usage() {
  cat <<'USAGE'
Usage: scripts/uninstall-production.sh [options]

Uninstall the production-style BlocKuntu system installation.

By default this removes installed BlocKuntu binaries, systemd units, Native
Messaging manifests, GUI launcher/icons, runtime files, and the BlocKuntu
managed hosts block. It preserves /etc/blockuntu and /var/lib/blockuntu.

Options:
  --purge-data               Also remove /etc/blockuntu and /var/lib/blockuntu.
  --remove-browser-policies  Remove BlocKuntu browser policy files if present.
                             Use only when those files are BlocKuntu-owned.
  --remove-group             Remove the blockuntu system group after uninstall.
  -y, --yes                  Do not ask for confirmation.
  -h, --help                 Show this help.
USAGE
}

log() {
  printf '[blockuntu-uninstall] %s\n' "$*"
}

die() {
  printf '[blockuntu-uninstall] error: %s\n' "$*" >&2
  exit 1
}

has_cmd() {
  command -v "$1" >/dev/null 2>&1
}

run_sudo() {
  if [[ "$#" -eq 0 ]]; then
    return 0
  fi
  sudo "$@"
}

systemctl_if_present() {
  local action="$1"
  shift
  local units=("$@")
  if ! has_cmd systemctl; then
    log "systemctl is not available; skipping ${action} for ${units[*]}"
    return 0
  fi
  run_sudo systemctl "${action}" "${units[@]}" >/dev/null 2>&1 || true
}

allow_systemd_stop() {
  if ! has_cmd systemctl; then
    return 0
  fi

  local temp_override
  temp_override="$(mktemp)"
  cat >"${temp_override}" <<'OVERRIDE'
[Unit]
RefuseManualStop=no

[Service]
Restart=no
OVERRIDE
  run_sudo install -Dm644 "${temp_override}" \
    /run/systemd/system/blockuntu.service.d/99-uninstall.conf
  run_sudo install -Dm644 "${temp_override}" \
    /run/systemd/system/blockuntu-watchdog.service.d/99-uninstall.conf
  rm -f "${temp_override}"
  run_sudo systemctl daemon-reload >/dev/null 2>&1 || true
}

remove_path() {
  local path="$1"
  if [[ -e "${path}" || -L "${path}" ]]; then
    log "removing ${path}"
    run_sudo rm -rf -- "${path}"
  fi
}

remove_empty_dir() {
  local path="$1"
  if [[ -d "${path}" ]]; then
    run_sudo rmdir --ignore-fail-on-non-empty "${path}" >/dev/null 2>&1 || true
  fi
}

remove_hosts_block() {
  local hosts_path="/etc/hosts"
  if [[ ! -f "${hosts_path}" ]]; then
    return 0
  fi
  if ! grep -q "BEGIN BLOCKUNTU MANAGED" "${hosts_path}" 2>/dev/null; then
    return 0
  fi

  log "removing BlocKuntu managed block from ${hosts_path}"
  if has_cmd chattr; then
    run_sudo chattr -i "${hosts_path}" >/dev/null 2>&1 || true
  fi

  local temp_hosts
  temp_hosts="$(mktemp)"
  awk '
    /BEGIN BLOCKUNTU MANAGED/ { skip = 1; next }
    /END BLOCKUNTU MANAGED/ { skip = 0; next }
    skip != 1 { print }
  ' "${hosts_path}" >"${temp_hosts}"

  run_sudo install -m 0644 "${temp_hosts}" "${hosts_path}"
  rm -f "${temp_hosts}"
}

remove_browser_policies() {
  if [[ "${REMOVE_BROWSER_POLICIES}" -ne 1 ]]; then
    return 0
  fi

  local firefox_policy="/etc/firefox/policies/policies.json"
  local chrome_policy="/etc/opt/chrome/policies/managed/blockuntu.json"
  local chrome_update_manifest="/usr/local/share/blockuntu/chrome-extension-updates.xml"

  if [[ -f "${firefox_policy}" ]]; then
    if grep -q "blockuntu" "${firefox_policy}" 2>/dev/null; then
      remove_path "${firefox_policy}"
      remove_empty_dir "/etc/firefox/policies"
      remove_empty_dir "/etc/firefox"
    else
      log "leaving ${firefox_policy}; it does not look BlocKuntu-owned"
    fi
  fi

  remove_path "${chrome_policy}"
  remove_path "${chrome_update_manifest}"
  remove_empty_dir "/etc/opt/chrome/policies/managed"
  remove_empty_dir "/etc/opt/chrome/policies"
}

remove_confined_firefox_native_hosts() {
  local flatpak_manifest="${HOME}/.var/app/org.mozilla.firefox/.mozilla/native-messaging-hosts/blockuntu_native.json"
  local flatpak_host="${HOME}/.var/app/org.mozilla.firefox/data/blockuntu/blockuntu-native"
  local flatpak_xpi="${HOME}/.var/app/org.mozilla.firefox/data/blockuntu/BlocKuntu-Signed.xpi"
  local flatpak_systemconfig="${XDG_DATA_HOME:-${HOME}/.local/share}/flatpak/extension/org.mozilla.firefox.systemconfig"
  local snap_manifest="${HOME}/snap/firefox/common/.mozilla/native-messaging-hosts/blockuntu_native.json"
  local snap_host="${HOME}/snap/firefox/common/.local/share/blockuntu/blockuntu-native"

  remove_path "${flatpak_manifest}"
  remove_path "${flatpak_host}"
  remove_path "${flatpak_xpi}"
  remove_empty_dir "${HOME}/.var/app/org.mozilla.firefox/.mozilla/native-messaging-hosts"
  remove_empty_dir "${HOME}/.var/app/org.mozilla.firefox/data/blockuntu"

  if [[ -d "${flatpak_systemconfig}" ]]; then
    while IFS= read -r -d '' policy_path; do
      if grep -qi "blockuntu" "${policy_path}" 2>/dev/null; then
        remove_path "${policy_path}"
        remove_empty_dir "$(dirname -- "${policy_path}")"
      fi
    done < <(find "${flatpak_systemconfig}" -path '*/policies/policies.json' -type f -print0)
  fi

  remove_path "${snap_manifest}"
  remove_path "${snap_host}"
  remove_empty_dir "${HOME}/snap/firefox/common/.mozilla/native-messaging-hosts"
  remove_empty_dir "${HOME}/snap/firefox/common/.local/share/blockuntu"

  if has_cmd flatpak; then
    flatpak override --user --nofilesystem=/run/blockuntu org.mozilla.firefox >/dev/null 2>&1 || true
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --purge-data)
      PURGE_DATA=1
      shift
      ;;
    --remove-browser-policies)
      REMOVE_BROWSER_POLICIES=1
      shift
      ;;
    --remove-group)
      REMOVE_GROUP=1
      shift
      ;;
    -y|--yes)
      ASSUME_YES=1
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

[[ "${EUID}" -ne 0 ]] || die "run this script as a normal user; it will call sudo for privileged uninstall steps"
has_cmd sudo || die "sudo is required"

if [[ "${ASSUME_YES}" -ne 1 ]]; then
  cat <<CONFIRM
This will uninstall the production-style BlocKuntu installation.

Removed by default:
  - blockuntu systemd units and drop-ins
  - /usr/local/bin/blockuntud
  - /usr/local/bin/blockuntu-native
  - /usr/local/bin/blockuntu-gui
  - system Native Messaging manifests
  - current-user Firefox Snap/Flatpak Native Messaging files
  - current-user Firefox Flatpak copied XPI and systemconfig policy
  - GUI desktop launcher/icons
  - /run/blockuntu
  - BlocKuntu managed block in /etc/hosts

Preserved by default:
  - /etc/blockuntu
  - /var/lib/blockuntu
  - browser policy files unless --remove-browser-policies is used
  - blockuntu group unless --remove-group is used
CONFIRM
  read -r -p "Continue? [y/N] " answer
  case "${answer}" in
    y|Y|yes|YES)
      ;;
    *)
      log "aborted"
      exit 0
      ;;
  esac
fi

log "stopping and disabling systemd units"
allow_systemd_stop
systemctl_if_present stop \
  blockuntu-hosts.path \
  blockuntu-hosts.service \
  blockuntu-watchdog.service \
  blockuntu.service \
  blockuntu.socket
systemctl_if_present disable \
  blockuntu-hosts.path \
  blockuntu-hosts.service \
  blockuntu-watchdog.service \
  blockuntu.service \
  blockuntu.socket

remove_hosts_block

log "removing systemd unit files"
remove_path "/etc/systemd/system/blockuntu.socket"
remove_path "/etc/systemd/system/blockuntu.service"
remove_path "/etc/systemd/system/blockuntu-watchdog.service"
remove_path "/etc/systemd/system/blockuntu-hosts.path"
remove_path "/etc/systemd/system/blockuntu-hosts.service"
remove_path "/etc/systemd/system/blockuntu.service.d/90-manual-browser-extensions.conf"
remove_path "/etc/systemd/system/blockuntu.service.d/90-defer-browser-policy.conf"
remove_empty_dir "/etc/systemd/system/blockuntu.service.d"
remove_path "/run/systemd/system/blockuntu.service.d/99-uninstall.conf"
remove_path "/run/systemd/system/blockuntu-watchdog.service.d/99-uninstall.conf"
remove_empty_dir "/run/systemd/system/blockuntu.service.d"
remove_empty_dir "/run/systemd/system/blockuntu-watchdog.service.d"

if has_cmd systemctl; then
  log "reloading systemd"
  run_sudo systemctl daemon-reload >/dev/null 2>&1 || true
  run_sudo systemctl reset-failed \
    blockuntu-hosts.path \
    blockuntu-hosts.service \
    blockuntu-watchdog.service \
    blockuntu.service \
    blockuntu.socket >/dev/null 2>&1 || true
fi

log "removing installed binaries and GUI files"
remove_path "/usr/local/bin/blockuntud"
remove_path "/usr/local/bin/blockuntu-native"
remove_path "/usr/local/bin/blockuntu-gui"
remove_path "/usr/share/applications/blockuntu.desktop"
remove_path "/usr/share/icons/hicolor/32x32/apps/blockuntu.png"
remove_path "/usr/share/icons/hicolor/64x64/apps/blockuntu.png"
remove_path "/usr/share/icons/hicolor/128x128/apps/blockuntu.png"
remove_path "/usr/local/share/blockuntu/BlocKuntu-Signed.xpi"
remove_path "/usr/local/share/blockuntu/browser-extension-chrome.crx"
remove_empty_dir "/usr/local/share/blockuntu"

log "removing Native Messaging manifests"
remove_path "/usr/lib/mozilla/native-messaging-hosts/blockuntu_native.json"
remove_path "/etc/opt/chrome/native-messaging-hosts/blockuntu_native.json"
remove_path "/etc/chromium/native-messaging-hosts/blockuntu_native.json"
remove_empty_dir "/usr/lib/mozilla/native-messaging-hosts"
remove_empty_dir "/etc/opt/chrome/native-messaging-hosts"
remove_empty_dir "/etc/chromium/native-messaging-hosts"
remove_confined_firefox_native_hosts

remove_browser_policies

log "removing runtime files"
remove_path "/run/blockuntu"

if [[ "${PURGE_DATA}" -eq 1 ]]; then
  log "purging config/data"
  remove_path "/etc/blockuntu"
  remove_path "/var/lib/blockuntu"
fi

if [[ "${REMOVE_GROUP}" -eq 1 ]]; then
  if getent group blockuntu >/dev/null 2>&1; then
    log "removing blockuntu group"
    run_sudo groupdel blockuntu >/dev/null 2>&1 || log "could not remove blockuntu group; it may still be in use"
  fi
fi

if has_cmd update-desktop-database; then
  run_sudo update-desktop-database /usr/share/applications >/dev/null 2>&1 || true
fi
if has_cmd gtk-update-icon-cache; then
  run_sudo gtk-update-icon-cache -q /usr/share/icons/hicolor >/dev/null 2>&1 || true
fi

cat <<SUMMARY

BlocKuntu uninstall complete.

Preserved unless you passed purge flags:
  /etc/blockuntu
  /var/lib/blockuntu
  blockuntu group
  browser policy files

Reboot or log out/in if your desktop session still has the old blockuntu group.
SUMMARY
