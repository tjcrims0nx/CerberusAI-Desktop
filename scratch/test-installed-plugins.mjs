import { spawn } from "node:child_process";
import fs from "node:fs/promises";
import path from "node:path";

const root = "C:\\Users\\tjcri\\.CerberusAI\\plugins";
const args = new Set(process.argv.slice(2));
const jsonOutput = args.has("--json");
const quiet = args.has("--quiet") || jsonOutput;
const reportDir = path.join(process.cwd(), "scratch", "plugin-verification");

async function findWrappers(dir, out = []) {
  for (const ent of await fs.readdir(dir, { withFileTypes: true })) {
    const fullPath = path.join(dir, ent.name);
    if (ent.isDirectory()) {
      if (ent.name !== "node_modules" && ent.name !== ".git") {
        await findWrappers(fullPath, out);
      }
    } else if (ent.name === "cerberus-skill-wrapper.mjs") {
      out.push(fullPath);
    }
  }
  return out;
}

function send(proc, id, method, params = {}) {
  proc.stdin.write(JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n");
}

async function testWrapper(wrapper) {
  return await new Promise((resolve) => {
    const proc = spawn("node", [wrapper], {
      cwd: path.dirname(wrapper),
      stdio: ["pipe", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    const messages = [];
    const timeout = setTimeout(() => {
      proc.kill();
      resolve({ wrapper, ok: false, error: "timeout", stderr, messages });
    }, 8000);

    proc.stderr.on("data", (data) => {
      stderr += data.toString();
    });

    proc.stdout.on("data", (data) => {
      stdout += data.toString();
      let idx;
      while ((idx = stdout.indexOf("\n")) >= 0) {
        const line = stdout.slice(0, idx).trim();
        stdout = stdout.slice(idx + 1);
        if (!line) continue;

        try {
          const msg = JSON.parse(line);
          messages.push(msg);
          if (msg.id === 1) {
            send(proc, 2, "tools/list");
          }
          if (msg.id === 2) {
            const toolName = msg.result?.tools?.[0]?.name;
            if (toolName) {
              send(proc, 3, "tools/call", {
                name: toolName,
                arguments: { prompt: "Smoke test this installed skill." },
              });
            } else {
              clearTimeout(timeout);
              proc.kill();
              resolve({ wrapper, ok: false, error: "no tools returned", stderr, messages });
            }
          }
          if (msg.id === 3) {
            clearTimeout(timeout);
            proc.kill();
            const listResult = messages.find((item) => item.id === 2);
            const tools = listResult?.result?.tools?.map((tool) => tool.name) ?? [];
            const text = msg.result?.content?.map((part) => part.text || "").join("\n") || "";
            resolve({ wrapper, ok: true, tools, callReturnedText: text.length > 40, stderr });
          }
        } catch (error) {
          messages.push({ parseError: String(error), line });
        }
      }
    });

    proc.on("error", (error) => {
      clearTimeout(timeout);
      resolve({ wrapper, ok: false, error: String(error), stderr, messages });
    });

    send(proc, 1, "initialize", {
      protocolVersion: "2024-11-05",
      capabilities: {},
      clientInfo: { name: "cerberus-plugin-test", version: "1.0.0" },
    });
  });
}

const wrappers = await findWrappers(root);
const results = [];
for (const wrapper of wrappers) {
  results.push(await testWrapper(wrapper));
}

const report = {
  checkedAt: new Date().toISOString(),
  root,
  total: results.length,
  passed: results.filter((result) => result.ok && result.callReturnedText).length,
  failed: results.filter((result) => !result.ok || !result.callReturnedText).length,
  results,
};

await fs.mkdir(reportDir, { recursive: true });
const stamp = report.checkedAt.replace(/[:.]/g, "-");
const reportPath = path.join(reportDir, `plugins-${stamp}.json`);
await fs.writeFile(reportPath, JSON.stringify(report, null, 2), "utf8");

if (jsonOutput) {
  console.log(JSON.stringify({ ...report, reportPath }, null, 2));
} else if (!quiet) {
  console.log(JSON.stringify(results, null, 2));
  console.log(`Report: ${reportPath}`);
} else {
  console.log(`${report.failed === 0 ? "PASS" : "FAIL"} ${report.passed}/${report.total} plugins verified`);
  console.log(`Report: ${reportPath}`);
}

if (report.failed > 0) {
  process.exitCode = 1;
}
