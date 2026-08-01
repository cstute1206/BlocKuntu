#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"

PACKAGE_NAME="blockuntu"
VERSION="0.1.0"
RELEASE="1"
OUTPUT_DIR="${REPO_ROOT}/target/arch"
IMAGE_NAME="blockuntu-arch-builder:local"
CONTAINER_NAME=""
COPY_DIR=""
BUILD_SUCCEEDED=false

usage() {
  cat <<'USAGE'
Usage: scripts/package-arch-docker.sh [options]

Build a local Arch Linux package candidate on a non-Arch host by using a
short-lived Arch Linux Docker container. The current checkout is staged as a
small source archive; build caches and the working tree are never mounted in
the container. The completed package is written to target/arch by default.

Docker must be usable by the invoking non-root user. The container has
passwordless sudo only within its own disposable filesystem so makepkg can
install the PKGBUILD's declared dependencies.

Options:
  --version VERSION   Package version, default 0.1.0.
  --release RELEASE   Package release, default 1.
  --output-dir DIR    Output directory, default target/arch.
  --image IMAGE       Builder image tag, default blockuntu-arch-builder:local.
  -h, --help          Show this help.
USAGE
}

log() {
  printf '[blockuntu-arch-docker] %s\n' "$*"
}

die() {
  printf '[blockuntu-arch-docker] error: %s\n' "$*" >&2
  exit 1
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

cleanup() {
  if [[ -n "${CONTAINER_NAME}" ]]; then
    if "${BUILD_SUCCEEDED}"; then
      docker rm -f "${CONTAINER_NAME}" >/dev/null 2>&1 || true
    else
      log "failed container retained for diagnostics: ${CONTAINER_NAME}"
    fi
  fi
  if [[ -n "${COPY_DIR}" && -d "${COPY_DIR}" ]]; then
    rm -rf -- "${COPY_DIR}"
  fi
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
    --image)
      [[ $# -ge 2 ]] || die "--image requires a value"
      IMAGE_NAME="$2"
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

[[ "${VERSION}" =~ ^[0-9][0-9A-Za-z._+]*$ ]] || \
  die "package version must start with a digit and contain only letters, digits, dot, underscore, or plus"
[[ "${RELEASE}" =~ ^[0-9][0-9A-Za-z._+]*$ ]] || \
  die "package release must start with a digit and contain only letters, digits, dot, underscore, or plus"
[[ "${EUID}" -ne 0 ]] || die "do not run this script as root; use the desktop/build user with Docker access"

require_cmd docker
require_cmd find
require_cmd install
require_cmd mktemp
require_cmd sha256sum

if [[ "${OUTPUT_DIR}" != /* ]]; then
  OUTPUT_DIR="${REPO_ROOT}/${OUTPUT_DIR}"
fi
mkdir -p "${OUTPUT_DIR}"

trap cleanup EXIT

log "creating a compact source archive from the current checkout"
"${SCRIPT_DIR}/package-arch.sh" \
  --source-only \
  --version "${VERSION}" \
  --release "${RELEASE}" \
  --output-dir "${OUTPUT_DIR}"

SOURCE_ARCHIVE="${OUTPUT_DIR}/${PACKAGE_NAME}-${VERSION}-${RELEASE}-source.tar.gz"
[[ -f "${SOURCE_ARCHIVE}" ]] || die "source archive was not created: ${SOURCE_ARCHIVE}"

log "building the disposable Arch Linux builder image"
docker build \
  --pull \
  --quiet \
  --file "${REPO_ROOT}/packaging/arch/Dockerfile" \
  --tag "${IMAGE_NAME}" \
  "${REPO_ROOT}/packaging/arch"

CONTAINER_NAME="${PACKAGE_NAME}-arch-build-$$_${RANDOM}"
CONTAINER_NAME="${CONTAINER_NAME/_/-}"
log "creating an isolated Arch package build container"
docker create \
  --name "${CONTAINER_NAME}" \
  --mount "type=bind,source=${SOURCE_ARCHIVE},target=/input/source.tar.gz,readonly" \
  "${IMAGE_NAME}" \
  bash -lc '
    set -euo pipefail
    version="$1"
    release="$2"
    tar -xzf /input/source.tar.gz -C /work
    cd "/work/blockuntu-${version}"
    ./scripts/package-arch.sh --version "${version}" --release "${release}" --output-dir /output
  ' -- "${VERSION}" "${RELEASE}" >/dev/null

log "building ${PACKAGE_NAME}-${VERSION}-${RELEASE} inside Arch Linux"
docker start "${CONTAINER_NAME}" >/dev/null
container_exit_code="$(docker wait "${CONTAINER_NAME}")"
if [[ "${container_exit_code}" != "0" ]]; then
  docker logs --tail 200 "${CONTAINER_NAME}" >&2 || true
  die "the Arch container build failed with exit code ${container_exit_code}"
fi

COPY_DIR="$(mktemp -d "${OUTPUT_DIR}/blockuntu-arch-docker.XXXXXX")"
docker cp "${CONTAINER_NAME}:/output/." "${COPY_DIR}"

mapfile -t package_paths < <(
  find "${COPY_DIR}" -maxdepth 1 -type f \
    -name "${PACKAGE_NAME}-${VERSION}-${RELEASE}-*.pkg.tar.*" -print | sort
)
[[ "${#package_paths[@]}" -gt 0 ]] || die "the Docker build did not produce a binary package"

for package_path in "${package_paths[@]}"; do
  destination="${OUTPUT_DIR}/$(basename -- "${package_path}")"
  checksum_path="${destination}.sha256"
  install -Dm644 "${package_path}" "${destination}"
  (
    cd "${OUTPUT_DIR}"
    sha256sum "$(basename -- "${destination}")" >"$(basename -- "${checksum_path}")"
  )
  log "package created: ${destination}"
  log "checksum created: ${checksum_path}"
done

BUILD_SUCCEEDED=true
