# TanStack Start with Nitro

This is a minimal reproducer for a TanStack Start project whose Vite wrapper
uses Nitro with a non-Node default preset.

The project intentionally has:

- no `start` script in `package.json`;
- no explicit Nitro preset in `vite.config.ts`;
- a modern text-based `bun.lock`.

Anybuild detects Nitro, builds with `NITRO_PRESET=node-server`, and starts the
generated server with `node .output/server/index.mjs`.

Run it with Wasmer:

```bash
cargo run -- examples/node-tanstack-start-nitro --wasmer --start
```
