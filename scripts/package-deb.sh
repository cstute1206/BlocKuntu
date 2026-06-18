#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"

PACKAGE_NAME="blockuntu"
VERSION="0.1.0-9"
ARCHITECTURE="$(dpkg --print-architecture 2>/dev/null || printf 'amd64')"
BUILD=1
OUTPUT_DIR="${REPO_ROOT}/target/debian"
WORK_DIR=""

usage() {
  cat <<'USAGE'
Usage: scripts/package-deb.sh [options]

Build a full BlocKuntu Debian package. The package installs the daemon,
native host, Tauri GUI, systemd units, Native Messaging manifests, default
config, and extension artifacts. It does not create browser policies at install
time; policy repair is deferred until the first browser-extension heartbeat.

Options:
  --no-build          Use existing release artifacts.
  --version VERSION   Package version, default 0.1.0-9.
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
    npm run tauri -- build --no-bundle
  )
else
  log "skipping builds"
fi

[[ -x focusd/target/release/blockuntud ]] || die "missing focusd/target/release/blockuntud"
[[ -x native-host/target/release/blockuntu-native ]] || die "missing native-host/target/release/blockuntu-native"
[[ -x focus-gui/src-tauri/target/release/blockuntu-gui ]] || die "missing focus-gui/src-tauri/target/release/blockuntu-gui"
[[ -f browser-extension-firefox/BlocKuntu-Signed.xpi ]] || die "missing browser-extension-firefox/BlocKuntu-Signed.xpi"
[[ -f browser-extension-chrome/browser-extension-chrome.crx ]] || die "missing browser-extension-chrome/browser-extension-chrome.crx"

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
install -Dm644 browser-extension-firefox/BlocKuntu-Signed.xpi \
  "${PKG_ROOT}/usr/share/blockuntu/BlocKuntu-Signed.xpi"
install -Dm644 browser-extension-chrome/browser-extension-chrome.crx \
  "${PKG_ROOT}/usr/share/blockuntu/browser-extension-chrome.crx"

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
cat >"${PKG_ROOT}/usr/share/applications/blockuntu.desktop" <<'DESKTOP'
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
DESKTOP
chmod 0644 "${PKG_ROOT}/usr/share/applications/blockuntu.desktop"

install -Dm644 packaging/systemd/blockuntu.socket "${PKG_ROOT}/lib/systemd/system/blockuntu.socket"
install -Dm644 packaging/systemd/blockuntu.service "${PKG_ROOT}/lib/systemd/system/blockuntu.service"
install -Dm644 packaging/systemd/blockuntu-watchdog.service \
  "${PKG_ROOT}/lib/systemd/system/blockuntu-watchdog.service"
install -Dm644 packaging/systemd/blockuntu-hosts.path \
  "${PKG_ROOT}/lib/systemd/system/blockuntu-hosts.path"
install -Dm644 packaging/systemd/blockuntu-hosts.service \
  "${PKG_ROOT}/lib/systemd/system/blockuntu-hosts.service"

sed -i \
  's#ExecStart=/usr/local/bin/blockuntud serve#ExecStart=/usr/bin/blockuntud --extension-xpi /usr/share/blockuntu/BlocKuntu-Signed.xpi --chrome-extension-crx-url file:///usr/share/blockuntu/browser-extension-chrome.crx --defer-browser-policy-repair-until-heartbeat serve#' \
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
 desktop GUI for local website and application blocking. Browser extensions
 are installed manually; managed browser policy is written only after the first
 extension heartbeat confirms the integration works.
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

create_recovery_phrase() {
  recovery_file="/etc/blockuntu/uninstall-recovery.txt"
  if [ -s "${recovery_file}" ]; then
    return 0
  fi

  random_hex="$(od -An -N24 -tx1 /dev/urandom | tr -d ' \n' | tr '[:lower:]' '[:upper:]')"
  chunks="$(printf '%s' "${random_hex}" | sed 's/.\{8\}/&-/g; s/-$//')"
  temp_file="$(mktemp)"
  printf 'BLOCKUNTU-UNINSTALL-RECOVERY-%s\n' "${chunks}" >"${temp_file}"
  install -d -o root -g root -m 0755 /etc/blockuntu
  install -o root -g blockuntu -m 0640 "${temp_file}" "${recovery_file}"
  rm -f "${temp_file}"
}

create_tier1_edit_key() {
  key_file="/etc/blockuntu/tier1-edit-key.txt"
  if [ -s "${key_file}" ]; then
    return 0
  fi

  random_hex="$(od -An -N24 -tx1 /dev/urandom | tr -d ' \n' | tr '[:lower:]' '[:upper:]')"
  chunks="$(printf '%s' "${random_hex}" | sed 's/.\{8\}/&-/g; s/-$//')"
  temp_file="$(mktemp)"
  printf 'BLOCKUNTU-TIER1-EDIT-%s\n' "${chunks}" >"${temp_file}"
  install -d -o root -g root -m 0755 /etc/blockuntu
  install -o root -g blockuntu -m 0640 "${temp_file}" "${key_file}"
  rm -f "${temp_file}"
}

create_recovery_phrase
create_tier1_edit_key

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

System Firefox, Firefox Snap, and Chrome policies are deferred. Install and
enable the BlocKuntu browser extension manually; the daemon writes managed
policy after the first heartbeat.
If you use Firefox Snap or Flatpak, run this as the desktop user and then
restart that Firefox build:
  blockuntu-setup-confined-firefox

Open the GUI once after the first login and store the uninstall phrase shown
in the First Run panel. The Admin uninstall action accepts that phrase or the
system recovery phrase.
Closing the GUI window keeps BlocKuntu available from the tray icon. On vanilla
GNOME, install or enable AppIndicator/KStatusNotifierItem support if the tray
icon is not visible.
A system recovery uninstall phrase is also stored at:
  /etc/blockuntu/uninstall-recovery.txt
The Tier 1 site-list edit key is stored at:
  /etc/blockuntu/tier1-edit-key.txt
MSG
POSTINST

cat >"${DEBIAN_DIR}/prerm" <<'PRERM'
#!/bin/sh
set -e

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
