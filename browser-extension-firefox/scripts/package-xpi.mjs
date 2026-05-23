import { existsSync, rmSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

const output = resolve("BlocKuntu.xpi");

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

assert(existsSync(resolve("manifest.json")), "manifest.json is missing");
assert(existsSync(resolve("blocked.html")), "blocked.html is missing");
assert(existsSync(resolve("dist/background.js")), "dist/background.js is missing; run npm run build first");

rmSync(output, { force: true });

const result = spawnSync(
  "zip",
  ["-r", "BlocKuntu.xpi", "manifest.json", "blocked.html", "dist"],
  { stdio: "inherit" }
);

if (result.error) {
  throw result.error;
}
if (result.status !== 0) {
  throw new Error(`zip exited with status ${result.status}`);
}

console.log(`Wrote ${output}`);
