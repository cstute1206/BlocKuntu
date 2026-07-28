import { cpSync, existsSync, mkdtempSync, rmSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const output = "BlocKuntu-Chrome.zip";
const required = [
  "manifest.json",
  "blocked.html",
  "dist/background.js",
  "dist/blocked.js",
  "icons/blockuntu-16.png",
  "icons/blockuntu-32.png",
  "icons/blockuntu-48.png",
  "icons/blockuntu-128.png",
];

for (const path of required) {
  if (!existsSync(path)) {
    throw new Error(`${path} is missing; run npm run verify first`);
  }
}

rmSync(output, { force: true });

const stagingDirectory = mkdtempSync(join(tmpdir(), "blockuntu-chrome-store-"));
try {
  cpSync("manifest.json", join(stagingDirectory, "manifest.json"));
  cpSync("blocked.html", join(stagingDirectory, "blocked.html"));
  cpSync("dist", join(stagingDirectory, "dist"), { recursive: true });
  cpSync("icons", join(stagingDirectory, "icons"), { recursive: true });

  const result = spawnSync(
    "zip",
    ["-r", resolve(output), "manifest.json", "blocked.html", "dist", "icons"],
    { cwd: stagingDirectory, stdio: "inherit" }
  );
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error(`zip failed with status ${result.status}`);
  }
} finally {
  rmSync(stagingDirectory, { recursive: true, force: true });
}

console.log(`Created ${output}`);
