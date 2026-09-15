"""Exercise the examples over MCP, using the SDK 2.x client for either server."""

import argparse
import json
from pathlib import Path

import anyio
from mcp import ClientSession
from mcp.client.sse import sse_client
from mcp.client.streamable_http import streamable_http_client


async def check(args):
    endpoint = "/sse" if args.transport == "sse" else "/mcp"
    connector = sse_client if args.transport == "sse" else streamable_http_client
    with anyio.fail_after(30):
        async with connector(args.url + endpoint) as streams:
            async with ClientSession(streams[0], streams[1]) as session:
                await session.initialize()
                tools = {tool.name for tool in (await session.list_tools()).tools}
                if "add" in tools:
                    result = await session.call_tool("add", {"a": 2, "b": 3})
                    assert not result.is_error, result
                    assert any(block.text.strip() == "5" for block in result.content)
                else:
                    assert {"search", "fetch"} <= tools, tools
                    record = json.loads(
                        (Path(args.project) / "records.json").read_text()
                    )[0]
                    result = await session.call_tool("fetch", {"id": record["id"]})
                    assert not result.is_error, result
                    assert record["id"] in str(result), result
                    result = await session.call_tool("search", {"query": "cupcake"})
                    assert not result.is_error, result
    print("MCP initialization and tool calls passed")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--url", required=True)
    parser.add_argument("--project", required=True)
    parser.add_argument("--transport", choices=["sse", "streamable-http"], required=True)
    anyio.run(check, parser.parse_args())
