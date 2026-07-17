#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." >/dev/null 2>&1 && pwd)"
RUNTIME_DIR="/tmp/blockuntu"
INSTALL_DIR="${HOME}/.local/share/blockuntu"
FIREFOX_MANIFEST_DIR="${HOME}/.mozilla/native-messaging-hosts"
CHROME_EXTENSION_ID="odedgejjcdilkoibeljkeohekonmdfea"
CHROME_MANIFEST_DIR="${HOME}/.config/google-chrome/NativeMessagingHosts"
CHROMIUM_MANIFEST_DIR="${HOME}/.config/chromium/NativeMessagingHosts"
WRAPPER_PATH="${INSTALL_DIR}/blockuntu-native-dev"
MANIFEST_PATH="${FIREFOX_MANIFEST_DIR}/blockuntu_native.json"
CHROME_MANIFEST_PATH="${CHROME_MANIFEST_DIR}/blockuntu_native.json"
CHROMIUM_MANIFEST_PATH="${CHROMIUM_MANIFEST_DIR}/blockuntu_native.json"
NATIVE_BIN="${REPO_ROOT}/native-host/target/debug/blockuntu-native"

mkdir -p \
  "${RUNTIME_DIR}" \
  "${INSTALL_DIR}" \
  "${FIREFOX_MANIFEST_DIR}" \
  "${CHROME_MANIFEST_DIR}" \
  "${CHROMIUM_MANIFEST_DIR}"

echo "[native-host-dev] building native host"
cargo build --manifest-path "${REPO_ROOT}/native-host/Cargo.toml"

cat > "${WRAPPER_PATH}" <<EOF
#!/usr/bin/env bash
exec "${NATIVE_BIN}" \\
  --socket "${RUNTIME_DIR}/blockuntud.sock" \\
  --revive-command "${REPO_ROOT}/scripts/start-dev-daemon.sh"
EOF
chmod 0755 "${WRAPPER_PATH}"

cat > "${MANIFEST_PATH}" <<EOF
{
  "name": "blockuntu_native",
  "description": "BlocKuntu development Native Messaging bridge",
  "path": "${WRAPPER_PATH}",
  "type": "stdio",
  "allowed_extensions": ["blockuntu@example.local", "{a7c3f3c4-6b1e-4c6f-9f2a-8d4e5b7c1a90}"]
}
EOF

for chrome_manifest in "${CHROME_MANIFEST_PATH}" "${CHROMIUM_MANIFEST_PATH}"; do
cat > "${chrome_manifest}" <<EOF
{
  "name": "blockuntu_native",
  "description": "BlocKuntu development Native Messaging bridge",
  "path": "${WRAPPER_PATH}",
  "type": "stdio",
  "allowed_origins": ["chrome-extension://${CHROME_EXTENSION_ID}/"]
}
EOF
done

echo "[native-host-dev] installed ${MANIFEST_PATH}"
echo "[native-host-dev] installed ${CHROME_MANIFEST_PATH}"
echo "[native-host-dev] installed ${CHROMIUM_MANIFEST_PATH}"
echo "[native-host-dev] wrapper uses socket ${RUNTIME_DIR}/blockuntud.sock"
echo "[native-host-dev] restart Firefox/Chrome after installing or updating these manifests"
