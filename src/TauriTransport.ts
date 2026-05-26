import { Transport } from "@modelcontextprotocol/sdk/shared/transport.js";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { JSONRPCMessage } from "@modelcontextprotocol/sdk/types.js";

/**
 * TauriTransport implements the MCP Transport interface by routing JSON-RPC
 * messages through Tauri's IPC to a Rust backend sidecar process.
 */
export class TauriTransport implements Transport {
    private pluginId: string;
    private unlistenStdout: UnlistenFn | null = null;
    private unlistenStderr: UnlistenFn | null = null;

    onclose?: () => void;
    onerror?: (error: Error) => void;
    onmessage?: (message: JSONRPCMessage) => void;

    /**
     * @param pluginId Unique identifier for this plugin instance
     * @param command The executable to run (e.g., "node", "python", or an absolute path)
     * @param args Arguments to pass to the executable
     * @param env Optional environment variables
     */
    constructor(
        pluginId: string,
        private command: string,
        private args: string[],
        private env?: Record<string, string>,
        private cwd?: string
    ) {
        this.pluginId = pluginId;
    }

    async start(): Promise<void> {
        // Set up event listeners for stdout and stderr from Rust
        this.unlistenStdout = await listen<{plugin_id: string, message: string}>(
            "mcp-stdout",
            (event) => {
                if (event.payload.plugin_id === this.pluginId) {
                    this.handleStdout(event.payload.message);
                }
            }
        );

        this.unlistenStderr = await listen<{plugin_id: string, error: string}>(
            "mcp-stderr",
            (event) => {
                if (event.payload.plugin_id === this.pluginId) {
                    console.error(`[MCP ${this.pluginId} STDERR]:`, event.payload.error);
                }
            }
        );

        // Spawn the server process via Rust backend
        try {
            await invoke("spawn_mcp_server", {
                pluginId: this.pluginId,
                command: this.command,
                args: this.args,
                env: this.env,
                cwd: this.cwd
            });
        } catch (error) {
            this.onerror?.(new Error(`Failed to start MCP server: ${error}`));
            throw error;
        }
    }

    private handleStdout(line: string) {
        try {
            const message = JSON.parse(line) as JSONRPCMessage;
            this.onmessage?.(message);
        } catch (error) {
            // Not JSON, might be a regular log line
            console.log(`[MCP ${this.pluginId} LOG]:`, line);
        }
    }

    async close(): Promise<void> {
        try {
            await invoke("kill_mcp_server", { pluginId: this.pluginId });
        } catch (error) {
            console.error(`Failed to kill MCP server ${this.pluginId}:`, error);
        }

        if (this.unlistenStdout) {
            this.unlistenStdout();
            this.unlistenStdout = null;
        }
        if (this.unlistenStderr) {
            this.unlistenStderr();
            this.unlistenStderr = null;
        }

        this.onclose?.();
    }

    async send(message: JSONRPCMessage): Promise<void> {
        try {
            const messageStr = JSON.stringify(message);
            await invoke("send_mcp_message", {
                pluginId: this.pluginId,
                message: messageStr
            });
        } catch (error) {
            this.onerror?.(new Error(`Failed to send message: ${error}`));
            throw error;
        }
    }
}
