import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { SSEClientTransport } from "@modelcontextprotocol/sdk/client/sse.js";
import { TauriTransport } from "./TauriTransport.js";
import { Tool } from "@modelcontextprotocol/sdk/types.js";

import { invoke } from "@tauri-apps/api/core";

export interface PluginConfig {
    id: string;
    name: string;
    command?: string;
    args?: string[];
    env?: Record<string, string>;
    url?: string;
    cwd?: string;
    enabled: boolean;
    requiresAuth?: boolean;
    verified?: boolean;
    verifiedAt?: string;
    verifyError?: string;
}

export class PluginManager {
    private clients: Map<string, Client> = new Map();
    private apiKey = "";

    constructor() {}

    setApiKey(apiKey: string) {
        this.apiKey = apiKey.trim();
    }

    /**
     * Load plugins dynamically from an .mcp.json file using the Rust API
     */
    async loadFromConfigFile(path: string): Promise<PluginConfig[]> {
        try {
            const discovered = await invoke<any[]>("load_mcp_config", { path });
            const configs = discovered.map(p => ({
                id: p.id,
                name: p.name,
                command: p.command,
                args: p.args,
                env: p.env,
                url: p.url,
                cwd: p.cwd,
                enabled: false // Users must explicitly enable newly discovered plugins
            }));
            return configs;
        } catch (error) {
            console.error(`Failed to load config from ${path}:`, error);
            return [];
        }
    }

    /**
     * Load plugins from configuration array
     */
    async loadPlugins(configs: PluginConfig[]) {
        for (const config of configs) {
            if (config.enabled) {
                try {
                    await this.startPlugin(config);
                } catch (error) {
                    console.warn(`Failed to sync plugin ${config.id}:`, error);
                }
            }
        }
    }

    /**
     * Make active clients match the saved plugin config exactly.
     */
    async syncPlugins(configs: PluginConfig[]) {
        const enabled = new Set(configs.filter(config => config.enabled).map(config => config.id));
        for (const pluginId of this.activePlugins) {
            if (!enabled.has(pluginId)) {
                await this.stopPlugin(pluginId);
            }
        }

        for (const config of configs) {
            if (config.enabled) {
                try {
                    await this.startPlugin(config);
                } catch (error) {
                    console.warn(`Failed to start plugin ${config.id} (${config.name}):`, error);
                }
            }
        }
    }

    /**
     * Starts a single plugin and initializes its MCP Client
     */
    async startPlugin(config: PluginConfig): Promise<void> {
        if (this.clients.has(config.id)) {
            console.warn(`Plugin ${config.id} is already running.`);
            return;
        }

        let transport;
        if (config.url) {
            const headers: Record<string, string> = {};
            if (config.requiresAuth && this.apiKey) {
                headers.Authorization = `Bearer ${this.apiKey}`;
            }
            transport = new SSEClientTransport(new URL(config.url), {
                requestInit: Object.keys(headers).length > 0 ? { headers } : undefined
            });
        } else if (config.command && config.args) {
            transport = new TauriTransport(config.id, config.command, config.args, config.env, config.cwd);
        } else {
            throw new Error(`Plugin ${config.name} must have either a URL or a command`);
        }

        const client = new Client(
            { name: "Cerberus-Desktop", version: "1.0.0" },
            { capabilities: { prompts: {}, resources: {}, tools: {} } as any }
        );

        try {
            await client.connect(transport);
            this.clients.set(config.id, client);
            console.log(`Plugin ${config.name} connected successfully.`);
        } catch (error) {
            console.error(`Failed to connect to plugin ${config.name}:`, error);
            throw error;
        }
    }

    /**
     * Stops a running plugin
     */
    async stopPlugin(pluginId: string): Promise<void> {
        const client = this.clients.get(pluginId);
        if (client) {
            await client.close();
            this.clients.delete(pluginId);
            console.log(`Plugin ${pluginId} stopped.`);
        }
    }

    /**
     * Gets all available tools across all active plugins
     */
    async getAllTools(): Promise<Array<{ pluginId: string, tool: Tool }>> {
        const allTools: Array<{ pluginId: string, tool: Tool }> = [];

        for (const [pluginId, client] of this.clients.entries()) {
            try {
                const response = await client.listTools();
                for (const tool of response.tools) {
                    allTools.push({ pluginId, tool });
                }
            } catch (error) {
                console.error(`Failed to list tools for plugin ${pluginId}:`, error);
            }
        }

        return allTools;
    }

    /**
     * Execute a tool provided by a specific plugin
     */
    async callTool(pluginId: string, toolName: string, args: Record<string, any>): Promise<any> {
        const client = this.clients.get(pluginId);
        if (!client) {
            throw new Error(`Plugin ${pluginId} is not active or not found.`);
        }

        return await client.callTool({
            name: toolName,
            arguments: args
        });
    }

    get activePlugins(): string[] {
        return Array.from(this.clients.keys());
    }

    /**
     * Auto-discover default plugins, such as the Cerberus Cloud Skills server
     */
    async discoverPlugins(): Promise<PluginConfig[]> {
        return [];
    }
}
