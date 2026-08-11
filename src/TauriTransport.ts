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
    private unlistenClose: UnlistenFn | null = null;
    private closed = false;

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

        // The backend emits `mcp-close` when the child's stdout pipe ends, i.e.
        // the process exited. Surface it as a transport error so the SDK rejects
        // any pending request — a server that dies mid-handshake fails now
        // instead of after the 60s request timeout.
        this.unlistenClose = await listen<{plugin_id: string}>(
            "mcp-close",
            (event) => {
                if (event.payload.plugin_id === this.pluginId) {
                    this.handleClose();
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

    /**
     * The backend reported the child process exited. Notify the SDK exactly
     * once — `onerror` unblocks a pending request (the handshake), `onclose`
     * marks the transport dead — then drop our listeners so a later `close()`
     * from the client is a no-op. Guarded by `closed` so a normal shutdown and
     * an unexpected exit can't both fire the callbacks.
     */
    private handleClose() {
        if (this.closed) return;
        this.closed = true;

        this.unlistenListeners();

        this.onerror?.(new Error(`MCP server ${this.pluginId} exited`));
        this.onclose?.();
    }

    private unlistenListeners() {
        if (this.unlistenStdout) {
            this.unlistenStdout();
            this.unlistenStdout = null;
        }
        if (this.unlistenStderr) {
            this.unlistenStderr();
            this.unlistenStderr = null;
        }
        if (this.unlistenClose) {
            this.unlistenClose();
            this.unlistenClose = null;
        }
    }

    async close(): Promise<void> {
        if (this.closed) return;
        this.closed = true;

        try {
            await invoke("kill_mcp_server", { pluginId: this.pluginId });
        } catch (error) {
            console.error(`Failed to kill MCP server ${this.pluginId}:`, error);
        }

        this.unlistenListeners();

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
