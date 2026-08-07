import express from "express";
import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { SSEServerTransport } from "@modelcontextprotocol/sdk/server/sse.js";
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";
import { TOOLS, handleToolCall } from "./tools.js";
import { pluginManager } from "./plugin_manager.js";

const app = express();
app.use(express.json());

// Track transports per session so multiple clients can connect concurrently.
const sessions = new Map<string, SSEServerTransport>();

function createServer(): Server {
  const server = new Server(
    {
      name: "helix-skills-http",
      version: "1.0.0",
    },
    {
      capabilities: {
        tools: {},
      },
    }
  );

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

  return server;
}

// Health check endpoint.
app.get("/health", (_req, res) => {
  res.json({ status: "ok", server: "helix-skills-http", version: "1.0.0" });
});

app.get("/sse", async (req, res) => {
  const transport = new SSEServerTransport("/message", res);
  const sessionId = transport.sessionId;
  sessions.set(sessionId, transport);

  // Clean up when the client disconnects.
  res.on("close", () => {
    sessions.delete(sessionId);
  });

  const server = createServer();
  await server.connect(transport);
});

app.post("/message", async (req, res) => {
  const sessionId = req.query.sessionId as string;
  const transport = sessionId ? sessions.get(sessionId) : undefined;
  if (transport) {
    try {
      await transport.handlePostMessage(req, res);
    } catch (err: any) {
      if (!res.headersSent) {
        res.status(500).json({ error: err.message });
      }
    }
  } else {
    res.status(400).json({
      error: "Invalid or expired session. Connect to /sse first to establish a session.",
    });
  }
});

const PORT = process.env.PORT || 3001;
app.listen(PORT, async () => {
  await pluginManager.init();
  console.log(`HELIX Skills HTTP Server running on port ${PORT}`);
  console.log(`Health:   http://localhost:${PORT}/health`);
  console.log(`SSE:      http://localhost:${PORT}/sse`);
  console.log(`Message:  http://localhost:${PORT}/message`);
});
