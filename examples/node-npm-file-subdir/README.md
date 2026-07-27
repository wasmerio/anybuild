# Node npm File Dependency Subdir

This example reproduces a runtime Node app that lives in a subdirectory and
uses npm local `file:` dependencies outside that app directory. The app
imports a local package from `../../packages/ui`.

Run it from the repository root with:

```sh
anybuild examples/node-npm-file-subdir --subdir=apps/dashboard \
  --wasmer --start --regenerate
```
