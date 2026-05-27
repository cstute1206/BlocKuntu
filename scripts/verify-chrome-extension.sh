#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." >/dev/null 2>&1 && pwd)"
EXT_DIR="${REPO_ROOT}/browser-extension-chrome"

echo "[chrome-extension] directory: ${EXT_DIR}"
cd "${EXT_DIR}"

if [ ! -x node_modules/.bin/tsc ]; then
  echo "[chrome-extension] installing npm dependencies"
  npm install
fi

echo "[chrome-extension] cleaning previous build artifacts"
npm run clean

echo "[chrome-extension] compiling TypeScript"
npm run build

echo "[chrome-extension] checking manifest/build outputs"
npm run check

echo "[chrome-extension] verification complete"
