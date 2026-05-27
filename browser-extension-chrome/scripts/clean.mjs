import { rmSync } from "node:fs";

for (const path of ["dist", "BlocKuntu-Chrome.zip"]) {
  rmSync(path, { recursive: true, force: true });
}
