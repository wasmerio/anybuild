# Cupcake MCP for Deep Research

This is a minimal example of a Deep Research style MCP server for searching and fetching cupcake orders.

## Set up & run

Python setup:

```shell
python -m venv env
source env/bin/activate
pip install -r requirements.txt
```

Run the server:

```shell
python sample_mcp.py
```

The server will start on `http://127.0.0.1:8000` using SSE transport.

## Files

- `sample_mcp.py`: Main server code
- `records.json`: Cupcake order data (must be present in the same directory)
