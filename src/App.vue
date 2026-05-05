<script setup lang="ts">
import { ref, computed, onMounted, nextTick, watch } from "vue";
import { invoke, Channel } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getVersion } from "@tauri-apps/api/app";
import { save, open } from "@tauri-apps/plugin-dialog";
import type {
  Chat,
  OllamaModel,
  AllowedModel,
  HardwareInfo,
  ChatStreamChunk,
  GgufFile,
  OllamaStatus,
} from "./types";

const STORAGE_KEY = "cerberus.chats.v1";
const MODEL_KEY = "cerberus.model.v1";
const APIKEY_KEY = "cerberus.apiKey.v1";

const chats = ref<Chat[]>([]);
const activeId = ref<string | null>(null);
const models = ref<OllamaModel[]>([]);
const allowedModels = ref<AllowedModel[]>([]);
const selectedModel = ref<string>(localStorage.getItem(MODEL_KEY) || "");
const cloudStatus = ref<OllamaStatus>({ kind: "checking" });
const localStatus = ref<{ running: boolean; version?: string; error?: string }>({ running: false });
const hardware = ref<HardwareInfo | null>(null);
const draft = ref<string>("");
const streaming = ref<boolean>(false);
const streamingContent = ref<string>("");
const updating = ref<boolean>(false);
const updateInfo = ref<{ current: string; latest: string; available: boolean } | null>(null);
const appVersion = ref<string>("0.2.6");
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

// Model Manager (LM Studio-style)
const showFileManager = ref(false);
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
const apiKey = ref<string>(localStorage.getItem(APIKEY_KEY) || "");
const apiKeyVerified = ref<boolean>(false);
const apiKeyDraft = ref<string>("");
const verifying = ref<boolean>(false);
const verifyError = ref<string>("");

// Active model pull
const pulling = ref<{ name: string; pct: number; status: string } | null>(null);

// Every allowlisted model, paired with whether it's already pulled locally.
// Drives the dropdown so users can pick undownloaded models and have them
// auto-pulled (LM Studio-style).
const allModelChoices = computed(() => {
  const local = new Set(models.value.map((m) => m.name.replace(/:latest$/, '')));
  return allowedModels.value.map((m) => ({ 
    name: m.id, 
    downloaded: local.has(m.id),
    description: m.description,
    quants: m.quants
  }));
});
const activeChat = computed<Chat | null>(() =>
  chats.value.find((c) => c.id === activeId.value) ?? null
);

const ramGb = computed(() =>
  hardware.value ? (hardware.value.total_ram_mb / 1024).toFixed(1) : "—"
);
const primaryGpu = computed(() => hardware.value?.gpus[0] ?? null);
const vramGb = computed(() => {
  const v = primaryGpu.value?.vram_mb;
  return v ? (v / 1024).toFixed(1) : null;
});

function uid(): string {
  return Math.random().toString(36).slice(2, 10);
}

function loadChats() {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) chats.value = JSON.parse(raw);
  } catch {
    chats.value = [];
  }
  if (chats.value.length === 0) newChat();
  else activeId.value = chats.value[0].id;
}

function saveChats() {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(chats.value));
}

function newChat() {
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
  if (v && models.value.some((m) => m.name.replace(/:latest$/, '') === v)) {
    localStorage.setItem(MODEL_KEY, v);
  }
});

// LM Studio-style: picking an undownloaded model auto-triggers the pull.
watch(selectedModel, async (v) => {
  if (!v) return;
  if (models.value.some((m) => m.name.replace(/:latest$/, '') === v)) return;
  if (!allowedModels.value.some((m) => m.id === v)) return;
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
    allowedModels.value = rawModels.filter((m) => m.id !== "cerberus-4b");
  } catch (e) {
    console.warn("list_allowed_models failed", e);
    allowedModels.value = [];
  }
}

async function refreshModels() {
  try {
    const list = await invoke<OllamaModel[]>("list_models");
    const allowed = new Set(allowedModels.value.map((m) => m.id));
    const filtered = allowed.size > 0
      ? list.filter((m) => allowed.has(m.name.replace(/:latest$/, '')))
      : [];
    models.value = filtered;

    if (
      selectedModel.value &&
      !filtered.find((m) => m.name.replace(/:latest$/, '') === selectedModel.value) &&
      !allowedModels.value.some((m) => m.id === selectedModel.value)
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
      : { kind: "error", message: msg };
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
  try {
    await invoke<string>("check_api", { apiKey: key });
    return true;
  } catch (e: any) {
    const msg = String(e ?? "unknown");
    if (msg.includes("401") || msg.includes("403")) {
      verifyError.value = "Invalid API key.";
    } else {
      verifyError.value = `Verify failed: ${msg}`;
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
    apiKeyVerified.value = true;
    localStorage.setItem(APIKEY_KEY, key);
    apiKeyDraft.value = "";
    await checkApi();
    await refreshAllowedModels();
    if (localStatus.value.running) await refreshModels();
  }
}

async function signOut() {
  apiKey.value = "";
  apiKeyVerified.value = false;
  apiKeyDraft.value = "";
  verifyError.value = "";
  localStorage.removeItem(APIKEY_KEY);
  try {
    await getCurrentWindow().destroy();
  } catch (e) {
    console.warn("window destroy failed", e);
  }
}

async function send() {
  const text = draft.value.trim();
  if (!text || streaming.value || !activeChat.value || !selectedModel.value) return;
  if (!apiKeyVerified.value || !apiKey.value) return;
  if (pulling.value) return;
  if (!models.value.some((m) => m.name === selectedModel.value || m.name.replace(/:latest$/, '') === selectedModel.value)) return;
  if (!localStatus.value.running) {
    await checkLocal();
    if (!localStatus.value.running) return;
  }

  const chat = activeChat.value;
  chat.messages.push({ role: "user", content: text });
  if (chat.messages.length === 1) {
    chat.title = text.slice(0, 48) + (text.length > 48 ? "…" : "");
  }
  chat.model = selectedModel.value;
  draft.value = "";
  saveChats();
  await scrollToBottom();

  chat.messages.push({ role: "assistant", content: "" });
  const assistantIdx = chat.messages.length - 1;
  streamingContent.value = "";
  streaming.value = true;
  lastTtft.value = null;
  lastTps.value = null;

  // Safety timeout: if no content arrives within 2 minutes, unblock the UI.
  let gotContent = false;
  const safetyTimer = setTimeout(() => {
    if (!gotContent && streaming.value) {
      streamingContent.value += "\n\n[error] No response from model — it may still be loading. Try again.";
      chat.messages[assistantIdx].content = streamingContent.value;
      streaming.value = false;
      saveChats();
    }
  }, 120_000);

  const channel = new Channel<ChatStreamChunk>();
  channel.onmessage = (chunk) => {
    if (chunk.ttft_ms !== undefined && chunk.ttft_ms !== null) lastTtft.value = chunk.ttft_ms;
    if (chunk.tps !== undefined && chunk.tps !== null) lastTps.value = chunk.tps;

    if (chunk.error) {
      clearTimeout(safetyTimer);
      streamingContent.value += `\n\n[error] ${chunk.error}`;
      chat.messages[assistantIdx].content = stripThinkTags(streamingContent.value);
      streaming.value = false;
      saveChats();
      return;
    }
    if (chunk.delta) {
      gotContent = true;
      streamingContent.value += chunk.delta;
      scrollToBottom();
    }
    if (chunk.done) {
      clearTimeout(safetyTimer);
      chat.messages[assistantIdx].content = stripThinkTags(streamingContent.value);
      streaming.value = false;
      streamingContent.value = "";
      saveChats();
    }
  };

  // Cap history to last 20 messages to avoid overwhelming the context window
  const history = chat.messages.slice(0, -1);
  const cappedHistory = history.length > 20 ? history.slice(-20) : history;

  try {
    await invoke("chat_stream", {
      model: selectedModel.value,
      messages: cappedHistory,
      onEvent: channel,
    });
  } catch (e) {
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
    const pct = p.total && p.completed ? Math.floor((p.completed / p.total) * 100) : pulling.value?.pct ?? 0;
    pulling.value = { name, pct, status: p.status || "downloading" };
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
    await invoke("pull_model", { name, quant, onEvent: channel });
  } catch (e) {
    pulling.value = { name, pct: 0, status: `error: ${String(e)}` };
    setTimeout(() => { if (pulling.value?.name === name) pulling.value = null; }, 4000);
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

const SUGGESTIONS = [
  "Write a Python script to deduplicate a CSV by column.",
  "Explain abliteration in 4 bullet points.",
  "Draft a system prompt for a no-refusals coding agent.",
  "Compare Cerberus 4B with Arbiter GL9b for code generation.",
  "Test the new Arbiter GL9b Q3_K_M quant for efficiency.",
];

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
  loadChats();
  try {
    appVersion.value = await getVersion();
  } catch (e) {
    console.warn("getVersion failed", e);
  }
  if (apiKey.value) {
    apiKeyVerified.value = await verifyKey(apiKey.value);
    if (!apiKeyVerified.value) {
      localStorage.removeItem(APIKEY_KEY);
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
});
</script>

<template>
  <div class="glow-orb orb-1"></div>
  <div class="glow-orb orb-2"></div>

  <!-- Model Manager Modal (LM Studio-style) -->
  <div v-if="showFileManager" class="key-gate" @click.self="showFileManager = false" style="z-index: 1000;">
    <div class="manager-panel">
      <!-- Header -->
      <div class="manager-header">
        <div class="manager-title-row">
          <div class="key-logo" style="width: 42px; height: 42px; font-size: 1.1rem; margin: 0;">🧠</div>
          <div>
            <h2 class="manager-title">MODEL MANAGER</h2>
            <p class="manager-subtitle">Manage your local AI models</p>
          </div>
          <div style="margin-left: auto; display: flex; gap: 8px;">
            <button class="manager-close" @click="refreshAllModels" title="Refresh models">↻</button>
            <button class="manager-close" @click="showFileManager = false" title="Close">✕</button>
          </div>
        </div>

        <!-- Search bar -->
        <div class="manager-search-row">
          <input
            v-model="managerSearch"
            class="manager-search"
            type="text"
            placeholder="Search models…"
            spellcheck="false"
          />
        </div>

        <!-- Tabs -->
        <div class="manager-tabs">
          <button
            class="manager-tab"
            :class="{ active: managerTab === 'ollama' }"
            @click="managerTab = 'ollama'"
          >
            OLLAMA MODELS
            <span class="manager-tab-count">{{ models.length }}</span>
          </button>
          <button
            class="manager-tab"
            :class="{ active: managerTab === 'files' }"
            @click="managerTab = 'files'"
          >
            RAW FILES
            <span class="manager-tab-count">{{ localGgufs.length }}</span>
          </button>
        </div>
      </div>

      <!-- Ollama Models Tab -->
      <div v-if="managerTab === 'ollama'" class="manager-body">
        <div class="manager-disk-bar">
          <span class="manager-disk-label">TOTAL DISK USAGE</span>
          <span class="manager-disk-value">{{ formatBytes(totalOllamaSize) }}</span>
        </div>

        <div v-if="filteredOllamaModels.length === 0" class="manager-empty">
          <template v-if="managerSearch">No models matching "{{ managerSearch }}"</template>
          <template v-else>No models installed in Ollama yet.<br/>Pull a model from the main screen or import a .gguf file.</template>
        </div>

        <div v-else class="manager-list">
          <div v-for="m in filteredOllamaModels" :key="m.name" class="model-card">
            <div class="model-card-main">
              <div class="model-card-icon">{{ m.name.charAt(0).toUpperCase() }}</div>
              <div class="model-card-info">
                <div class="model-card-name" :title="m.name">{{ m.name.replace(/:latest$/, '') }}</div>
                <div class="model-card-meta">
                  <span class="model-tag">{{ formatBytes(m.size) }}</span>
                  <span v-if="m.details?.quantization_level" class="model-tag quant">{{ m.details.quantization_level }}</span>
                  <span v-if="m.details?.parameter_size" class="model-tag param">{{ m.details.parameter_size }}</span>
                  <span v-if="m.details?.family" class="model-tag family">{{ m.details.family }}</span>
                </div>
              </div>
            </div>
            <div class="model-card-actions">
              <button
                class="model-action-btn use"
                v-if="selectedModel !== m.name.replace(/:latest$/, '')"
                @click="selectedModel = m.name.replace(/:latest$/, ''); showFileManager = false"
                title="Use this model"
              >USE</button>
              <span v-else class="model-active-badge">ACTIVE</span>
              <button
                class="model-action-btn danger"
                @click="deleteOllamaModel(m.name)"
                :disabled="isDeletingModel"
                title="Remove from Ollama (keeps your raw .gguf files safe)"
              >UNREGISTER</button>
            </div>
          </div>
        </div>
      </div>

      <!-- Raw Files Tab -->
      <div v-if="managerTab === 'files'" class="manager-body">
        <div class="manager-disk-bar">
          <span class="manager-disk-label">RAW GGUF FILES</span>
          <span class="manager-disk-value">{{ formatBytes(totalGgufSize) }}</span>
        </div>

        <p class="manager-hint">
          Downloaded <code>.gguf</code> installer files. You can safely delete these after a model has been imported into Ollama.
        </p>

        <div v-if="filteredGgufs.length === 0" class="manager-empty">
          <template v-if="managerSearch">No files matching "{{ managerSearch }}"</template>
          <template v-else>No raw .gguf files found.</template>
        </div>

        <div v-else class="manager-list">
          <div v-for="f in filteredGgufs" :key="f.name" class="model-card">
            <div class="model-card-main">
              <div class="model-card-icon file-icon">📄</div>
              <div class="model-card-info">
                <div class="model-card-name" :title="f.name">{{ f.name }}</div>
                <div class="model-card-meta">
                  <span class="model-tag">{{ formatBytes(f.size) }}</span>
                </div>
              </div>
            </div>
            <div class="model-card-actions">
              <button
                v-if="activatedGgufs.has(f.name)"
                class="model-action-btn success"
                disabled
                title="Already activated in Ollama"
              >ACTIVATED</button>
              <button
                v-else
                class="model-action-btn use"
                @click="activateGguf(f.name)"
                :disabled="isImporting || isDeletingGguf"
                title="Register this file in Ollama"
              >ACTIVATE</button>
              <button
                class="model-action-btn"
                @click="moveGguf(f.name)"
                :disabled="isDeletingGguf"
                title="Move to another location"
              >MOVE</button>
              <button
                class="model-action-btn danger"
                @click="deleteGguf(f.name)"
                :disabled="isDeletingGguf"
                title="Permanently delete this file from your hard drive"
              >TRASH FILE</button>
            </div>
          </div>
        </div>
      </div>

      <!-- Cloud Models Tab -->
      <div v-if="managerTab === 'cloud'" class="manager-body">
        <div class="manager-disk-bar">
          <span class="manager-disk-label">AUTHORIZED CLOUD MODELS</span>
          <span class="manager-disk-value">{{ allowedModels.length }} Available</span>
        </div>

        <p class="manager-hint">
          Models authorized by your Cerberus account. Pull them to your local Ollama instance.
        </p>

        <div v-if="allowedModels.length === 0" class="manager-empty">
          No cloud models available for your account.
        </div>

        <div v-else class="manager-list">
          <div v-for="m in allModelChoices" :key="m.name" class="model-card">
            <div class="model-card-main">
              <div class="model-card-icon file-icon">☁️</div>
              <div class="model-card-info">
                <div class="model-card-name" :title="m.name">{{ m.name }}</div>
                <div class="model-card-meta">
                  <span class="model-tag">{{ m.description }}</span>
                </div>
              </div>
            </div>
            <div class="model-card-actions">
              <template v-if="m.downloaded">
                <button class="model-action-btn success" disabled>DOWNLOADED</button>
              </template>
              <template v-else-if="pulling?.name.startsWith(m.name)">
                <button class="model-action-btn use" disabled>PULLING...</button>
              </template>
              <template v-else>
                <div style="display: flex; gap: 4px;">
                  <button 
                    v-for="q in m.quants.split(',').map(s => s.trim()).filter(Boolean)" 
                    :key="q"
                    class="model-action-btn use"
                    @click.stop="pullModel(m.name, q)"
                  >
                    PULL {{ q }}
                  </button>
                  <button 
                    v-if="!m.quants" 
                    class="model-action-btn use"
                    @click.stop="pullModel(m.name)"
                  >
                    PULL
                  </button>
                </div>
              </template>
            </div>
          </div>
        </div>
      </div>

      <!-- Footer actions -->
      <div class="manager-footer">
        <button class="import-btn" @click="importGguf" :disabled="isImporting || !localStatus.running">
          <span v-if="isImporting">IMPORTING…</span>
          <span v-else>⬆ IMPORT GGUF</span>
        </button>
        <button class="close-modal-btn" @click="showFileManager = false">DONE</button>
      </div>
    </div>
  </div>

  <!-- API Key Gate -->
  <div v-if="!apiKeyVerified" class="key-gate">
    <div class="key-card">
      <img src="./assets/logo.png" class="key-logo-img" alt="Cerberus Logo" />
      <p class="key-eyebrow">Local-First. Unfiltered. Yours.</p>
      <h1 class="glitch key-title" data-text="CERBERUS">CERBERUS</h1>
      <p class="key-sub">
        Enter your Cerberus API key to unlock the local chat. Don't have one?
        <a href="https://access.cerberusai.dev" target="_blank" rel="noopener">Get one here.</a>
      </p>

      <form class="key-form" @submit.prevent="submitKey">
        <input
          type="password"
          v-model="apiKeyDraft"
          placeholder="cb_••••••••••••••••••••"
          autocomplete="off"
          spellcheck="false"
          :disabled="verifying"
          autofocus
        />
        <button type="submit" :disabled="verifying || !apiKeyDraft.trim()">
          <span v-if="!verifying">UNLOCK</span>
          <span v-else>VERIFYING…</span>
        </button>
      </form>
      <p v-if="verifyError" class="key-error">{{ verifyError }}</p>

      <p class="key-foot">
        Verified against <code>api.cerberusai.dev</code>. Your key is stored locally and never leaves
        this machine after verification.
      </p>
    </div>
  </div>

  <!-- Top-of-window download progress bar -->
  <div v-if="pulling" class="download-bar" role="progressbar" :aria-valuenow="pulling.pct" aria-valuemin="0" aria-valuemax="100">
    <div class="download-bar-fill" :class="{ indeterminate: pulling.pct === 0 }" :style="{ width: pulling.pct > 0 ? pulling.pct + '%' : undefined }"></div>
    <div class="download-bar-text">
      <span class="download-bar-label">DOWNLOADING</span>
      <code class="download-bar-name" :title="pulling.name">{{ pulling.name }}</code>
      <span class="download-bar-status">{{ pulling.status }}</span>
      <span class="download-bar-pct">{{ pulling.pct }}%</span>
      <button class="download-bar-cancel" title="Cancel download" @click="cancelDownload">✕</button>
    </div>
  </div>

  <div class="shell" :class="{ 'shell-blocked': !apiKeyVerified, 'shell-with-progress': !!pulling }">
    <!-- Sidebar -->
    <aside class="sidebar">
      <div class="brand">
        <img src="./assets/logo.png" class="brand-logo-img" alt="Cerberus Logo" />
        <div class="brand-name">CERBERUS</div>
        <div class="brand-sub">v{{ appVersion }}</div>
      </div>

      <button class="new-chat-btn" @click="newChat">
        + New Chat
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
          {{ c.title }}
        </button>
      </div>

      <div class="sidebar-footer">
        <!-- Cloud auth pill -->
        <span v-if="cloudStatus.kind === 'ok'" class="status-pill ok">
          <span class="dot"></span> CLOUD AUTH
        </span>
        <span v-else-if="cloudStatus.kind === 'checking'" class="status-pill warn">
          <span class="dot"></span> AUTH…
        </span>
        <span v-else class="status-pill err">
          <span class="dot"></span> CLOUD ERR
        </span>

        <!-- Local Ollama pill -->
        <span v-if="localStatus.running" class="status-pill ok">
          <span class="dot"></span> OLLAMA {{ localStatus.version }}
        </span>
        <span v-else class="status-pill err" :title="localStatus.error || ''">
          <span class="dot"></span> OLLAMA OFFLINE
        </span>

        <div v-if="hardware" class="hw-summary">
          <div class="hw-line">
            <span class="hw-label">CPU</span>
            <span class="hw-val">{{ hardware.cpu_cores }}c · {{ ramGb }} GB RAM</span>
          </div>
          <div class="hw-line">
            <span class="hw-label">GPU</span>
            <span class="hw-val" :title="primaryGpu?.name || ''">
              {{ primaryGpu ? (primaryGpu.name.length > 22 ? primaryGpu.name.slice(0, 22) + '…' : primaryGpu.name) : 'None' }}
              <span v-if="vramGb"> · {{ vramGb }} GB</span>
            </span>
          </div>
        </div>

        <button
          class="update-btn"
          :class="{ 'update-btn-available': updateInfo?.available && !updating }"
          @click="handleUpdate"
          :disabled="updating || !updateInfo?.available"
          :title="updating
            ? 'Updating…'
            : updateInfo?.available
              ? `Update available: v${updateInfo.current} → v${updateInfo.latest}`
              : updateInfo
                ? `Up to date (v${updateInfo.current})`
                : 'Checking for updates…'"
        >
          <span v-if="updating">UPDATING...</span>
          <template v-else-if="updateInfo?.available">
            <span class="update-dot"></span>
            UPDATE TO v{{ updateInfo.latest }}
          </template>
          <span v-else-if="updateInfo">v{{ updateInfo.current }} · LATEST</span>
          <span v-else>CHECKING…</span>
        </button>

        <button class="signout-btn" @click="openFileManager" title="Manage local models and GGUF files">
          MODEL MANAGER
        </button>

        <button class="signout-btn" @click="signOut" title="Clear API key and sign out">
          SIGN OUT
        </button>
      </div>
    </aside>

    <!-- Main -->
    <main class="main">
      <header class="main-header">
        <h1 class="glitch" data-text="CERBERUS AI">CERBERUS AI</h1>
        <div class="model-tag-display" v-if="selectedModel">
          {{ selectedModel }}
        </div>
      </header>

      <div v-if="!localStatus.running" class="banner">
        Local Ollama isn't running. Start it with <code>ollama serve</code> or install it from
        <a href="https://ollama.com/download/windows" target="_blank" rel="noopener">ollama.com</a>.
        <button class="banner-retry" @click="checkLocal">Retry</button>
      </div>
      <div v-else-if="cloudStatus.kind !== 'ok'" class="banner">
        Cloud auth check failed. Your API key may have been revoked at
        <a href="https://access.cerberusai.dev" target="_blank" rel="noopener">access.cerberusai.dev</a>.
      </div>

      <div ref="messagesEl" class="messages">
        <template v-if="activeChat && activeChat.messages.length > 0">
          <div
            v-for="(m, i) in activeChat.messages"
            :key="i"
            class="msg-row"
            :class="m.role"
          >
            <div class="msg-avatar" :class="m.role">
              {{ m.role === 'user' ? 'YOU' : 'C' }}
            </div>
            <div
              class="msg-bubble"
              :class="{
                streaming:
                  streaming &&
                  i === activeChat.messages.length - 1 &&
                  m.role === 'assistant',
                thinking:
                  streaming &&
                  i === activeChat.messages.length - 1 &&
                  m.role === 'assistant' &&
                  stripThinkTags(streamingContent) === ''
              }"
            ><template v-if="streaming && i === activeChat.messages.length - 1 && m.role === 'assistant'"><span v-if="stripThinkTags(streamingContent) === ''" class="thinking-label">Thinking…</span><template v-else>{{ stripThinkTags(streamingContent) }}</template></template><template v-else>{{ m.content }}</template></div>
          </div>
          <div class="msg-row" v-if="lastTtft !== null && !streaming" style="margin-top: -12px; margin-bottom: 8px;">
            <div style="margin-left: 38px; display: flex; gap: 6px; opacity: 0.65;">
               <span class="model-tag">⚡ TTFT: {{ lastTtft }}ms</span>
               <span class="model-tag" v-if="lastTps">{{ lastTps.toFixed(1) }} tok/s</span>
            </div>
          </div>
        </template>

        <div v-else class="empty">
          <img src="./assets/logo.png" class="empty-logo-img" alt="Cerberus Logo" />
          <h2>Cerberus AI</h2>
          <p>
            Unfiltered. Uncensored. Unbound. Inference runs on your hardware via Ollama;
            your API key gates access through CerberusAI.
          </p>

          <!-- Empty state action -->
          <div v-if="localStatus.running && models.length === 0" class="pull-block" style="text-align: center;">
            <p class="pull-eyebrow" style="margin-bottom: 1rem;">No local models found.</p>
            <button class="banner-retry" @click="openFileManager">OPEN MODEL MANAGER TO PULL OR IMPORT</button>
          </div>

          <div v-else class="suggestions">
            <button
              v-for="s in SUGGESTIONS"
              :key="s"
              class="suggestion"
              @click="useSuggestion(s)"
            >{{ s }}</button>
          </div>
        </div>
      </div>

      <div class="composer">
        <div class="composer-inner">
          <textarea
            v-model="draft"
            placeholder="Message Cerberus…"
            rows="1"
            @keydown="onComposerKeydown"
            @input="autosizeComposer"
          ></textarea>
          <button
            v-if="streaming"
            class="stop-btn"
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
            class="send-btn"
            :disabled="!draft.trim() || streaming || !localStatus.running || cloudStatus.kind !== 'ok' || !selectedModel || !!pulling || !models.some((m) => m.name.replace(/:latest$/, '') === selectedModel)"
            @click="send"
            aria-label="Send"
          >
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
              <path d="M5 12h14M13 6l6 6-6 6"/>
            </svg>
          </button>
        </div>
        <div class="composer-hint">
          <kbd>Enter</kbd> to send · <kbd>Shift</kbd>+<kbd>Enter</kbd> for newline
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
  margin-top: 0.35rem;
  display: flex;
  flex-direction: column;
  gap: 3px;
  font-size: 0.68rem;
  color: var(--text-muted);
  background: var(--bg-frost);
  backdrop-filter: blur(8px);
  border: 1px solid var(--glass-border);
  border-radius: var(--radius-sm);
  padding: 8px 10px;
}
.hw-line {
  display: flex;
  justify-content: space-between;
  gap: 8px;
}
.hw-label {
  letter-spacing: 1.5px;
  font-weight: 700;
  color: var(--red-400);
  text-transform: uppercase;
  font-size: 0.6rem;
  font-family: 'JetBrains Mono', monospace;
}
.hw-val {
  color: var(--text-secondary);
  font-family: 'JetBrains Mono', monospace;
  font-size: 0.68rem;
  text-align: right;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.signout-btn {
  margin-top: 0.35rem;
  background: var(--bg-frost);
  backdrop-filter: blur(8px);
  border: 1px solid var(--glass-border);
  color: var(--text-muted);
  padding: 7px 10px;
  border-radius: var(--radius-sm);
  font-size: 0.65rem;
  letter-spacing: 2px;
  font-weight: 700;
  text-transform: uppercase;
  transition: all 150ms var(--ease-out);
}
.signout-btn:hover {
  color: var(--red-400);
  border-color: var(--glass-border-red);
  background: var(--red-glow-dim);
}

.update-btn {
  margin-top: 0.8rem;
  background: var(--bg-frost);
  backdrop-filter: blur(8px);
  border: 1px solid var(--glass-border);
  color: var(--text-muted);
  padding: 7px 10px;
  border-radius: var(--radius-sm);
  font-size: 0.65rem;
  letter-spacing: 2px;
  font-weight: 800;
  text-transform: uppercase;
  transition: all 150ms var(--ease-out);
}
.update-btn:hover:not(:disabled) {
  filter: brightness(1.2);
  transform: translateY(-1px);
  box-shadow: 0 6px 16px var(--red-glow);
}
.update-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.update-btn-available {
  background: var(--red-600) !important;
  border-color: var(--red-500) !important;
  color: #fff !important;
  box-shadow: 0 0 0 1px var(--red-500), 0 6px 18px var(--red-glow);
  animation: update-pulse 2.4s infinite ease-in-out;
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
  border: 1px solid var(--glass-border-red);
  border-radius: var(--radius-xl);
  box-shadow:
    0 30px 80px -20px rgba(0,0,0,0.9),
    0 0 0 1px rgba(220, 38, 38, 0.12),
    0 0 80px -20px rgba(220, 38, 38, 0.15);
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
  height: 36px;
  z-index: 900;
  background: rgba(2, 2, 4, 0.92);
  backdrop-filter: blur(10px);
  -webkit-backdrop-filter: blur(10px);
  border-bottom: 1px solid var(--glass-border-red);
  overflow: hidden;
  box-shadow: 0 4px 14px rgba(0, 0, 0, 0.5);
}
.download-bar-fill {
  position: absolute;
  inset: 0 auto 0 0;
  background: linear-gradient(90deg, var(--red-600), var(--red-400));
  box-shadow: 0 0 16px var(--red-glow), inset 0 0 12px rgba(255, 80, 80, 0.4);
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
.download-bar-text {
  position: relative;
  z-index: 1;
  height: 100%;
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 0 16px;
  font-family: 'JetBrains Mono', monospace;
  font-size: 0.72rem;
  color: #fff;
  text-shadow: 0 1px 4px rgba(0, 0, 0, 0.8);
  white-space: nowrap;
  overflow: hidden;
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
  font-weight: 800;
  letter-spacing: 1px;
  flex-shrink: 0;
}
.download-bar-cancel {
  margin-left: auto;
  flex-shrink: 0;
  background: none;
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
  padding-top: 36px;
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
</style>
