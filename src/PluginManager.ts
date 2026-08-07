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

/** Secure-storage key holding the live plugin list. */
const PLUGINS_KEY = "mcp-plugins";

/**
 * Secure-storage key holding a one-time snapshot of the plugin list as it was
 * immediately before the Cerberus→HELIX id migration first rewrote it.
 *
 * Written once and never overwritten, so a later load — including one running a
 * buggy migration — cannot clobber a known-good snapshot.
 */
const LEGACY_BACKUP_KEY = "mcp-plugins.pre-helix-backup";

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
            { name: "HELIX-Desktop", version: "1.0.0" },
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
     * Auto-discover default plugins.
     *
     * Returns the bundled `skills-server` so it shows up in Plugin Settings as a
     * ready-to-toggle entry instead of something the user has to hand-type a
     * command for. It is reported **disabled**: the server exposes
     * `run_command`, `write_file` and `read_url`, so auto-enabling it would give
     * every model arbitrary shell and filesystem access the moment the app
     * starts. Turning it on is an explicit choice, and side-effecting tools are
     * additionally gated per call (see `src/toolPolicy.ts`).
     *
     * The backend returns `null` when the server is not present on disk — an
     * installed build missing its resources, or a dev tree where
     * `skills-server` has not been built yet. In that case nothing is offered,
     * rather than an entry that could only ever fail to start.
     */
    async discoverPlugins(): Promise<PluginConfig[]> {
        try {
            const bundled = await invoke<any | null>("get_bundled_skills_server");
            if (!bundled) return [];
            return [{
                id: bundled.id,
                name: bundled.name,
                command: bundled.command,
                args: bundled.args,
                env: bundled.env,
                cwd: bundled.cwd,
                enabled: false
            }];
        } catch (error) {
            console.error("Failed to locate the bundled skills server:", error);
            return [];
        }
    }

    /**
     * True when a saved entry is a pre-rebrand copy of the bundled skills server.
     *
     * Deliberately narrow: the legacy id alone is not enough, because a user is
     * free to hand-add a plugin and name it anything. Requiring the
     * `skills-server` path signature as well means an unrelated plugin that
     * happens to carry a cerberus-ish id is left completely alone.
     *
     * Shared by the migration itself and by the backup check in
     * `loadWithDiscovered`, so the two can never disagree about what counts as a
     * migratable row — a backup that triggered on a different condition than the
     * rewrite would be worse than no backup at all.
     */
    private isLegacySkillsEntry(p: PluginConfig): boolean {
        const legacyId = p.id === 'cerberus-skills' || p.id.startsWith('cerberus_skills');
        if (!legacyId) return false;
        return !!p.cwd?.includes('skills-server')
            || !!p.args?.some(a => a.includes('skills-server'));
    }

    /**
     * Read the saved plugin list, then fold auto-discovered servers into it.
     *
     * Storage errors propagate. This used to swallow them and fall back to
     * discovery alone, which meant a user whose saved servers failed to load saw
     * a plausible-looking list with their own entries quietly missing. Secure
     * storage is reliable now that the credential-store backend is actually
     * compiled in (see `src-tauri/Cargo.toml`), so a rejection here is a real
     * fault and the caller should show it rather than paper over it.
     */
    async loadWithDiscovered(): Promise<PluginConfig[]> {
        const raw = await invoke<string | null>("db_get_kv", { key: PLUGINS_KEY });

        let saved: PluginConfig[] = [];
        if (raw) {
            const parsed = JSON.parse(raw);
            if (Array.isArray(parsed)) saved = parsed;
        }

        // The legacy-id migration below rewrites rows in place, collapsing a
        // stale pre-rebrand entry into the current one. That is not something a
        // user could reconstruct by hand, so snapshot the original list before
        // the first such rewrite.
        //
        // The backup is taken *only* when a legacy row is actually present, and
        // only if no snapshot exists yet — after the migration has run there is
        // nothing left to match, so the good snapshot is never overwritten by a
        // later load.
        //
        // A failure here is deliberately fatal rather than logged-and-ignored:
        // proceeding would destroy the pre-migration list with nothing to fall
        // back on, which is exactly the failure mode this guard exists to
        // prevent. Better to show the user an error and leave their config
        // untouched than to silently make a one-way change.
        if (raw && saved.some(p => this.isLegacySkillsEntry(p))) {
            const existing = await invoke<string | null>("db_get_kv", { key: LEGACY_BACKUP_KEY });
            if (existing === null || existing === undefined) {
                await invoke("db_set_kv", { key: LEGACY_BACKUP_KEY, value: raw });
            }
        }

        const configs = await this.withDiscovered(saved);
        if (JSON.stringify(configs) !== raw) {
            await invoke("db_set_kv", { key: PLUGINS_KEY, value: JSON.stringify(configs) });
        }
        return configs;
    }

    /**
     * Whether a pre-migration snapshot is available to restore.
     *
     * Read-only probe so the UI can offer recovery without performing it.
     */
    async hasLegacyBackup(): Promise<boolean> {
        try {
            const backup = await invoke<string | null>("db_get_kv", { key: LEGACY_BACKUP_KEY });
            return !!backup;
        } catch {
            // A storage fault here should not break the plugin panel; the worst
            // case is that the restore affordance stays hidden.
            return false;
        }
    }

    /**
     * Restore the plugin list captured before the Cerberus→HELIX migration.
     *
     * Recovery path for the snapshot `loadWithDiscovered` takes. Returns the
     * restored list, or `null` when no snapshot exists (the common case — a user
     * who never had a pre-rebrand entry never triggers a backup).
     *
     * Note the caller is expected to re-sync afterwards: this only rewrites
     * storage, it does not start or stop any client.
     */
    async restoreLegacyBackup(): Promise<PluginConfig[] | null> {
        const backup = await invoke<string | null>("db_get_kv", { key: LEGACY_BACKUP_KEY });
        if (!backup) return null;

        const parsed = JSON.parse(backup);
        if (!Array.isArray(parsed)) return null;

        await invoke("db_set_kv", { key: PLUGINS_KEY, value: backup });
        return parsed as PluginConfig[];
    }

    /**
     * Fold auto-discovered servers into a saved plugin list.
     *
     * Called on every load rather than only on first run, for two reasons.
     * A user who already has saved plugins would otherwise never be offered the
     * built-in server, since the saved list short-circuits discovery. And the
     * bundled server's path is machine-derived — it moves between a dev tree
     * and an installed build, and changes again when the app is reinstalled —
     * so a saved entry can point at a path that no longer exists.
     *
     * Discovery therefore refreshes `command`/`args`/`cwd`/`env` for entries it
     * owns, while everything the user decided (`enabled` above all) is kept.
     * Plugins the user added by hand are untouched.
     */
    async withDiscovered(saved: PluginConfig[]): Promise<PluginConfig[]> {
        const discovered = await this.discoverPlugins();
        if (discovered.length === 0) return saved;

        // Migrate legacy Cerberus Skills id to HELIX Skills id so the saved entry
        // merges with the discovered bundled server instead of duplicating.
        // `isLegacySkillsEntry` is the same test `loadWithDiscovered` uses to
        // decide whether to snapshot the list first — keep them on one predicate.
        const renamed = saved.map(p =>
            this.isLegacySkillsEntry(p)
                ? { ...p, id: 'helix-skills', name: 'HELIX Skills' }
                : p
        );

        // The rename can collide with an existing HELIX Skills entry (a user who
        // has both a stale Cerberus row and a fresh HELIX one). Collapse any
        // duplicate ids down to the first occurrence, keeping the plugin enabled
        // if *any* of its copies was enabled so the user's choice is not lost.
        const byId = new Map<string, PluginConfig>();
        for (const p of renamed) {
            const existing = byId.get(p.id);
            if (existing) {
                existing.enabled = existing.enabled || p.enabled;
            } else {
                byId.set(p.id, { ...p });
            }
        }
        const merged = Array.from(byId.values());

        for (const config of discovered) {
            const index = merged.findIndex(p => p.id === config.id);
            if (index === -1) {
                merged.push(config);
            } else {
                merged[index] = {
                    ...merged[index],
                    command: config.command,
                    args: config.args,
                    env: config.env,
                    cwd: config.cwd
                };
            }
        }
        return merged;
    }
}
