import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

const manifestPath = resolve("manifest.json");
const backgroundPath = resolve("dist/background.js");
const blockedPath = resolve("blocked.html");

const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

assert(manifest.manifest_version === 3, "manifest_version must be 3");
assert(
  manifest.browser_specific_settings?.gecko?.id === "{a7c3f3c4-6b1e-4c6f-9f2a-8d4e5b7c1a90}",
  "Firefox extension id must match enterprise policy"
);
assert(
  manifest.permissions?.includes("nativeMessaging"),
  "nativeMessaging permission is required"
);
assert(manifest.permissions?.includes("tabs"), "tabs permission is required");
assert(
  manifest.permissions?.includes("webNavigation"),
  "webNavigation permission is required"
);
assert(
  manifest.background?.scripts?.includes("dist/background.js"),
  "manifest must load compiled TypeScript output"
);
assert(existsSync(backgroundPath), "dist/background.js was not built");
assert(existsSync(blockedPath), "blocked.html is missing");

console.log("Firefox extension manifest check passed");
