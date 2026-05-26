import { mkdir, writeFile } from 'node:fs/promises';
import { dirname } from 'node:path';

const output = process.env.SHIPIT_EXAMPLE_OUTPUT || 'dist';
const title = process.env.SHIPIT_EXAMPLE_TITLE || 'Static Example';
const html = `<!doctype html>
<html>
<head><meta charset="utf-8"><title>${title}</title></head>
<body><main><h1>${title}</h1></main></body>
</html>
`;

await mkdir(dirname(`${output}/index.html`), { recursive: true });
await writeFile(`${output}/index.html`, html);
