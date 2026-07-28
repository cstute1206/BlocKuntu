import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

const manifestPath = resolve("manifest.json");
const backgroundPath = resolve("dist/background.js");
const blockedScriptPath = resolve("dist/blocked.js");
const blockedPath = resolve("blocked.html");
const requiredIcons = {
  16: "icons/blockuntu-16.png",
  32: "icons/blockuntu-32.png",
  48: "icons/blockuntu-48.png",
  128: "icons/blockuntu-128.png",
};

const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

assert(manifest.manifest_version === 3, "manifest_version must be 3");
assert(
  manifest.background?.service_worker === "dist/background.js",
  "Chrome MV3 must use a service worker"
);
assert(!Object.hasOwn(manifest, "key"), "manifest must not contain a self-hosted CRX key");
for (const [size, path] of Object.entries(requiredIcons)) {
  assert(manifest.icons?.[size] === path, `manifest icon ${size} must be ${path}`);
  assert(existsSync(resolve(path)), `manifest icon is missing: ${path}`);
}
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

console.log("Chrome extension manifest check passed (Chrome Web Store source manifest)");
