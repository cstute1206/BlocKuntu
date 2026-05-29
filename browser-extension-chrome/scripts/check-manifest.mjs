import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

const EXPECTED_EXTENSION_ID = "odedgejjcdilkoibeljkeohekonmdfea";

const manifestPath = resolve("manifest.json");
const backgroundPath = resolve("dist/background.js");
const blockedScriptPath = resolve("dist/blocked.js");
const blockedPath = resolve("blocked.html");

const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

function extensionIdFromKey(key) {
  const der = Buffer.from(key, "base64");
  const hash = createHash("sha256").update(der).digest();
  const alphabet = "abcdefghijklmnop";
  return Array.from(hash.subarray(0, 16), (byte) => alphabet[byte >> 4] + alphabet[byte & 15]).join(
    ""
  );
}

assert(manifest.manifest_version === 3, "manifest_version must be 3");
assert(
  manifest.background?.service_worker === "dist/background.js",
  "Chrome MV3 must use a service worker"
);
assert(
  extensionIdFromKey(manifest.key) === EXPECTED_EXTENSION_ID,
  "manifest key must produce the documented Chrome extension id"
);
assert(manifest.permissions?.includes("alarms"), "alarms permission is required");
assert(
  manifest.permissions?.includes("nativeMessaging"),
  "nativeMessaging permission is required"
);
assert(manifest.permissions?.includes("tabs"), "tabs permission is required");
assert(
  manifest.permissions?.includes("webNavigation"),
  "webNavigation permission is required"
);
assert(existsSync(backgroundPath), "dist/background.js was not built");
assert(existsSync(blockedScriptPath), "dist/blocked.js was not built");
assert(existsSync(blockedPath), "blocked.html is missing");

console.log(`Chrome extension manifest check passed (${EXPECTED_EXTENSION_ID})`);
