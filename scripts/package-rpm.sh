#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"

PACKAGE_NAME="blockuntu"
VERSION="0.1.0"
RELEASE="18"
OUTPUT_DIR="${REPO_ROOT}/target/rpm"
WORK_DIR=""
IGNORE_BUILDREQUIRES=false

usage() {
  cat <<'USAGE'
Usage: scripts/package-rpm.sh [options]

Build a self-hosted Fedora RPM containing the BlocKuntu daemon, Native
Messaging bridge, Tauri GUI, systemd units, default configuration, and no
browser-extension artifacts. Firefox and Chrome-family browser policies remain
deferred until each matching store-installed extension sends a verified
heartbeat.

This command is for the self-hosted RPM release path. It is not a Fedora
repository submission workflow: such a submission needs vendored/offline Rust
and npm dependencies.

Options:
  --version VERSION   RPM version, default 0.1.0.
  --release RELEASE   RPM release, default 18.
  --output-dir DIR    Output directory, default target/rpm.
  --ignore-buildrequires
                      Build on a non-RPM host such as Ubuntu. This skips only
                      rpmbuild's RPM-database BuildRequires check; you must
                      install the equivalent native build dependencies first.
  -h, --help          Show this help.
USAGE
}

log() {
  printf '[blockuntu-rpm] %s\n' "$*"
}

die() {
  printf '[blockuntu-rpm] error: %s\n' "$*" >&2
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
    --ignore-buildrequires)
      IGNORE_BUILDREQUIRES=true
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
  die "RPM version must start with a digit and contain only letters, digits, dot, underscore, or plus"
[[ "${RELEASE}" =~ ^[0-9][0-9A-Za-z._+]*$ ]] || \
  die "RPM release must start with a digit and contain only letters, digits, dot, underscore, or plus"

require_cmd rpmbuild
require_cmd tar

cd "${REPO_ROOT}"
[[ -f packaging/rpm/blockuntu.spec ]] || die "missing packaging/rpm/blockuntu.spec"
[[ -f LICENSE ]] || die "missing LICENSE required by the RPM package"

mkdir -p "${OUTPUT_DIR}"
WORK_DIR="$(mktemp -d "${OUTPUT_DIR}/blockuntu-rpm.XXXXXX")"
trap 'rm -rf "${WORK_DIR}"' EXIT

RPM_TOPDIR="${WORK_DIR}/rpmbuild"
SOURCE_DIR="${RPM_TOPDIR}/SOURCES"
SPEC_DIR="${RPM_TOPDIR}/SPECS"
TMP_DIR="${WORK_DIR}/tmp"
SOURCE_TREE="${WORK_DIR}/${PACKAGE_NAME}-${VERSION}"
SOURCE_ARCHIVE="${SOURCE_DIR}/${PACKAGE_NAME}-${VERSION}.tar.gz"
mkdir -p "${SOURCE_DIR}" "${SPEC_DIR}" "${RPM_TOPDIR}/BUILD" "${RPM_TOPDIR}/BUILDROOT" \
  "${RPM_TOPDIR}/RPMS" "${RPM_TOPDIR}/SRPMS" "${SOURCE_TREE}" "${TMP_DIR}"

log "staging the current tracked source tree"
if command -v git >/dev/null 2>&1 && git -C "${REPO_ROOT}" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  git -C "${REPO_ROOT}" ls-files -z --cached --others --exclude-standard | \
    tar -C "${REPO_ROOT}" --null --files-from=- -cf - | tar -xf - -C "${SOURCE_TREE}"
else
  log "Git metadata is unavailable; staging the supplied source tree without build artifacts"
  tar -C "${REPO_ROOT}" \
    --exclude='./.git' \
    --exclude='./target' \
    --exclude='*/target' \
    --exclude='./node_modules' \
    --exclude='*/node_modules' \
    -cf - . | tar -xf - -C "${SOURCE_TREE}"
fi

log "creating source archive"
tar -C "${WORK_DIR}" -czf "${SOURCE_ARCHIVE}" "${PACKAGE_NAME}-${VERSION}"
install -Dm644 packaging/rpm/blockuntu.spec "${SPEC_DIR}/blockuntu.spec"

log "building ${PACKAGE_NAME}-${VERSION}-${RELEASE}"
rpmbuild_args=(
  -bb "${SPEC_DIR}/blockuntu.spec"
  --define "_topdir ${RPM_TOPDIR}"
  --define "_tmppath ${TMP_DIR}"
  --define "blockuntu_version ${VERSION}"
  --define "blockuntu_release ${RELEASE}"
)
if "${IGNORE_BUILDREQUIRES}"; then
  log "skipping RPM BuildRequires verification for this non-RPM host"
  rpmbuild_args+=(--nodeps)
fi
rpmbuild "${rpmbuild_args[@]}"

mapfile -t package_paths < <(find "${RPM_TOPDIR}/RPMS" -type f -name "${PACKAGE_NAME}-${VERSION}-${RELEASE}*.rpm" -print | sort)
[[ "${#package_paths[@]}" -gt 0 ]] || die "rpmbuild did not produce a binary RPM"

for package_path in "${package_paths[@]}"; do
  destination="${OUTPUT_DIR}/$(basename -- "${package_path}")"
  install -Dm644 "${package_path}" "${destination}"
  log "package created: ${destination}"
done
