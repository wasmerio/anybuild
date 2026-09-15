# MCP SDK 1: FastMCP

This example uses MCP SDK 1.x and serves Streamable HTTP at `/mcp`.
It listens on `HOST` (default `0.0.0.0`) and `PORT` (default `8080`).

```sh
anybuild . --start
anybuild . --runner=wasmer --start
```

The `-v1` examples retain the original FastMCP API, including the ChatGPT
example's SSE transport. The examples without that suffix use MCPServer
and stateless Streamable HTTP with SDK 2.x. Both versions remain supported.
