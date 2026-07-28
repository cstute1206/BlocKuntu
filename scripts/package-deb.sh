#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"

PACKAGE_NAME="blockuntu"
VERSION="0.1.0-18"
ARCHITECTURE="$(dpkg --print-architecture 2>/dev/null || printf 'amd64')"
BUILD=1
OUTPUT_DIR="${REPO_ROOT}/target/debian"
WORK_DIR=""
PACKAGE_VERSION_STAMP="${REPO_ROOT}/target/.blockuntu-package-version"

usage() {
  cat <<'USAGE'
Usage: scripts/package-deb.sh [options]

Build a full BlocKuntu Debian package. The package installs the daemon,
native host, Tauri GUI, systemd units, Native Messaging manifests, default
config, and no browser-extension artifacts. Install Firefox from AMO and
Chrome from the Chrome Web Store. Their policies are written after each
extension's first verified heartbeat, then lock the store-installed extension.

Options:
  --no-build          Use existing release artifacts.
  --version VERSION   Package version, default 0.1.0-18.
  --output-dir DIR    Output directory, default target/debian.
  -h, --help          Show this help.
USAGE
}

log() {
  printf '[blockuntu-deb] %s\n' "$*"
}

die() {
  printf '[blockuntu-deb] error: %s\n' "$*" >&2
  exit 1
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --no-build)
      BUILD=0
      shift
      ;;
    --version)
      [[ $# -ge 2 ]] || die "--version requires a value"
      VERSION="$2"
      shift 2
      ;;
    --output-dir)
      [[ $# -ge 2 ]] || die "--output-dir requires a value"
      OUTPUT_DIR="$2"
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

cd "${REPO_ROOT}"

require_cmd dpkg-deb
require_cmd install
require_cmd sed

if [[ "${BUILD}" -eq 1 ]]; then
  require_cmd cargo
  require_cmd npm

  log "building release daemon"
  cargo build --manifest-path focusd/Cargo.toml --release --locked

  log "building release native host"
  cargo build --manifest-path native-host/Cargo.toml --release --locked

  log "building Tauri GUI"
  (
    cd focus-gui
    npm ci
    export BLOCKUNTU_BUILD_NUMBER="${VERSION}"
    npm run tauri -- build --no-bundle
  )
  install -d "$(dirname -- "${PACKAGE_VERSION_STAMP}")"
  printf '%s\n' "${VERSION}" >"${PACKAGE_VERSION_STAMP}"
else
  log "skipping builds"
  [[ -f "${PACKAGE_VERSION_STAMP}" ]] || die "missing package-version stamp; run without --no-build first"
  [[ "$(tr -d '[:space:]' <"${PACKAGE_VERSION_STAMP}")" == "${VERSION}" ]] || \
    die "release artifacts were built for a different package version; run without --no-build"
fi

[[ -x focusd/target/release/blockuntud ]] || die "missing focusd/target/release/blockuntud"
[[ -x native-host/target/release/blockuntu-native ]] || die "missing native-host/target/release/blockuntu-native"
[[ -x focus-gui/src-tauri/target/release/blockuntu-gui ]] || die "missing focus-gui/src-tauri/target/release/blockuntu-gui"

mkdir -p "${OUTPUT_DIR}"
WORK_DIR="$(mktemp -d "${OUTPUT_DIR}/blockuntu-deb.XXXXXX")"
trap 'rm -rf "${WORK_DIR}"' EXIT

PKG_ROOT="${WORK_DIR}/${PACKAGE_NAME}_${VERSION}_${ARCHITECTURE}"
DEBIAN_DIR="${PKG_ROOT}/DEBIAN"
mkdir -p "${DEBIAN_DIR}"

log "creating package tree ${PKG_ROOT}"

install -Dm755 focusd/target/release/blockuntud "${PKG_ROOT}/usr/bin/blockuntud"
install -Dm755 native-host/target/release/blockuntu-native "${PKG_ROOT}/usr/bin/blockuntu-native"
install -Dm755 focus-gui/src-tauri/target/release/blockuntu-gui "${PKG_ROOT}/usr/bin/blockuntu-gui"
install -Dm755 scripts/setup-confined-firefox-native-host.sh \
  "${PKG_ROOT}/usr/lib/blockuntu/setup-confined-firefox-native-host.sh"
cat >"${PKG_ROOT}/usr/bin/blockuntu-setup-confined-firefox" <<'SH'
#!/bin/sh
exec /usr/lib/blockuntu/setup-confined-firefox-native-host.sh "$@"
SH
chmod 0755 "${PKG_ROOT}/usr/bin/blockuntu-setup-confined-firefox"

install -Dm644 packaging/deb/blockuntu.toml "${PKG_ROOT}/etc/blockuntu/config.toml"

install -Dm644 focus-gui/src-tauri/icons/32x32.png \
  "${PKG_ROOT}/usr/share/icons/hicolor/32x32/apps/blockuntu.png"
install -Dm644 focus-gui/src-tauri/icons/32x32.png \
  "${PKG_ROOT}/usr/share/icons/hicolor/32x32/apps/blockuntu-gui.png"
install -Dm644 focus-gui/src-tauri/icons/64x64.png \
  "${PKG_ROOT}/usr/share/icons/hicolor/64x64/apps/blockuntu.png"
install -Dm644 focus-gui/src-tauri/icons/64x64.png \
  "${PKG_ROOT}/usr/share/icons/hicolor/64x64/apps/blockuntu-gui.png"
install -Dm644 focus-gui/src-tauri/icons/128x128.png \
  "${PKG_ROOT}/usr/share/icons/hicolor/128x128/apps/blockuntu.png"
install -Dm644 focus-gui/src-tauri/icons/128x128.png \
  "${PKG_ROOT}/usr/share/icons/hicolor/128x128/apps/blockuntu-gui.png"

install -d "${PKG_ROOT}/usr/share/applications"
cat >"${PKG_ROOT}/usr/share/applications/local.blockuntu.gui.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=BlocKuntu
Comment=Linux focus blocker frontend
Exec=/usr/bin/blockuntu-gui
Icon=blockuntu-gui
StartupWMClass=blockuntu-gui
StartupNotify=true
Terminal=false
Categories=Utility;
X-GNOME-UsesNotifications=true
DESKTOP
chmod 0644 "${PKG_ROOT}/usr/share/applications/local.blockuntu.gui.desktop"

install -Dm644 packaging/systemd/blockuntu.socket "${PKG_ROOT}/lib/systemd/system/blockuntu.socket"
install -Dm644 packaging/systemd/blockuntu.service "${PKG_ROOT}/lib/systemd/system/blockuntu.service"
install -Dm644 packaging/systemd/blockuntu-watchdog.service \
  "${PKG_ROOT}/lib/systemd/system/blockuntu-watchdog.service"
install -Dm644 packaging/systemd/blockuntu-hosts.path \
  "${PKG_ROOT}/lib/systemd/system/blockuntu-hosts.path"
install -Dm644 packaging/systemd/blockuntu-hosts.service \
  "${PKG_ROOT}/lib/systemd/system/blockuntu-hosts.service"

sed -i \
  's#ExecStart=/usr/local/bin/blockuntud serve#ExecStart=/usr/bin/blockuntud --defer-browser-policy-repair-until-heartbeat serve#' \
  "${PKG_ROOT}/lib/systemd/system/blockuntu.service"
sed -i 's#ExecStart=/usr/local/bin/blockuntud repair-hosts#ExecStart=/usr/bin/blockuntud repair-hosts#' \
  "${PKG_ROOT}/lib/systemd/system/blockuntu-hosts.service"

install -Dm644 packaging/native-messaging/blockuntu_native.json \
  "${PKG_ROOT}/usr/lib/mozilla/native-messaging-hosts/blockuntu_native.json"
install -Dm644 packaging/native-messaging/blockuntu_native.chrome.json \
  "${PKG_ROOT}/etc/opt/chrome/native-messaging-hosts/blockuntu_native.json"
install -Dm644 packaging/native-messaging/blockuntu_native.chrome.json \
  "${PKG_ROOT}/etc/chromium/native-messaging-hosts/blockuntu_native.json"
sed -i 's#/usr/local/bin/blockuntu-native#/usr/bin/blockuntu-native#g' \
  "${PKG_ROOT}/usr/lib/mozilla/native-messaging-hosts/blockuntu_native.json" \
  "${PKG_ROOT}/etc/opt/chrome/native-messaging-hosts/blockuntu_native.json" \
  "${PKG_ROOT}/etc/chromium/native-messaging-hosts/blockuntu_native.json"

cat >"${DEBIAN_DIR}/control" <<CONTROL
Package: ${PACKAGE_NAME}
Version: ${VERSION}
Section: utils
Priority: optional
Architecture: ${ARCHITECTURE}
Depends: systemd, passwd, e2fsprogs, pkexec, libwebkit2gtk-4.1-0, libgtk-3-0, libayatana-appindicator3-1, librsvg2-2
Recommends: wmctrl
Maintainer: BlocKuntu <local@blockuntu.invalid>
Description: Local Linux focus blocker
 BlocKuntu installs a privileged daemon, Native Messaging bridge, and Tauri
 desktop GUI for local website and application blocking. Firefox and Chrome
 browser policies are deferred until each store-installed extension sends its
 first verified heartbeat, then lock that extension against removal.
CONTROL

cat >"${DEBIAN_DIR}/conffiles" <<'CONFFILES'
/etc/blockuntu/config.toml
/etc/opt/chrome/native-messaging-hosts/blockuntu_native.json
/etc/chromium/native-messaging-hosts/blockuntu_native.json
CONFFILES

cat >"${DEBIAN_DIR}/postinst" <<'POSTINST'
#!/bin/sh
set -e

if ! getent group blockuntu >/dev/null 2>&1; then
  groupadd --system blockuntu
fi

create_installation_serial() {
  serial_file="/etc/blockuntu/installation-id"
  legacy_serial_file="/var/lib/blockuntu/installation-id"
  if [ -s "${serial_file}" ] && \
    grep -Eq '^BKI-[0-9A-F]{8}-[0-9A-F]{8}-[0-9A-F]{8}-[0-9A-F]{8}$' "${serial_file}"; then
    rm -f "${legacy_serial_file}"
    return 0
  fi

  install -d -o root -g root -m 0755 /etc/blockuntu
  if [ -s "${legacy_serial_file}" ] && \
    grep -Eq '^BKI-[0-9A-F]{8}-[0-9A-F]{8}-[0-9A-F]{8}-[0-9A-F]{8}$' "${legacy_serial_file}"; then
    install -o root -g root -m 0644 "${legacy_serial_file}" "${serial_file}"
    rm -f "${legacy_serial_file}"
    return 0
  fi

  random_hex="$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n' | tr '[:lower:]' '[:upper:]')"
  chunks="$(printf '%s' "${random_hex}" | sed 's/.\{8\}/&-/g; s/-$//')"
  temp_file="$(mktemp)"
  printf 'BKI-%s\n' "${chunks}" >"${temp_file}"
  install -o root -g root -m 0644 "${temp_file}" "${serial_file}"
  rm -f "${temp_file}"
  rm -f "${legacy_serial_file}"
}

create_recovery_credential() {
  credential_file="$1"
  prefix="$2"
  if [ -s "${credential_file}" ]; then
    return 0
  fi
  random_hex="$(od -An -N24 -tx1 /dev/urandom | tr -d ' \n' | tr '[:lower:]' '[:upper:]')"
  chunks="$(printf '%s' "${random_hex}" | sed 's/.\{8\}/&-/g; s/-$//')"
  temp_file="$(mktemp)"
  printf '%s-%s\n' "${prefix}" "${chunks}" >"${temp_file}"
  install -d -o root -g root -m 0755 /etc/blockuntu
  install -o root -g blockuntu -m 0640 "${temp_file}" "${credential_file}"
  rm -f "${temp_file}"
}

create_installation_serial
if [ ! -e /var/lib/blockuntu/recovery-credentials-hidden ]; then
  create_recovery_credential /etc/blockuntu/uninstall-recovery.txt BLOCKUNTU-UNINSTALL-RECOVERY
  create_recovery_credential /etc/blockuntu/tier1-edit-key.txt BLOCKUNTU-TIER1-EDIT
fi

if command -v systemctl >/dev/null 2>&1; then
  systemctl daemon-reload || true
  systemctl enable --now blockuntu.socket blockuntu.service blockuntu-watchdog.service blockuntu-hosts.path || true
fi

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database /usr/share/applications >/dev/null 2>&1 || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -q /usr/share/icons/hicolor >/dev/null 2>&1 || true
fi

cat <<'MSG'
BlocKuntu installed.

Add the desktop user to the socket group, then log out and back in:
  sudo usermod -aG blockuntu "$USER"

Install and enable the BlocKuntu Firefox extension from AMO and the BlocKuntu
Chrome extension from the Chrome Web Store. After each extension's first
verified heartbeat, BlocKuntu writes its managed policy and locks that
store-installed extension against removal.
If you use Firefox Snap or Flatpak, BlocKuntu configures its per-user browser
integration automatically when the GUI starts. Its Flatpak policy is written
after the first verified Firefox heartbeat.

Open the GUI once after the first login and store the recovery credentials shown
in the welcome modal. You can hide and remove them permanently from Settings.
Closing the GUI window keeps BlocKuntu available from the tray icon. On vanilla
GNOME, install or enable AppIndicator/KStatusNotifierItem support if the tray
icon is not visible.
MSG
POSTINST

cat >"${DEBIAN_DIR}/prerm" <<'PRERM'
#!/bin/sh
set -e
unset TZ

reject_package_uninstall() {
  cat >&2 <<'MSG'
BlocKuntu refuses direct package-manager removal.

Open BlocKuntu Settings and use its uninstall action instead. That action is
available only on Sunday between 20:00 and 23:59 local time and prepares this
package removal safely before it invokes dpkg.
MSG
  exit 1
}

authorize_settings_uninstall() {
  lease_path="/run/blockuntu/package-removal-lease"
  lease_token="${BLOCKUNTU_PACKAGE_REMOVAL_LEASE:-}"

  [ -n "${lease_token}" ] || reject_package_uninstall
  [ -r "${lease_path}" ] || reject_package_uninstall

  IFS=' ' read -r expected_token expires_at <"${lease_path}" || reject_package_uninstall
  case "${expires_at}" in
    ''|*[!0-9]*) reject_package_uninstall ;;
  esac
  now="$(/bin/date -u +%s)"
  [ "${now}" -le "${expires_at}" ] || reject_package_uninstall
  [ "${lease_token}" = "${expected_token}" ] || reject_package_uninstall

  rm -f "${lease_path}"
}

case "$1" in
  remove)
    authorize_settings_uninstall
    ;;
esac

policy_recovery="/etc/blockuntu/policy-recovery.toml"
if [ -e "${policy_recovery}" ] && command -v chattr >/dev/null 2>&1; then
  chattr -i "${policy_recovery}" >/dev/null 2>&1 || true
fi

remove_empty_dir() {
  rmdir "$1" >/dev/null 2>&1 || true
}

remove_hosts_block() {
  hosts_path="/etc/hosts"
  if [ ! -f "${hosts_path}" ]; then
    return 0
  fi
  if ! grep -q "BEGIN BLOCKUNTU MANAGED" "${hosts_path}" 2>/dev/null; then
    return 0
  fi

  if command -v chattr >/dev/null 2>&1; then
    chattr -i "${hosts_path}" >/dev/null 2>&1 || true
  fi

  temp_hosts="$(mktemp)"
  awk '
    /BEGIN BLOCKUNTU MANAGED/ { skip = 1; next }
    /END BLOCKUNTU MANAGED/ { skip = 0; next }
    skip != 1 { print }
  ' "${hosts_path}" >"${temp_hosts}"

  install -m 0644 "${temp_hosts}" "${hosts_path}"
  rm -f "${temp_hosts}"
}

remove_browser_policies() {
  firefox_policy="/etc/firefox/policies/policies.json"
  chrome_policy="/etc/opt/chrome/policies/managed/blockuntu.json"
  chrome_update_manifest="/usr/local/share/blockuntu/chrome-extension-updates.xml"

  if [ -f "${firefox_policy}" ] && grep -qi "blockuntu" "${firefox_policy}" 2>/dev/null; then
    rm -f "${firefox_policy}"
    remove_empty_dir "/etc/firefox/policies"
    remove_empty_dir "/etc/firefox"
  fi

  rm -f "${chrome_policy}"
  rm -f "${chrome_update_manifest}"
  remove_empty_dir "/etc/opt/chrome/policies/managed"
  remove_empty_dir "/etc/opt/chrome/policies"
  remove_empty_dir "/usr/local/share/blockuntu"
}

if command -v systemctl >/dev/null 2>&1; then
  mkdir -p /run/systemd/system/blockuntu.service.d \
    /run/systemd/system/blockuntu-watchdog.service.d
  cat >/run/systemd/system/blockuntu.service.d/99-package-remove.conf <<'OVERRIDE'
[Unit]
RefuseManualStop=no

[Service]
Restart=no
OVERRIDE
  cat >/run/systemd/system/blockuntu-watchdog.service.d/99-package-remove.conf <<'OVERRIDE'
[Unit]
RefuseManualStop=no

[Service]
Restart=no
OVERRIDE
  systemctl daemon-reload >/dev/null 2>&1 || true
  systemctl stop blockuntu-hosts.path blockuntu-hosts.service blockuntu-watchdog.service blockuntu.service blockuntu.socket >/dev/null 2>&1 || true
  systemctl disable blockuntu-hosts.path blockuntu-hosts.service blockuntu-watchdog.service blockuntu.service blockuntu.socket >/dev/null 2>&1 || true
fi

remove_hosts_block
remove_browser_policies
rm -rf /run/blockuntu
PRERM

cat >"${DEBIAN_DIR}/postrm" <<'POSTRM'
#!/bin/sh
set -e

if command -v systemctl >/dev/null 2>&1; then
  rm -f /run/systemd/system/blockuntu.service.d/99-package-remove.conf
  rm -f /run/systemd/system/blockuntu-watchdog.service.d/99-package-remove.conf
  rmdir /run/systemd/system/blockuntu.service.d >/dev/null 2>&1 || true
  rmdir /run/systemd/system/blockuntu-watchdog.service.d >/dev/null 2>&1 || true
  systemctl daemon-reload >/dev/null 2>&1 || true
  systemctl reset-failed blockuntu-hosts.path blockuntu-hosts.service blockuntu-watchdog.service blockuntu.service blockuntu.socket >/dev/null 2>&1 || true
fi

case "$1" in
  remove|purge)
    rm -f /etc/blockuntu/installation-id
    rm -f /var/lib/blockuntu/installation-id
    ;;
esac

if [ "$1" = "purge" ]; then
  rm -rf /etc/blockuntu /var/lib/blockuntu /run/blockuntu
fi

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database /usr/share/applications >/dev/null 2>&1 || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -q /usr/share/icons/hicolor >/dev/null 2>&1 || true
fi
POSTRM

chmod 0755 "${DEBIAN_DIR}/postinst" "${DEBIAN_DIR}/prerm" "${DEBIAN_DIR}/postrm"

PACKAGE_PATH="${OUTPUT_DIR}/${PACKAGE_NAME}_${VERSION}_${ARCHITECTURE}.deb"
log "building ${PACKAGE_PATH}"
dpkg-deb --build --root-owner-group "${PKG_ROOT}" "${PACKAGE_PATH}"

log "package created: ${PACKAGE_PATH}"
