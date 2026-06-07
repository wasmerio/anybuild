import {
  createMcpExpressApp,
} from '@modelcontextprotocol/sdk/server/express.js';
import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import {
  StreamableHTTPServerTransport,
} from '@modelcontextprotocol/sdk/server/streamableHttp.js';
import * as z from 'zod/v4';

function createServer() {
  const server = new McpServer({
    name: 'stackmachine-mcp-example',
    version: '1.0.0',
  });

  server.registerTool(
    'greet',
    {
      description: 'Return a friendly greeting.',
      inputSchema: {
        name: z.string().default('StackMachine').describe('Name to greet'),
      },
    },
    async ({ name }) => ({
      content: [{ type: 'text', text: 'Hello, ' + name + '!' }],
    }),
  );

  return server;
}

const app = createMcpExpressApp({
  // This is required to make the server accessible without having allowedHosts
  host: '0.0.0.0',
});

app.get('/', (_req, res) => {
  res
    .type('text/plain')
    .send('MCP server is running. Connect your MCP client to /mcp.');
});

app.post('/mcp', async (req, res) => {
  const server = createServer();
  const transport = new StreamableHTTPServerTransport({
    sessionIdGenerator: undefined,
  });

  res.on('close', () => {
    void transport.close();
    void server.close();
  });

  try {
    await server.connect(transport);
    await transport.handleRequest(req, res, req.body);
  } catch (error) {
    console.error('Error handling MCP request:', error);
    if (!res.headersSent) {
      res.status(500).json({
        jsonrpc: '2.0',
        error: { code: -32603, message: 'Internal server error' },
        id: null,
      });
    }
  }
});

app.get('/mcp', (_req, res) => {
  res.status(405).set('Allow', 'POST').send('Method Not Allowed');
});

app.delete('/mcp', (_req, res) => {
  res.status(405).set('Allow', 'POST').send('Method Not Allowed');
});

const port = process.env.PORT || 3000;
app.listen(port, () => {
  console.log('MCP server listening on port ' + port);
});
