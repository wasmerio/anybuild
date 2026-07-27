# Node pnpm Workspace Subdir

This example reproduces a runtime Node app that lives inside a pnpm
workspace subdirectory. The app imports a local workspace package, and that
package depends on `clsx`.

Run it from the repository root with:

```sh
anybuild examples/node-pnpm-workspace-subdir --subdir=apps/dashboard \
  --wasmer --start --regenerate
```
