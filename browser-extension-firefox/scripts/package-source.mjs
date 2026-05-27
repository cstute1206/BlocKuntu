import { rmSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

const output = "Source.zip";
const sourceEntries = [
  "SOURCE_SUBMISSION.md",
  "README.md",
  "package.json",
  "package-lock.json",
  "tsconfig.json",
  "manifest.json",
  "blocked.html",
  "src",
  "scripts",
];

rmSync(resolve(output), { force: true });

const result = spawnSync("zip", ["-r", output, ...sourceEntries], { stdio: "inherit" });

if (result.error) {
  throw result.error;
}
if (result.status !== 0) {
  throw new Error(`zip exited with status ${result.status}`);
}

console.log(`Wrote ${resolve(output)}`);
