<template>
  <div v-if="show" class="web-overlay" @click.self="closeAll">
    <div class="web-panel" @click.stop>
      <div class="web-header">
        <div style="display: flex; align-items: center; gap: 8px;">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="#d8b4fe" stroke-width="2"><circle cx="12" cy="12" r="10"></circle><line x1="2" y1="12" x2="22" y2="12"></line><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"></path></svg>
          <h2 style="font-size: 1.1rem; font-weight: 700; color: #fff; margin: 0;">Web Browser</h2>
        </div>
        <button @click="closeAll" class="web-close" title="Close browser">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>
        </button>
      </div>

      <!-- Address bar -->
      <div class="web-addressbar">
        <button @click="goBack" class="nav-btn" title="Back" :disabled="!isOpen">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="15 18 9 12 15 6"></polyline></svg>
        </button>
        <button @click="goForward" class="nav-btn" title="Forward" :disabled="!isOpen">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="9 18 15 12 9 6"></polyline></svg>
        </button>
        <button @click="reload" class="nav-btn" title="Reload" :disabled="!isOpen">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="23 4 23 10 17 10"></polyline><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"></path></svg>
        </button>
        <input
          v-model="addressInput"
          @keydown.enter="go"
          class="address-input"
          placeholder="Search or enter a website"
          spellcheck="false"
        />
        <button @click="go" class="go-btn">Go</button>
      </div>

      <!-- Status / preview area -->
      <div class="web-body">
        <div v-if="!isOpen" class="web-idle">
          <svg width="52" height="52" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.3" style="opacity: 0.35; margin-bottom: 1rem;"><circle cx="12" cy="12" r="10"></circle><line x1="2" y1="12" x2="22" y2="12"></line><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"></path></svg>
          <p style="font-size: 0.95rem; color: rgba(255,255,255,0.7); margin-bottom: 0.4rem;">Enter a URL above to start browsing</p>
          <p style="font-size: 0.8rem; color: rgba(255,255,255,0.4); max-width: 380px; text-align: center; line-height: 1.5;">
            The live page opens in its own window. Use the controls here to navigate, then capture a screenshot or the page text and attach it to your message.
          </p>
          <div class="quick-sites">
            <button v-for="site in quickSites" :key="site.url" @click="openSite(site.url)" class="quick-site-btn">{{ site.label }}</button>
          </div>
        </div>

        <div v-else class="web-live">
          <div class="live-badge">
            <span class="live-dot"></span>
            Live in separate window
          </div>
          <div class="current-url" :title="currentUrl">{{ currentUrl || addressInput }}</div>
          <div v-if="shotPreview" class="shot-preview">
            <img :src="'data:image/png;base64,' + shotPreview" />
          </div>
          <p class="web-hint">Bring the browser window to the front, then capture from below.</p>
        </div>

        <div v-if="statusMsg" class="web-status" :class="{ error: isError }">{{ statusMsg }}</div>
      </div>

      <!-- Capture actions -->
      <div class="web-actions">
        <button @click="captureScreenshot" class="capture-btn" :disabled="!isOpen || busy">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M23 19a2 2 0 0 1-2 2H3a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4l2-3h6l2 3h4a2 2 0 0 1 2 2z"></path><circle cx="12" cy="13" r="4"></circle></svg>
          Attach screenshot
        </button>
        <button @click="captureText" class="capture-btn" :disabled="!isOpen || busy">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"></path><polyline points="14 2 14 8 20 8"></polyline><line x1="16" y1="13" x2="8" y2="13"></line><line x1="16" y1="17" x2="8" y2="17"></line></svg>
          Attach page text
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";

const props = defineProps<{ show: boolean }>();
const emit = defineEmits<{
  (e: "close"): void;
  (e: "attachImage", base64: string): void;
  (e: "attachFile", file: { name: string; content: string }): void;
}>();

const addressInput = ref("");
const currentUrl = ref("");
const isOpen = ref(false);
const busy = ref(false);
const statusMsg = ref("");
const isError = ref(false);
const shotPreview = ref("");

const quickSites = [
  { label: "DuckDuckGo", url: "https://duckduckgo.com" },
  { label: "Wikipedia", url: "https://wikipedia.org" },
  { label: "GitHub", url: "https://github.com" },
  { label: "Hacker News", url: "https://news.ycombinator.com" },
];

let urlPoll: ReturnType<typeof setInterval> | null = null;

function normalizeUrl(input: string): string {
  const s = input.trim();
  if (!s) return "";
  if (/^https?:\/\//i.test(s)) return s;
  // A bare domain (has a dot, no spaces) → prepend https. Otherwise search.
  if (/^[^\s]+\.[^\s]+$/.test(s)) return "https://" + s;
  return "https://duckduckgo.com/?q=" + encodeURIComponent(s);
}

function setStatus(msg: string, error = false) {
  statusMsg.value = msg;
  isError.value = error;
  if (msg && !error) {
    setTimeout(() => { if (statusMsg.value === msg) statusMsg.value = ""; }, 3000);
  }
}

async function openSite(url: string) {
  addressInput.value = url;
  await go();
}

async function go() {
  const url = normalizeUrl(addressInput.value);
  if (!url) return;
  busy.value = true;
  try {
    await invoke("browser_open", { url });
    isOpen.value = true;
    currentUrl.value = url;
    startPolling();
  } catch (e: any) {
    setStatus(typeof e === "string" ? e : "Could not open the browser window.", true);
  } finally {
    busy.value = false;
  }
}

async function goBack() { try { await invoke("browser_back"); } catch {} }
async function goForward() { try { await invoke("browser_forward"); } catch {} }
async function reload() { try { await invoke("browser_reload"); } catch {} }

function startPolling() {
  stopPolling();
  urlPoll = setInterval(async () => {
    try {
      const u = await invoke<string>("browser_get_url");
      if (u) {
        currentUrl.value = u;
        if (document.activeElement?.className !== "address-input") addressInput.value = u;
      }
    } catch {
      // Window was closed by the user.
      isOpen.value = false;
      stopPolling();
    }
  }, 1500);
}

function stopPolling() {
  if (urlPoll) { clearInterval(urlPoll); urlPoll = null; }
}

async function captureScreenshot() {
  busy.value = true;
  setStatus("Capturing…");
  try {
    const b64 = await invoke<string>("browser_screenshot");
    shotPreview.value = b64;
    emit("attachImage", b64);
    setStatus("Screenshot attached to your message.");
  } catch (e: any) {
    setStatus(typeof e === "string" ? e : "Screenshot failed.", true);
  } finally {
    busy.value = false;
  }
}

async function captureText() {
  busy.value = true;
  setStatus("Reading page…");
  try {
    const res = await invoke<{ url: string; title: string; text: string }>("browser_extract_text");
    const name = (res.title || res.url || "web-page").replace(/[\\/:*?"<>|]/g, "_").slice(0, 80) + ".txt";
    const header = `Source: ${res.url}\nTitle: ${res.title}\n\n`;
    emit("attachFile", { name, content: header + res.text });
    setStatus("Page text attached to your message.");
  } catch (e: any) {
    setStatus(typeof e === "string" ? e : "Could not read the page text.", true);
  } finally {
    busy.value = false;
  }
}

async function closeBrowserWindow() {
  stopPolling();
  isOpen.value = false;
  shotPreview.value = "";
  try { await invoke("browser_close"); } catch {}
}

async function closeAll() {
  await closeBrowserWindow();
  emit("close");
}

watch(
  () => props.show,
  (open) => {
    if (!open) {
      // Leave the browser window open? No — close it with the panel to avoid orphans.
      closeBrowserWindow();
    }
  }
);

onUnmounted(() => { stopPolling(); });
</script>

<style scoped>
.web-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.7);
  backdrop-filter: blur(8px);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 2000;
  padding: 2rem;
}
.web-panel {
  background: linear-gradient(135deg, #120421 0%, #050505 100%);
  border: 1px solid rgba(168, 85, 247, 0.2);
  border-radius: 18px;
  box-shadow: 0 25px 80px rgba(0, 0, 0, 0.8);
  width: 100%;
  max-width: 560px;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.web-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 1rem 1.3rem;
  border-bottom: 1px solid rgba(255, 255, 255, 0.06);
}
.web-close {
  background: rgba(255, 255, 255, 0.05);
  border: 1px solid rgba(255, 255, 255, 0.1);
  color: rgba(255, 255, 255, 0.7);
  width: 30px;
  height: 30px;
  border-radius: 8px;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
}
.web-close:hover { background: rgba(255, 255, 255, 0.1); color: #fff; }
.web-addressbar {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 0.8rem 1.3rem;
}
.nav-btn {
  background: rgba(255, 255, 255, 0.04);
  border: 1px solid rgba(255, 255, 255, 0.08);
  color: rgba(255, 255, 255, 0.75);
  width: 32px;
  height: 32px;
  border-radius: 8px;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  flex-shrink: 0;
}
.nav-btn:disabled { opacity: 0.3; cursor: default; }
.nav-btn:not(:disabled):hover { background: rgba(88, 28, 135, 0.3); }
.address-input {
  flex: 1;
  min-width: 0;
  background: rgba(0, 0, 0, 0.4);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 8px;
  padding: 8px 12px;
  color: #fff;
  font-size: 0.85rem;
  outline: none;
}
.address-input:focus { border-color: rgba(168, 85, 247, 0.5); }
.go-btn {
  background: linear-gradient(135deg, #7c3aed, #a855f7);
  border: none;
  color: #fff;
  padding: 8px 16px;
  border-radius: 8px;
  font-weight: 700;
  font-size: 0.85rem;
  cursor: pointer;
  flex-shrink: 0;
}
.web-body {
  padding: 1.2rem 1.3rem;
  min-height: 180px;
  display: flex;
  flex-direction: column;
}
.web-idle {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  color: rgba(255, 255, 255, 0.5);
  text-align: center;
}
.quick-sites { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 1.4rem; justify-content: center; }
.quick-site-btn {
  padding: 6px 14px;
  background: rgba(255, 255, 255, 0.03);
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: 20px;
  color: rgba(255, 255, 255, 0.7);
  font-size: 0.8rem;
  cursor: pointer;
}
.quick-site-btn:hover { background: rgba(88, 28, 135, 0.3); border-color: rgba(168, 85, 247, 0.4); color: #fff; }
.web-live { flex: 1; }
.live-badge {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 0.72rem;
  font-weight: 700;
  letter-spacing: 0.5px;
  color: #86efac;
  background: rgba(34, 197, 94, 0.1);
  border: 1px solid rgba(34, 197, 94, 0.25);
  padding: 4px 10px;
  border-radius: 20px;
  margin-bottom: 0.8rem;
}
.live-dot { width: 7px; height: 7px; border-radius: 50%; background: #22c55e; box-shadow: 0 0 8px #22c55e; }
.current-url {
  font-size: 0.82rem;
  color: rgba(255, 255, 255, 0.8);
  font-family: ui-monospace, monospace;
  background: rgba(0, 0, 0, 0.3);
  border: 1px solid rgba(255, 255, 255, 0.06);
  border-radius: 8px;
  padding: 8px 12px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.shot-preview {
  margin-top: 0.9rem;
  border-radius: 10px;
  overflow: hidden;
  border: 1px solid rgba(255, 255, 255, 0.1);
  max-height: 200px;
}
.shot-preview img { width: 100%; display: block; }
.web-hint { font-size: 0.78rem; color: rgba(255, 255, 255, 0.4); margin-top: 0.9rem; }
.web-status {
  margin-top: 0.9rem;
  font-size: 0.82rem;
  color: #86efac;
  padding: 8px 12px;
  background: rgba(34, 197, 94, 0.08);
  border-radius: 8px;
}
.web-status.error { color: #fca5a5; background: rgba(239, 68, 68, 0.08); }
.web-actions {
  display: flex;
  gap: 10px;
  padding: 1rem 1.3rem 1.3rem;
  border-top: 1px solid rgba(255, 255, 255, 0.06);
}
.capture-btn {
  flex: 1;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 11px 16px;
  background: rgba(255, 255, 255, 0.03);
  border: 1px solid rgba(168, 85, 247, 0.25);
  border-radius: 10px;
  color: #d8b4fe;
  font-size: 0.85rem;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.2s ease;
}
.capture-btn:not(:disabled):hover { background: rgba(88, 28, 135, 0.3); border-color: rgba(168, 85, 247, 0.5); }
.capture-btn:disabled { opacity: 0.35; cursor: default; }
</style>
