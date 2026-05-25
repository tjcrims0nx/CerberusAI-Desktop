import express from "express";
import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { SSEServerTransport } from "@modelcontextprotocol/sdk/server/sse.js";
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";
import { TOOLS, handleToolCall } from "./tools.js";

const app = express();
app.use(express.json());

const server = new Server(
  {
    name: "cerberus-skills-http",
    version: "1.0.0",
  },
  {
    capabilities: {
      tools: {},
    },
  }
);

import { pluginManager } from "./plugin_manager.js";

server.setRequestHandler(ListToolsRequestSchema, async () => {
  const proxyTools = await pluginManager.proxyListTools();
  return {
    tools: [...TOOLS, ...proxyTools],
  };
});

server.setRequestHandler(CallToolRequestSchema, async (request) => {
  const { name, arguments: args } = request.params;
  if (!args) {
    throw new Error(`Missing arguments for tool ${name}`);
  }
  return await handleToolCall(name, args);
});

let transport: SSEServerTransport;

app.get("/sse", async (req, res) => {
  transport = new SSEServerTransport("/message", res);
  await server.connect(transport);
});

app.post("/message", async (req, res) => {
  if (transport) {
    await transport.handlePostMessage(req, res);
  } else {
    res.status(500).send("Transport not initialized. Connect to /sse first.");
  }
});

const PORT = process.env.PORT || 3001;
app.listen(PORT, async () => {
  await pluginManager.init();
  console.log(`Cerberus Skills HTTP Server running on port ${PORT}`);
  console.log(`SSE endpoint: http://localhost:${PORT}/sse`);
  console.log(`Message endpoint: http://localhost:${PORT}/message`);
});
