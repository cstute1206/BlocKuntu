import { existsSync, rmSync } from "node:fs";
import { spawnSync } from "node:child_process";

const output = "BlocKuntu-Chrome.zip";
const required = ["manifest.json", "blocked.html", "dist/background.js", "dist/blocked.js"];

for (const path of required) {
  if (!existsSync(path)) {
    throw new Error(`${path} is missing; run npm run verify first`);
  }
}

rmSync(output, { force: true });

const result = spawnSync("zip", ["-r", output, "manifest.json", "blocked.html", "dist"], {
  stdio: "inherit",
});
if (result.status !== 0) {
  throw new Error(`zip failed with status ${result.status}`);
}

console.log(`Created ${output}`);
