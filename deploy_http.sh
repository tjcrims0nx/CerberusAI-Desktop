cat << 'EOF' > /root/skills-server/src/http.ts
import express from 'express';
import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { SSEServerTransport } from '@modelcontextprotocol/sdk/server/sse.js';
import { CallToolRequestSchema, ListToolsRequestSchema } from '@modelcontextprotocol/sdk/types.js';
import { TOOLS, handleToolCall } from './tools.js';
import { pluginManager } from './plugin_manager.js';

const app = express();
app.use(express.json());

let server: Server;
let transport: SSEServerTransport;

function initServer() {
  server = new Server({ name: 'cerberus-skills-http', version: '1.0.0' }, { capabilities: { tools: {} } });
  server.setRequestHandler(ListToolsRequestSchema, async () => {
    const proxyTools = await pluginManager.proxyListTools();
    return { tools: [...TOOLS, ...proxyTools] };
  });
  server.setRequestHandler(CallToolRequestSchema, async (request) => {
    const { name, arguments: args } = request.params;
    if (!args) throw new Error(`Missing arguments for tool ${name}`);
    return await handleToolCall(name, args);
  });
}

initServer();

app.get('/skills-sse', async (req, res) => {
  try {
      if (transport) {
        await server.close(); // Close old connection
      }
  } catch (e) {}
  
  initServer(); // Re-init server for the new connection
  transport = new SSEServerTransport('/skills-message', res);
  await server.connect(transport);
});

app.post('/skills-message', async (req, res) => {
  if (transport) {
    await transport.handlePostMessage(req, res);
  } else {
    res.status(500).send('Transport not initialized');
  }
});

const PORT = process.env.PORT || 3001;
app.listen(PORT, async () => {
  await pluginManager.init();
  console.log(`Cerberus Skills HTTP Server running on port ${PORT}`);
});
EOF
cd /root/skills-server && npm run build && systemctl restart cerberus-skills
