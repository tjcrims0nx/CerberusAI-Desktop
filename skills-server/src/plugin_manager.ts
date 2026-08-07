import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import fs from "fs/promises";
import os from "os";
import path from "path";
import { execFile } from "child_process";
import util from "util";
import { CallToolResultSchema, ListToolsResultSchema } from "@modelcontextprotocol/sdk/types.js";

const execFileAsync = util.promisify(execFile);
const npmCommand = process.platform === "win32" ? "npm.cmd" : "npm";

export interface PluginInfo {
    name: string;
    description: string;
    url: string;
}

/** A `SKILL.md` loaded from disk and exposed as a single MCP tool. */
interface LoadedSkill {
    toolName: string;
    name: string;
    description: string;
    markdown: string;
    filePath: string;
}

export class PluginManager {
    public clients: Map<string, Client> = new Map();
    public pluginTools: Map<string, any[]> = new Map(); // ClientID -> Tool[]
    private skills: Map<string, LoadedSkill> = new Map(); // toolName -> skill
    private pluginsDir: string;

    constructor() {
        // The app installs skills under the user's home directory (see
        // `app_plugins_dir` in src-tauri/src/mcp_bridge.rs, which uses the same
        // convention). `process.cwd()` is whatever directory the server happened
        // to be launched from, so resolving against it found nothing.
        this.pluginsDir =
            process.env.HELIX_PLUGINS_DIR ||
            path.join(os.homedir(), ".HELIX", "plugins");
    }

    async init() {
        await fs.mkdir(this.pluginsDir, { recursive: true });
        await this.loadInstalledPlugins();
    }

    async fetchAwesomeSkills(): Promise<PluginInfo[]> {
        const res = await fetch("https://awesome-skills.com/");
        if (!res.ok) {
            throw new Error(`awesome-skills.com returned HTTP ${res.status}`);
        }
        const html = await res.text();
        const jsonLdMatches = html.match(/<script type="application\/ld\+json">([\s\S]*?)<\/script>/g);
        let skills: PluginInfo[] = [];

        if (jsonLdMatches) {
            for (const match of jsonLdMatches) {
                const jsonStr = match.replace(/<script type="application\/ld\+json">|<\/script>/g, "").trim();
                try {
                    const data = JSON.parse(jsonStr);
                    if (data["@type"] === "ItemList" && data.itemListElement) {
                        for (const item of data.itemListElement) {
                            if (item.item) {
                                skills.push({
                                    name: item.item.name,
                                    description: item.item.description,
                                    url: item.item.url
                                });
                            }
                        }
                    }
                } catch (e) {
                    console.error("Error parsing JSON-LD", e);
                }
            }
        }
        return skills;
    }

    async installPlugin(githubUrl: string, name: string): Promise<string> {
        const safeName = name.replace(/[^a-zA-Z0-9_-]/g, "_");
        if (!safeName) {
            throw new Error("Plugin name must include at least one letter, number, dash, or underscore.");
        }
        const parsedUrl = new URL(githubUrl);
        if (parsedUrl.protocol !== "https:" || parsedUrl.hostname !== "github.com") {
            throw new Error("Only HTTPS GitHub plugin URLs are supported.");
        }
        const targetDir = path.join(this.pluginsDir, safeName);

        try {
            const stat = await fs.stat(targetDir);
            if (stat.isDirectory()) {
                return `Plugin ${name} is already installed at ${targetDir}`;
            }
        } catch (e) {
            // Does not exist, proceed
        }

        try {
            await execFileAsync("git", ["clone", githubUrl, targetDir]);
            // Attempt to install dependencies if package.json exists
            try {
                await fs.stat(path.join(targetDir, "package.json"));
                await execFileAsync(npmCommand, ["install"], { cwd: targetDir });
                // Attempt build if there's a build script
                const pkgJson = JSON.parse(await fs.readFile(path.join(targetDir, "package.json"), "utf-8"));
                if (pkgJson.scripts && pkgJson.scripts.build) {
                    await execFileAsync(npmCommand, ["run", "build"], { cwd: targetDir });
                }
            } catch (e) {
                // Ignore if no package.json
            }

            // Attempt to load it
            await this.loadPlugin(targetDir, safeName);
            return `Successfully installed and loaded plugin: ${name}`;
        } catch (e: any) {
            throw new Error(`Failed to install plugin ${name}: ${e.message}`);
        }
    }

    async loadInstalledPlugins() {
        try {
            const files = await fs.readdir(this.pluginsDir, { withFileTypes: true });
            for (const file of files) {
                if (file.isDirectory()) {
                    await this.loadPlugin(path.join(this.pluginsDir, file.name), file.name).catch(e => {
                        console.error(`Failed to load plugin ${file.name}:`, e);
                    });
                }
            }
        } catch (e) {
            console.error("Error loading installed plugins", e);
        }
    }

    private async findSkillFiles(dir: string): Promise<string[]> {
        const found: string[] = [];
        const walk = async (current: string) => {
            const entries = await fs.readdir(current, { withFileTypes: true });
            for (const entry of entries) {
                if (entry.name === "node_modules" || entry.name === ".git") continue;
                const fullPath = path.join(current, entry.name);
                if (entry.isDirectory()) {
                    await walk(fullPath);
                } else if (entry.isFile() && entry.name.toLowerCase() === "skill.md") {
                    found.push(fullPath);
                }
            }
        };
        await walk(dir);
        return found;
    }

    private slugify(value: string): string {
        return String(value || "skill")
            .toLowerCase()
            .replace(/[^a-z0-9]+/g, "_")
            .replace(/^_+|_+$/g, "")
            .slice(0, 48) || "skill";
    }

    private parseSkill(markdown: string, filePath: string): LoadedSkill {
        const frontmatter = markdown.match(/^---\s*\n([\s\S]*?)\n---\s*\n?/);
        const meta: Record<string, string> = {};
        if (frontmatter) {
            for (const line of frontmatter[1].split("\n")) {
                const match = line.match(/^([A-Za-z0-9_-]+):\s*(.*)$/);
                if (match) meta[match[1].toLowerCase()] = match[2].replace(/^["']|["']$/g, "").trim();
            }
        }
        const body = markdown.replace(/^---\s*\n[\s\S]*?\n---\s*\n?/, "").trim();
        const heading = body.match(/^#\s+(.+)$/m);
        const name = meta.name || (heading ? heading[1].trim() : path.basename(path.dirname(filePath)));
        const description =
            meta.description ||
            body.split("\n").find((line) => line.trim() && !line.startsWith("#")) ||
            "HELIX-compatible converted agent skill.";

        return {
            toolName: "skill_" + this.slugify(name),
            name,
            description: description.slice(0, 480),
            markdown,
            filePath,
        };
    }

    /**
     * Load every `SKILL.md` under `pluginDir` into this process.
     *
     * Skills are plain markdown, so there is nothing to execute — they are read
     * and served directly rather than by spawning a child MCP server. An earlier
     * version wrote a generated wrapper script into the plugin directory and ran
     * it with `node`, but the wrapper imported `@modelcontextprotocol/sdk` and
     * installed skills have no `node_modules` anywhere up their tree, so it could
     * only ever fail with ERR_MODULE_NOT_FOUND. Loading in-process also avoids
     * writing files into the user's plugin directory.
     *
     * @returns how many `SKILL.md` files were seen and how many were newly
     *   registered. These differ whenever a directory holds only skills another
     *   directory already contributed, which is the normal case: `installPlugin`
     *   clones the whole source repository per skill, so four installs from
     *   anthropics/skills produce four identical copies of all of them. Callers
     *   need `found` to tell "this directory has no skills" apart from "this
     *   directory's skills were all duplicates" — the first is a load failure,
     *   the second is success.
     */
    private async loadSkillsFrom(pluginDir: string): Promise<{ found: number; added: number }> {
        const skillFiles = await this.findSkillFiles(pluginDir);
        let added = 0;
        for (const filePath of skillFiles) {
            try {
                const markdown = await fs.readFile(filePath, "utf-8");
                const skill = this.parseSkill(markdown, filePath);

                // The first copy to claim a name wins; registering every copy
                // would flood the model with dozens of identical tools. A
                // genuinely different skill that collides on the slug gets a
                // numeric suffix so it stays reachable.
                const existing = this.skills.get(skill.toolName);
                if (existing) {
                    if (existing.markdown === skill.markdown) continue;
                    let toolName = skill.toolName;
                    for (let n = 2; this.skills.has(toolName); n += 1) {
                        toolName = `${skill.toolName}_${n}`;
                    }
                    this.skills.set(toolName, { ...skill, toolName });
                } else {
                    this.skills.set(skill.toolName, skill);
                }
                added += 1;
            } catch (e: any) {
                console.error(`Failed to read skill ${filePath}: ${e.message}`);
            }
        }
        return { found: skillFiles.length, added };
    }

    private async connectPlugin(id: string, command: string, args: string[], cwd: string) {
        const transport = new StdioClientTransport({
            command,
            args,
            cwd,
            stderr: "inherit"
        });

        const client = new Client({
            name: "helix-aggregator",
            version: "1.0.0"
        }, {
            capabilities: {}
        });

        await client.connect(transport);
        const toolsResult = await client.request({ method: "tools/list" }, ListToolsResultSchema) as any;
        this.clients.set(id, client);
        if (toolsResult && toolsResult.tools) {
            this.pluginTools.set(id, toolsResult.tools);
        }
    }

    private async loadPlugin(pluginDir: string, id: string) {
        // A real MCP server declares an entry point in package.json and is spawned
        // as a child process. Anything else is treated as a collection of
        // `SKILL.md` files and loaded in-process.
        let args: string[] = [];

        try {
            const pkgPath = path.join(pluginDir, "package.json");
            const pkgStr = await fs.readFile(pkgPath, "utf-8");
            const pkg = JSON.parse(pkgStr);

            if (pkg.bin) {
                const binPath = typeof pkg.bin === "string" ? pkg.bin : Object.values(pkg.bin)[0];
                args = [path.join(pluginDir, binPath as string)];
            } else if (pkg.main) {
                args = [path.join(pluginDir, pkg.main)];
            } else {
                throw new Error("No main or bin found in package.json");
            }
        } catch (e) {
            const { found, added } = await this.loadSkillsFrom(pluginDir);
            if (found > 0) {
                console.error(
                    `Loaded ${added} new skill(s) from ${id}` +
                    (added < found ? ` (${found - added} already provided by another plugin)` : "")
                );
                return;
            }
            throw new Error(`${id} has neither an MCP entry point nor any SKILL.md files`);
        }

        try {
            await this.connectPlugin(id, "node", args, pluginDir);
        } catch (error) {
            // The declared entry point failed to start. If the directory also
            // ships skills, fall back to those rather than losing the plugin.
            const { found, added } = await this.loadSkillsFrom(pluginDir);
            if (found > 0) {
                console.error(`${id} failed to start as an MCP server; loaded ${added} skill(s) instead`);
                return;
            }
            throw error;
        }
    }

    async proxyListTools(): Promise<any[]> {
        let allTools: any[] = [];
        for (const [id, tools] of this.pluginTools.entries()) {
            allTools = allTools.concat(tools);
        }
        // Skills loaded in-process are advertised alongside proxied server tools.
        for (const skill of this.skills.values()) {
            allTools.push({
                name: skill.toolName,
                description: skill.description,
                inputSchema: {
                    type: "object",
                    properties: {
                        prompt: { type: "string", description: "The user's current task or question." },
                        context: { type: "string", description: "Optional local context to apply the skill to." }
                    }
                }
            });
        }
        return allTools;
    }

    async proxyCallTool(name: string, args: any): Promise<any> {
        const skill = this.skills.get(name);
        if (skill) {
            const text = [
                "# HELIX Skill",
                "Name: " + skill.name,
                "Source: " + skill.filePath,
                "",
                "Use the following skill instructions to complete the user's task.",
                "",
                skill.markdown,
                args?.prompt ? "\n## User task\n" + args.prompt : "",
                args?.context ? "\n## Context\n" + args.context : ""
            ].filter(Boolean).join("\n");
            return { content: [{ type: "text", text }] };
        }

        // Find which client owns this tool
        for (const [id, tools] of this.pluginTools.entries()) {
            if (tools.find(t => t.name === name)) {
                const client = this.clients.get(id);
                if (client) {
                    return await client.request(
                        { method: "tools/call", params: { name, arguments: args } },
                        CallToolResultSchema
                    );
                }
            }
        }
        throw new Error(`Tool ${name} not found in any loaded plugins.`);
    }
}

export const pluginManager = new PluginManager();
