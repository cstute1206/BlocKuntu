#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"

BUILD=1
START_SERVICES=1
INSTALL_GUI=1
INSTALL_PREREQS=1
INSTALL_CONFINED_FIREFOX=1
OVERWRITE_CONFIG=0
TARGET_USER="${SUDO_USER:-${USER:-}}"

usage() {
  cat <<'USAGE'
Usage: scripts/install-production.sh [options]

Build and install BlocKuntu as a production-style system service.

Options:
  --no-build          Install existing build artifacts instead of building first.
  --no-start          Install files but do not enable/start systemd units.
  --skip-prereqs     Do not install missing build/runtime prerequisites.
  --skip-gui          Do not build or install the Tauri GUI.
  --skip-confined-firefox
                      Do not configure Snap/Flatpak Firefox integration.
  --overwrite-config  Replace /etc/blockuntu/config.toml with the minimal production config.
  --user USER         Desktop user to add to the blockuntu socket group.
  -h, --help          Show this help.

Browser extensions are not installed or force-installed for system Firefox,
Firefox Snap, or Chrome by this script. It installs Native Messaging manifests
and starts blockuntud with --defer-browser-policy-repair-until-heartbeat, so
the user must install and enable those browser extensions manually before
managed policy is written. Firefox Flatpak is configured with a per-user
systemconfig policy because it cannot read the host /etc/firefox policy path.
USAGE
}

log() {
  printf '[blockuntu-install] %s\n' "$*"
}

die() {
  printf '[blockuntu-install] error: %s\n' "$*" >&2
  exit 1
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

has_cmd() {
  command -v "$1" >/dev/null 2>&1
}

snap_firefox_installed() {
  if ! has_cmd snap; then
    return 1
  fi
  if has_cmd timeout; then
    timeout 5 snap list firefox >/dev/null 2>&1
  else
    snap list firefox >/dev/null 2>&1
  fi
}

flatpak_firefox_installed() {
  has_cmd flatpak && flatpak info org.mozilla.firefox >/dev/null 2>&1
}

pkg_config_has() {
  has_cmd pkg-config && pkg-config --exists "$1" >/dev/null 2>&1
}

install_packages() {
  local manager="$1"
  shift
  [[ $# -gt 0 ]] || return 0

  case "${manager}" in
    apt)
      sudo apt-get update
      sudo DEBIAN_FRONTEND=noninteractive apt-get install -y "$@"
      ;;
    dnf)
      sudo dnf install -y "$@"
      ;;
    pacman)
      sudo pacman -Syu --needed --noconfirm "$@"
      ;;
    zypper)
      sudo zypper --non-interactive install --no-recommends "$@"
      ;;
    *)
      die "unsupported package manager: ${manager}"
      ;;
  esac
}

detect_package_manager() {
  if has_cmd apt-get; then
    printf 'apt\n'
  elif has_cmd dnf; then
    printf 'dnf\n'
  elif has_cmd pacman; then
    printf 'pacman\n'
  elif has_cmd zypper; then
    printf 'zypper\n'
  else
    return 1
  fi
}

base_packages_for() {
  case "$1" in
    apt)
      printf '%s\n' curl e2fsprogs file git jq pkg-config socat systemd unzip wget wmctrl zip
      ;;
    dnf)
      printf '%s\n' curl e2fsprogs file git jq pkgconf-pkg-config socat systemd unzip wget wmctrl zip
      ;;
    pacman)
      printf '%s\n' curl e2fsprogs file git jq pkgconf socat systemd unzip wget wmctrl zip
      ;;
    zypper)
      printf '%s\n' curl e2fsprogs file git jq pkg-config socat systemd unzip wget wmctrl zip
      ;;
  esac
}

build_packages_for() {
  case "$1" in
    apt)
      printf '%s\n' build-essential cargo libssl-dev nodejs npm rustc
      ;;
    dnf)
      printf '%s\n' cargo gcc gcc-c++ make nodejs npm openssl-devel rust
      ;;
    pacman)
      printf '%s\n' base-devel nodejs npm openssl rust
      ;;
    zypper)
      printf '%s\n' cargo gcc gcc-c++ libopenssl-devel make nodejs npm rust
      ;;
  esac
}

gui_packages_for() {
  case "$1" in
    apt)
      printf '%s\n' libayatana-appindicator3-dev librsvg2-dev libwebkit2gtk-4.1-dev libxdo-dev
      ;;
    dnf)
      printf '%s\n' libappindicator-gtk3-devel librsvg2-devel libxdo-devel webkit2gtk4.1-devel
      ;;
    pacman)
      printf '%s\n' appmenu-gtk-module libappindicator-gtk3 libxdo librsvg webkit2gtk-4.1 xdotool
      ;;
    zypper)
      printf '%s\n' libappindicator3-1 librsvg-devel webkit2gtk3-devel
      ;;
  esac
}

prerequisites_satisfied() {
  local missing=0

  for cmd in install sudo; do
    if ! has_cmd "${cmd}"; then
      log "missing command: ${cmd}"
      missing=1
    fi
  done

  if [[ "${BUILD}" -eq 1 ]]; then
    for cmd in cargo rustc; do
      if ! has_cmd "${cmd}"; then
        log "missing build command: ${cmd}"
        missing=1
      fi
    done
  fi

  if [[ "${BUILD}" -eq 1 && "${INSTALL_GUI}" -eq 1 ]]; then
    for cmd in node npm; do
      if ! has_cmd "${cmd}"; then
        log "missing GUI build command: ${cmd}"
        missing=1
      fi
    done
  fi

  if [[ "${INSTALL_GUI}" -eq 1 ]]; then
    if ! pkg_config_has webkit2gtk-4.1; then
      log "missing Tauri WebKitGTK package"
      missing=1
    fi
  fi

  for cmd in systemctl systemd-analyze jq socat wmctrl zip unzip; do
    if ! has_cmd "${cmd}"; then
      log "missing runtime/helper command: ${cmd}"
      missing=1
    fi
  done

  return "${missing}"
}

install_prerequisites() {
  if prerequisites_satisfied; then
    log "prerequisites already look complete"
    return 0
  fi

  local manager
  manager="$(detect_package_manager)" || die "missing prerequisites, but no supported package manager was found; supported: apt-get, dnf, pacman, zypper"
  log "installing missing prerequisites with ${manager}"

  local packages=()
  while IFS= read -r package; do
    packages+=("${package}")
  done < <(base_packages_for "${manager}")

  if [[ "${BUILD}" -eq 1 ]]; then
    while IFS= read -r package; do
      packages+=("${package}")
    done < <(build_packages_for "${manager}")
  fi

  if [[ "${INSTALL_GUI}" -eq 1 ]]; then
    while IFS= read -r package; do
      packages+=("${package}")
    done < <(gui_packages_for "${manager}")
  fi

  install_packages "${manager}" "${packages[@]}"

  if ! prerequisites_satisfied; then
    die "some prerequisites are still missing after package installation"
  fi
}

warn_browser_prerequisites() {
  if snap_firefox_installed; then
    log "Firefox Snap detected; confined Firefox native-host setup will be installed for ${TARGET_USER}"
  fi
  if flatpak_firefox_installed; then
    log "Firefox Flatpak detected; confined Firefox native-host and policy setup will be installed for ${TARGET_USER}"
  fi
  if ! has_cmd firefox; then
    log "warning: firefox was not found on PATH; install a system Firefox package before using the Firefox extension"
  fi
  if ! has_cmd google-chrome && ! has_cmd google-chrome-stable && ! has_cmd chromium && ! has_cmd chromium-browser; then
    log "warning: no Chrome/Chromium binary was found on PATH; ignore this if you only use Firefox"
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --no-build)
      BUILD=0
      shift
      ;;
    --no-start)
      START_SERVICES=0
      shift
      ;;
    --skip-prereqs)
      INSTALL_PREREQS=0
      shift
      ;;
    --skip-gui)
      INSTALL_GUI=0
      shift
      ;;
    --skip-confined-firefox)
      INSTALL_CONFINED_FIREFOX=0
      shift
      ;;
    --overwrite-config)
      OVERWRITE_CONFIG=1
      shift
      ;;
    --user)
      [[ $# -ge 2 ]] || die "--user requires a value"
      TARGET_USER="$2"
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

[[ "${EUID}" -ne 0 ]] || die "run this script as the desktop user, not as root; it will call sudo for privileged install steps"
[[ -n "${TARGET_USER}" ]] || die "could not determine target desktop user; pass --user USER"

cd "${REPO_ROOT}"

require_cmd sudo
if [[ "${INSTALL_PREREQS}" -eq 1 ]]; then
  install_prerequisites
else
  log "skipping prerequisite installation"
fi
warn_browser_prerequisites

require_cmd install
require_cmd systemctl
require_cmd systemd-analyze
if [[ "${BUILD}" -eq 1 ]]; then
  require_cmd cargo
  if [[ "${INSTALL_GUI}" -eq 1 ]]; then
    require_cmd npm
  fi
fi

if [[ "${BUILD}" -eq 1 ]]; then
  log "building release daemon"
  cargo build --manifest-path focusd/Cargo.toml --release --locked

  log "building release native host"
  cargo build --manifest-path native-host/Cargo.toml --release --locked

  if [[ "${INSTALL_GUI}" -eq 1 ]]; then
    log "building Tauri GUI"
    (
      cd focus-gui
      npm ci
      npm run tauri -- build --no-bundle
    )
  fi
else
  log "skipping builds; existing artifacts will be installed"
fi

[[ -x focusd/target/release/blockuntud ]] || die "missing focusd/target/release/blockuntud; rerun without --no-build"
[[ -x native-host/target/release/blockuntu-native ]] || die "missing native-host/target/release/blockuntu-native; rerun without --no-build"
if [[ "${INSTALL_GUI}" -eq 1 ]]; then
  [[ -x focus-gui/src-tauri/target/release/blockuntu-gui ]] || die "missing focus-gui/src-tauri/target/release/blockuntu-gui; rerun without --no-build"
fi

log "creating blockuntu socket group and adding ${TARGET_USER}"
sudo groupadd --system blockuntu 2>/dev/null || true
sudo usermod -aG blockuntu "${TARGET_USER}"

if [[ ! -s /etc/blockuntu/tier1-edit-key.txt ]]; then
  log "creating Tier 1 edit key"
  random_hex="$(od -An -N24 -tx1 /dev/urandom | tr -d ' \n' | tr '[:lower:]' '[:upper:]')"
  chunks="$(printf '%s' "${random_hex}" | sed 's/.\{8\}/&-/g; s/-$//')"
  temp_key="$(mktemp)"
  printf 'BLOCKUNTU-TIER1-EDIT-%s\n' "${chunks}" >"${temp_key}"
  sudo install -d -o root -g root -m 0755 /etc/blockuntu
  sudo install -o root -g blockuntu -m 0640 "${temp_key}" /etc/blockuntu/tier1-edit-key.txt
  rm -f "${temp_key}"
else
  log "preserving existing /etc/blockuntu/tier1-edit-key.txt"
fi

log "installing daemon and native host binaries"
sudo install -Dm755 focusd/target/release/blockuntud /usr/local/bin/blockuntud
sudo install -Dm755 native-host/target/release/blockuntu-native /usr/local/bin/blockuntu-native

if [[ -f browser-extension-firefox/BlocKuntu-Signed.xpi ]]; then
  log "installing Firefox extension artifact for deferred managed policy"
  sudo install -Dm644 browser-extension-firefox/BlocKuntu-Signed.xpi \
    /usr/local/share/blockuntu/BlocKuntu-Signed.xpi
else
  log "warning: browser-extension-firefox/BlocKuntu-Signed.xpi is missing; deferred Firefox policy cannot force-install a local XPI"
fi

chrome_crx_url="https://nx57427.your-storageshare.de/s/EB9j77etxD4ojkC/download"
if [[ -f browser-extension-chrome/browser-extension-chrome.crx ]]; then
  log "installing Chrome extension artifact for deferred managed policy"
  sudo install -Dm644 browser-extension-chrome/browser-extension-chrome.crx \
    /usr/local/share/blockuntu/browser-extension-chrome.crx
  chrome_crx_url="file:///usr/local/share/blockuntu/browser-extension-chrome.crx"
fi

if [[ "${OVERWRITE_CONFIG}" -eq 1 || ! -f /etc/blockuntu/config.toml ]]; then
  log "installing /etc/blockuntu/config.toml"
  sudo install -Dm644 packaging/deb/blockuntu.toml /etc/blockuntu/config.toml
else
  log "preserving existing /etc/blockuntu/config.toml"
fi

log "installing Native Messaging manifests"
sudo install -Dm644 packaging/native-messaging/blockuntu_native.json \
  /usr/lib/mozilla/native-messaging-hosts/blockuntu_native.json
sudo install -Dm644 packaging/native-messaging/blockuntu_native.chrome.json \
  /etc/opt/chrome/native-messaging-hosts/blockuntu_native.json
sudo install -Dm644 packaging/native-messaging/blockuntu_native.chrome.json \
  /etc/chromium/native-messaging-hosts/blockuntu_native.json

if [[ "${INSTALL_CONFINED_FIREFOX}" -eq 1 ]]; then
  "${REPO_ROOT}/scripts/setup-confined-firefox-native-host.sh" \
    --user "${TARGET_USER}" \
    --native-host /usr/local/bin/blockuntu-native \
    --targets auto
else
  log "skipping confined Firefox setup"
fi

log "installing systemd units"
sudo install -Dm644 packaging/systemd/blockuntu.socket /etc/systemd/system/blockuntu.socket
sudo install -Dm644 packaging/systemd/blockuntu.service /etc/systemd/system/blockuntu.service
sudo install -Dm644 packaging/systemd/blockuntu-watchdog.service /etc/systemd/system/blockuntu-watchdog.service
sudo install -Dm644 packaging/systemd/blockuntu-hosts.path /etc/systemd/system/blockuntu-hosts.path
sudo install -Dm644 packaging/systemd/blockuntu-hosts.service /etc/systemd/system/blockuntu-hosts.service

override_file="$(mktemp)"
desktop_file=""
cleanup() {
  rm -f "${override_file}"
  if [[ -n "${desktop_file}" ]]; then
    rm -f "${desktop_file}"
  fi
}
trap cleanup EXIT

cat >"${override_file}" <<'OVERRIDE'
[Service]
ExecStart=
OVERRIDE
cat >>"${override_file}" <<OVERRIDE
ExecStart=/usr/local/bin/blockuntud --extension-xpi /usr/local/share/blockuntu/BlocKuntu-Signed.xpi --chrome-extension-crx-url ${chrome_crx_url} --defer-browser-policy-repair-until-heartbeat serve
OVERRIDE

log "installing deferred browser-policy service override"
sudo install -Dm644 "${override_file}" \
  /etc/systemd/system/blockuntu.service.d/90-defer-browser-policy.conf

if [[ -f /etc/firefox/policies/policies.json ]]; then
  log "leaving existing /etc/firefox/policies/policies.json untouched"
fi
if [[ -f /etc/opt/chrome/policies/managed/blockuntu.json ]]; then
  log "leaving existing /etc/opt/chrome/policies/managed/blockuntu.json untouched"
fi

if [[ "${INSTALL_GUI}" -eq 1 ]]; then
  log "installing Tauri GUI binary and launcher"
  sudo install -Dm755 focus-gui/src-tauri/target/release/blockuntu-gui \
    /usr/local/bin/blockuntu-gui
  sudo install -Dm644 focus-gui/src-tauri/icons/32x32.png \
    /usr/share/icons/hicolor/32x32/apps/blockuntu.png
  sudo install -Dm644 focus-gui/src-tauri/icons/32x32.png \
    /usr/share/icons/hicolor/32x32/apps/blockuntu-gui.png
  sudo install -Dm644 focus-gui/src-tauri/icons/64x64.png \
    /usr/share/icons/hicolor/64x64/apps/blockuntu.png
  sudo install -Dm644 focus-gui/src-tauri/icons/64x64.png \
    /usr/share/icons/hicolor/64x64/apps/blockuntu-gui.png
  sudo install -Dm644 focus-gui/src-tauri/icons/128x128.png \
    /usr/share/icons/hicolor/128x128/apps/blockuntu.png
  sudo install -Dm644 focus-gui/src-tauri/icons/128x128.png \
    /usr/share/icons/hicolor/128x128/apps/blockuntu-gui.png

  desktop_file="$(mktemp)"
  cat >"${desktop_file}" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=BlocKuntu
Comment=Linux focus blocker frontend
Exec=/usr/local/bin/blockuntu-gui
Icon=blockuntu-gui
StartupWMClass=blockuntu-gui
StartupNotify=true
Terminal=false
Categories=Utility;
DESKTOP
  sudo install -Dm644 "${desktop_file}" /usr/share/applications/blockuntu.desktop

  if command -v update-desktop-database >/dev/null 2>&1; then
    sudo update-desktop-database /usr/share/applications >/dev/null 2>&1 || true
  fi
  if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    sudo gtk-update-icon-cache -q /usr/share/icons/hicolor >/dev/null 2>&1 || true
  fi
fi

log "verifying systemd units"
sudo systemd-analyze verify \
  /etc/systemd/system/blockuntu.socket \
  /etc/systemd/system/blockuntu.service \
  /etc/systemd/system/blockuntu-watchdog.service \
  /etc/systemd/system/blockuntu-hosts.path \
  /etc/systemd/system/blockuntu-hosts.service

log "reloading systemd"
sudo systemctl daemon-reload

if [[ "${START_SERVICES}" -eq 1 ]]; then
  log "enabling and starting BlocKuntu units"
  sudo systemctl enable --now \
    blockuntu.socket \
    blockuntu.service \
    blockuntu-watchdog.service \
    blockuntu-hosts.path
else
  log "skipping service start"
fi

cat <<SUMMARY

BlocKuntu installation complete.

Important next steps:
  1. Log out and back in so ${TARGET_USER} receives the blockuntu group.
  2. Install and enable the BlocKuntu Firefox and/or Chrome extension manually.
  3. Restart the browser after installing the extension.
  4. Start the GUI from the app launcher or run: blockuntu-gui

The Tier 1 site-list edit key is stored at:
  /etc/blockuntu/tier1-edit-key.txt

Browser policy repair is deferred until the first extension heartbeat in:
  /etc/systemd/system/blockuntu.service.d/90-defer-browser-policy.conf

No system Firefox, Firefox Snap, or Chrome policy file is created until the
matching browser extension sends its first heartbeat. Firefox Flatpak uses a
per-user systemconfig policy that is written by the confined Firefox helper.

Native Messaging manifests were installed, so the manually installed extension
can reach /run/blockuntu/blockuntud.sock through blockuntu-native.
If Firefox Snap is installed for ${TARGET_USER}, its per-user manifest and host
copy were installed too. If Firefox Flatpak is installed, its per-user manifest,
host copy, copied XPI, and systemconfig policy were installed. Restart those
browsers before testing.
SUMMARY
