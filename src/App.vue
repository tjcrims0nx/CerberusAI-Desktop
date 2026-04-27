<script setup lang="ts">
import { ref, computed, onMounted, nextTick, watch } from "vue";
import { invoke, Channel } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type {
  Chat,
  Message,
  OllamaModel,
  OllamaStatus,
  HardwareInfo,
  ChatStreamChunk,
} from "./types";

const STORAGE_KEY = "cerberus.chats.v1";
const MODEL_KEY = "cerberus.model.v1";
const APIKEY_KEY = "cerberus.apiKey.v1";

const chats = ref<Chat[]>([]);
const activeId = ref<string | null>(null);
const models = ref<OllamaModel[]>([]);
const selectedModel = ref<string>(localStorage.getItem(MODEL_KEY) || "");
const cloudStatus = ref<OllamaStatus>({ kind: "checking" });
const localStatus = ref<{ running: boolean; version?: string; error?: string }>({ running: false });
const hardware = ref<HardwareInfo | null>(null);
const draft = ref<string>("");
const streaming = ref<boolean>(false);
const messagesEl = ref<HTMLElement | null>(null);

// API key gate
const apiKey = ref<string>(localStorage.getItem(APIKEY_KEY) || "");
const apiKeyVerified = ref<boolean>(false);
const apiKeyDraft = ref<string>("");
const verifying = ref<boolean>(false);
const verifyError = ref<string>("");

// Suggested Cerberus models that the user can pull on demand
const SUGGESTED_MODELS = [
  { tag: "cerberus-4b-v2-abliterated", size: "~2.6 GB", note: "Q4_K_M · runs on most hardware" },
  { tag: "Arbiter-GL9b", size: "~6.2 GB", note: "Q4_K_M · high-performance reasoning" },
];

// Active model pull
const pulling = ref<{ name: string; pct: number; status: string } | null>(null);

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
  if (v) localStorage.setItem(MODEL_KEY, v);
});

async function refreshModels() {
  try {
    const list = await invoke<OllamaModel[]>("list_models");
    // Only allow Cerberus and Arbiter models
    const filtered = list.filter(m => 
      m.name.toLowerCase().includes("cerberus") || 
      m.name.toLowerCase().includes("arbiter")
    );
    models.value = filtered;

    // If current selected model is not in the filtered list, reset it
    if (selectedModel.value && !filtered.find(m => m.name === selectedModel.value)) {
      selectedModel.value = "";
    }

    if (!selectedModel.value && filtered.length) {
      const cerberus = filtered.find((m) => m.name.toLowerCase().includes("cerberus"));
      selectedModel.value = cerberus?.name ?? filtered[0]?.name ?? "";
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
    // Now that we have a key, connect to the API
    await checkApi();
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

  const assistantMsg: Message = { role: "assistant", content: "" };
  chat.messages.push(assistantMsg);
  streaming.value = true;

  const channel = new Channel<ChatStreamChunk>();
  channel.onmessage = (chunk) => {
    if (chunk.error) {
      assistantMsg.content += `\n\n[error] ${chunk.error}`;
      streaming.value = false;
      saveChats();
      return;
    }
    if (chunk.delta) {
      assistantMsg.content += chunk.delta;
      scrollToBottom();
    }
    if (chunk.done) {
      streaming.value = false;
      saveChats();
    }
  };

  try {
    await invoke("chat_stream", {
      model: selectedModel.value,
      messages: chat.messages.slice(0, -1),
      onEvent: channel,
    });
  } catch (e) {
    assistantMsg.content += `\n\n[error] ${String(e)}`;
    streaming.value = false;
    saveChats();
  }
}

interface PullProgress {
  status: string;
  completed?: number;
  total?: number;
  done: boolean;
  error?: string;
}

async function pullModel(name: string) {
  if (pulling.value) return;
  pulling.value = { name, pct: 0, status: "starting…" };
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
    await invoke("pull_model", { name, onEvent: channel });
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
];

function useSuggestion(text: string) {
  draft.value = text;
}

onMounted(async () => {
  loadChats();
  if (apiKey.value) {
    apiKeyVerified.value = await verifyKey(apiKey.value);
    if (!apiKeyVerified.value) {
      localStorage.removeItem(APIKEY_KEY);
      apiKey.value = "";
    }
  }
  await Promise.all([
    detectHardware(),
    apiKey.value ? checkApi() : Promise.resolve(),
    checkLocal(),
  ]);
});
</script>

<template>
  <div class="glow-orb orb-1"></div>
  <div class="glow-orb orb-2"></div>

  <!-- API Key Gate -->
  <div v-if="!apiKeyVerified" class="key-gate">
    <div class="key-card">
      <div class="key-logo">C</div>
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

  <div class="shell" :class="{ 'shell-blocked': !apiKeyVerified }">
    <!-- Sidebar -->
    <aside class="sidebar">
      <div class="brand">
        <div class="brand-logo">C</div>
        <div class="brand-name">CERBERUS</div>
        <div class="brand-sub">v0.1</div>
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
        <select
          class="model-select"
          v-model="selectedModel"
          :disabled="!localStatus.running || models.length === 0"
        >
          <option v-if="models.length === 0" value="">No models pulled</option>
          <option v-for="m in models" :key="m.name" :value="m.name">
            {{ m.name }}
          </option>
        </select>

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
      <div v-else-if="pulling" class="banner pulling">
        Pulling <code>{{ pulling.name }}</code> · {{ pulling.status }}
        <span v-if="pulling.pct > 0"> · {{ pulling.pct }}%</span>
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
                  m.role === 'assistant'
              }"
            >{{ m.content }}</div>
          </div>
        </template>

        <div v-else class="empty">
          <div class="empty-logo">C</div>
          <h2>Cerberus AI</h2>
          <p>
            Unfiltered. Uncensored. Unbound. Inference runs on your hardware via Ollama;
            your API key gates access through CerberusAI.
          </p>

          <!-- Model picker if none pulled yet -->
          <div v-if="localStatus.running && models.length === 0" class="pull-block">
            <p class="pull-eyebrow">No local models yet — pull one to start</p>
            <div class="pull-grid">
              <button
                v-for="m in SUGGESTED_MODELS"
                :key="m.tag"
                class="pull-card"
                :disabled="!!pulling"
                @click="pullModel(m.tag)"
              >
                <span class="pull-name">{{ m.tag }}</span>
                <span class="pull-meta">{{ m.size }} · {{ m.note }}</span>
                <span class="pull-action">{{ pulling?.name === m.tag ? 'Pulling…' : 'Pull' }}</span>
              </button>
            </div>
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
            class="send-btn"
            :disabled="!draft.trim() || streaming || !localStatus.running || cloudStatus.kind !== 'ok' || !selectedModel"
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
  background: var(--bg-frost-deep);
  backdrop-filter: blur(var(--frost-blur-heavy));
  -webkit-backdrop-filter: blur(var(--frost-blur-heavy));
  border: 1px solid var(--glass-border-red);
  border-radius: var(--radius-xl);
  padding: 2.5rem 2rem;
  text-align: center;
  box-shadow:
    0 30px 80px -20px rgba(0,0,0,0.9),
    0 0 0 1px rgba(220, 38, 38, 0.12),
    0 0 80px -20px rgba(220, 38, 38, 0.15);
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
