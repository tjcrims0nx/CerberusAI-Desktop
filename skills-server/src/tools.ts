import fs from "fs/promises";
import util from "util";
import { exec } from "child_process";
import puppeteer from "puppeteer";

const execAsync = util.promisify(exec);

export const TOOLS = [
  {
    name: "read_file",
    description: "Read contents of a file from the local filesystem.",
    inputSchema: {
      type: "object",
      properties: { path: { type: "string" } },
      required: ["path"],
    },
  },
  {
    name: "write_file",
    description: "Write content to a file. Overwrites the file if it exists.",
    inputSchema: {
      type: "object",
      properties: { path: { type: "string" }, content: { type: "string" } },
      required: ["path", "content"],
    },
  },
  {
    name: "replace_in_file",
    description: "Replace a specific string in a file.",
    inputSchema: {
      type: "object",
      properties: {
        path: { type: "string" },
        targetContent: { type: "string" },
        replacementContent: { type: "string" }
      },
      required: ["path", "targetContent", "replacementContent"],
    },
  },
  {
    name: "list_dir",
    description: "List directory contents including files and subdirectories.",
    inputSchema: {
      type: "object",
      properties: { path: { type: "string" } },
      required: ["path"],
    },
  },
  {
    name: "run_command",
    description: "Run a shell command (bash/powershell) in a specific directory.",
    inputSchema: {
      type: "object",
      properties: { command: { type: "string" }, cwd: { type: "string" } },
      required: ["command"],
    },
  },
  {
    name: "read_url",
    description: "Read and extract text content from a URL using a headless browser.",
    inputSchema: {
      type: "object",
      properties: { url: { type: "string" } },
      required: ["url"],
    },
  },
  {
    name: "list_awesome_skills",
    description: "Fetch a list of available MCP plugins from awesome-skills.com.",
    inputSchema: {
      type: "object",
      properties: {},
      required: [],
    },
  },
  {
    name: "install_awesome_skill",
    description: "Install an MCP plugin from awesome-skills.com using its GitHub URL.",
    inputSchema: {
      type: "object",
      properties: {
        url: { type: "string", description: "The GitHub URL of the plugin" },
        name: { type: "string", description: "A unique name for the plugin" }
      },
      required: ["url", "name"],
    },
  }
];

import { pluginManager } from "./plugin_manager.js";

export async function handleToolCall(name: string, args: any): Promise<any> {
  try {
    switch (name) {
      case "read_file": {
        const content = await fs.readFile(args.path, "utf-8");
        return { content: [{ type: "text", text: content }] };
      }
      case "write_file": {
        await fs.writeFile(args.path, args.content, "utf-8");
        return { content: [{ type: "text", text: `Successfully wrote to ${args.path}` }] };
      }
      case "replace_in_file": {
        let content = await fs.readFile(args.path, "utf-8");
        if (!content.includes(args.targetContent)) {
            throw new Error("targetContent not found in file.");
        }
        content = content.replace(args.targetContent, args.replacementContent);
        await fs.writeFile(args.path, content, "utf-8");
        return { content: [{ type: "text", text: `Successfully replaced content in ${args.path}` }] };
      }
      case "list_dir": {
        const files = await fs.readdir(args.path, { withFileTypes: true });
        const list = files.map(f => `${f.isDirectory() ? "[DIR] " : "[FILE]"} ${f.name}`).join("\n");
        return { content: [{ type: "text", text: list || "(empty directory)" }] };
      }
      case "run_command": {
        const { stdout, stderr } = await execAsync(args.command, { cwd: args.cwd || process.cwd() });
        let text = "";
        if (stdout) text += `STDOUT:\n${stdout}\n`;
        if (stderr) text += `STDERR:\n${stderr}\n`;
        return { content: [{ type: "text", text: text || "Command executed successfully with no output." }] };
      }
      case "read_url": {
        const browser = await puppeteer.launch({ headless: true });
        const page = await browser.newPage();
        await page.goto(args.url, { waitUntil: "networkidle2" });
        const text = await page.evaluate(() => document.body.innerText);
        await browser.close();
        return { content: [{ type: "text", text: text.substring(0, 10000) }] };
      }
      case "list_awesome_skills": {
        const skills = await pluginManager.fetchAwesomeSkills();
        const text = skills.map(s => `- **${s.name}**: ${s.description}\n  URL: ${s.url}`).join("\n\n");
        return { content: [{ type: "text", text: text || "No skills found." }] };
      }
      case "install_awesome_skill": {
        const result = await pluginManager.installPlugin(args.url, args.name);
        return { content: [{ type: "text", text: result }] };
      }
      default: {
        try {
          return await pluginManager.proxyCallTool(name, args);
        } catch (err: any) {
          throw new Error(`Unknown tool: ${name}. Proxy error: ${err.message}`);
        }
      }
    }
  } catch (e: any) {
    return { content: [{ type: "text", text: `Error: ${e.message}` }], isError: true };
  }
}
