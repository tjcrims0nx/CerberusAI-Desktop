// Drives the built skills-server over stdio exactly as the app's TauriTransport
// does: initialize -> initialized -> tools/list -> tools/call. Verifies the server
// starts, advertises its built-in tools plus the SKILL.md files found in the
// plugins directory, and can actually execute one of each kind.
//
// Usage: node scratch/mcp-handshake.mjs
import { spawn } from "node:child_process";
import path from "node:path";
import os from "node:os";
import fs from "node:fs";

const SERVER = path.resolve("skills-server/dist/index.js");
const helixPlugins = path.join(os.homedir(), ".HELIX", "plugins");
const cerberusPlugins = path.join(os.homedir(), ".CerberusAI", "plugins");
let defaultPlugins = fs.existsSync(helixPlugins) ? helixPlugins : cerberusPlugins;

if (!fs.existsSync(defaultPlugins)) {
  const tempPlugin = path.join(os.tmpdir(), "helix-ci-plugins", "smoke_test_plugin");
  fs.mkdirSync(tempPlugin, { recursive: true });
  fs.writeFileSync(
    path.join(tempPlugin, "SKILL.md"),
    "---\nname: CI Smoke Test Skill\ndescription: Skill for CI testing\n---\n# HELIX Skill\n\nInstructions for smoke test."
  );
  defaultPlugins = path.dirname(tempPlugin);
}

const PLUGINS = process.env.HELIX_PLUGINS_DIR || process.env.CERBERUS_PLUGINS_DIR || defaultPlugins;

const child = spawn(process.execPath, [SERVER], {
  stdio: ["pipe", "pipe", "pipe"],
  env: { ...process.env, HELIX_PLUGINS_DIR: PLUGINS, CERBERUS_PLUGINS_DIR: PLUGINS },
});

const stderr = [];
child.stderr.on("data", (d) => stderr.push(d.toString()));

const pending = new Map();
let buf = "";
child.stdout.on("data", (d) => {
  buf += d.toString();
  let nl;
  while ((nl = buf.indexOf("\n")) !== -1) {
    const line = buf.slice(0, nl).trim();
    buf = buf.slice(nl + 1);
    if (!line) continue;
    let msg;
    try { msg = JSON.parse(line); } catch { continue; }
    if (msg.id !== undefined && pending.has(msg.id)) {
      pending.get(msg.id)(msg);
      pending.delete(msg.id);
    }
  }
});

let nextId = 1;
function request(method, params) {
  const id = nextId++;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`timeout waiting for ${method}`)), 30000);
    pending.set(id, (msg) => { clearTimeout(timer); resolve(msg); });
    child.stdin.write(JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n");
  });
}
function notify(method, params) {
  child.stdin.write(JSON.stringify({ jsonrpc: "2.0", method, params }) + "\n");
}

let failures = 0;
function check(name, cond, detail = "") {
  if (!cond) failures++;
  console.log(`${cond ? "PASS" : "FAIL"}  ${name}${detail ? "  — " + detail : ""}`);
}

try {
  const init = await request("initialize", {
    protocolVersion: "2024-11-05",
    capabilities: {},
    clientInfo: { name: "handshake-smoketest", version: "1.0.0" },
  });
  check("initialize returns a result", !!init.result, init.error ? JSON.stringify(init.error) : "");
  const serverName = init.result?.serverInfo?.name || "";
  check("server identifies itself", serverName === "helix-skills" || serverName === "cerberus-skills", serverName);

  notify("notifications/initialized", {});

  const list = await request("tools/list", {});
  const tools = list.result?.tools ?? [];
  const names = tools.map((t) => t.name);
  check("tools/list returns tools", tools.length > 0, `${tools.length} tools`);

  const builtins = ["read_file", "write_file", "replace_in_file", "list_dir", "run_command", "read_url"];
  for (const b of builtins) check(`built-in present: ${b}`, names.includes(b));

  const skillTools = names.filter((n) => n.startsWith("skill_"));
  check("SKILL.md files were converted to tools", skillTools.length > 0,
    skillTools.length ? skillTools.join(", ") : `none found in ${PLUGINS}`);

  check("every tool has an inputSchema", tools.every((t) => t.inputSchema && t.inputSchema.type === "object"));

  // Execute a safe built-in.
  const targetDir = fs.existsSync(PLUGINS) ? PLUGINS : os.homedir();
  const dir = await request("tools/call", { name: "list_dir", arguments: { path: targetDir } });
  const dirText = dir.result?.content?.[0]?.text ?? "";
  check("list_dir executes", !!dirText && !dir.result?.isError, dirText.split("\n")[0]);

  // Execute a converted skill, if any were found.
  if (skillTools.length) {
    const call = await request("tools/call", {
      name: skillTools[0],
      arguments: { prompt: "smoke test" },
    });
    const text = call.result?.content?.[0]?.text ?? "";
    check(`${skillTools[0]} executes and returns instructions`,
      (text.includes("HELIX Skill") || text.includes("Helix Skill") || text.includes("Cerberus Skill") || text.includes("Converted")) && text.includes("smoke test"), `${text.length} chars`);
  }

  // Error path: isError must be set, not silently swallowed.
  const bad = await request("tools/call", { name: "read_file", arguments: { path: "definitely-not-here.xyz" } });
  check("failed tool sets isError", bad.result?.isError === true,
    JSON.stringify(bad.result?.content?.[0]?.text || "").slice(0, 60));
} catch (e) {
  failures++;
  console.log("FAIL  harness error — " + e.message);
} finally {
  child.kill();
}

if (stderr.length) console.log("\n--- server stderr ---\n" + stderr.join("").trim());
console.log(failures === 0 ? "\nAll handshake checks passed." : `\n${failures} FAILING`);
process.exit(failures === 0 ? 0 : 1);
