import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";
import { TOOLS, handleToolCall } from "./tools.js";

const server = new Server(
  {
    name: "cerberus-skills",
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

async function main() {
  await pluginManager.init();
  const transport = new StdioServerTransport();
  await server.connect(transport);
  console.error("Cerberus Skills MCP Server running on stdio");
}

main().catch((error) => {
  console.error("Server error:", error);
  process.exit(1);
});
