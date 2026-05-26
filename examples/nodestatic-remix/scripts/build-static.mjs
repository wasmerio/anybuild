import { access, mkdir } from "node:fs/promises";

await mkdir("public", { recursive: true });
await access("public/index.html");
console.log("Remix static output is ready in public/");
