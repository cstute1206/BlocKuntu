#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"

PACKAGE_NAME="blockuntu"
VERSION="0.1.0"
RELEASE="1"
OUTPUT_DIR="${REPO_ROOT}/target/arch"
SYNC_DEPS=true
SOURCE_ONLY=false
WORK_DIR=""

usage() {
  cat <<'USAGE'
Usage: scripts/package-arch.sh [options]

Build a local Arch Linux package candidate from the current checkout. The
package contains the BlocKuntu daemon, Native Messaging bridge, Tauri GUI,
systemd units, default configuration, and no browser-extension artifacts.
Browser policies remain deferred until each matching store-installed extension
sends a verified heartbeat.

Run this on Arch Linux as a normal user. makepkg may request authorization to
install declared build dependencies; never run this script as root.

Options:
  --version VERSION   Package version, default 0.1.0.
  --release RELEASE   Package release, default 1.
  --output-dir DIR    Output directory, default target/arch.
  --source-only       Create a compact source archive instead of a package.
  --no-sync-deps      Do not ask makepkg to install missing dependencies.
  -h, --help          Show this help.
USAGE
}

log() {
  printf '[blockuntu-arch] %s\n' "$*"
}

die() {
  printf '[blockuntu-arch] error: %s\n' "$*" >&2
  exit 1
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      [[ $# -ge 2 ]] || die "--version requires a value"
      VERSION="$2"
      shift 2
      ;;
    --release)
      [[ $# -ge 2 ]] || die "--release requires a value"
      RELEASE="$2"
      shift 2
      ;;
    --output-dir)
      [[ $# -ge 2 ]] || die "--output-dir requires a value"
      OUTPUT_DIR="$2"
      shift 2
      ;;
    --no-sync-deps)
      SYNC_DEPS=false
      shift
      ;;
    --source-only)
      SOURCE_ONLY=true
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

[[ "${VERSION}" =~ ^[0-9][0-9A-Za-z._+]*$ ]] || \
  die "package version must start with a digit and contain only letters, digits, dot, underscore, or plus"
[[ "${RELEASE}" =~ ^[0-9][0-9A-Za-z._+]*$ ]] || \
  die "package release must start with a digit and contain only letters, digits, dot, underscore, or plus"
[[ "${EUID}" -ne 0 ]] || die "do not run this script as root; run it as the Arch desktop/build user"

require_cmd find
require_cmd sed
require_cmd sha256sum
require_cmd tar
if ! "${SOURCE_ONLY}"; then
  require_cmd makepkg
fi

if [[ "${OUTPUT_DIR}" != /* ]]; then
  OUTPUT_DIR="${REPO_ROOT}/${OUTPUT_DIR}"
fi

cd "${REPO_ROOT}"
[[ -f packaging/arch/PKGBUILD ]] || die "missing packaging/arch/PKGBUILD"
[[ -f packaging/arch/blockuntu.install ]] || die "missing packaging/arch/blockuntu.install"
[[ -f LICENSE ]] || die "missing LICENSE required by the package"

mkdir -p "${OUTPUT_DIR}"
WORK_DIR="$(mktemp -d "${OUTPUT_DIR}/blockuntu-arch.XXXXXX")"
trap 'rm -rf "${WORK_DIR}"' EXIT

SOURCE_ROOT="${WORK_DIR}/source"
SOURCE_TREE="${SOURCE_ROOT}/${PACKAGE_NAME}-${VERSION}"
PACKAGE_BUILD_DIR="${WORK_DIR}/pkgbuild"
SOURCE_ARCHIVE="${PACKAGE_BUILD_DIR}/${PACKAGE_NAME}-${VERSION}.tar.gz"
mkdir -p "${SOURCE_TREE}" "${PACKAGE_BUILD_DIR}"

if command -v git >/dev/null 2>&1 && git -C "${REPO_ROOT}" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  log "staging tracked and non-ignored source files"
  git -C "${REPO_ROOT}" ls-files -z --cached --others --exclude-standard | \
    tar -C "${REPO_ROOT}" --null --files-from=- -cf - | tar -xf - -C "${SOURCE_TREE}"
else
  log "staging the extracted source tree"
  tar \
    --exclude='./.git' \
    --exclude='./target' \
    --exclude='./node_modules' \
    --exclude='./focus-gui/node_modules' \
    --exclude='./focus-gui/src-tauri/target' \
    --exclude='./focusd/target' \
    --exclude='./native-host/target' \
    -C "${REPO_ROOT}" -cf - . | tar -xf - -C "${SOURCE_TREE}"
fi

log "creating the local source archive"
tar -C "${SOURCE_ROOT}" -czf "${SOURCE_ARCHIVE}" "${PACKAGE_NAME}-${VERSION}"
SOURCE_SHA256="$(sha256sum "${SOURCE_ARCHIVE}" | awk '{print $1}')"

if "${SOURCE_ONLY}"; then
  destination="${OUTPUT_DIR}/${PACKAGE_NAME}-${VERSION}-${RELEASE}-source.tar.gz"
  install -Dm644 "${SOURCE_ARCHIVE}" "${destination}"
  log "source archive created: ${destination}"
  log "SHA256: $(sha256sum "${destination}" | awk '{print $1}')"
  exit 0
fi

sed \
  -e "s/^pkgver=.*/pkgver=${VERSION}/" \
  -e "s/^pkgrel=.*/pkgrel=${RELEASE}/" \
  -e "s/ARCH_SOURCE_SHA256_PLACEHOLDER/${SOURCE_SHA256}/" \
  packaging/arch/PKGBUILD >"${PACKAGE_BUILD_DIR}/PKGBUILD"
install -Dm644 packaging/arch/blockuntu.install "${PACKAGE_BUILD_DIR}/blockuntu.install"

makepkg_args=(--cleanbuild --noconfirm)
if "${SYNC_DEPS}"; then
  makepkg_args+=(--syncdeps)
fi

log "building ${PACKAGE_NAME}-${VERSION}-${RELEASE}"
log "this development candidate may use configured Cargo and npm registries; do not publish it as a release artifact until the dependency inputs are vendored and declared"
(
  cd "${PACKAGE_BUILD_DIR}"
  makepkg "${makepkg_args[@]}"
)

mapfile -t package_paths < <(
  find "${PACKAGE_BUILD_DIR}" -maxdepth 1 -type f \
    -name "${PACKAGE_NAME}-${VERSION}-${RELEASE}-*.pkg.tar.*" -print | sort
)
[[ "${#package_paths[@]}" -gt 0 ]] || die "makepkg did not produce a binary package"

for package_path in "${package_paths[@]}"; do
  destination="${OUTPUT_DIR}/$(basename -- "${package_path}")"
  install -Dm644 "${package_path}" "${destination}"
  log "package created: ${destination}"
done
