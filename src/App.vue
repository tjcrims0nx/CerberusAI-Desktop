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
const VERIFY_URL = "https://api.cerberusai.dev/v1/models";

const chats = ref<Chat[]>([]);
const activeId = ref<string | null>(null);
const models = ref<OllamaModel[]>([]);
const selectedModel = ref<string>(localStorage.getItem(MODEL_KEY) || "");
const status = ref<OllamaStatus>({ kind: "checking" });
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
    models.value = list;
    if (!selectedModel.value && list.length) {
      selectedModel.value = pickRecommendedModel(list);
    }
  } catch (e) {
    console.warn("list_models failed", e);
  }
}

function pickRecommendedModel(list: OllamaModel[]): string {
  const vram = primaryGpu.value?.vram_mb ?? 0;
  const cerberus = list.find((m) => m.name.toLowerCase().includes("cerberus"));
  if (cerberus) return cerberus.name;
  const sized = [...list].sort((a, b) => a.size - b.size);
  if (vram > 0 && vram < 8 * 1024) return sized[0]?.name ?? "";
  return sized[Math.floor(sized.length / 2)]?.name ?? sized[0]?.name ?? "";
}

async function checkOllama() {
  status.value = { kind: "checking" };
  try {
    const version = await invoke<string>("check_ollama");
    status.value = { kind: "ok", version };
    await refreshModels();
  } catch (e: any) {
    const msg = String(e ?? "unknown");
    status.value = msg.includes("connection")
      ? { kind: "missing" }
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
    const r = await fetch(VERIFY_URL, {
      method: "GET",
      headers: { Authorization: `Bearer ${key}` },
    });
    if (r.status === 200) return true;
    if (r.status === 401 || r.status === 403) {
      verifyError.value = "Invalid API key.";
      return false;
    }
    verifyError.value = `Verify failed (HTTP ${r.status}). Try again.`;
    return false;
  } catch (e: any) {
    verifyError.value = `Network error: ${e?.message ?? e}`;
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
  if (!apiKeyVerified.value) return;
  if (status.value.kind !== "ok") {
    await checkOllama();
    if ((status.value as OllamaStatus).kind !== "ok") return;
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
  "Summarize the last commit on this repo.",
];

function useSuggestion(text: string) {
  draft.value = text;
}

onMounted(async () => {
  loadChats();
  if (apiKey.value) {
    apiKeyVerified.value = await verifyKey(apiKey.value);
    if (!apiKeyVerified.value) {
      // Stored key is no longer valid — clear it so the gate prompts again.
      localStorage.removeItem(APIKEY_KEY);
      apiKey.value = "";
    }
  }
  await Promise.all([detectHardware(), checkOllama()]);
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
          :disabled="status.kind !== 'ok' || models.length === 0"
        >
          <option v-if="models.length === 0" value="">No models found</option>
          <option v-for="m in models" :key="m.name" :value="m.name">
            {{ m.name }}
          </option>
        </select>

        <span v-if="status.kind === 'ok'" class="status-pill ok">
          <span class="dot"></span> OLLAMA {{ status.version }}
        </span>
        <span v-else-if="status.kind === 'checking'" class="status-pill warn">
          <span class="dot"></span> CHECKING…
        </span>
        <span v-else-if="status.kind === 'missing'" class="status-pill err">
          <span class="dot"></span> OLLAMA OFFLINE
        </span>
        <span v-else class="status-pill err">
          <span class="dot"></span> ERROR
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
        <h1 class="glitch" data-text="CERBERUS LOCAL">CERBERUS LOCAL</h1>
        <div class="model-tag-display" v-if="selectedModel">
          {{ selectedModel }}
        </div>
      </header>

      <div v-if="status.kind === 'missing'" class="banner">
        Ollama isn't running. Start it with <code>ollama serve</code> or install it from
        <a href="https://ollama.com/download/windows" target="_blank" rel="noopener">ollama.com</a>.
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
          <h2>Cerberus Local</h2>
          <p>
            Unfiltered chat that runs on your machine. Pick a model, type a prompt, send.
            Nothing leaves this computer.
          </p>
          <div class="suggestions">
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
            :disabled="!draft.trim() || streaming || status.kind !== 'ok' || !selectedModel"
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
  font-size: 0.72rem;
  color: var(--text-muted);
  background: rgba(255,255,255,0.04);
  border: 1px solid rgba(255,255,255,0.08);
  padding: 4px 10px;
  border-radius: 50px;
  letter-spacing: 1px;
  text-transform: uppercase;
  font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
}

.hw-summary {
  margin-top: 0.4rem;
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 0.72rem;
  color: var(--text-muted);
}
.hw-line {
  display: flex;
  justify-content: space-between;
  gap: 8px;
}
.hw-label {
  letter-spacing: 1.5px;
  font-weight: 700;
  color: rgba(255,255,255,0.45);
  text-transform: uppercase;
  font-size: 0.65rem;
}
.hw-val {
  color: #d8dae0;
  font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
  font-size: 0.72rem;
  text-align: right;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.signout-btn {
  margin-top: 0.4rem;
  background: transparent;
  border: 1px solid rgba(255,255,255,0.08);
  color: var(--text-muted);
  padding: 6px 10px;
  border-radius: 8px;
  font-size: 0.7rem;
  letter-spacing: 1.5px;
  font-weight: 700;
  transition: color 140ms ease, border-color 140ms ease, background 140ms ease;
}
.signout-btn:hover {
  color: #fff;
  border-color: rgba(255, 26, 64, 0.32);
  background: rgba(255, 26, 64, 0.05);
}

/* API Key Gate */
.shell-blocked {
  filter: blur(8px) brightness(0.4);
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
  background: rgba(3, 4, 7, 0.55);
  backdrop-filter: blur(6px);
  -webkit-backdrop-filter: blur(6px);
}

.key-card {
  width: 100%;
  max-width: 460px;
  background: var(--bg-elev-1);
  backdrop-filter: blur(20px);
  -webkit-backdrop-filter: blur(20px);
  border: 1px solid var(--glass-border);
  border-radius: 20px;
  padding: 2.5rem 2rem;
  text-align: center;
  box-shadow:
    0 25px 60px -15px rgba(0,0,0,0.85),
    0 0 0 1px rgba(255, 26, 64, 0.18),
    0 0 60px -10px rgba(255, 26, 64, 0.18);
  animation: gateIn 320ms cubic-bezier(0.2, 0.9, 0.3, 1.2);
}
@keyframes gateIn {
  from { opacity: 0; transform: translateY(12px) scale(0.97); }
  to   { opacity: 1; transform: none; }
}

.key-logo {
  width: 64px; height: 64px;
  margin: 0 auto 1.25rem;
  border-radius: 16px;
  background: linear-gradient(135deg, var(--primary), var(--secondary));
  display: flex; align-items: center; justify-content: center;
  color: #fff; font-weight: 900; font-size: 1.7rem;
  box-shadow: 0 0 40px rgba(255, 26, 64, 0.55);
  animation: pulse 3s infinite alternate;
}

.key-eyebrow {
  color: var(--primary);
  font-weight: 700;
  letter-spacing: 2px;
  text-transform: uppercase;
  font-size: 0.78rem;
  margin-bottom: 0.4rem;
}

.key-title {
  font-size: 2.4rem !important;
  letter-spacing: 3px !important;
  margin-bottom: 0.6rem !important;
}

.key-sub {
  color: var(--text-muted);
  font-size: 0.9rem;
  line-height: 1.55;
  margin-bottom: 1.6rem;
}
.key-sub a { color: var(--primary); }

.key-form {
  display: flex;
  flex-direction: column;
  gap: 0.6rem;
}

.key-form input {
  background: rgba(0,0,0,0.35);
  border: 1px solid rgba(255,255,255,0.1);
  border-radius: 10px;
  padding: 13px 16px;
  color: #fff;
  font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
  font-size: 0.95rem;
  letter-spacing: 1px;
  text-align: center;
  outline: none;
  transition: border-color 140ms ease, box-shadow 140ms ease;
}
.key-form input:focus {
  border-color: rgba(255, 26, 64, 0.5);
  box-shadow: 0 0 0 3px rgba(255, 26, 64, 0.1);
}

.key-form button {
  border: 1px solid rgba(255, 61, 46, 0.55);
  background: linear-gradient(135deg, #ff5445 0%, #ff3d2e 55%, #c61f12 100%);
  color: #fff;
  padding: 13px 16px;
  border-radius: 10px;
  font-weight: 900;
  letter-spacing: 2.5px;
  font-size: 0.85rem;
  text-transform: uppercase;
  box-shadow: 0 14px 32px -16px rgba(255, 61, 46, 0.6);
  transition: transform 120ms ease, filter 120ms ease, opacity 120ms ease;
}
.key-form button:hover:not(:disabled) {
  transform: translateY(-1px);
  filter: brightness(1.06);
}
.key-form button:disabled { opacity: 0.45; cursor: not-allowed; }

.key-error {
  color: var(--err);
  font-size: 0.82rem;
  margin-top: 0.8rem;
  font-weight: 500;
}

.key-foot {
  margin-top: 1.6rem;
  font-size: 0.72rem;
  color: var(--text-muted);
  line-height: 1.55;
}
.key-foot code {
  background: rgba(255,255,255,0.06);
  border: 1px solid rgba(255,255,255,0.08);
  padding: 1px 6px;
  border-radius: 4px;
  font-family: ui-monospace, monospace;
  color: #fff;
  font-size: 0.7rem;
}
</style>
