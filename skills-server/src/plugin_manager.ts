import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import fs from "fs/promises";
import path from "path";
import { execFile } from "child_process";
import util from "util";
import { CallToolRequestSchema, ListToolsRequestSchema } from "@modelcontextprotocol/sdk/types.js";

const execFileAsync = util.promisify(execFile);
const npmCommand = process.platform === "win32" ? "npm.cmd" : "npm";

export interface PluginInfo {
    name: string;
    description: string;
    url: string;
}

export class PluginManager {
    public clients: Map<string, Client> = new Map();
    public pluginTools: Map<string, any[]> = new Map(); // ClientID -> Tool[]
    private pluginsDir: string;
    private wrapperFile = "cerberus-skill-wrapper.mjs";

    constructor() {
        this.pluginsDir = path.join(process.cwd(), "plugins");
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

    private async writeSkillWrapper(pluginDir: string): Promise<string> {
        const wrapperPath = path.join(pluginDir, this.wrapperFile);
        const wrapperSource = `#!/usr/bin/env node
import fs from "node:fs/promises";
import path from "node:path";
import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { CallToolRequestSchema, ListToolsRequestSchema } from "@modelcontextprotocol/sdk/types.js";

const root = process.cwd();

function slug(value) {
  return String(value || "skill")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "")
    .slice(0, 48) || "skill";
}

function parseSkill(markdown, filePath) {
  const frontmatter = markdown.match(/^---\\s*\\n([\\s\\S]*?)\\n---\\s*\\n?/);
  const meta = {};
  if (frontmatter) {
    for (const line of frontmatter[1].split("\\n")) {
      const match = line.match(/^([A-Za-z0-9_-]+):\\s*(.*)$/);
      if (match) meta[match[1].toLowerCase()] = match[2].replace(/^["']|["']$/g, "").trim();
    }
  }
  const body = markdown.replace(/^---\\s*\\n[\\s\\S]*?\\n---\\s*\\n?/, "").trim();
  const heading = body.match(/^#\\s+(.+)$/m);
  const name = meta.name || (heading ? heading[1].trim() : path.basename(path.dirname(filePath)));
  const description = meta.description || body.split("\\n").find((line) => line.trim() && !line.startsWith("#")) || "Cerberus-compatible converted agent skill.";
  return { name, description: description.slice(0, 480), markdown, filePath };
}

async function walk(current, found = []) {
  const entries = await fs.readdir(current, { withFileTypes: true });
  for (const entry of entries) {
    if (entry.name === "node_modules" || entry.name === ".git") continue;
    const fullPath = path.join(current, entry.name);
    if (entry.isDirectory()) await walk(fullPath, found);
    else if (entry.isFile() && entry.name.toLowerCase() === "skill.md") found.push(fullPath);
  }
  return found;
}

const skillFiles = await walk(root);
const skills = [];
for (const filePath of skillFiles) {
  const markdown = await fs.readFile(filePath, "utf8");
  const parsed = parseSkill(markdown, filePath);
  skills.push({
    ...parsed,
    toolName: "skill_" + slug(parsed.name)
  });
}

const server = new Server(
  { name: "cerberus-converted-skills", version: "1.0.0" },
  { capabilities: { tools: {} } }
);

server.setRequestHandler(ListToolsRequestSchema, async () => ({
  tools: skills.map((skill) => ({
    name: skill.toolName,
    description: skill.description,
    inputSchema: {
      type: "object",
      properties: {
        prompt: { type: "string", description: "The user's current task or question." },
        context: { type: "string", description: "Optional local context to apply the skill to." }
      }
    }
  }))
}));

server.setRequestHandler(CallToolRequestSchema, async (request) => {
  const skill = skills.find((item) => item.toolName === request.params.name);
  if (!skill) throw new Error("Unknown converted skill: " + request.params.name);
  const args = request.params.arguments || {};
  const text = [
    "# Converted Cerberus Skill",
    "Name: " + skill.name,
    "Source: " + path.relative(root, skill.filePath),
    "",
    "Use the following skill instructions to complete the user's task inside Cerberus.",
    "",
    skill.markdown,
    args.prompt ? "\\n## User task\\n" + args.prompt : "",
    args.context ? "\\n## Context\\n" + args.context : ""
  ].filter(Boolean).join("\\n");
  return { content: [{ type: "text", text }] };
});

await server.connect(new StdioServerTransport());
`;
        await fs.writeFile(wrapperPath, wrapperSource, "utf-8");
        return wrapperPath;
    }

    private async connectPlugin(id: string, command: string, args: string[], cwd: string) {
        const transport = new StdioClientTransport({
            command,
            args,
            cwd,
            stderr: "inherit"
        });

        const client = new Client({
            name: "cerberus-aggregator",
            version: "1.0.0"
        }, {
            capabilities: {}
        });

        await client.connect(transport);
        const toolsResult = await client.request({ method: "tools/list" }, ListToolsRequestSchema) as any;
        this.clients.set(id, client);
        if (toolsResult && toolsResult.tools) {
            this.pluginTools.set(id, toolsResult.tools);
        }
    }

    private async loadPlugin(pluginDir: string, id: string) {
        // Simple heuristic: if there's a package.json, run `npm start` or find main
        // Better: just use npx or node directly if we know the entry
        // For now, let's look for package.json
        let command = "node";
        let args: string[] = [];
        let usingConvertedWrapper = false;

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
            const skillFiles = await this.findSkillFiles(pluginDir);
            if (skillFiles.length > 0) {
                const wrapperPath = await this.writeSkillWrapper(pluginDir);
                command = "node";
                args = [wrapperPath];
                usingConvertedWrapper = true;
            } else {
                // Fallback to npx using the directory if package.json failed
                command = "npx";
                args = ["-y", "."];
            }
        }

        try {
            await this.connectPlugin(id, command, args, pluginDir);
        } catch (error) {
            const skillFiles = await this.findSkillFiles(pluginDir);
            if (!usingConvertedWrapper && skillFiles.length > 0) {
                const wrapperPath = await this.writeSkillWrapper(pluginDir);
                await this.connectPlugin(id, "node", [wrapperPath], pluginDir);
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
        return allTools;
    }

    async proxyCallTool(name: string, args: any): Promise<any> {
        // Find which client owns this tool
        for (const [id, tools] of this.pluginTools.entries()) {
            if (tools.find(t => t.name === name)) {
                const client = this.clients.get(id);
                if (client) {
                    return await client.request(
                        { method: "tools/call", params: { name, arguments: args } },
                        CallToolRequestSchema
                    );
                }
            }
        }
        throw new Error(`Tool ${name} not found in any loaded plugins.`);
    }
}

export const pluginManager = new PluginManager();
