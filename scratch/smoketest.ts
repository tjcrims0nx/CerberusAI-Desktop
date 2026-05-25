import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { SSEClientTransport } from "@modelcontextprotocol/sdk/client/sse.js";

async function run() {
    console.log("Connecting to https://api.cerberusai.dev/skills-sse ...");
    const transport = new SSEClientTransport(new URL("https://api.cerberusai.dev/skills-sse"));
    
    const client = new Client(
        { name: "Smoketest", version: "1.0.0" },
        { capabilities: { tools: {} } }
    );

    try {
        await client.connect(transport);
        console.log("Connected successfully!");
        
        console.log("Fetching tools...");
        const response = await client.listTools();
        console.log(`Received ${response.tools.length} tools:`);
        response.tools.forEach(tool => {
            console.log(`- ${tool.name}: ${tool.description?.substring(0, 50)}...`);
        });
        
    } catch (err) {
        console.error("Error during smoketest:", err);
    } finally {
        await client.close();
        console.log("Connection closed.");
    }
}

run();
