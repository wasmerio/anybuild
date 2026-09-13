"""Run either supported MCP SDK generation on Anybuild's configured port."""

import importlib.metadata
import os
import runpy
import sys
from pathlib import Path


def main():
    major = int(importlib.metadata.version("mcp").split(".", 1)[0])
    if major == 1:
        from mcp.server.fastmcp import FastMCP as Server
    elif major == 2:
        from mcp.server.mcpserver import MCPServer as Server
    else:
        sys.exit(f"Anybuild supports MCP SDK 1.x and 2.x; found major {major}")

    filename, _, object_name = sys.argv[1].partition(":")
    source = Path(filename).resolve()
    sys.path.insert(0, str(source.parent))
    namespace = runpy.run_path(str(source))
    names = [object_name] if object_name else ["mcp", "server", "app"]
    server = next(
        (namespace[name] for name in names
         if isinstance(namespace.get(name), Server)),
        None,
    )
    if server is None:
        sys.exit("Anybuild: expose an MCP server as mcp, server, app, or file:object")
    host = os.environ.get("HOST", "0.0.0.0")
    port = int(os.environ.get("PORT", "8080"))
    if major == 1:
        server.settings.host = host
        server.settings.port = port
        server.run(transport="streamable-http")
    else:
        server.run(transport="streamable-http", host=host, port=port)


if __name__ == "__main__":
    main()
