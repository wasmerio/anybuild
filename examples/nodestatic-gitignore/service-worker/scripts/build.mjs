import { mkdir, writeFile } from "node:fs/promises";

await mkdir("dist/.wasmer", { recursive: true });
await writeFile(
  "dist/index.html",
  "<!doctype html><h1>Git-ignored Node-static example</h1>\n",
);
await writeFile(
  "dist/.wasmer/host.html",
  "<!doctype html><h1>Hidden Node-static output</h1>\n",
);
