# Git-ignored Node-static subdirectory

This example verifies two properties of Node-static builds:

- staging a subdirectory application follows the repository's `.gitignore`;
- copying the static build output preserves hidden directories.

The ignored `target/broken-link` symlink intentionally points to a missing
file. A source copy that does not honor `.gitignore` fails while trying to
follow it. The application build also writes `.wasmer/host.html`, which a
shell `dist/*` copy silently omits.

From the repository root, run:

```console
anybuild examples/nodestatic-gitignore --subdir app --wasmer --start
```

Then verify both outputs:

```console
curl http://127.0.0.1:8080/
curl http://127.0.0.1:8080/.wasmer/host.html
```
