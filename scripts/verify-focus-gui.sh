#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." >/dev/null 2>&1 && pwd)"
GUI_DIR="${REPO_ROOT}/focus-gui"

echo "[focus-gui] directory: ${GUI_DIR}"
cd "${GUI_DIR}"

if [ ! -d node_modules ]; then
  echo "[focus-gui] installing npm dependencies"
  npm install
fi

echo "[focus-gui] checking Svelte and TypeScript"
npm run check

echo "[focus-gui] building frontend"
npm run build

echo "[focus-gui] checking Tauri backend"
cargo check --manifest-path src-tauri/Cargo.toml

echo "[focus-gui] verification complete"
