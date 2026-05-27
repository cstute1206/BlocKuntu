import { existsSync, rmSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

const outputs = ["BlocKuntu.xpi", "Archive.zip"];
const packageEntries = ["manifest.json", "blocked.html", "dist"];

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

assert(existsSync(resolve("manifest.json")), "manifest.json is missing");
assert(existsSync(resolve("blocked.html")), "blocked.html is missing");
assert(existsSync(resolve("dist/background.js")), "dist/background.js is missing; run npm run build first");
assert(existsSync(resolve("dist/blocked.js")), "dist/blocked.js is missing; run npm run build first");

for (const output of outputs) {
  rmSync(resolve(output), { force: true });

  const result = spawnSync("zip", ["-r", output, ...packageEntries], { stdio: "inherit" });

  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error(`zip exited with status ${result.status}`);
  }

  console.log(`Wrote ${resolve(output)}`);
}
