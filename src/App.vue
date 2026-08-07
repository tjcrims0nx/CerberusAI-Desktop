<script setup lang="ts">
import { ref, computed, onMounted, watch } from "vue";
import { invoke, Channel } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { getVersion } from "@tauri-apps/api/app";
import { save, open } from "@tauri-apps/plugin-dialog";
import { marked } from "marked";
import DOMPurify from "dompurify";
import PluginSettings from "./PluginSettings.vue";
import Dashboard from "./components/Dashboard.vue";
import ChatView from "./components/Chat.vue";
import Models from "./components/Models.vue";
import FileBrowser from "./components/FileBrowser.vue";
import WebBrowser from "./components/WebBrowser.vue";
import { PluginManager } from "./PluginManager";
import { requiresApproval, describeToolCall } from "./toolPolicy";
import AnimatedBackground from "./AnimatedBackground.vue";

function getFilenameForLang(lang: string): string {
  const l = (lang || "").toLowerCase().trim();
  switch (l) {
    case "py": case "python": return "script.py";
    case "js": case "javascript": return "script.js";
    case "ts": case "typescript": return "script.ts";
    case "vue": return "Component.vue";
    case "html": case "htm": return "index.html";
    case "css": return "style.css";
    case "rs": case "rust": return "main.rs";
    case "cpp": case "c++": return "main.cpp";
    case "c": return "main.c";
    case "json": return "data.json";
    case "sql": return "query.sql";
    case "ps1": case "powershell": return "script.ps1";
    case "sh": case "bash": case "zsh": return "script.sh";
    case "json5": case "yaml": case "yml": return "config.yaml";
    case "md": case "markdown": return "document.md";
    default: return "file.txt";
  }
}

const customCodeRenderer = {
  code({ text, lang }: { text: string; lang?: string }) {
    const cleanLang = (lang || "code").toLowerCase();
    const filename = getFilenameForLang(cleanLang);
    const escapedCode = text
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;");

    return `<div class="code-block-container" data-lang="${cleanLang}" data-filename="${filename}">
  <div class="code-block-header">
    <div class="code-block-info">
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"></path><polyline points="14 2 14 8 20 8"></polyline></svg>
      <span class="code-lang-tag">${cleanLang.toUpperCase()}</span>
      <span class="code-filename-hint">${filename}</span>
    </div>
    <div class="code-block-actions">
      <button type="button" class="code-btn copy-code-btn">
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path></svg>
        <span>Copy</span>
      </button>
      <button type="button" class="code-btn save-code-btn">
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"></path><polyline points="7 10 12 15 17 10"></polyline><line x1="12" y1="15" x2="12" y2="3"></line></svg>
        <span>Download</span>
      </button>
    </div>
  </div>
  <pre><code class="language-${cleanLang}">${escapedCode}</code></pre>
</div>`;
  }
};

marked.use({ renderer: customCodeRenderer as any });

/**
 * Models routinely write a `---` line directly under a sentence, meaning it as
 * a horizontal rule. CommonMark reads that as a *setext heading* and promotes
 * the line above into an <h2>, so an ordinary sentence renders as a giant
 * title. Inserting a blank line restores the intended <hr>.
 *
 * Only `---` is demoted. The other setext marker, `===`, is not a valid
 * horizontal rule, so separating it would render a literal "===" paragraph —
 * a line of `===` is always meant as a heading underline and is left alone.
 * Fenced code blocks and table separator rows (which start with `|`) are also
 * untouched, as is a leading `---` front-matter delimiter.
 */
function demoteSetextRules(text: string): string {
  const lines = text.split("\n");
  const out: string[] = [];
  let inFence = false;
  for (const line of lines) {
    if (/^\s{0,3}(```|~~~)/.test(line)) inFence = !inFence;
    if (
      !inFence &&
      /^\s{0,3}-{3,}\s*$/.test(line) &&
      out.length > 0 &&
      out[out.length - 1].trim() !== ""
    ) {
      out.push("");
    }
    out.push(line);
  }
  return out.join("\n");
}

function renderMarkdown(text: string): string {
  if (!text) return "";
  try {
    const html = marked.parse(demoteSetextRules(text), { async: false }) as string;
    return DOMPurify.sanitize(html, {
      ADD_TAGS: ["button", "svg", "path", "polyline", "line", "rect", "circle", "span", "div"],
      ADD_ATTR: ["target", "rel", "data-lang", "data-filename", "class", "type", "viewBox", "fill", "stroke", "stroke-width", "stroke-linecap", "stroke-linejoin", "width", "height", "x", "y", "rx", "ry", "points", "x1", "y1", "x2", "y2", "d"],
    });
  } catch {
    const escaped = text
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;");
    return `<p>${escaped}</p>`;
  }
}
import type {
  Chat,
  OllamaModel,
  HuggingFaceModel,
  HuggingFaceFile,
  HardwareInfo,
  UsageSample,
  ChatStreamChunk,
  GgufFile,
  ToolCallChunk,
} from "./types";

const STORAGE_KEY = "helix.chats.v1";
const MODEL_KEY = "helix.model.v1";

// ─── Window controls (custom titlebar) ────────────────────────────────────
const appWindow = getCurrentWindow();
const isMaximized = ref(false);

async function minimizeWindow() { await appWindow.minimize(); }
async function toggleMaximize() {
  await appWindow.toggleMaximize();
  isMaximized.value = await appWindow.isMaximized();
}
async function closeWindow() { await appWindow.close(); }

// Track maximize state
appWindow.onResized(async () => {
  isMaximized.value = await appWindow.isMaximized();
});


// Ollama lowercases all model names when it stores them via `ollama create`.
// Use case-insensitive comparison everywhere we compare local Ollama names
// against the cloud allowlist.
function modelKey(name: string | null | undefined): string {
  if (!name) return "";
  return name.replace(/:latest$/, "").toLowerCase();
}



const chats = ref<Chat[]>([]);
const activeId = ref<string | null>(null);
const models = ref<OllamaModel[]>([]);
const selectedModel = ref<string>("");
const localStatus = ref<{ running: boolean; version?: string; error?: string }>({ running: false });
const hardware = ref<HardwareInfo | null>(null);
const cpuUsage = ref<number>(0);
const ramUsage = ref<number>(0);
const vramUsage = ref<number | null>(null);
const usageDetail = ref<UsageSample | null>(null);
const draft = ref<string>("");
const dragActive = ref<boolean>(false);
const attachedImages = ref<string[]>([]);
const attachedFiles = ref<{name: string, content: string}[]>([]);
const streaming = ref<boolean>(false);

async function handleDrop(e: DragEvent) {
  dragActive.value = false;
  if (!e.dataTransfer || !e.dataTransfer.files) return;

  for (let i = 0; i < e.dataTransfer.files.length; i++) {
    const file = e.dataTransfer.files[i];
    if (file.type.startsWith("image/")) {
      const reader = new FileReader();
      reader.onload = (ev) => {
        const result = ev.target?.result as string;
        if (result) {
          const base64 = result.split(",")[1];
          attachedImages.value.push(base64);
        }
      };
      reader.readAsDataURL(file);
    } else {
      const text = await file.text();
      attachedFiles.value.push({ name: file.name, content: text });
    }
  }
}

function removeImage(index: number) {
  attachedImages.value.splice(index, 1);
}

function removeFile(index: number) {
  attachedFiles.value.splice(index, 1);
}

// Attachments coming from the Files & Pictures manager (Library or Browse tab).
function onBrowserAttachImage(base64: string) {
  attachedImages.value.push(base64);
}
function onBrowserAttachFile(file: { name: string; content: string }) {
  attachedFiles.value.push(file);
}

const fileInput = ref<HTMLInputElement | null>(null);
const imageInput = ref<HTMLInputElement | null>(null);
const showAttachMenu = ref(false);

function triggerFileInput() {
  fileInput.value?.click();
}

function triggerImageInput() {
  imageInput.value?.click();
}

async function handleFileSelect(e: Event) {
  const target = e.target as HTMLInputElement;
  if (!target.files) return;

  for (let i = 0; i < target.files.length; i++) {
    const file = target.files[i];
    if (file.type.startsWith("image/")) {
      const reader = new FileReader();
      reader.onload = (ev) => {
        const result = ev.target?.result as string;
        if (result) {
          const base64 = result.split(",")[1];
          attachedImages.value.push(base64);
        }
      };
      reader.readAsDataURL(file);
    } else {
      const text = await file.text();
      attachedFiles.value.push({ name: file.name, content: text });
    }
  }
  target.value = '';
}

const streamingContent = ref<string>("");
const updating = ref<boolean>(false);
const updateProgressText = ref<string>("");
const updateInfo = ref<{ current: string; latest: string; available: boolean } | null>(null);
const appVersion = ref<string>("0.1.0");
const messagesEl = ref<HTMLElement | null>(null);
const lastTtft = ref<number | null>(null);
const lastTps = ref<number | null>(null);

// Placeholder shown while a cold model loads. Held as a constant so the stream
// handler can recognise it and avoid committing it as message content.
const LOADING_HINT =
  "_loading model into memory — first response after a model switch can take a minute…_";

function stripThinkTags(text: string): string {
  // Remove completed <think>...</think> blocks (including multiline)
  let result = text.replace(/<think>[\s\S]*?<\/think>/g, "");
  // Remove an unclosed <think> block still being streamed
  result = result.replace(/<think>[\s\S]*$/, "");
  return result.trimStart();
}

// Aggressive cleanup applied only when a message FINISHES streaming —
// safe to strip leading/trailing artifacts here because we have the full text.
function finalizeMessage(text: string): string {
  if (!text) return text;
  try {
    let r = stripThinkTags(text);
    // Drop trailing horizontal-rule separators: ---, ***, === (3+)
    r = r.replace(/\n*\s*[-*=_]{3,}\s*$/g, "");
    // Drop a trailing lone quote/backtick line
    r = r.replace(/\n*\s*["'`]+\s*$/g, "");
    // Drop leaked chat-template tokens anywhere
    r = r.replace(/<\|im_(?:start|end)\|>/g, "");
    r = r.replace(/<(?:start|end)_of_turn>/g, "");
    // Drop orphan leading punctuation/whitespace before the first letter or emoji
    r = r.replace(/^[\s,;:.!?'"`]+(?=[A-Za-z0-9À-])/, "");
    r = r.trim();
    return r.length === 0 && text.trim().length > 0 ? text.trim() : r;
  } catch {
    return text;
  }
}

// Model Manager (LM Studio-style)
const showFileManager = ref(false);
const showFileBrowser = ref(false);
const showWebBrowser = ref(false);
const showPluginManager = ref(false);

// The tool call awaiting user approval, and the resolver that the dialog's
// Allow/Deny buttons complete. Null whenever no dialog is open.
const pendingApproval = ref<{ toolName: string; detail: string } | null>(null);
let approvalResolver: ((approved: boolean) => void) | null = null;
const pluginManager = new PluginManager();
const mcpToolMap = ref<Map<string, string>>(new Map()); // toolName -> pluginId
const mcpConfigs = ref<any[]>([]);
let mcpStartupPromise: Promise<void> | null = null;
const localGgufs = ref<GgufFile[]>([]);
const isDeletingGguf = ref(false);
const managerTab = ref<'ollama' | 'files' | 'cloud'>('ollama');
const managerSearch = ref('');
const isDeletingModel = ref(false);
const activatedGgufs = ref<Set<string>>(new Set());

const filteredOllamaModels = computed(() => {
  const q = managerSearch.value.toLowerCase().trim();
  if (!q) return models.value;
  return models.value.filter(m => m.name.toLowerCase().includes(q));
});

const filteredGgufs = computed(() => {
  const q = managerSearch.value.toLowerCase().trim();
  if (!q) return localGgufs.value;
  return localGgufs.value.filter(f => f.name.toLowerCase().includes(q));
});

const totalOllamaSize = computed(() =>
  models.value.reduce((sum, m) => sum + (m.size || 0), 0)
);

const totalGgufSize = computed(() =>
  localGgufs.value.reduce((sum, f) => sum + f.size, 0)
);

async function openFileManager() {
  showFileManager.value = true;
  managerSearch.value = '';
  await refreshAllModels();
}

async function refreshAllModels() {
  await refreshModels();
  await refreshLocalGgufs();
}

async function refreshLocalGgufs() {
  try {
    localGgufs.value = await invoke<GgufFile[]>("list_local_ggufs");
  } catch (e) {
    console.error("Failed to list ggufs", e);
    localGgufs.value = [];
  }
}

async function refreshMcpTools() {
  const allTools = await pluginManager.getAllTools();
  const newMap = new Map<string, string>();
  const ollamaTools = allTools.map(({ pluginId, tool }) => {
    newMap.set(tool.name, pluginId);
    return {
      type: "function",
      function: {
        name: tool.name,
        description: tool.description || "",
        parameters: tool.inputSchema || { type: "object", properties: {} },
      },
    };
  });
  mcpToolMap.value = newMap;
  return ollamaTools;
}

const activePluginIds = ref<string[]>([]);

async function connectMcpPlugins(configs?: any[]) {
  if (configs) {
    mcpConfigs.value = configs;
  } else {
    mcpConfigs.value = await pluginManager.loadWithDiscovered();
  }

  await pluginManager.syncPlugins(mcpConfigs.value);
  activePluginIds.value = [...pluginManager.activePlugins];
  await refreshMcpTools();
}

function startMcpImmediately() {
  if (mcpStartupPromise) return mcpStartupPromise;
  mcpStartupPromise = connectMcpPlugins()
    .catch((e) => {
      console.warn("Failed to connect MCP plugins at startup", e);
      mcpStartupPromise = null;
    });
  return mcpStartupPromise;
}

async function handlePluginsChanged(configs: any[]) {
  await connectMcpPlugins(configs);
}

async function deleteGguf(filename: string) {
  if (isDeletingGguf.value) return;
  isDeletingGguf.value = true;
  try {
    await invoke("delete_local_gguf", { filename });
    await refreshLocalGgufs();
  } catch (e) {
    alert("Failed to delete file: " + e);
  } finally {
    isDeletingGguf.value = false;
  }
}

async function deleteOllamaModel(name: string) {
  if (isDeletingModel.value) return;
  if (!confirm(`Remove "${name}" from Ollama? This will free disk space but you'll need to re-download it to use again.`)) return;
  isDeletingModel.value = true;
  try {
    await invoke("delete_ollama_model", { name });
    await refreshModels();
  } catch (e) {
    alert("Failed to delete model: " + e);
  } finally {
    isDeletingModel.value = false;
  }
}

async function moveGguf(filename: string) {
  if (isDeletingGguf.value) return;

  try {
    const destination = await save({
      defaultPath: filename,
      filters: [{ name: 'GGUF Models', extensions: ['gguf'] }]
    });

    if (destination === null) {
      // user cancelled dialog
      return;
    }

    isDeletingGguf.value = true;
    await invoke("move_local_gguf", { filename, destination });
    await refreshLocalGgufs();
  } catch (e) {
    alert("Failed to move file: " + e);
  } finally {
    isDeletingGguf.value = false;
  }
}

const isImporting = ref(false);

async function importGguf() {
  if (isImporting.value) return;
  try {
    const selected = await open({
      multiple: false,
      filters: [{ name: 'GGUF Models', extensions: ['gguf'] }],
      title: 'Select a .gguf model file to import',
    });
    if (!selected) return;

    const filePath = typeof selected === 'string' ? selected : String(selected);
    // Derive a default model name from filename (strip extension + quant suffix)
    const basename = filePath.split(/[\\/]/).pop() || 'custom-model';
    const defaultName = basename.replace(/\.gguf$/i, '').replace(/[-_](f16|F16|Q[\d]+_K_[MSL]|Q[\d]+_[\d]+|IQ[\d]+_[A-Z]+|Q[\d]+)$/i, '');

    const modelName = prompt('Model name for Ollama:', defaultName);
    if (!modelName || !modelName.trim()) return;

    isImporting.value = true;
    const result = await invoke<string>('import_local_gguf', {
      sourcePath: filePath,
      modelName: modelName.trim(),
    });
    alert(result);
    await refreshModels();
    await refreshLocalGgufs();
  } catch (e) {
    alert('Import failed: ' + e);
  } finally {
    isImporting.value = false;
  }
}

async function activateGguf(filename: string) {
  if (isImporting.value) return;
  const basename = filename.split(/[\\/]/).pop() || 'custom-model';
  const defaultName = basename.replace(/\.gguf$/i, '').replace(/[-_](f16|F16|Q[\d]+_K_[MSL]|Q[\d]+_[\d]+|IQ[\d]+_[A-Z]+|Q[\d]+)$/i, '');

  const modelName = prompt(`Activate ${filename} in Ollama?\n\nEnter model name:`, defaultName);
  if (!modelName || !modelName.trim()) return;

  isImporting.value = true;
  try {
    const result = await invoke<string>('activate_managed_gguf', {
      filename: filename,
      modelName: modelName.trim(),
    });
    alert(result);
    activatedGgufs.value.add(filename);
    await refreshModels();
  } catch (e) {
    alert('Activation failed: ' + e);
  } finally {
    isImporting.value = false;
  }
}

function formatBytes(bytes: number) {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
}


// API key gate
const apiKey = ref<string>("");

// Active model pull
const pulling = ref<{
  name: string;
  pct: number;
  status: string;
  completed?: number;
  total?: number;
  /** Bytes/sec averaged over the last few seconds */
  bps?: number;
  /** Seconds remaining (rough estimate) */
  eta?: number;
  /** True if this download picked up where a previous run left off */
  resumed?: boolean;
} | null>(null);

/** Format a bytes/sec value like 12.4 MB/s. */
function fmtRate(bps?: number): string {
  if (!bps || bps <= 0) return "";
  if (bps >= 1024 * 1024) return (bps / 1024 / 1024).toFixed(1) + " MB/s";
  if (bps >= 1024) return (bps / 1024).toFixed(1) + " KB/s";
  return `${bps} B/s`;
}

/** Format remaining seconds as 1h 12m / 4m 30s / 25s. */
function fmtEta(secs?: number): string {
  if (secs === undefined || secs <= 0) return "";
  if (secs >= 3600) {
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    return `${h}h ${m}m`;
  }
  if (secs >= 60) {
    const m = Math.floor(secs / 60);
    const s = secs % 60;
    return `${m}m ${s}s`;
  }
  return `${secs}s`;
}

function fmtDownloadBytes(done?: number, total?: number): string {
  if (!done && !total) return "";
  if (!total) return formatBytes(done ?? 0);
  return `${formatBytes(Math.min(done ?? 0, total))} / ${formatBytes(total)}`;
}

function clampDownloadProgress(done?: number, total?: number): { completed?: number; total?: number; pct: number } {
  if (!total || total <= 0) {
    return { completed: done, total, pct: pulling.value?.pct ?? 0 };
  }
  const completed = Math.max(0, Math.min(done ?? 0, total));
  return {
    completed,
    total,
    pct: Math.max(0, Math.min(100, Math.floor((completed / total) * 100))),
  };
}

function downloadPhase(status?: string): string {
  const s = (status || "").toLowerCase();
  if (s.includes("import") || s.includes("register") || s.includes("create")) return "IMPORTING";
  if (s.includes("verif") || s.includes("final")) return "VERIFYING";
  if (s.includes("resume")) return "RESUMING";
  if (s.includes("error")) return "ERROR";
  return "PULLING MODEL";
}

// Every allowlisted model, paired with whether it's already pulled locally.
// Drives the dropdown so users can pick undownloaded models and have them
// auto-pulled (LM Studio-style).

const activeChat = computed<Chat | null>(() =>
  chats.value.find((c) => c.id === activeId.value) ?? null
);

// const ramGb = computed(() =>
//   hardware.value ? (hardware.value.total_ram_mb / 1024).toFixed(1) : "—"
// );
const primaryGpu = computed(() => hardware.value?.gpus[0] ?? null);

const ramTooltip = computed(() => {
  const u = usageDetail.value;
  if (!u) return "";
  return `${(u.ram_used_mb / 1024).toFixed(1)} / ${(u.ram_total_mb / 1024).toFixed(1)} GB`;
});

const vramTooltip = computed(() => {
  const gpu = primaryGpu.value?.name ?? "";
  const u = usageDetail.value;
  if (!u || u.vram_used_mb === null || u.vram_total_mb === null) {
    return gpu ? `${gpu} — VRAM usage unavailable` : "VRAM usage unavailable";
  }
  const amounts = `${(u.vram_used_mb / 1024).toFixed(1)} / ${(u.vram_total_mb / 1024).toFixed(1)} GB`;
  return gpu ? `${gpu} — ${amounts}` : amounts;
});
const vramGb = computed(() => {
  const v = primaryGpu.value?.vram_mb;
  return v ? (v / 1024).toFixed(1) : null;
});

/**
 * VRAM available for a freshly-loaded model, in bytes.
 * We assume ~70% of the dedicated VRAM is usable (the rest goes to display
 * compositor + driver overhead) and conservatively warn on anything bigger.
 * Returns null if we don't have GPU info — in that case we don't show hints.
 */
const usableVramBytes = computed<number | null>(() => {
  const mb = primaryGpu.value?.vram_mb;
  if (!mb) return null;
  // Convert MB to bytes, then take 70%.
  return Math.floor(mb * 1024 * 1024 * 0.7);
});

/**
 * For a given quant file size in bytes, classify how it'll behave on this GPU:
 *   "fits"      — comfortable on GPU, will run fast
 *   "tight"     — within 90% of usable VRAM; might offload partially
 *   "too-big"   — won't fit, Ollama will fall back to CPU (very slow)
 *   "unknown"   — no VRAM info or no size info
 */
function quantFit(bytes: number | undefined): "fits" | "tight" | "too-big" | "unknown" {
  if (!bytes || !usableVramBytes.value) return "unknown";
  if (bytes <= usableVramBytes.value * 0.9) return "fits";
  if (bytes <= usableVramBytes.value) return "tight";
  return "too-big";
}

function fmtSizeGb(bytes: number | undefined): string {
  if (!bytes) return "";
  return (bytes / 1024 / 1024 / 1024).toFixed(1) + " GB";
}

const selectedModelSize = computed(() => {
  if (!selectedModel.value) return "";
  const match = localGgufs.value.find(
    (g: any) => g.name === selectedModel.value || g.name.toLowerCase() === selectedModel.value.toLowerCase()
  );
  return match ? formatBytes(match.size) : "";
});

function uid(): string {
  return Math.random().toString(36).slice(2, 10);
}

async function loadChats() {
  try {
    const raw = await invoke<string | null>("db_get_kv", { key: STORAGE_KEY });
    if (raw) chats.value = JSON.parse(raw);
  } catch {
    chats.value = [];
  }
  if (chats.value.length === 0) newChat();
  else activeId.value = chats.value[0].id;
}

function saveChats() {
  invoke("db_set_kv", { key: STORAGE_KEY, value: JSON.stringify(chats.value) }).catch(console.error);
}

function getSystemPrompt(modelName: string): string {
  const modelDisplayName = modelName ? modelName.replace(/\.gguf$/i, '') : "Local LLM";
  return [
    `You are ${modelDisplayName}, an advanced AI model running locally via the desktop engine.`,
    `Always recognize and identify yourself as ${modelDisplayName}, not as HELIX.`,
    "You have access to Model Context Protocol (MCP) tools. You MUST proactively use these tools to read files, run commands, fetch data, and edit code to fulfill the user's requests.",
    "Do NOT ask the user for permission to use tools, just use the tools to find the information yourself.",
    "If you encounter an error using a tool, reason about it and try to fix it or use a different approach.",
    "Use `<think>` blocks to outline your plan before executing complex tool sequences or code edits.",
    "Format every response in GitHub-flavored markdown. Use elements like bold, italic, code blocks, lists, and headings appropriately.",
    "Use emojis when they add meaning to a list item or heading (e.g., ✅ ❌ ⚠️ 🔧 📁 💻 🧠).",
    "Be concise. Skip filler text and greetings."
  ].join("\n");
}

function newChat() {
  if (streaming.value) {
    invoke("cancel_chat").catch(console.warn);
  }
  streaming.value = false;
  streamingContent.value = "";
  draft.value = "";
  attachedImages.value = [];
  attachedFiles.value = [];
  dragActive.value = false;
  lastTtft.value = null;
  lastTps.value = null;

  const id = uid();
  chats.value.unshift({
    id,
    title: "New chat",
    model: selectedModel.value,
    messages: [],
    createdAt: Date.now(),
  });
  activeId.value = id;
  saveChats();
}

function selectChat(id: string) {
  activeId.value = id;
}

function deleteChat(id: string, evt: Event) {
  evt.stopPropagation();
  chats.value = chats.value.filter((c) => c.id !== id);
  if (activeId.value === id) {
    activeId.value = chats.value[0]?.id ?? null;
    if (!activeId.value) newChat();
  }
  saveChats();
}

watch(selectedModel, (v) => {
  // Persist once the model is actually present locally (Ollama list or GGUF file)
  const inModels = v && models.value.some((m) => modelKey(m.name) === modelKey(v));
  const inGgufs = v && localGgufs.value.some((g) => g.name === v || g.name.toLowerCase() === v.toLowerCase());
  if (inModels || inGgufs) {
    invoke("db_set_kv", { key: MODEL_KEY, value: v }).catch(console.error);
  }
});



async function refreshModels() {
  try {
    // No model filtering — every local model Ollama or the GGUF scan reports is
    // shown in the picker, LM-Studio style.
    const filtered = await invoke<OllamaModel[]>("list_models");
    models.value = filtered;

    if (
      selectedModel.value &&
      !filtered.find((m) => modelKey(m.name) === modelKey(selectedModel.value))
    ) {
      selectedModel.value = "";
    }
    if (!selectedModel.value && filtered.length) {
      selectedModel.value = filtered[0].name;
    }
  } catch (e) {
    console.warn("list_models failed", e);
    models.value = [];
  }
}

async function checkLocal() {
  try {
    localStatus.value = await invoke("check_local_ollama");
  } catch (e) {
    localStatus.value = { running: false, error: String(e) };
  }
  // Always refresh models — list_models scans local GGUFs even without Ollama
  await refreshModels();
  await refreshLocalGgufs();
}

async function checkForUpdate() {
  try {
    const info = await invoke<{ current: string; latest: string; available: boolean }>(
      "check_for_update"
    );
    updateInfo.value = info;
  } catch (e) {
    console.warn("check_for_update failed", e);
    // If it fails, we set a dummy state so the UI doesn't hang in "CHECKING..."
    updateInfo.value = { current: appVersion.value, latest: "unknown", available: false };
  }
}

async function detectHardware() {
  try {
    hardware.value = await invoke<HardwareInfo>("detect_hardware");
  } catch (e) {
    console.warn("detect_hardware failed", e);
  }
}

/** Poll real CPU / RAM / VRAM utilization for the sidebar meters. */
async function sampleUsage() {
  try {
    const sample = await invoke<UsageSample>("sample_usage");
    usageDetail.value = sample;
    cpuUsage.value = sample.cpu_pct;
    ramUsage.value = sample.ram_pct;
    vramUsage.value = sample.vram_pct;
  } catch (e) {
    console.warn("sample_usage failed", e);
  }
}

let scrollAnimationFrame: number | null = null;
async function scrollToBottom() {
  if (scrollAnimationFrame) return;
  scrollAnimationFrame = requestAnimationFrame(() => {
    scrollAnimationFrame = null;
    if (messagesEl.value) {
      messagesEl.value.scrollTop = messagesEl.value.scrollHeight;
    }
  });
}

// API key and cloud auth have been removed

async function send() {
  let text = draft.value.trim();

  if (attachedFiles.value.length > 0) {
    for (const f of attachedFiles.value) {
      text += (text ? "\n\n" : "") + `--- File: ${f.name} ---\n${f.content}\n`;
    }
  }

  if ((!text && attachedImages.value.length === 0) || streaming.value || !activeChat.value || !selectedModel.value) return;
  if (pulling.value) return;
  // Accept model if it's in the models list OR is a known local GGUF file
  const modelInList = models.value.some((m) => modelKey(m.name) === modelKey(selectedModel.value));
  const modelIsGguf = localGgufs.value.some((g) => g.name === selectedModel.value || g.name.toLowerCase() === selectedModel.value.toLowerCase());
  if (!modelInList && !modelIsGguf) return;

  const chat = activeChat.value;

  if (text.startsWith("/mcp")) {
    const parts = text.split(" ").filter(Boolean);
    const subCmd = (parts[1] || "list").toLowerCase();
    const target = parts.slice(2).join(" ").trim().toLowerCase();

    draft.value = "";
    attachedImages.value = [];
    attachedFiles.value = [];

    const userMsg: any = { role: "user", content: text };
    chat.messages.push(userMsg);

    let systemReply = "";

    if (subCmd === "list") {
      if (mcpConfigs.value.length === 0) {
        systemReply = "🔌 **No MCP plugins configured.**\n\nType `/mcp open` or click **MCP Plugins** in the sidebar to add local or remote servers.";
      } else {
        const items = mcpConfigs.value.map(p => {
          const active = pluginManager.activePlugins.includes(p.id);
          const statusIcon = p.enabled ? (active ? "🟢" : "🟡") : "🔴";
          const statusLabel = p.enabled ? (active ? "Connected & Ready" : "Connecting...") : "Disabled";
          const details = p.url ? `URL: \`${p.url}\`` : `Command: \`${p.command} ${(p.args || []).join(" ")}\``;
          return `${statusIcon} **${p.name}** (\`${p.id}\`)\n  - Status: *${statusLabel}*\n  - ${details}`;
        }).join("\n\n");
        systemReply = `🔌 **MCP Plugins (${mcpConfigs.value.length})**\n\n${items}\n\n*Quick Commands:*\n- \`/mcp enable <name|id>\` — Activate a plugin\n- \`/mcp disable <name|id>\` — Deactivate a plugin\n- \`/mcp open\` — Open Plugin Manager modal`;
      }
    } else if (subCmd === "enable" || subCmd === "on" || subCmd === "start") {
      const match = mcpConfigs.value.find(p => p.id.toLowerCase() === target || p.name.toLowerCase().includes(target));
      if (match) {
        match.enabled = true;
        await invoke("db_set_kv", { key: 'mcp-plugins', value: JSON.stringify(mcpConfigs.value) });
        await connectMcpPlugins(mcpConfigs.value);
        systemReply = `✅ **Activated MCP Plugin:** ${match.name}\n\nConnected to server and tools are ready for chat prompts.`;
      } else {
        systemReply = `❌ **Plugin not found matching:** "${target}".\n\nType \`/mcp list\` to view available plugin IDs.`;
      }
    } else if (subCmd === "disable" || subCmd === "off" || subCmd === "stop") {
      const match = mcpConfigs.value.find(p => p.id.toLowerCase() === target || p.name.toLowerCase().includes(target));
      if (match) {
        match.enabled = false;
        await invoke("db_set_kv", { key: 'mcp-plugins', value: JSON.stringify(mcpConfigs.value) });
        await connectMcpPlugins(mcpConfigs.value);
        systemReply = `🔌 **Deactivated MCP Plugin:** ${match.name}`;
      } else {
        systemReply = `❌ **Plugin not found matching:** "${target}".\n\nType \`/mcp list\` to view available plugin IDs.`;
      }
    } else if (subCmd === "open" || subCmd === "settings" || subCmd === "gui") {
      showPluginManager.value = true;
      systemReply = "⚙️ Opened **MCP Plugin Manager**.";
    } else {
      systemReply = "ℹ️ **MCP Chat Commands:**\n- `/mcp list` — Show all plugins and connection status\n- `/mcp enable <id>` — Activate a plugin by ID or name\n- `/mcp disable <id>` — Deactivate a plugin\n- `/mcp open` — Open Plugin Manager modal";
    }

    chat.messages.push({ role: "assistant", content: systemReply });
    saveChats();
    await scrollToBottom();
    return;
  }

  const userMsg: any = { role: "user", content: text };
  if (attachedImages.value.length > 0) {
    userMsg.images = [...attachedImages.value];
  }
  chat.messages.push(userMsg);

  if (chat.messages.length === 1) {
    chat.title = text.slice(0, 48) + (text.length > 48 ? "…" : "");
  }
  chat.model = selectedModel.value;
  draft.value = "";
  attachedImages.value = [];
  attachedFiles.value = [];
  saveChats();
  await scrollToBottom();

  chat.messages.push({ role: "assistant", content: "" });
  const assistantIdx = chat.messages.length - 1;
  streamingContent.value = "";
  streaming.value = true;
  lastTtft.value = null;
  lastTps.value = null;

  // Safety timeout: if no content arrives within 5 minutes, unblock the UI.
  // Cold-loading a multi-GB GGUF off a slow disk can easily take 60-120s,
  // and that delay shouldn't trip a "no response" error.
  // Both timers fire long after `targetIdx` is initialised below, and write to
  // it rather than `assistantIdx` so a tool round can't send them to a stale row.
  let gotContent = false;
  const loadingHintTimer = setTimeout(() => {
    if (!gotContent && streaming.value) {
      streamingContent.value = LOADING_HINT;
      chat.messages[targetIdx].content = streamingContent.value;
    }
  }, 30_000);
  const safetyTimer = setTimeout(() => {
    if (!gotContent && streaming.value) {
      streamingContent.value =
        "\n\n[error] No response from model after 5 minutes — model engine may be unresponsive. Try re-selecting model or restarting.";
      chat.messages[targetIdx].content = streamingContent.value;
      streaming.value = false;
      saveChats();
    }
  }, 300_000);

  let ollamaTools: any[] | undefined;
  try {
    await startMcpImmediately();
    const tools = await refreshMcpTools();
    ollamaTools = tools.length > 0 ? tools : undefined;
  } catch (e) {
    console.warn("Failed to gather MCP tools", e);
  }

  // Tool-call loop: the model may ask for tools, we run them and feed the
  // results back so it can continue — repeatedly, since answering one tool
  // call often reveals the need for the next.
  const MAX_TOOL_ROUNDS = 8;

  // The assistant message the stream is currently writing into. Each tool
  // round appends a fresh one, so the callback reads this instead of closing
  // over a fixed index.
  let targetIdx = assistantIdx;
  let pendingToolCalls: ToolCallChunk[] | null = null;
  let streamError: string | null = null;

  const channel = new Channel<ChatStreamChunk>();
  channel.onmessage = (chunk) => {
    if (chunk.ttft_ms !== undefined && chunk.ttft_ms !== null) lastTtft.value = chunk.ttft_ms;
    if (chunk.tps !== undefined && chunk.tps !== null) lastTps.value = chunk.tps;

    if (chunk.error) {
      clearTimeout(loadingHintTimer);
      clearTimeout(safetyTimer);
      streamError = chunk.error;
      streamingContent.value += `\n\n[error] ${chunk.error}`;
      chat.messages[targetIdx].content = finalizeMessage(streamingContent.value);
      streaming.value = false;
      saveChats();
      return;
    }
    if (chunk.delta) {
      // First real token clears the loading hint and replaces it with the actual content.
      if (!gotContent) {
        clearTimeout(loadingHintTimer);
        streamingContent.value = "";
        chat.messages[targetIdx].content = "";
      }
      gotContent = true;
      streamingContent.value += chunk.delta;
      scrollToBottom();
    }
    // Capture any tool calls from the chunk
    if (chunk.tool_calls && chunk.tool_calls.length > 0) {
      pendingToolCalls = chunk.tool_calls;
    }
    if (chunk.done) {
      clearTimeout(loadingHintTimer);
      clearTimeout(safetyTimer);
      // The loading hint is UI chrome, not an answer, so it must never be
      // committed as the message body. Compared exactly rather than inferred
      // from `gotContent`, so a real error already written by the safety
      // timeout survives a late `done`.
      const body = streamingContent.value === LOADING_HINT ? "" : streamingContent.value;
      chat.messages[targetIdx].content = finalizeMessage(body);
      streaming.value = false;
      streamingContent.value = "";
      saveChats();
    }
  };

  // Everything before the assistant placeholder, capped to the last 20 messages
  // so a long chat doesn't overwhelm the context window.
  const buildMessages = (upTo: number) => {
    const history = chat.messages.slice(0, upTo);
    const capped = history.length > 20 ? history.slice(-20) : history;
    return [
      { role: "system", content: getSystemPrompt(selectedModel.value) },
      ...capped,
    ];
  };

  try {
    for (let round = 1; ; round += 1) {
      pendingToolCalls = null;

      await invoke("chat_stream", {
        // Ollama stores model names lowercase. Use modelKey() so a UI value
        // like "Phi-4-Mini-Instruct-Annihilated" is dispatched consistently.
        model: modelKey(selectedModel.value),
        messages: buildMessages(targetIdx),
        tools: ollamaTools,
        onEvent: channel,
      });

      const toolCalls = pendingToolCalls as ToolCallChunk[] | null;
      if (streamError || !toolCalls || toolCalls.length === 0) break;

      // Record the assistant turn that requested the tools.
      chat.messages[targetIdx].tool_calls = toolCalls;

      if (round >= MAX_TOOL_ROUNDS) {
        chat.messages[targetIdx].content = finalizeMessage(
          `${chat.messages[targetIdx].content}\n\n_[stopped after ${MAX_TOOL_ROUNDS} tool rounds]_`.trim()
        );
        streaming.value = false;
        saveChats();
        break;
      }

      await runToolCalls(toolCalls, chat);

      // Continue with the tool results in context.
      streaming.value = true;
      streamingContent.value = "";
      gotContent = false;
      targetIdx = chat.messages.length;
      chat.messages.push({ role: "assistant", content: "" });
      await scrollToBottom();
    }
  } catch (e) {
    clearTimeout(loadingHintTimer);
    clearTimeout(safetyTimer);
    streamingContent.value += `\n\n[error] ${String(e)}`;
    chat.messages[targetIdx].content = streamingContent.value;
    streaming.value = false;
    streamingContent.value = "";
    saveChats();
  }
}

/**
 * Run each requested tool through its MCP plugin, appending one `tool` message
 * per call so the model can read the results on the next round.
 */
async function runToolCalls(toolCalls: ToolCallChunk[], chat: Chat) {
  for (const tc of toolCalls) {
    const toolName = tc.function.name;

    let toolArgs: any;
    try {
      toolArgs = typeof tc.function.arguments === "string"
        ? JSON.parse(tc.function.arguments)
        : tc.function.arguments;
    } catch {
      chat.messages.push({
        role: "tool",
        content: `[error] Malformed arguments for ${toolName}: ${tc.function.arguments}`,
      });
      continue;
    }

    const pluginId = mcpToolMap.value.get(toolName);
    if (!pluginId) {
      chat.messages.push({ role: "tool", content: `[error] Unknown tool: ${toolName}` });
      continue;
    }

    // Side-effecting tools need the user to see the exact call before it runs.
    if (requiresApproval(toolName)) {
      const approved = await requestToolApproval(toolName, toolArgs);
      if (!approved) {
        // Reported back to the model rather than silently skipped, so it can
        // explain itself or choose another approach instead of stalling.
        chat.messages.push({
          role: "tool",
          content: `[denied] The user declined to run ${toolName}.`,
        });
        continue;
      }
    }

    // Show the user what's happening
    streamingContent.value = `_🔧 Running tool: ${toolName}…_`;
    streaming.value = true;
    const toolIdx = chat.messages.length;
    chat.messages.push({ role: "tool", content: "" });
    await scrollToBottom();

    try {
      const result = await pluginManager.callTool(pluginId, toolName, toolArgs);
      const text = result.content
        ?.map((c: any) => c.text ?? JSON.stringify(c))
        .join("\n") ?? JSON.stringify(result);
      // MCP reports tool failures as a normal result carrying `isError`, so
      // without this the model reads a failure as a success.
      chat.messages[toolIdx].content = result.isError ? `[tool error] ${text}` : text;
    } catch (e) {
      chat.messages[toolIdx].content = `[tool error] ${String(e)}`;
    }
  }
}

/**
 * Show the approval dialog and resolve once the user answers.
 *
 * The dialog is plain state plus a stored resolver rather than a component
 * event, so the tool loop can simply `await` it inline.
 */
function requestToolApproval(toolName: string, args: any): Promise<boolean> {
  pendingApproval.value = {
    toolName,
    detail: describeToolCall(toolName, args),
  };
  return new Promise<boolean>((resolve) => {
    approvalResolver = resolve;
  });
}

function resolveToolApproval(approved: boolean) {
  pendingApproval.value = null;
  const resolver = approvalResolver;
  approvalResolver = null;
  resolver?.(approved);
}

async function stopChat() {
  if (!streaming.value) return;
  try {
    await invoke("cancel_chat");
  } catch (e) {
    console.warn("Failed to cancel chat", e);
  }
}

interface PullProgress {
  status: string;
  completed?: number;
  total?: number;
  done: boolean;
  error?: string;
  bytes_per_second?: number;
  eta_seconds?: number;
  resumed?: boolean;
}

async function cancelDownload() {
  await invoke("cancel_pull").catch(() => {});
  pulling.value = null;
}

async function pullModel(repoId: string, filename: string) {
  if (pulling.value) return;
  const displayName = filename.replace(/\.gguf$/i, '');
  pulling.value = { name: displayName, pct: 0, status: "starting…" };
  const channel = new Channel<PullProgress>();
  channel.onmessage = (p) => {
    const progress = clampDownloadProgress(p.completed, p.total);
    pulling.value = {
      name: displayName,
      pct: progress.pct,
      status: p.status || "downloading",
      completed: progress.completed,
      total: progress.total,
      bps: p.bytes_per_second,
      eta: p.eta_seconds,
      resumed: p.resumed ?? pulling.value?.resumed,
    };
    if (p.done) {
      const failed = !!p.error;
      pulling.value = null;
      if (!failed) {
        refreshModels().then(() => {
          const expectedName = filename.replace(/\.gguf$/i, '').toLowerCase();
          if (!selectedModel.value) selectedModel.value = expectedName;
        });
      }
    }
  };
  try {
    await invoke("pull_model", { repoId, filename, onEvent: channel });
  } catch (e) {
    pulling.value = { name: displayName, pct: 0, status: `error: ${String(e)}` };
    setTimeout(() => { if (pulling.value?.name === displayName) pulling.value = null; }, 4000);
  }
}

async function searchHuggingFace(query: string): Promise<HuggingFaceModel[]> {
  try {
    return await invoke<HuggingFaceModel[]>("search_huggingface", { query });
  } catch (e) {
    console.warn("search_huggingface failed", e);
    return [];
  }
}

async function listHuggingFaceFiles(repoId: string): Promise<HuggingFaceFile[]> {
  try {
    return await invoke<HuggingFaceFile[]>("list_huggingface_files", { repoId });
  } catch (e) {
    console.warn("list_huggingface_files failed", e);
    return [];
  }
}

function onComposerKeydown(e: KeyboardEvent) {
  if (e.key === "Enter" && !e.shiftKey) {
    e.preventDefault();
    send();
  }
}

function autosizeComposer(e: Event) {
  const el = e.target as HTMLTextAreaElement;
  el.style.height = "auto";
  el.style.height = Math.min(el.scrollHeight, 200) + "px";
}

async function handleUpdate() {
  if (updating.value) return;
  if (!updateInfo.value?.available) return;
  updating.value = true;
  updateProgressText.value = "DOWNLOADING...";

  const onEvent = new Channel<any>();
  onEvent.onmessage = (evt) => {
    if (evt.completed && evt.total) {
      const pct = Math.round((evt.completed / evt.total) * 100);
      const mb = (evt.completed / (1024 * 1024)).toFixed(1);
      const totalMb = (evt.total / (1024 * 1024)).toFixed(1);
      updateProgressText.value = `${pct}% (${mb}/${totalMb} MB)`;
    } else if (evt.status) {
      updateProgressText.value = evt.status.toUpperCase();
    }
  };

  try {
    await invoke("install_update", { tag: `v${updateInfo.value.latest}`, onEvent });
  } catch (e) {
    console.error("In-app update failed, opening release URL:", e);
    const url = (updateInfo.value as any).release_url || "https://github.com/tjcrims0nx/Helix/releases/latest";
    await invoke("open_external_url", { url });
  } finally {
    updating.value = false;
    updateProgressText.value = "";
  }
}

function useSuggestion(text: string) {
  draft.value = text;
}

onMounted(async () => {
  try {
    const savedModel = await invoke<string | null>("db_get_kv", { key: MODEL_KEY });
    if (savedModel) selectedModel.value = savedModel;

  } catch (e) {
    console.warn("Failed to load initial DB state", e);
  }

  startMcpImmediately();

  await loadChats();
  try {
    appVersion.value = await getVersion();
  } catch (e) {
    console.warn("getVersion failed", e);
  }
  await Promise.all([
    detectHardware(),
    checkForUpdate(),
  ]);
  await checkLocal();

  setInterval(sampleUsage, 1000);
  sampleUsage();

  await startMcpImmediately();
});
</script>

<template>
  <!-- Model Manager Modal (LM Studio-style) -->
  <Models
    v-if="showFileManager"
    :managerSearch="managerSearch"
    :managerTab="managerTab"
    :models="models"
    :localGgufs="localGgufs"
    :totalOllamaSize="totalOllamaSize"
    :filteredOllamaModels="filteredOllamaModels"
    :selectedModel="selectedModel"
    :isDeletingModel="isDeletingModel"
    :totalGgufSize="totalGgufSize"
    :filteredGgufs="filteredGgufs"
    :activatedGgufs="activatedGgufs"
    :isImporting="isImporting"
    :isDeletingGguf="isDeletingGguf"
    :searchHuggingFace="searchHuggingFace"
    :listHuggingFaceFiles="listHuggingFaceFiles"
    :pulling="pulling"
    :vramGb="vramGb"
    :localStatus="localStatus"
    :formatBytes="formatBytes"
    :modelKey="modelKey"
    :quantFit="quantFit"
    :fmtSizeGb="fmtSizeGb"
    @update:managerSearch="managerSearch = $event"
    @update:managerTab="managerTab = $event"
    @update:selectedModel="selectedModel = $event"
    @deleteOllamaModel="deleteOllamaModel"
    @activateGguf="activateGguf"
    @moveGguf="moveGguf"
    @deleteGguf="deleteGguf"
    @pullModel="pullModel"
    @importGguf="importGguf"
    @close="showFileManager = false"
  />

  <!-- Files & Pictures Manager (Library + Browse) -->
  <FileBrowser
    :show="showFileBrowser"
    @close="showFileBrowser = false"
    @attachImage="onBrowserAttachImage"
    @attachFile="onBrowserAttachFile"
  />

  <!-- In-app Web Browser (live page + attach screenshot / page text) -->
  <WebBrowser
    :show="showWebBrowser"
    @close="showWebBrowser = false"
    @attachImage="onBrowserAttachImage"
    @attachFile="onBrowserAttachFile"
  />

  <!-- MCP Plugin Manager Modal -->
  <!--
    Tool approval gate. Sits above every other overlay (z-index in style.css) so a
    pending call cannot be hidden behind another panel, and offers no dismiss-by-
    backdrop: the only ways out are Allow and Deny, both of which answer the model.
  -->
  <div v-if="pendingApproval" class="gate-overlay approval-overlay">
    <div class="approval-panel">
      <div class="approval-header">
        <div class="approval-icon">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"></path><line x1="12" y1="9" x2="12" y2="13"></line><line x1="12" y1="17" x2="12.01" y2="17"></line></svg>
        </div>
        <div>
          <h3 class="approval-title">Allow this action?</h3>
          <p class="approval-subtitle">
            The model wants to run <code>{{ pendingApproval.toolName }}</code>
          </p>
        </div>
      </div>

      <pre class="approval-detail">{{ pendingApproval.detail }}</pre>

      <div class="approval-actions">
        <button class="approval-deny" @click="resolveToolApproval(false)">Deny</button>
        <button class="approval-allow" @click="resolveToolApproval(true)">Allow</button>
      </div>
    </div>
  </div>

  <div v-if="showPluginManager" class="gate-overlay" style="background: rgba(0,0,0,0.72); backdrop-filter: blur(24px) saturate(1.4);">
    <div class="manager-panel" style="width: min(900px, 92vw); height: min(850px, 88vh);">
      <div class="manager-header">
        <div class="manager-title-row">
          <div style="width: 36px; height: 36px; border-radius: 8px; background: linear-gradient(135deg, rgba(52, 211, 153, 0.85), rgba(5, 150, 105, 0.85)); display: flex; align-items: center; justify-content: center; box-shadow: 0 4px 14px rgba(52,211,153,0.25);">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M2 12h20"></path><path d="M12 2v20"></path><path d="M20 16a4 4 0 0 0-4-4h-8a4 4 0 0 0-4 4"></path></svg>
          </div>
          <div>
            <h2 class="manager-title" style="letter-spacing: 0.04em;">MCP PLUGINS</h2>
            <p class="manager-subtitle">Extend HELIX with external capabilities</p>
          </div>
          <button class="manager-close" @click="showPluginManager = false">✕</button>
        </div>
      </div>
      <div style="flex: 1; overflow-y: auto; padding: 0;">
        <PluginSettings :apiKey="apiKey" :activePlugins="activePluginIds" @pluginsChanged="handlePluginsChanged" />
      </div>
    </div>
  </div>



  <!-- Top-of-window model pull loader -->
  <div v-if="pulling" class="download-bar" role="progressbar" :aria-valuenow="pulling.pct" aria-valuemin="0" aria-valuemax="100">
    <div class="download-bar-glow"></div>
    <div class="download-bar-fill" :class="{ indeterminate: pulling.pct === 0 }" :style="{ width: pulling.pct > 0 ? pulling.pct + '%' : undefined }"></div>
    <div class="download-loader">
      <div class="download-spinner" aria-hidden="true"></div>
      <div class="download-copy">
        <div class="download-primary">
          <span class="download-bar-label">{{ downloadPhase(pulling.status) }}</span>
          <code class="download-bar-name" :title="pulling.name">{{ pulling.name }}</code>
          <span v-if="pulling.resumed" class="download-bar-resumed" title="Resumed previous download">RESUMED</span>
        </div>
        <div class="download-secondary">
          <span class="download-bar-status">{{ pulling.status }}</span>
          <span v-if="fmtDownloadBytes(pulling.completed, pulling.total)" class="download-bar-bytes">{{ fmtDownloadBytes(pulling.completed, pulling.total) }}</span>
          <span v-if="pulling.bps" class="download-bar-rate">{{ fmtRate(pulling.bps) }}</span>
          <span v-if="pulling.eta && pulling.eta > 0" class="download-bar-eta">ETA {{ fmtEta(pulling.eta) }}</span>
        </div>
      </div>
      <span class="download-bar-pct">{{ pulling.pct }}%</span>
      <button class="download-bar-cancel" title="Cancel download" @click="cancelDownload">✕</button>
    </div>
  </div>

  <div class="shell"
       :class="{ 'shell-with-progress': !!pulling }"
       @drop.prevent="handleDrop"
       @dragover.prevent="dragActive = true"
       @dragleave.prevent="dragActive = false">

    <AnimatedBackground />

    <div v-if="dragActive" class="drag-overlay">
      <div class="drag-overlay-content">
        Drop files here to attach
      </div>
    </div>

    <!-- Sidebar -->
    <aside class="sidebar">

      <div class="brand">
        <div class="brand-logo-container">
          <div class="brand-logo-glow"></div>
          <img src="/helix-logo.png" alt="HELIX" class="brand-logo-img" />
        </div>
        <div style="display: flex; flex-direction: column;">
          <div class="brand-name">HELIX</div>
          <div class="brand-sub">v{{ appVersion }}</div>
        </div>
      </div>

      <button class="new-chat-btn" @click="newChat">
        + NEW CHAT
      </button>

      <div class="chats-list">
        <button
          v-for="c in chats"
          :key="c.id"
          class="chat-item"
          :class="{ active: c.id === activeId }"
          :title="c.title"
          @click="selectChat(c.id)"
          @auxclick="deleteChat(c.id, $event)"
        >
          <div v-if="c.id === activeId" class="active-chat-prism"></div>
          <span style="position: relative; z-index: 10;">{{ c.title }}</span>
        </button>
      </div>

      <div class="sidebar-footer">
        <div class="hw-summary card-metal">
          <div class="hw-line">
            <span class="hw-label" style="color: #60a5fa; width: 38px;">CPU</span>
            <div style="flex: 1; height: 6px; background: rgba(0,0,0,0.6); border-radius: 4px; overflow: hidden; margin: 0 10px; box-shadow: inset 0 1px 3px rgba(0,0,0,0.8), 0 1px 0 rgba(255,255,255,0.05);">
              <div :style="{ width: Math.round(cpuUsage) + '%', height: '100%', background: 'linear-gradient(90deg, #1e3a8a, #60a5fa)', transition: 'width 1s cubic-bezier(0.4, 0, 0.2, 1)', boxShadow: '0 0 10px rgba(96, 165, 250, 0.6)' }"></div>
            </div>
            <span class="hw-val" style="width: 34px; text-align: right; font-variant-numeric: tabular-nums;">{{ Math.round(cpuUsage) }}%</span>
          </div>
          <div class="hw-line">
            <span class="hw-label" style="color: #34d399; width: 38px;">RAM</span>
            <div style="flex: 1; height: 6px; background: rgba(0,0,0,0.6); border-radius: 4px; overflow: hidden; margin: 0 10px; box-shadow: inset 0 1px 3px rgba(0,0,0,0.8), 0 1px 0 rgba(255,255,255,0.05);">
              <div :style="{ width: Math.round(ramUsage) + '%', height: '100%', background: 'linear-gradient(90deg, #064e3b, #34d399)', transition: 'width 1s cubic-bezier(0.4, 0, 0.2, 1)', boxShadow: '0 0 10px rgba(52, 211, 153, 0.6)' }"></div>
            </div>
            <span class="hw-val" :title="ramTooltip" style="width: 34px; text-align: right; font-variant-numeric: tabular-nums;">{{ Math.round(ramUsage) }}%</span>
          </div>
          <div class="hw-line">
            <span class="hw-label" style="color: #a855f7; width: 38px;">VRAM</span>
            <div style="flex: 1; height: 6px; background: rgba(0,0,0,0.6); border-radius: 4px; overflow: hidden; margin: 0 10px; box-shadow: inset 0 1px 3px rgba(0,0,0,0.8), 0 1px 0 rgba(255,255,255,0.05);">
              <div :style="{ width: Math.round(vramUsage ?? 0) + '%', height: '100%', background: 'linear-gradient(90deg, #4c1d95, #a855f7)', transition: 'width 1s cubic-bezier(0.4, 0, 0.2, 1)', boxShadow: '0 0 10px rgba(168, 85, 247, 0.6)' }"></div>
            </div>
            <span class="hw-val" :title="vramTooltip" style="width: 34px; text-align: right; font-variant-numeric: tabular-nums;">
              {{ vramUsage === null ? '—' : Math.round(vramUsage) + '%' }}
            </span>
          </div>
        </div>

        <button
          class="sidebar-action-btn"
          :class="{ 'update-btn-available': updateInfo?.available && !updating }"
          @click="handleUpdate"
          :disabled="updating || !updateInfo?.available"
          style="position: relative;"
        >
          <span v-if="updateInfo?.available && !updating" class="update-badge-dot"></span>
          <span v-if="updating">{{ updateProgressText || 'UPDATING...' }}</span>
          <template v-else-if="updateInfo?.available">
            🚀 UPDATE TO v{{ updateInfo.latest }}
          </template>
          <span v-else-if="updateInfo">v{{ updateInfo.current }} · LATEST</span>
          <span v-else>CHECKING…</span>
        </button>

        <button class="sidebar-action-btn" @click="openFileManager">
          MODEL MANAGER
        </button>

        <button class="sidebar-action-btn" @click="showPluginManager = true">
          🔌 MCP PLUGINS
        </button>

      </div>
    </aside>

    <!-- Main -->
    <main class="main">
      <header class="main-header">
        <div style="display: flex; align-items: center; gap: 10px;">
          <h1 style="font-size: 0.82rem; font-weight: 600; letter-spacing: 0.08em; text-transform: uppercase; color: rgba(255,255,255,0.45); margin: 0;">HELIX</h1>
        </div>
        <div style="display: flex; align-items: center; gap: 8px;">
          <div class="model-tag-display" v-if="selectedModel" style="background: rgba(168,85,247,0.12); border: 1px solid rgba(168,85,247,0.25); color: #d8b4fe; font-size: 0.75rem; padding: 4px 10px; border-radius: 6px; display: flex; align-items: center; gap: 6px;">
            <span>{{ selectedModel }}</span>
            <span v-if="selectedModelSize" class="model-size-badge" style="background: rgba(255,255,255,0.12); padding: 1px 6px; border-radius: 4px; font-weight: 800; color: #fff; font-size: 0.7rem;">💾 {{ selectedModelSize }}</span>
          </div>
          <!-- Window controls -->
          <div class="window-controls">
            <button class="win-btn" @click="minimizeWindow" title="Minimize">
              <svg width="10" height="1" viewBox="0 0 10 1"><rect width="10" height="1" fill="currentColor"/></svg>
            </button>
            <button class="win-btn" @click="toggleMaximize" :title="isMaximized ? 'Restore' : 'Maximize'">
              <svg v-if="!isMaximized" width="10" height="10" viewBox="0 0 10 10"><rect x="0.5" y="0.5" width="9" height="9" fill="none" stroke="currentColor" stroke-width="1"/></svg>
              <svg v-else width="10" height="10" viewBox="0 0 10 10"><rect x="2.5" y="0.5" width="7" height="7" fill="none" stroke="currentColor" stroke-width="1"/><rect x="0.5" y="2.5" width="7" height="7" fill="none" stroke="currentColor" stroke-width="1"/></svg>
            </button>
            <button class="win-btn win-close" @click="closeWindow" title="Close">
              <svg width="10" height="10" viewBox="0 0 10 10"><line x1="0" y1="0" x2="10" y2="10" stroke="currentColor" stroke-width="1.2"/><line x1="10" y1="0" x2="0" y2="10" stroke="currentColor" stroke-width="1.2"/></svg>
            </button>
          </div>
        </div>
      </header>

      <div class="chats-container" ref="chatsContainer">
        <div class="chats-inner">
          <ChatView
            v-if="activeChat && activeChat.messages.length > 0"
            :activeChat="activeChat"
            :models="models"
            :streaming="streaming"
            :streamingContent="streamingContent"
            :lastTtft="lastTtft"
            :lastTps="lastTps"
            :renderMarkdown="renderMarkdown"
            :stripThinkTags="stripThinkTags"
          />

          <Dashboard
            v-else
            :localStatus="localStatus"
            :models="models"
            :appVersion="appVersion"
            @useSuggestion="useSuggestion"
            @openFileManager="openFileManager"
            @openPluginManager="showPluginManager = true"
          />
        </div>
      </div>

      <div class="composer">
        <div v-if="attachedImages.length > 0 || attachedFiles.length > 0" class="composer-attachments">
          <div v-for="(img, idx) in attachedImages" :key="'img'+idx" class="attachment-thumb">
            <img :src="'data:image/jpeg;base64,' + img" />
            <button @click="removeImage(idx)" class="remove-btn">✕</button>
          </div>
          <div v-for="(file, idx) in attachedFiles" :key="'file'+idx" class="attachment-file-pill">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="margin-right: 6px;"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"></path><polyline points="14 2 14 8 20 8"></polyline><line x1="16" y1="13" x2="8" y2="13"></line><line x1="16" y1="17" x2="8" y2="17"></line><polyline points="10 9 9 9 8 9"></polyline></svg>
            {{ file.name }}
            <button @click="removeFile(idx)" class="remove-btn" style="margin-left: 8px;">✕</button>
          </div>
        </div>
        <div class="composer-wrapper" style="max-width: 700px; margin: 0 auto; width: 100%;">
          <div class="composer-inner" style="background: linear-gradient(135deg, #180530 0%, #050505 100%); border: 1px solid rgba(168,85,247,0.15); border-radius: 28px; box-shadow: 0 15px 50px rgba(0,0,0,0.8), inset 0 1px 0 rgba(255,255,255,0.05); padding: 0.8rem 1rem; position: relative; display: flex; align-items: flex-end; gap: 12px; transition: all 0.3s ease;">
            <div class="prism-edge" style="position: absolute; top: 0; left: 0; right: 0; height: 1px; border-radius: 28px 28px 0 0;"></div>

            <div style="position: relative;">
              <button class="attach-btn" @click="showAttachMenu = !showAttachMenu" style="width: 40px; height: 40px; border-radius: 50%; background: rgba(255,255,255,0.02); border: 1px solid rgba(255,255,255,0.05); display: flex; align-items: center; justify-content: center; color: rgba(255,255,255,0.5); flex-shrink: 0; transition: all 0.2s ease; margin-bottom: 0;" title="Attach options">
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><line x1="12" y1="5" x2="12" y2="19"></line><line x1="5" y1="12" x2="19" y2="12"></line></svg>
              </button>

              <div v-if="showAttachMenu" style="position: absolute; bottom: 50px; left: 0; padding: 0.5rem; border-radius: 12px; display: flex; flex-direction: column; width: 180px; z-index: 100; background: #1e1e1e; border: 1px solid rgba(255,255,255,0.08); box-shadow: 0 4px 20px rgba(0,0,0,0.5);">
                <div style="font-size: 0.75rem; color: rgba(255,255,255,0.4); font-weight: 600; padding: 8px 12px; margin-bottom: 4px;">Add Context</div>
                <button @click="showFileBrowser = true; showAttachMenu = false" class="attach-menu-item">
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="opacity: 0.7;"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path></svg>
                  <span>Files &amp; Pictures</span>
                </button>
                <button @click="showWebBrowser = true; showAttachMenu = false" class="attach-menu-item">
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="opacity: 0.7;"><circle cx="12" cy="12" r="10"></circle><line x1="2" y1="12" x2="22" y2="12"></line><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"></path></svg>
                  <span>Web Browser</span>
                </button>
                <button @click="triggerImageInput(); showAttachMenu = false" class="attach-menu-item">
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="opacity: 0.7;"><rect x="3" y="3" width="18" height="18" rx="2" ry="2"></rect><circle cx="8.5" cy="8.5" r="1.5"></circle><polyline points="21 15 16 10 5 21"></polyline></svg>
                  <span>Media</span>
                </button>
                <button @click="triggerFileInput(); showAttachMenu = false" class="attach-menu-item">
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="opacity: 0.7;"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"></path><polyline points="14 2 14 8 20 8"></polyline><line x1="16" y1="13" x2="8" y2="13"></line><line x1="16" y1="17" x2="8" y2="17"></line><polyline points="10 9 9 9 8 9"></polyline></svg>
                  <span>File</span>
                </button>
                <button @click="showPluginManager = true; showAttachMenu = false" class="attach-menu-item">
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="opacity: 0.7;"><path d="M2 12h20"></path><path d="M12 2v20"></path><path d="M20 16a4 4 0 0 0-4-4h-8a4 4 0 0 0-4 4"></path></svg>
                  <span>MCP Plugins</span>
                </button>
              </div>
            </div>
            <input type="file" ref="fileInput" @change="handleFileSelect" style="display: none" multiple />
            <input type="file" ref="imageInput" accept="image/*" @change="handleFileSelect" style="display: none" multiple />

            <textarea
              v-model="draft"
              placeholder="Message HELIX…"
              rows="1"
              style="flex: 1; background: transparent; border: none; color: #fff; font-size: 0.95rem; line-height: 24px; resize: none; min-height: 24px; max-height: 200px; padding: 8px 0; outline: none; margin-bottom: 0;"
              @keydown="onComposerKeydown"
              @input="autosizeComposer"
            ></textarea>
            <button
              v-if="streaming"
              class="stop-btn"
              style="width: 40px; height: 40px; border-radius: 14px; background: linear-gradient(135deg, #ef4444, #7f1d1d); border: 1px solid #450a0a; color: #fff; display: flex; align-items: center; justify-content: center; box-shadow: 0 4px 15px rgba(239,68,68,0.3), inset 0 1px 0 rgba(255,255,255,0.2); flex-shrink: 0;"
              @click="stopChat"
              aria-label="Stop generating"
              title="Stop generating"
            >
              <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor">
                <rect x="6" y="6" width="12" height="12" rx="2" />
              </svg>
            </button>
            <button
              v-else
              class="send-btn btn-metal-dark"
              style="width: 40px; height: 40px; border-radius: 14px; display: flex; align-items: center; justify-content: center; padding: 0; flex-shrink: 0;"
              @click="send"
              :disabled="!draft.trim() && attachedImages.length === 0 && attachedFiles.length === 0"
            >
              <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="#fff" stroke-width="2"><path d="M22 2L11 13M22 2l-7 20-4-9-9-4 20-7z"></path></svg>
            </button>
          </div>
          <div style="font-size: 0.65rem; color: rgba(255,255,255,0.3); text-align: left; padding-left: 1.2rem; margin-top: 8px;">
            <strong style="color: #fff">Enter</strong> to send • <strong>Shift+Enter</strong> for newline
          </div>
        </div>
      </div>
    </main>
  </div>
</template>

<style scoped>
.model-tag-display {
  font-size: 0.68rem;
  color: var(--text-secondary);
  background: var(--bg-frost);
  backdrop-filter: blur(8px);
  border: 1px solid var(--glass-border);
  padding: 4px 12px;
  border-radius: 50px;
  letter-spacing: 1.5px;
  text-transform: uppercase;
  font-family: 'JetBrains Mono', monospace;
  font-weight: 500;
}

.hw-summary {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 12px 16px;
  border-radius: 16px;
  margin-bottom: 4px;
}
.hw-line {
  display: flex;
  justify-content: space-between;
  gap: 8px;
  align-items: center;
}
.hw-label {
  letter-spacing: 2px;
  font-weight: 800;
  text-transform: uppercase;
  font-size: 0.65rem;
  font-family: 'JetBrains Mono', monospace;
}
.hw-val {
  color: #fff;
  font-family: 'JetBrains Mono', monospace;
  font-size: 0.75rem;
  font-weight: 600;
  text-align: right;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.sidebar-action-btn {
  width: 100%;
  text-align: left;
  background: linear-gradient(135deg, rgba(255, 255, 255, 0.03) 0%, rgba(0, 0, 0, 0.5) 100%);
  border: 1px solid rgba(0, 0, 0, 0.8);
  border-top: 1px solid rgba(255, 255, 255, 0.1);
  box-shadow:
    0 4px 10px rgba(0, 0, 0, 0.5),
    inset 0 1px 3px rgba(255, 255, 255, 0.05),
    inset 0 -2px 5px rgba(0, 0, 0, 0.4),
    inset 1px 0 2px rgba(255, 255, 255, 0.02),
    inset -1px 0 2px rgba(0, 0, 0, 0.3);
  color: rgba(255, 255, 255, 0.5);
  padding: 10px 16px;
  border-radius: 12px;
  font-size: 0.7rem;
  letter-spacing: 2px;
  font-weight: 800;
  text-transform: uppercase;
  transition: all 200ms cubic-bezier(0.4, 0, 0.2, 1);
}
.sidebar-action-btn:hover {
  background: linear-gradient(135deg, rgba(255, 255, 255, 0.06) 0%, rgba(20, 20, 20, 0.6) 100%);
  border-color: rgba(0, 0, 0, 1);
  border-top: 1px solid rgba(255, 255, 255, 0.15);
  color: #fff;
  transform: translateY(-2px);
  box-shadow:
    0 6px 15px rgba(0, 0, 0, 0.7),
    inset 0 1px 3px rgba(255, 255, 255, 0.1),
    inset 0 -2px 5px rgba(0, 0, 0, 0.6),
    inset 1px 0 2px rgba(255, 255, 255, 0.04),
    inset -1px 0 2px rgba(0, 0, 0, 0.4);
}
.sidebar-action-btn.danger {
  color: #ef4444;
}
.sidebar-action-btn.danger:hover {
  background: linear-gradient(135deg, rgba(239, 68, 68, 0.15) 0%, rgba(100, 10, 10, 0.6) 100%);
  border-top: 1px solid rgba(239, 68, 68, 0.4);
  color: #fca5a5;
}

.update-btn-available {
  background: linear-gradient(135deg, rgba(147, 51, 234, 0.7), rgba(88, 28, 135, 0.9)) !important;
  border-color: rgba(168, 85, 247, 0.6) !important;
  color: #fff !important;
  box-shadow:
    0 4px 20px rgba(147, 51, 234, 0.4),
    0 0 30px rgba(168, 85, 247, 0.15),
    inset 0 1px 2px rgba(255, 255, 255, 0.15);
  animation: update-glow-pulse 2.4s infinite ease-in-out;
  cursor: pointer !important;
}
.update-btn-available:hover {
  background: linear-gradient(135deg, rgba(168, 85, 247, 0.85), rgba(107, 33, 168, 0.95)) !important;
  box-shadow:
    0 6px 30px rgba(147, 51, 234, 0.6),
    0 0 50px rgba(168, 85, 247, 0.25),
    inset 0 1px 2px rgba(255, 255, 255, 0.2);
}
@keyframes update-glow-pulse {
  0%, 100% {
    box-shadow:
      0 4px 20px rgba(147, 51, 234, 0.4),
      0 0 30px rgba(168, 85, 247, 0.15),
      inset 0 1px 2px rgba(255, 255, 255, 0.15);
  }
  50% {
    box-shadow:
      0 4px 30px rgba(147, 51, 234, 0.7),
      0 0 50px rgba(168, 85, 247, 0.3),
      inset 0 1px 2px rgba(255, 255, 255, 0.2);
  }
}
.update-badge-dot {
  position: absolute;
  top: -4px;
  right: -4px;
  width: 12px;
  height: 12px;
  background: radial-gradient(circle, #c084fc 0%, #a855f7 50%, #7c3aed 100%);
  border-radius: 50%;
  border: 2px solid rgba(15, 10, 25, 0.8);
  box-shadow: 0 0 8px rgba(168, 85, 247, 0.8), 0 0 16px rgba(147, 51, 234, 0.4);
  animation: dot-pulse 1.8s infinite ease-in-out;
  z-index: 10;
  pointer-events: none;
}
@keyframes dot-pulse {
  0%, 100% {
    transform: scale(1);
    box-shadow: 0 0 8px rgba(168, 85, 247, 0.8), 0 0 16px rgba(147, 51, 234, 0.4);
  }
  50% {
    transform: scale(1.25);
    box-shadow: 0 0 14px rgba(168, 85, 247, 1), 0 0 28px rgba(147, 51, 234, 0.6);
  }
}

/* ─── Model Manager Panel (LM Studio-style) ──────────────────────────── */
.manager-panel {
  width: 94%;
  max-width: 720px;
  max-height: 85vh;
  display: flex;
  flex-direction: column;
  background: var(--bg-frost-deep);
  backdrop-filter: blur(var(--frost-blur-heavy));
  -webkit-backdrop-filter: blur(var(--frost-blur-heavy));
  border: 1px solid var(--glass-border-purp);
  border-radius: var(--radius-xl);
  box-shadow:
    0 30px 80px -20px rgba(0,0,0,0.9),
    inset 0 2px 4px rgba(255,255,255,0.05),
    inset 0 -2px 6px rgba(0,0,0,0.4),
    0 0 80px -20px rgba(168, 85, 247, 0.15);
  animation: gateIn 350ms var(--ease-spring);
  overflow: hidden;
}

.manager-header {
  padding: 1.5rem 1.5rem 0;
  flex-shrink: 0;
}

.manager-title-row {
  display: flex;
  align-items: center;
  gap: 12px;
}

.manager-title {
  font-size: 1.2rem;
  font-weight: 900;
  letter-spacing: 3px;
  color: #fff;
  margin: 0;
  line-height: 1.2;
}

.manager-subtitle {
  font-size: 0.7rem;
  color: var(--text-muted);
  margin: 2px 0 0;
  letter-spacing: 0.5px;
}

.manager-close {
  margin-left: auto;
  background: none;
  border: 1px solid var(--glass-border);
  color: var(--text-muted);
  width: 30px;
  height: 30px;
  border-radius: 6px;
  cursor: pointer;
  font-size: 0.8rem;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 150ms ease;
}
.manager-close:hover {
  color: var(--red-400);
  border-color: var(--glass-border-red);
  background: var(--red-glow-dim);
}

.manager-search-row {
  margin-top: 1rem;
}

.manager-search {
  width: 100%;
  background: rgba(0, 0, 0, 0.35);
  border: 1px solid var(--glass-border);
  border-radius: var(--radius-md);
  padding: 10px 14px;
  color: #fff;
  font-family: 'JetBrains Mono', monospace;
  font-size: 0.8rem;
  outline: none;
  transition: border-color 180ms ease;
  box-sizing: border-box;
}
.manager-search:focus {
  border-color: var(--glass-border-red);
  box-shadow: 0 0 0 3px var(--red-glow-dim);
}
.manager-search::placeholder {
  color: var(--text-muted);
}

.manager-tabs {
  display: flex;
  gap: 0;
  margin-top: 1rem;
  border-bottom: 1px solid var(--glass-border);
}

.manager-tab {
  flex: 1;
  background: none;
  border: none;
  border-bottom: 2px solid transparent;
  color: var(--text-muted);
  padding: 10px 12px;
  font-size: 0.65rem;
  font-weight: 800;
  letter-spacing: 2px;
  text-transform: uppercase;
  cursor: pointer;
  transition: all 150ms ease;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
}
.manager-tab:hover {
  color: var(--text-secondary);
}
.manager-tab.active {
  color: var(--red-400);
  border-bottom-color: var(--red-400);
}

.manager-tab-count {
  background: rgba(255, 255, 255, 0.08);
  border: 1px solid var(--glass-border);
  padding: 1px 7px;
  border-radius: 10px;
  font-size: 0.6rem;
  font-weight: 700;
  color: var(--text-muted);
}
.manager-tab.active .manager-tab-count {
  background: var(--red-glow-dim);
  border-color: rgba(220, 38, 38, 0.3);
  color: var(--red-400);
}

.manager-body {
  flex: 1;
  overflow-y: auto;
  padding: 1rem 1.5rem;
  min-height: 0;
}

.manager-disk-bar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  background: rgba(0, 0, 0, 0.25);
  border: 1px solid var(--glass-border);
  border-radius: var(--radius-sm);
  padding: 8px 12px;
  margin-bottom: 0.75rem;
}

.manager-disk-label {
  font-size: 0.6rem;
  font-weight: 800;
  letter-spacing: 2px;
  color: var(--text-muted);
  text-transform: uppercase;
}

.manager-disk-value {
  font-family: 'JetBrains Mono', monospace;
  font-size: 0.75rem;
  font-weight: 700;
  color: var(--red-400);
}

.manager-hint {
  font-size: 0.72rem;
  color: var(--text-muted);
  line-height: 1.5;
  margin-bottom: 0.75rem;
}
.manager-hint code {
  background: var(--bg-frost);
  border: 1px solid var(--glass-border);
  padding: 1px 5px;
  border-radius: 3px;
  font-family: 'JetBrains Mono', monospace;
  color: var(--text-secondary);
  font-size: 0.65rem;
}

.manager-empty {
  text-align: center;
  padding: 2.5rem 1rem;
  color: var(--text-muted);
  font-size: 0.8rem;
  line-height: 1.6;
}

.manager-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

/* ─── Model Cards ─────────────────────────────────────────────────────── */
.model-card {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  background: rgba(0, 0, 0, 0.2);
  border: 1px solid var(--glass-border);
  border-radius: var(--radius-md);
  padding: 12px 14px;
  transition: all 180ms ease;
}
.model-card:hover {
  border-color: rgba(220, 38, 38, 0.25);
  background: rgba(220, 38, 38, 0.04);
}

.model-card-main {
  display: flex;
  align-items: center;
  gap: 12px;
  min-width: 0;
  flex: 1;
}

.model-card-icon {
  width: 36px;
  height: 36px;
  flex-shrink: 0;
  border-radius: 8px;
  background: linear-gradient(135deg, var(--red-600), #8b0000);
  color: #fff;
  font-weight: 900;
  font-size: 0.9rem;
  display: flex;
  align-items: center;
  justify-content: center;
  box-shadow: 0 0 12px var(--red-glow-dim);
}
.model-card-icon.file-icon {
  background: linear-gradient(135deg, rgba(100, 116, 139, 0.4), rgba(100, 116, 139, 0.2));
  box-shadow: none;
  font-size: 1rem;
}

.model-card-info {
  min-width: 0;
  flex: 1;
}

.model-card-name {
  font-family: 'JetBrains Mono', monospace;
  font-size: 0.78rem;
  font-weight: 700;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.model-card-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 5px;
  margin-top: 5px;
}

.model-tag {
  font-size: 0.58rem;
  font-weight: 700;
  letter-spacing: 0.5px;
  padding: 2px 7px;
  border-radius: 4px;
  background: rgba(255, 255, 255, 0.06);
  border: 1px solid rgba(255, 255, 255, 0.08);
  color: var(--text-muted);
  font-family: 'JetBrains Mono', monospace;
  text-transform: uppercase;
}
.model-tag.quant {
  background: rgba(220, 38, 38, 0.1);
  border-color: rgba(220, 38, 38, 0.2);
  color: var(--red-400);
}
.model-tag.param {
  background: rgba(59, 130, 246, 0.1);
  border-color: rgba(59, 130, 246, 0.2);
  color: #60a5fa;
}
.model-tag.family {
  background: rgba(168, 85, 247, 0.1);
  border-color: rgba(168, 85, 247, 0.2);
  color: #c084fc;
}

.model-card-actions {
  display: flex;
  gap: 6px;
  flex-shrink: 0;
}

.model-action-btn {
  background: rgba(255, 255, 255, 0.05);
  border: 1px solid var(--glass-border);
  color: var(--text-secondary);
  padding: 5px 10px;
  border-radius: var(--radius-sm);
  font-size: 0.6rem;
  font-weight: 800;
  letter-spacing: 1.5px;
  text-transform: uppercase;
  cursor: pointer;
  transition: all 150ms ease;
  white-space: nowrap;
}
.model-action-btn:hover:not(:disabled) {
  background: var(--bg-frost);
  color: var(--text-primary);
  transform: translateY(-1px);
}
.model-action-btn.use {
  background: linear-gradient(180deg, var(--red-500), #8b0000);
  border: 1px solid #400;
  color: #fff;
  box-shadow: inset 0 1px 1px rgba(255,255,255,0.3), 0 2px 5px rgba(0,0,0,0.4);
}
.model-action-btn.use:hover {
  filter: brightness(1.1);
  transform: translateY(-1px);
  box-shadow: inset 0 1px 1px rgba(255,255,255,0.4), 0 4px 12px var(--red-glow);
}
.model-action-btn.danger:hover:not(:disabled) {
  background: rgba(220, 38, 38, 0.3);
  border-color: var(--red-600);
  color: #fff;
}
.model-action-btn:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}
.quant-buttons {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}
.quant-size {
  margin-left: 6px;
  opacity: 0.75;
  font-weight: 500;
  font-variant-numeric: tabular-nums;
}
/* Tight fit — yellow border to signal partial-offload risk */
.model-action-btn.use.fit-tight {
  background: linear-gradient(180deg, #b25f00, #6a3500);
  border: 1px solid rgba(255, 200, 100, 0.6);
}
.model-action-btn.use.fit-tight:hover {
  filter: brightness(1.1);
  box-shadow: inset 0 1px 1px rgba(255,255,255,0.4), 0 4px 12px rgba(255, 170, 60, 0.25);
}
/* Too big — dim and stamp warning */
.model-action-btn.use.fit-too-big {
  background: linear-gradient(180deg, #4a1218, #2a0608);
  border: 1px solid rgba(255, 80, 80, 0.5);
  opacity: 0.85;
}
.model-action-btn.use.fit-too-big::after {
  content: " ⚠";
  margin-left: 2px;
}
.model-action-btn.use.fit-too-big:hover {
  filter: brightness(1.05);
}
.model-action-btn.success {
  background: rgba(16, 185, 129, 0.15);
  color: #10b981;
  border-color: rgba(16, 185, 129, 0.3);
}
.model-action-btn.success:disabled {
  opacity: 1;
  cursor: default;
}

.model-active-badge {
  font-size: 0.55rem;
  font-weight: 800;
  letter-spacing: 2px;
  color: #4ade80;
  padding: 5px 8px;
  border: 1px solid rgba(74, 222, 128, 0.25);
  border-radius: var(--radius-sm);
  background: rgba(74, 222, 128, 0.08);
}

/* ─── Manager Footer ──────────────────────────────────────────────────── */
.manager-footer {
  flex-shrink: 0;
  padding: 1rem 1.5rem;
  border-top: 1px solid var(--glass-border);
  display: flex;
  gap: 8px;
}

.close-modal-btn {
  flex: 1;
  background: linear-gradient(180deg, #1f1f26 0%, #121218 100%);
  border: 1px solid rgba(0,0,0,0.8);
  box-shadow: inset 0 1px 1px rgba(255,255,255,0.15), 0 4px 10px rgba(0,0,0,0.4);
  color: var(--text-primary);
  padding: 10px;
  border-radius: var(--radius-md);
  font-weight: 800;
  letter-spacing: 2px;
  font-size: 0.7rem;
  cursor: pointer;
  transition: all 150ms ease;
}
.close-modal-btn:hover {
  background: linear-gradient(180deg, #2a2a33 0%, #1a1a22 100%);
  color: #fff;
  transform: translateY(-1px);
}
.close-modal-btn:active {
  transform: translateY(1px);
  box-shadow: inset 0 2px 4px rgba(0,0,0,0.6);
}

.import-btn {
  flex: 1;
  background: linear-gradient(180deg, var(--red-500), #8b0000);
  border: 1px solid #400;
  color: #fff;
  box-shadow: inset 0 1px 1px rgba(255,255,255,0.3), 0 4px 10px rgba(0,0,0,0.4);
  padding: 10px 16px;
  border-radius: var(--radius-md);
  font-size: 0.7rem;
  font-weight: 800;
  letter-spacing: 2px;
  text-transform: uppercase;
  cursor: pointer;
  transition: all 180ms var(--ease-out);
}
.import-btn:hover:not(:disabled) {
  filter: brightness(1.1);
  transform: translateY(-2px);
  box-shadow: inset 0 1px 1px rgba(255,255,255,0.4), 0 8px 24px -4px var(--red-glow);
}
.import-btn:active:not(:disabled) {
  transform: translateY(1px);
  box-shadow: inset 0 2px 6px rgba(0,0,0,0.6);
  background: #8b0000;
}
.import-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
  box-shadow: none;
}
@keyframes update-pulse {
  0%, 100% { box-shadow: 0 0 0 1px var(--red-500), 0 6px 18px var(--red-glow); }
  50%      { box-shadow: 0 0 0 1px var(--red-400), 0 8px 24px var(--red-glow); }
}
.update-dot {
  display: inline-block;
  width: 6px;
  height: 6px;
  margin-right: 6px;
  border-radius: 50%;
  background: #fff;
  box-shadow: 0 0 6px #fff;
  animation: dot-pulse 1.4s infinite ease-in-out;
  vertical-align: middle;
}
@keyframes dot-pulse {
  0%, 100% { opacity: 0.6; }
  50%      { opacity: 1; }
}

/* API Key Gate */
.shell-blocked {
  filter: blur(10px) brightness(0.3);
  pointer-events: none;
  user-select: none;
}

.key-gate {
  position: fixed;
  inset: 0;
  z-index: 1000;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 1.5rem;
  background: rgba(2, 2, 4, 0.7);
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
}

.key-card {
  width: 100%;
  max-width: 440px;
  background: linear-gradient(180deg, #101015 0%, #060609 100%);
  border: 1px solid #000;
  box-shadow: inset 0 1px 1px rgba(255,255,255,0.06), 0 40px 100px -20px rgba(0,0,0,1);
  border-radius: var(--radius-xl);
  padding: 2.5rem 2rem;
  text-align: center;
  animation: gateIn 350ms var(--ease-spring);
}
@keyframes gateIn {
  from { opacity: 0; transform: translateY(16px) scale(0.96); }
  to   { opacity: 1; transform: none; }
}

.key-logo {
  width: 60px; height: 60px;
  margin: 0 auto 1.25rem;
  border-radius: var(--radius-md);
  background: linear-gradient(135deg, var(--red-600), #8b0000);
  display: flex; align-items: center; justify-content: center;
  color: #fff; font-weight: 900; font-size: 1.6rem;
  box-shadow: 0 0 40px var(--red-glow);
  animation: logo-pulse 4s infinite alternate ease-in-out;
  position: relative;
}
.key-logo::after {
  content: '';
  position: absolute;
  inset: -1px;
  border-radius: inherit;
  background: linear-gradient(135deg, var(--red-500), transparent);
  z-index: -1;
  opacity: 0.3;
  filter: blur(1px);
}
.key-logo-img {
  width: 72px; height: 72px;
  margin: 0 auto 1.25rem;
  border-radius: var(--radius-md);
  object-fit: contain;
  filter: drop-shadow(0 0 24px var(--red-glow));
  animation: logo-pulse 4s infinite alternate ease-in-out;
}

.key-eyebrow {
  color: var(--red-400);
  font-weight: 700;
  letter-spacing: 2.5px;
  text-transform: uppercase;
  font-size: 0.72rem;
  margin-bottom: 0.4rem;
}

.key-title {
  font-size: 2.2rem !important;
  letter-spacing: 4px !important;
  margin-bottom: 0.6rem !important;
  color: #fff !important;
}

.key-sub {
  color: var(--text-secondary);
  font-size: 0.88rem;
  line-height: 1.55;
  margin-bottom: 1.6rem;
}
.key-sub a { color: var(--red-400); font-weight: 600; }

.key-form {
  display: flex;
  flex-direction: column;
  gap: 0.55rem;
}

.key-form input {
  background: rgba(0,0,0,0.45);
  border: 1px solid var(--glass-border);
  border-radius: var(--radius-md);
  padding: 13px 16px;
  color: #fff;
  font-family: 'JetBrains Mono', monospace;
  font-size: 0.9rem;
  letter-spacing: 1.5px;
  text-align: center;
  outline: none;
  transition: all 180ms var(--ease-out);
}
.key-form input:focus {
  border-color: var(--glass-border-red);
  box-shadow: 0 0 0 3px var(--red-glow-dim);
}

.key-form button {
  border: 1px solid rgba(220, 38, 38, 0.4);
  background: linear-gradient(135deg, var(--red-600) 0%, #8b0000 100%);
  color: #fff;
  padding: 13px 16px;
  border-radius: var(--radius-md);
  font-weight: 800;
  letter-spacing: 3px;
  font-size: 0.8rem;
  text-transform: uppercase;
  box-shadow: 0 12px 32px -12px var(--red-glow);
  transition: all 180ms var(--ease-out);
}
.key-form button:hover:not(:disabled) {
  transform: translateY(-1px);
  box-shadow: 0 16px 40px -12px var(--red-glow);
  filter: brightness(1.1);
}
.key-form button:disabled { opacity: 0.35; cursor: not-allowed; }

.key-error {
  color: var(--err);
  font-size: 0.8rem;
  margin-top: 0.7rem;
  font-weight: 600;
}

.key-foot {
  margin-top: 1.5rem;
  font-size: 0.68rem;
  color: var(--text-muted);
  line-height: 1.55;
}
.key-foot code {
  background: var(--bg-frost);
  border: 1px solid var(--glass-border);
  padding: 1px 6px;
  border-radius: 3px;
  font-family: 'JetBrains Mono', monospace;
  color: var(--text-secondary);
  font-size: 0.64rem;
}

/* Banner retry button */
.banner-retry {
  margin-left: 0.6rem;
  padding: 3px 10px;
  border-radius: 6px;
  border: 1px solid rgba(255,255,255,0.15);
  background: rgba(255,255,255,0.04);
  color: var(--text-primary);
  font-size: 0.7rem;
  font-weight: 700;
  letter-spacing: 1.5px;
  text-transform: uppercase;
  transition: all 150ms var(--ease-out);
}
.banner-retry:hover {
  border-color: var(--glass-border-red);
  background: var(--red-glow-dim);
  color: var(--red-400);
}

/* Top-of-window download progress bar */
.download-bar {
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  height: 68px;
  z-index: 99999;
  background:
    radial-gradient(circle at 8% 0%, rgba(220, 38, 38, 0.26), transparent 34%),
    radial-gradient(circle at 90% 0%, rgba(124, 58, 237, 0.24), transparent 30%),
    rgba(2, 2, 4, 0.94);
  backdrop-filter: blur(16px);
  -webkit-backdrop-filter: blur(16px);
  border-bottom: 1px solid rgba(248, 113, 113, 0.28);
  overflow: hidden;
  box-shadow: 0 14px 38px rgba(0, 0, 0, 0.58), inset 0 1px 0 rgba(255, 255, 255, 0.08);
}
.download-bar-fill {
  position: absolute;
  left: 0;
  bottom: 0;
  height: 5px;
  background: linear-gradient(90deg, #dc2626, #f97316, #7c3aed);
  box-shadow: 0 0 18px rgba(248, 113, 113, 0.65), inset 0 0 12px rgba(255, 255, 255, 0.24);
  transition: width 240ms var(--ease-out);
  width: 0;
}
.download-bar-fill.indeterminate {
  width: 28% !important;
  animation: download-indeterminate 1.4s ease-in-out infinite;
}
@keyframes download-indeterminate {
  0%   { left: -30%; }
  100% { left: 100%; }
}
.download-bar-glow {
  position: absolute;
  inset: 0;
  opacity: 0.36;
  background: linear-gradient(115deg, transparent 12%, rgba(255, 255, 255, 0.12) 34%, transparent 52%);
  animation: download-sheen 2.4s ease-in-out infinite;
}
@keyframes download-sheen {
  0% { transform: translateX(-45%); }
  100% { transform: translateX(45%); }
}
.download-loader {
  position: relative;
  z-index: 1;
  height: 100%;
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 0 18px;
  font-family: 'JetBrains Mono', monospace;
  color: #fff;
  text-shadow: 0 1px 4px rgba(0, 0, 0, 0.8);
  white-space: nowrap;
  overflow: hidden;
}
.download-spinner {
  width: 34px;
  height: 34px;
  flex: 0 0 auto;
  border-radius: 50%;
  border: 2px solid rgba(255, 255, 255, 0.13);
  border-top-color: #fb7185;
  border-right-color: #a78bfa;
  box-shadow: 0 0 22px rgba(248, 113, 113, 0.22);
  animation: download-spin 800ms linear infinite;
}
@keyframes download-spin {
  to { transform: rotate(360deg); }
}
.download-copy {
  min-width: 0;
  display: grid;
  gap: 5px;
  flex: 1;
}
.download-primary,
.download-secondary {
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 10px;
}
.download-secondary {
  font-size: 0.72rem;
}
.download-bar-label {
  font-weight: 800;
  letter-spacing: 2.5px;
  color: #fff;
  flex-shrink: 0;
}
.download-bar-name {
  font-weight: 600;
  background: rgba(0, 0, 0, 0.35);
  border: 1px solid rgba(255, 255, 255, 0.15);
  padding: 2px 8px;
  border-radius: 4px;
  overflow: hidden;
  text-overflow: ellipsis;
  min-width: 0;
  max-width: min(52vw, 560px);
}
.download-bar-status {
  color: rgba(255, 255, 255, 0.75);
  text-transform: lowercase;
  letter-spacing: 0.5px;
  flex-shrink: 1;
  overflow: hidden;
  text-overflow: ellipsis;
}
.download-bar-pct {
  min-width: 58px;
  font-weight: 800;
  font-size: 1.05rem;
  letter-spacing: 0;
  text-align: right;
  flex-shrink: 0;
}
.download-bar-rate,
.download-bar-eta,
.download-bar-bytes {
  font-size: 0.78rem;
  color: rgba(255, 255, 255, 0.7);
  font-variant-numeric: tabular-nums;
  flex-shrink: 0;
  white-space: nowrap;
}
.download-bar-resumed {
  color: #fde68a;
  font-size: 0.66rem;
  border: 1px solid rgba(253, 230, 138, 0.24);
  border-radius: 999px;
  padding: 2px 7px;
  background: rgba(253, 230, 138, 0.08);
  flex-shrink: 0;
}
.download-bar-cancel {
  flex-shrink: 0;
  background: rgba(255, 255, 255, 0.06);
  border: 1px solid rgba(255, 255, 255, 0.25);
  color: rgba(255, 255, 255, 0.7);
  border-radius: 4px;
  width: 22px;
  height: 22px;
  line-height: 1;
  cursor: pointer;
  font-size: 0.75rem;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: background 150ms, color 150ms;
}
.download-bar-cancel:hover {
  background: rgba(255, 60, 60, 0.35);
  color: #fff;
  border-color: rgba(255, 80, 80, 0.6);
}
.shell-with-progress {
  padding-top: 68px;
}

/* Pulling banner variant */
.banner.pulling {
  background: rgba(220, 38, 38, 0.05);
  color: var(--red-400);
  border-color: var(--glass-border-red);
}
.banner.pulling code {
  background: rgba(0,0,0,0.4);
  border: 1px solid var(--glass-border);
  padding: 1px 6px;
  border-radius: 4px;
  font-family: 'JetBrains Mono', monospace;
  color: var(--text-primary);
  font-size: 0.78rem;
}

/* Model pull block in empty state */
.pull-block {
  margin-top: 1.5rem;
}
.pull-eyebrow {
  font-size: 0.7rem;
  font-weight: 700;
  letter-spacing: 2px;
  text-transform: uppercase;
  color: var(--red-400);
  margin-bottom: 0.6rem;
}
.pull-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
  gap: 0.6rem;
}
.pull-card {
  display: flex;
  flex-direction: column;
  gap: 4px;
  text-align: left;
  background: var(--bg-frost-deep);
  border: 1px solid var(--glass-border-red);
  border-radius: var(--radius-md);
  padding: 0.85rem 1rem;
  color: var(--text-primary);
  transition: all 150ms var(--ease-out);
  position: relative;
}
.pull-card:hover:not(:disabled) {
  border-color: var(--red-500);
  background: var(--red-glow-dim);
  transform: translateY(-1px);
  box-shadow: 0 8px 22px -10px var(--red-glow);
}
.pull-card:disabled { opacity: 0.55; cursor: not-allowed; }
.pull-name {
  font-family: 'JetBrains Mono', monospace;
  font-size: 0.85rem;
  font-weight: 700;
}
.pull-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}
.pull-quants-container {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 10px;
}
.quant-btn {
  background: var(--bg-frost);
  border: 1px solid var(--glass-border);
  color: var(--text-secondary);
  padding: 4px 10px;
  border-radius: var(--radius-sm);
  font-size: 0.65rem;
  font-weight: 700;
  letter-spacing: 1px;
  text-transform: uppercase;
  cursor: pointer;
  transition: all 150ms ease;
}
.quant-btn:hover:not(:disabled) {
  background: var(--red-glow-dim);
  border-color: var(--red-500);
  color: #fff;
  transform: translateY(-1px);
}
.quant-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
.pull-meta {
  font-size: 0.72rem;
  color: var(--text-secondary);
}
.pull-action {
  align-self: flex-start;
  margin-top: 4px;
  font-size: 0.65rem;
  font-weight: 800;
  letter-spacing: 2px;
  color: var(--red-400);
  text-transform: uppercase;
}

.attach-menu-item {
  text-align: left;
  padding: 8px 12px;
  font-size: 0.85rem;
  border-radius: 8px;
  color: rgba(255, 255, 255, 0.85);
  background: transparent;
  border: none;
  display: flex;
  align-items: center;
  justify-content: flex-start;
  gap: 12px;
  transition: background 0.2s;
  cursor: pointer;
  width: 100%;
}
.attach-menu-item:hover {
  background: rgba(255, 255, 255, 0.08);
  color: #fff;
}
</style>
