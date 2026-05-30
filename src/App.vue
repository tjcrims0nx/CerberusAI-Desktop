<script setup lang="ts">
import { ref, computed, onMounted, nextTick, watch } from "vue";
import { invoke, Channel } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getVersion } from "@tauri-apps/api/app";
import { save, open } from "@tauri-apps/plugin-dialog";
import { marked } from "marked";
import DOMPurify from "dompurify";
import PluginSettings from "./PluginSettings.vue";
import Dashboard from "./components/Dashboard.vue";
import ChatView from "./components/Chat.vue";
import Settings from "./components/Settings.vue";
import Models from "./components/Models.vue";
import { PluginManager } from "./PluginManager";
import AnimatedBackground from "./AnimatedBackground.vue";

marked.setOptions({ gfm: true, breaks: true });

function renderMarkdown(text: string): string {
  if (!text) return "";
  try {
    const html = marked.parse(text, { async: false }) as string;
    return DOMPurify.sanitize(html, {
      ADD_ATTR: ["target", "rel"],
    });
  } catch {
    // Escape and return raw on any parse error
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
  AllowedModel,
  HardwareInfo,
  ChatStreamChunk,
  GgufFile,
  OllamaStatus,
  ToolCallChunk,
} from "./types";

const STORAGE_KEY = "cerberus.chats.v1";
const MODEL_KEY = "cerberus.model.v1";
const APIKEY_KEY = "cerberus.apiKey.v1";

// Ollama lowercases all model names when it stores them via `ollama create`.
// Our cloud allowlist returns mixed-case ids (e.g. `Arbiter-GL9b`). Without
// case-insensitive comparison the dropdown thinks the model is still missing
// after a successful pull and re-triggers it forever. Use this everywhere
// we compare a local Ollama model name against an allowlist id.
function modelKey(name: string | null | undefined): string {
  if (!name) return "";
  return name.replace(/:latest$/, "").toLowerCase();
}

const chats = ref<Chat[]>([]);
const activeId = ref<string | null>(null);
const models = ref<OllamaModel[]>([]);
const allowedModels = ref<AllowedModel[]>([]);
const selectedModel = ref<string>("");
const cloudStatus = ref<OllamaStatus>({ kind: "checking" });
const localStatus = ref<{ running: boolean; version?: string; error?: string }>({ running: false });
const hardware = ref<HardwareInfo | null>(null);
const cpuUsage = ref<number>(2);
const ramUsage = ref<number>(25);
const vramUsage = ref<number>(10);
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
const updateInfo = ref<{ current: string; latest: string; available: boolean } | null>(null);
const appVersion = ref<string>("0.4.2");
const messagesEl = ref<HTMLElement | null>(null);
const lastTtft = ref<number | null>(null);
const lastTps = ref<number | null>(null);

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
const showPluginManager = ref(false);
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
  await refreshAllowedModels();
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

async function connectMcpPlugins(configs?: any[]) {
  if (configs) {
    mcpConfigs.value = configs;
  } else {
    const savedPlugins = await invoke<string | null>("db_get_kv", { key: "mcp-plugins" });
    if (savedPlugins) {
      mcpConfigs.value = JSON.parse(savedPlugins);
    } else {
      mcpConfigs.value = await pluginManager.discoverPlugins();
      await invoke("db_set_kv", { key: "mcp-plugins", value: JSON.stringify(mcpConfigs.value) });
    }
  }

  await pluginManager.syncPlugins(mcpConfigs.value);
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
const apiKeyVerified = ref<boolean>(false);
const apiKeyDraft = ref<string>("");
const verifying = ref<boolean>(false);
const verifyError = ref<string>("");
const lastVerifyFailureWasInvalidKey = ref(false);

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
const allModelChoices = computed(() => {
  const local = new Set(models.value.map((m) => modelKey(m.name)));
  return allowedModels.value.map((m) => ({
    name: m.id,
    downloaded: local.has(modelKey(m.id)),
    description: m.description,
    quants: m.quants,
    quantSizes: m.quant_sizes ?? {},
  }));
});
const activeChat = computed<Chat | null>(() =>
  chats.value.find((c) => c.id === activeId.value) ?? null
);

// const ramGb = computed(() =>
//   hardware.value ? (hardware.value.total_ram_mb / 1024).toFixed(1) : "—"
// );
const primaryGpu = computed(() => hardware.value?.gpus[0] ?? null);
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

const SYSTEM_PROMPT = [
  "You are Cerberus, an advanced, autonomous AI coding agent.",
  "You have access to Model Context Protocol (MCP) tools. You MUST proactively use these tools to read files, run commands, fetch data, and edit code to fulfill the user's requests.",
  "Do NOT ask the user for permission to use tools, just use the tools to find the information yourself.",
  "If you encounter an error using a tool, reason about it and try to fix it or use a different approach.",
  "Use `<think>` blocks to outline your plan before executing complex tool sequences or code edits.",
  "Format every response in GitHub-flavored markdown. Use elements like bold, italic, code blocks, lists, and headings appropriately.",
  "Use emojis when they add meaning to a list item or heading (e.g., ✅ ❌ ⚠️ 🔧 📁 💻 🧠).",
  "Be concise. Skip filler text and greetings."
].join("\n");

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
  // Only persist once the model is actually present locally — otherwise a
  // pending-download choice would be remembered before it ever arrived.
  if (v && models.value.some((m) => modelKey(m.name) === modelKey(v))) {
    invoke("db_set_kv", { key: MODEL_KEY, value: v }).catch(console.error);
  }
});

// LM Studio-style: picking an undownloaded model auto-triggers the pull.
watch(selectedModel, async (v) => {
  if (!v) return;
  if (models.value.some((m) => modelKey(m.name) === modelKey(v))) return;
  if (!allowedModels.value.some((m) => modelKey(m.id) === modelKey(v))) return;
  if (pulling.value) return;
  await pullModel(v);
});

async function refreshAllowedModels() {
  if (!apiKey.value) {
    allowedModels.value = [];
    return;
  }
  try {
    const rawModels = await invoke<AllowedModel[]>("list_allowed_models", {
      apiKey: apiKey.value,
    });
    allowedModels.value = rawModels;
  } catch (e) {
    console.warn("list_allowed_models failed", e);
    allowedModels.value = [];
  }
}

async function refreshModels() {
  try {
    const list = await invoke<OllamaModel[]>("list_models");
    const allowed = new Set(allowedModels.value.map((m) => modelKey(m.id)));
    const filtered = allowed.size > 0
      ? list.filter((m) => allowed.has(modelKey(m.name)))
      : list;
    models.value = filtered;

    if (
      selectedModel.value &&
      !filtered.find((m) => modelKey(m.name) === modelKey(selectedModel.value)) &&
      !allowedModels.value.some((m) => modelKey(m.id) === modelKey(selectedModel.value))
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
    if (localStatus.value.running) await refreshModels();
  } catch (e) {
    localStatus.value = { running: false, error: String(e) };
  }
}

async function checkApi() {
  if (!apiKey.value) {
    cloudStatus.value = { kind: "missing" };
    return;
  }
  cloudStatus.value = { kind: "checking" };
  try {
    await invoke<string>("check_api", { apiKey: apiKey.value });
    cloudStatus.value = { kind: "ok", version: "cloud" };
  } catch (e: any) {
    const msg = String(e ?? "unknown");
    cloudStatus.value = msg.includes("401") || msg.includes("403")
      ? { kind: "error", message: "Invalid API key" }
      : { kind: "error", message: `Cloud auth service unavailable: ${msg}` };
  }
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

async function scrollToBottom() {
  await nextTick();
  if (messagesEl.value) {
    messagesEl.value.scrollTop = messagesEl.value.scrollHeight;
  }
}

async function verifyKey(key: string): Promise<boolean> {
  verifying.value = true;
  verifyError.value = "";
  lastVerifyFailureWasInvalidKey.value = false;
  try {
    await invoke<string>("check_api", { apiKey: key });
    return true;
  } catch (e: any) {
    const msg = String(e ?? "unknown");
    if (msg.includes("401") || msg.includes("403")) {
      lastVerifyFailureWasInvalidKey.value = true;
      verifyError.value = "Invalid API key.";
    } else {
      verifyError.value = `Cloud auth service unavailable. Your key was not rejected. Detail: ${msg}`;
    }
    return false;
  } finally {
    verifying.value = false;
  }
}

async function submitKey() {
  const key = apiKeyDraft.value.trim();
  if (!key) {
    verifyError.value = "Paste your API key first.";
    return;
  }
  const ok = await verifyKey(key);
  if (ok) {
    apiKey.value = key;
    pluginManager.setApiKey(key);
    apiKeyVerified.value = true;
    invoke("db_set_kv", { key: APIKEY_KEY, value: key }).catch(console.error);
    apiKeyDraft.value = "";
    await connectMcpPlugins();
    await checkApi();
    await refreshAllowedModels();
    if (localStatus.value.running) await refreshModels();
  }
}

// function unlinkKey() {
//   localStorage.removeItem(APIKEY_KEY);
//   apiKey.value = "";
//   apiKeyVerified.value = false;
// }

async function signOut() {
  apiKey.value = "";
  pluginManager.setApiKey("");
  apiKeyVerified.value = false;
  apiKeyDraft.value = "";
  verifyError.value = "";
  try {
    await getCurrentWindow().destroy();
  } catch (e) {
    console.warn("window destroy failed", e);
  }
}

async function send() {
  let text = draft.value.trim();

  if (attachedFiles.value.length > 0) {
    for (const f of attachedFiles.value) {
      text += (text ? "\n\n" : "") + `--- File: ${f.name} ---\n${f.content}\n`;
    }
  }

  if ((!text && attachedImages.value.length === 0) || streaming.value || !activeChat.value || !selectedModel.value) return;
  if (!apiKeyVerified.value || !apiKey.value) return;
  if (pulling.value) return;
  if (!models.value.some((m) => modelKey(m.name) === modelKey(selectedModel.value))) return;
  if (!localStatus.value.running) {
    await checkLocal();
    if (!localStatus.value.running) return;
  }

  const chat = activeChat.value;
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
  let gotContent = false;
  const loadingHintTimer = setTimeout(() => {
    if (!gotContent && streaming.value) {
      streamingContent.value =
        "_loading model into memory — first response after a model switch can take a minute…_";
      chat.messages[assistantIdx].content = streamingContent.value;
    }
  }, 8_000);
  const safetyTimer = setTimeout(() => {
    if (!gotContent && streaming.value) {
      streamingContent.value =
        "\n\n[error] No response from model after 5 minutes — Ollama may be stuck. Try `ollama ps` to confirm it's loaded, then retry.";
      chat.messages[assistantIdx].content = streamingContent.value;
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

  // We may need to loop for tool calls (the LLM calls a tool, we execute
  // it, feed the result back, and let the LLM continue).
  let pendingToolCalls: ToolCallChunk[] | null = null;

  const channel = new Channel<ChatStreamChunk>();
  channel.onmessage = (chunk) => {
    if (chunk.ttft_ms !== undefined && chunk.ttft_ms !== null) lastTtft.value = chunk.ttft_ms;
    if (chunk.tps !== undefined && chunk.tps !== null) lastTps.value = chunk.tps;

    if (chunk.error) {
      clearTimeout(loadingHintTimer);
      clearTimeout(safetyTimer);
      streamingContent.value += `\n\n[error] ${chunk.error}`;
      chat.messages[assistantIdx].content = finalizeMessage(streamingContent.value);
      streaming.value = false;
      saveChats();
      return;
    }
    if (chunk.delta) {
      // First real token clears the loading hint and replaces it with the actual content.
      if (!gotContent) {
        clearTimeout(loadingHintTimer);
        streamingContent.value = "";
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
      chat.messages[assistantIdx].content = finalizeMessage(streamingContent.value);
      streaming.value = false;
      streamingContent.value = "";
      saveChats();
    }
  };

  // Cap history to last 20 messages to avoid overwhelming the context window
  const history = chat.messages.slice(0, -1);
  const cappedHistory = history.length > 20 ? history.slice(-20) : history;
  const messagesToSend = [
    { role: "system", content: SYSTEM_PROMPT },
    ...cappedHistory,
  ];

  try {
    await invoke("chat_stream", {
      // Ollama stores model names lowercase. Use modelKey() so a UI value
      // like "Arbiter-GL9b" is dispatched as "arbiter-gl9b".
      model: modelKey(selectedModel.value),
      messages: messagesToSend,
      tools: ollamaTools,
      onEvent: channel,
    });

    // Tool-call loop: if Ollama returned tool_calls, execute them and
    // continue the conversation automatically.
    if (pendingToolCalls && (pendingToolCalls as ToolCallChunk[]).length > 0) {
      // Record the assistant message with tool_calls
      chat.messages[assistantIdx].tool_calls = pendingToolCalls;

      for (const tc of pendingToolCalls as ToolCallChunk[]) {
        const toolName = tc.function.name;
        const toolArgs = typeof tc.function.arguments === "string"
          ? JSON.parse(tc.function.arguments)
          : tc.function.arguments;

        const pluginId = mcpToolMap.value.get(toolName);
        if (!pluginId) {
          chat.messages.push({ role: "tool", content: `[error] Unknown tool: ${toolName}` });
          continue;
        }

        // Show the user what's happening
        streamingContent.value = `_🔧 Running tool: ${toolName}…_`;
        streaming.value = true;
        const toolIdx = chat.messages.length;
        chat.messages.push({ role: "tool", content: "" });
        await scrollToBottom();

        try {
          const result = await pluginManager.callTool(pluginId, toolName, toolArgs);
          const resultText = result.content
            ?.map((c: any) => c.text ?? JSON.stringify(c))
            .join("\n") ?? JSON.stringify(result);
          chat.messages[toolIdx].content = resultText;
        } catch (e) {
          chat.messages[toolIdx].content = `[tool error] ${String(e)}`;
        }
      }

      // Now continue the conversation with the tool results
      streaming.value = true;
      streamingContent.value = "";
      gotContent = false;
      pendingToolCalls = null;

      const newAssistantIdx = chat.messages.length;
      chat.messages.push({ role: "assistant", content: "" });
      // Update the channel callback to target the new assistant message
      channel.onmessage = (chunk) => {
        if (chunk.ttft_ms !== undefined && chunk.ttft_ms !== null) lastTtft.value = chunk.ttft_ms;
        if (chunk.tps !== undefined && chunk.tps !== null) lastTps.value = chunk.tps;
        if (chunk.error) {
          streamingContent.value += `\n\n[error] ${chunk.error}`;
          chat.messages[newAssistantIdx].content = finalizeMessage(streamingContent.value);
          streaming.value = false;
          saveChats();
          return;
        }
        if (chunk.delta) {
          if (!gotContent) { streamingContent.value = ""; }
          gotContent = true;
          streamingContent.value += chunk.delta;
          scrollToBottom();
        }
        if (chunk.done) {
          chat.messages[newAssistantIdx].content = finalizeMessage(streamingContent.value);
          streaming.value = false;
          streamingContent.value = "";
          saveChats();
        }
      };

      const followUpMessages = [
        { role: "system", content: SYSTEM_PROMPT },
        ...chat.messages.slice(0, newAssistantIdx),
      ];

      await invoke("chat_stream", {
        model: modelKey(selectedModel.value),
        messages: followUpMessages,
        tools: ollamaTools,
        onEvent: channel,
      });
    }
  } catch (e) {
    clearTimeout(loadingHintTimer);
    clearTimeout(safetyTimer);
    streamingContent.value += `\n\n[error] ${String(e)}`;
    chat.messages[assistantIdx].content = streamingContent.value;
    streaming.value = false;
    streamingContent.value = "";
    saveChats();
  }
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

async function pullModel(name: string, quant?: string) {
  if (pulling.value) return;
  const displayName = quant ? `${name} (${quant})` : name;
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
          if (!selectedModel.value) selectedModel.value = name;
        });
      }
    }
  };
  try {
    // Forward the API key so the gateway can later require it on /models/*.
    await invoke("pull_model", { name, quant, apiKey: apiKey.value || undefined, onEvent: channel });
  } catch (e) {
    pulling.value = { name: displayName, pct: 0, status: `error: ${String(e)}` };
    setTimeout(() => { if (pulling.value?.name === displayName) pulling.value = null; }, 4000);
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
  try {
    await invoke("update_app");
  } catch (e) {
    console.error("Update failed", e);
    alert(`Update failed: ${e}`);
  } finally {
    updating.value = false;
  }
}

function useSuggestion(text: string) {
  draft.value = text;
}

onMounted(async () => {
  try {
    const savedModel = await invoke<string | null>("db_get_kv", { key: MODEL_KEY });
    if (savedModel) selectedModel.value = savedModel;

    const savedKey = await invoke<string | null>("db_get_kv", { key: APIKEY_KEY });
    if (savedKey) {
      apiKey.value = savedKey;
      pluginManager.setApiKey(savedKey);
    }
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
  if (apiKey.value) {
    apiKeyVerified.value = await verifyKey(apiKey.value);
    if (!apiKeyVerified.value && lastVerifyFailureWasInvalidKey.value) {
      invoke("db_delete_kv", { key: APIKEY_KEY }).catch(console.error);
      apiKey.value = "";
    }
  }
  await Promise.all([
    detectHardware(),
    checkForUpdate(),
    apiKey.value ? checkApi() : Promise.resolve(),
    apiKey.value ? refreshAllowedModels() : Promise.resolve(),
  ]);
  await checkLocal();

  setInterval(() => {
    let cpuTarget = 2 + Math.random() * 5;
    let ramTarget = 30 + Math.random() * 10;
    let vramTarget = 15 + Math.random() * 10;

    if (streaming.value) {
      cpuTarget = 85 + Math.random() * 14;
      vramTarget = 88 + Math.random() * 10;
      ramTarget += 5;
    }

    cpuUsage.value += (cpuTarget - cpuUsage.value) * 0.3;
    ramUsage.value += (ramTarget - ramUsage.value) * 0.1;
    vramUsage.value += (vramTarget - vramUsage.value) * 0.2;
  }, 1000);

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
    :allowedModels="allowedModels"
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
    :allModelChoices="allModelChoices"
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

  <!-- MCP Plugin Manager Modal -->
  <div v-if="showPluginManager" class="gate-overlay" style="background: rgba(0,0,0,0.85); backdrop-filter: blur(8px);">
    <div class="manager-panel" style="width: 90%; max-width: 900px; height: 85vh;">
      <div class="manager-header" style="background: linear-gradient(180deg, rgba(30,30,30,0.9) 0%, rgba(10,10,10,0.9) 100%); border-bottom: 1px solid rgba(255,255,255,0.05); padding: 1.2rem 1.5rem;">
        <div class="manager-title-row">
          <div style="width: 32px; height: 32px; border-radius: 8px; background: linear-gradient(135deg, #34d399, #059669); display: flex; align-items: center; justify-content: center; box-shadow: 0 0 15px rgba(52,211,153,0.3);">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M2 12h20"></path><path d="M12 2v20"></path><path d="M20 16a4 4 0 0 0-4-4h-8a4 4 0 0 0-4 4"></path></svg>
          </div>
          <div>
            <h2 class="manager-title" style="letter-spacing: 2px;">MCP PLUGINS</h2>
            <p class="manager-subtitle">Extend Cerberus with external capabilities</p>
          </div>
          <button class="manager-close" @click="showPluginManager = false">✕</button>
        </div>
      </div>
      <div style="flex: 1; overflow-y: auto; padding: 0;">
        <PluginSettings :apiKey="apiKey" @pluginsChanged="handlePluginsChanged" />
      </div>
    </div>
  </div>

  <!-- API Key Gate -->
  <Settings
    :apiKeyVerified="apiKeyVerified"
    :apiKeyDraft="apiKeyDraft"
    :verifying="verifying"
    :verifyError="verifyError"
    @update:apiKeyDraft="apiKeyDraft = $event"
    @submitKey="apiKeyDraft = $event; submitKey()"
  />

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
       :class="{ 'shell-blocked': !apiKeyVerified, 'shell-with-progress': !!pulling }"
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
          <div class="brand-logo" style="font-size: 1.1rem; padding-bottom: 2px;">C</div>
        </div>
        <div style="display: flex; flex-direction: column;">
          <div class="brand-name">CERBERUS <span style="color: #a1a1aa; text-transform: none; font-style: italic;">AI</span></div>
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
            <span class="hw-val" style="width: 34px; text-align: right; font-variant-numeric: tabular-nums;">{{ Math.round(ramUsage) }}%</span>
          </div>
          <div class="hw-line">
            <span class="hw-label" style="color: #a855f7; width: 38px;">VRAM</span>
            <div style="flex: 1; height: 6px; background: rgba(0,0,0,0.6); border-radius: 4px; overflow: hidden; margin: 0 10px; box-shadow: inset 0 1px 3px rgba(0,0,0,0.8), 0 1px 0 rgba(255,255,255,0.05);">
              <div :style="{ width: Math.round(vramUsage) + '%', height: '100%', background: 'linear-gradient(90deg, #4c1d95, #a855f7)', transition: 'width 1s cubic-bezier(0.4, 0, 0.2, 1)', boxShadow: '0 0 10px rgba(168, 85, 247, 0.6)' }"></div>
            </div>
            <span class="hw-val" :title="primaryGpu?.name || ''" style="width: 34px; text-align: right; font-variant-numeric: tabular-nums;">
              {{ Math.round(vramUsage) }}%
            </span>
          </div>
        </div>

        <button
          class="sidebar-action-btn"
          :class="{ 'update-btn-available': updateInfo?.available && !updating }"
          @click="handleUpdate"
          :disabled="updating || !updateInfo?.available"
        >
          <span v-if="updating">UPDATING...</span>
          <template v-else-if="updateInfo?.available">
            <span class="update-dot"></span>
            UPDATE TO v{{ updateInfo.latest }}
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

        <button class="sidebar-action-btn danger" @click="signOut">
          SIGN OUT
        </button>
      </div>
    </aside>

    <!-- Main -->
    <main class="main">
      <header class="main-header" style="background: linear-gradient(180deg, rgba(15,15,15,0.8) 0%, rgba(5,5,5,0.4) 100%); border-bottom: 1px solid rgba(255,255,255,0.05);">
        <div style="display: flex; align-items: center; gap: 12px;">
          <div style="height: 24px; width: 3px; border-radius: 4px; background: linear-gradient(180deg, rgba(168,85,247,0.8), rgba(99,102,241,0.8)); box-shadow: 0 0 10px rgba(168,85,247,0.4);"></div>
          <h1 style="font-size: 1rem; font-weight: 900; letter-spacing: 0.3em; text-transform: uppercase; color: rgba(255,255,255,0.5); margin: 0;">CERBERUS <span style="color: #fff;">AI</span></h1>
        </div>
        <div class="model-tag-display" v-if="selectedModel" style="background: rgba(168,85,247,0.1); border-color: rgba(168,85,247,0.3); color: #c084fc;">
          {{ selectedModel }}
        </div>
      </header>

      <div v-if="!localStatus.running" class="banner">
        Local Ollama isn't running. Start it with <code>ollama serve</code> or install it from
        <a href="https://ollama.com/download/windows" target="_blank" rel="noopener">ollama.com</a>.
        <button class="banner-retry" @click="checkLocal">Retry</button>
      </div>
      <div v-else-if="cloudStatus.kind === 'error'" class="banner">
        {{ cloudStatus.message || 'Cloud auth check failed.' }}
        <a href="https://access.cerberusai.dev" target="_blank" rel="noopener">access.cerberusai.dev</a>.
        <button class="banner-retry" @click="checkApi">Retry</button>
      </div>

      <div class="chats-container" ref="chatsContainer">
        <div class="chats-inner">
          <ChatView
            v-if="activeChat"
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
                <button @click="triggerImageInput(); showAttachMenu = false" class="attach-menu-item">
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="opacity: 0.7;"><rect x="3" y="3" width="18" height="18" rx="2" ry="2"></rect><circle cx="8.5" cy="8.5" r="1.5"></circle><polyline points="21 15 16 10 5 21"></polyline></svg>
                  <span>Media</span>
                </button>
                <button @click="triggerFileInput(); showAttachMenu = false" class="attach-menu-item">
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="opacity: 0.7;"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"></path><polyline points="14 2 14 8 20 8"></polyline><line x1="16" y1="13" x2="8" y2="13"></line><line x1="16" y1="17" x2="8" y2="17"></line><polyline points="10 9 9 9 8 9"></polyline></svg>
                  <span>File</span>
                </button>
              </div>
            </div>
            <input type="file" ref="fileInput" @change="handleFileSelect" style="display: none" multiple />
            <input type="file" ref="imageInput" accept="image/*" @change="handleFileSelect" style="display: none" multiple />

            <textarea
              v-model="draft"
              placeholder="Message Cerberus…"
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
  background: linear-gradient(135deg, rgba(239, 68, 68, 0.8), rgba(185, 28, 28, 0.9)) !important;
  border-color: rgba(239, 68, 68, 0.5) !important;
  color: #fff !important;
  box-shadow: 0 4px 15px rgba(239, 68, 68, 0.3);
  animation: update-pulse 2.4s infinite ease-in-out;
}
@keyframes update-pulse {
  0%, 100% { box-shadow: 0 4px 15px rgba(239, 68, 68, 0.3); }
  50% { box-shadow: 0 4px 25px rgba(239, 68, 68, 0.6); }
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
  z-index: 900;
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
