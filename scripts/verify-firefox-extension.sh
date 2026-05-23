#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." >/dev/null 2>&1 && pwd)"
EXT_DIR="${REPO_ROOT}/browser-extension-firefox"

echo "[firefox-extension] directory: ${EXT_DIR}"
cd "${EXT_DIR}"

if [ ! -x node_modules/.bin/tsc ]; then
  echo "[firefox-extension] installing npm dependencies"
  npm install
fi

echo "[firefox-extension] cleaning previous build artifacts"
npm run clean

echo "[firefox-extension] compiling TypeScript"
npm run build

echo "[firefox-extension] checking manifest/build outputs"
npm run check

echo "[firefox-extension] verification complete"
