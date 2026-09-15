# MCP SDK 2: MCPServer

This server exposes `search` and `fetch` tools for cupcake orders stored in
`records.json`. Keep that file beside `main.py` when copying the example.

This example uses MCP SDK 2.x and serves Streamable HTTP at `/mcp`.
It listens on `HOST` (default `0.0.0.0`) and `PORT` (default `8080`).

```sh
anybuild . --start
anybuild . --runner=wasmer --start
```

The `-v1` examples retain the original FastMCP API, including the ChatGPT
example's SSE transport. The examples without that suffix use MCPServer
and stateless Streamable HTTP with SDK 2.x. Both versions remain supported.
