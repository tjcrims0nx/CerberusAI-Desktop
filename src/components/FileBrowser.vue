<template>
  <div v-if="show" class="file-browser-overlay" @click.self="$emit('close')">
    <div class="browser-panel" @click.stop>
      <div class="browser-header">
        <h2 style="font-size: 1.2rem; font-weight: 700; color: #fff; margin: 0;">Files & Pictures</h2>
        <button @click="$emit('close')" class="close-btn" style="background: rgba(255,255,255,0.05); border: 1px solid rgba(255,255,255,0.1); color: rgba(255,255,255,0.7); width: 32px; height: 32px; border-radius: 8px; display: flex; align-items: center; justify-content: center; transition: all 0.2s ease;">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>
        </button>
      </div>

      <div class="browser-tabs">
        <button @click="activeTab = 'library'" :class="{ active: activeTab === 'library' }" class="tab-btn">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="margin-right: 6px;"><rect x="3" y="3" width="18" height="18" rx="2"></rect><circle cx="8.5" cy="8.5" r="1.5"></circle><polyline points="21 15 16 10 5 21"></polyline></svg>
          Library
        </button>
        <button @click="activeTab = 'browse'" :class="{ active: activeTab === 'browse' }" class="tab-btn">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="margin-right: 6px;"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path></svg>
          Browse
        </button>
      </div>

      <!-- Library Tab -->
      <div v-if="activeTab === 'library'" class="tab-content">
        <div class="library-toolbar">
          <input type="file" ref="libraryFileInput" @change="handleLibraryUpload" style="display: none" multiple />
          <button @click="triggerLibraryUpload" class="btn-metal-dark" style="padding: 8px 16px; font-size: 0.85rem; display: inline-flex; align-items: center; gap: 6px;">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="12" y1="5" x2="12" y2="19"></line><line x1="5" y1="12" x2="19" y2="12"></line></svg>
            Upload
          </button>
          <span style="color: rgba(255,255,255,0.4); font-size: 0.8rem; margin-left: 12px;">{{ libraryItems.length }} item{{ libraryItems.length !== 1 ? 's' : '' }}</span>
        </div>

        <div v-if="libraryItems.length === 0" style="text-align: center; padding: 4rem 2rem; color: rgba(255,255,255,0.4);">
          <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" style="margin: 0 auto 1rem; opacity: 0.3;"><rect x="3" y="3" width="18" height="18" rx="2"></rect><circle cx="8.5" cy="8.5" r="1.5"></circle><polyline points="21 15 16 10 5 21"></polyline></svg>
          <p style="font-size: 0.9rem; margin-bottom: 0.5rem;">No files in library</p>
          <p style="font-size: 0.8rem; opacity: 0.7;">Upload files to reuse them across chats</p>
        </div>

        <div v-else class="library-grid">
          <div v-for="item in libraryItems" :key="item.id" class="library-item">
            <div v-if="item.kind === 'image'" class="library-thumb" @click="attachLibraryImage(item)">
              <img :src="'data:image/' + item.ext + ';base64,' + (libraryThumbs[item.id] || '')" v-if="libraryThumbs[item.id]" />
              <div v-else style="display: flex; align-items: center; justify-content: center; height: 100%;">
                <div class="spinner-small"></div>
              </div>
              <div class="library-overlay">
                <button class="attach-icon-btn" title="Attach to message">
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><line x1="12" y1="5" x2="12" y2="19"></line><line x1="5" y1="12" x2="19" y2="12"></line></svg>
                </button>
              </div>
            </div>
            <div v-else class="library-file-card" @click="attachLibraryFile(item)">
              <svg width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" style="opacity: 0.5;"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"></path><polyline points="14 2 14 8 20 8"></polyline></svg>
            </div>
            <div class="library-item-footer">
              <span class="library-item-name" :title="item.name">{{ item.name }}</span>
              <button @click.stop="deleteLibraryItem(item.id)" class="delete-icon-btn" title="Delete">
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"></polyline><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"></path></svg>
              </button>
            </div>
          </div>
        </div>
      </div>

      <!-- Browse Tab -->
      <div v-if="activeTab === 'browse'" class="tab-content">
        <div class="browse-header">
          <div class="quick-dirs">
            <button v-for="dir in quickDirs" :key="dir.path" @click="navigateTo(dir.path)" class="quick-dir-btn" :title="dir.path">
              {{ dir.label }}
            </button>
          </div>
        </div>

        <div class="breadcrumb">
          <button @click="navigateUp" :disabled="!currentListing.parent" class="breadcrumb-up" title="Go up">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="15 18 9 12 15 6"></polyline></svg>
          </button>
          <span class="breadcrumb-path">{{ currentListing.path || 'Home' }}</span>
        </div>

        <div v-if="loadingDir" style="padding: 3rem; text-align: center;">
          <div class="spinner"></div>
        </div>

        <div v-else-if="dirError" style="padding: 2rem; text-align: center; color: #ef4444;">
          <p>{{ dirError }}</p>
          <button @click="loadQuickDirs" class="btn-metal-dark" style="margin-top: 1rem; padding: 8px 16px; font-size: 0.85rem;">Go Home</button>
        </div>

        <div v-else class="browse-grid">
          <div v-for="entry in currentListing.entries" :key="entry.path" class="browse-entry" @click="handleEntryClick(entry)">
            <div class="browse-entry-icon">
              <svg v-if="entry.is_dir" width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path></svg>
              <div v-else-if="entry.is_image" class="browse-thumb">
                <img :src="'data:image/' + entry.ext + ';base64,' + (browseThumbs[entry.path] || '')" v-if="browseThumbs[entry.path]" />
                <div v-else style="display: flex; align-items: center; justify-content: center; height: 100%; font-size: 0.7rem; color: rgba(255,255,255,0.3);">
                  {{ entry.ext.toUpperCase() }}
                </div>
              </div>
              <svg v-else width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"></path><polyline points="14 2 14 8 20 8"></polyline></svg>
            </div>
            <div class="browse-entry-details">
              <div class="browse-entry-name" :title="entry.name">{{ entry.name }}</div>
              <div class="browse-entry-meta">
                <span v-if="!entry.is_dir">{{ formatSize(entry.size) }}</span>
                <span v-if="entry.ext">.{{ entry.ext }}</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";

const props = defineProps<{ show: boolean }>();
const emit = defineEmits<{
  (e: "close"): void;
  (e: "attachImage", base64: string): void;
  (e: "attachFile", file: { name: string; content: string }): void;
}>();

type LibraryItem = { id: string; name: string; kind: string; size: number; added_at: number; ext: string };
type FsEntry = { name: string; path: string; is_dir: boolean; is_image: boolean; size: number; ext: string };
type DirListing = { path: string; parent: string | null; entries: FsEntry[] };
type QuickDir = { label: string; path: string };

const activeTab = ref<"library" | "browse">("library");

// ---- Library ----
const libraryItems = ref<LibraryItem[]>([]);
const libraryThumbs = ref<Record<string, string>>({});
const libraryFileInput = ref<HTMLInputElement | null>(null);

async function loadLibrary() {
  try {
    libraryItems.value = await invoke<LibraryItem[]>("library_list");
    // Lazily fetch thumbnails for image items.
    for (const item of libraryItems.value) {
      if (item.kind === "image" && !libraryThumbs.value[item.id]) {
        invoke<string>("library_read_base64", { id: item.id })
          .then((b64) => { libraryThumbs.value[item.id] = b64; })
          .catch(() => {});
      }
    }
  } catch (e) {
    console.error("Failed to load library:", e);
  }
}

function triggerLibraryUpload() {
  libraryFileInput.value?.click();
}

async function handleLibraryUpload(e: Event) {
  const target = e.target as HTMLInputElement;
  if (!target.files) return;
  for (let i = 0; i < target.files.length; i++) {
    const file = target.files[i];
    const base64 = await fileToBase64(file);
    try {
      await invoke<LibraryItem>("library_save", { name: file.name, dataBase64: base64 });
    } catch (err) {
      console.error("Failed to save to library:", err);
    }
  }
  target.value = "";
  await loadLibrary();
}

function fileToBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = (ev) => {
      const result = ev.target?.result as string;
      resolve(result ? result.split(",")[1] : "");
    };
    reader.onerror = reject;
    reader.readAsDataURL(file);
  });
}

async function attachLibraryImage(item: LibraryItem) {
  try {
    const b64 = libraryThumbs.value[item.id] || (await invoke<string>("library_read_base64", { id: item.id }));
    emit("attachImage", b64);
    emit("close");
  } catch (e) {
    console.error("Failed to attach library image:", e);
  }
}

async function attachLibraryFile(item: LibraryItem) {
  try {
    const content = await invoke<string>("library_read_text", { id: item.id });
    emit("attachFile", { name: item.name, content });
    emit("close");
  } catch (e) {
    console.error("Failed to attach library file:", e);
  }
}

async function deleteLibraryItem(id: string) {
  try {
    await invoke("library_delete", { id });
    delete libraryThumbs.value[id];
    await loadLibrary();
  } catch (e) {
    console.error("Failed to delete library item:", e);
  }
}

// ---- Browse ----
const quickDirs = ref<QuickDir[]>([]);
const currentListing = ref<DirListing>({ path: "", parent: null, entries: [] });
const browseThumbs = ref<Record<string, string>>({});
const loadingDir = ref(false);
const dirError = ref("");

async function loadQuickDirs() {
  try {
    quickDirs.value = await invoke<QuickDir[]>("fb_quick_dirs");
  } catch (e) {
    console.error("Failed to load quick dirs:", e);
  }
  await navigateTo("");
}

async function navigateTo(path: string) {
  loadingDir.value = true;
  dirError.value = "";
  browseThumbs.value = {};
  try {
    currentListing.value = await invoke<DirListing>("fb_list_dir", { path });
    // Lazily fetch image thumbnails.
    for (const entry of currentListing.value.entries) {
      if (entry.is_image) {
        invoke<string>("fb_read_base64", { path: entry.path })
          .then((b64) => { browseThumbs.value[entry.path] = b64; })
          .catch(() => {});
      }
    }
  } catch (e: any) {
    dirError.value = typeof e === "string" ? e : "Could not open this folder.";
  } finally {
    loadingDir.value = false;
  }
}

function navigateUp() {
  if (currentListing.value.parent) navigateTo(currentListing.value.parent);
}

async function handleEntryClick(entry: FsEntry) {
  if (entry.is_dir) {
    await navigateTo(entry.path);
    return;
  }
  if (entry.is_image) {
    try {
      const b64 = browseThumbs.value[entry.path] || (await invoke<string>("fb_read_base64", { path: entry.path }));
      emit("attachImage", b64);
      emit("close");
    } catch (e) {
      console.error("Failed to attach image:", e);
    }
  } else {
    try {
      const content = await invoke<string>("fb_read_text", { path: entry.path });
      emit("attachFile", { name: entry.name, content });
      emit("close");
    } catch (e) {
      dirError.value = typeof e === "string" ? e : "Could not read this file.";
    }
  }
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return bytes + " B";
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + " KB";
  return (bytes / (1024 * 1024)).toFixed(1) + " MB";
}

// Load data when the modal opens.
watch(
  () => props.show,
  (open) => {
    if (open) {
      loadLibrary();
      if (quickDirs.value.length === 0) loadQuickDirs();
    }
  }
);
</script>

<style scoped>
.file-browser-overlay {
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
.browser-panel {
  background: linear-gradient(135deg, #120421 0%, #050505 100%);
  border: 1px solid rgba(168, 85, 247, 0.2);
  border-radius: 20px;
  box-shadow: 0 25px 80px rgba(0, 0, 0, 0.8);
  width: 100%;
  max-width: 860px;
  height: 80vh;
  max-height: 700px;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.browser-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 1.2rem 1.5rem;
  border-bottom: 1px solid rgba(255, 255, 255, 0.06);
}
.close-btn:hover { background: rgba(255, 255, 255, 0.1) !important; color: #fff !important; }
.browser-tabs {
  display: flex;
  gap: 4px;
  padding: 0.8rem 1.5rem 0;
  border-bottom: 1px solid rgba(255, 255, 255, 0.06);
}
.tab-btn {
  display: inline-flex;
  align-items: center;
  padding: 10px 18px;
  background: transparent;
  border: none;
  border-bottom: 2px solid transparent;
  color: rgba(255, 255, 255, 0.5);
  font-size: 0.9rem;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.2s ease;
}
.tab-btn:hover { color: rgba(255, 255, 255, 0.8); }
.tab-btn.active { color: #d8b4fe; border-bottom-color: #a855f7; }
.tab-content { flex: 1; overflow-y: auto; padding: 1.2rem 1.5rem; }

.library-toolbar { display: flex; align-items: center; margin-bottom: 1.2rem; }
.library-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
  gap: 1rem;
}
.library-item {
  background: rgba(255, 255, 255, 0.02);
  border: 1px solid rgba(255, 255, 255, 0.06);
  border-radius: 12px;
  overflow: hidden;
  transition: all 0.2s ease;
}
.library-item:hover { border-color: rgba(168, 85, 247, 0.4); }
.library-thumb {
  position: relative;
  aspect-ratio: 1;
  cursor: pointer;
  background: rgba(0, 0, 0, 0.3);
  display: flex;
  align-items: center;
  justify-content: center;
}
.library-thumb img { width: 100%; height: 100%; object-fit: cover; }
.library-overlay {
  position: absolute;
  inset: 0;
  background: rgba(88, 28, 135, 0.4);
  display: flex;
  align-items: center;
  justify-content: center;
  opacity: 0;
  transition: opacity 0.2s ease;
}
.library-thumb:hover .library-overlay { opacity: 1; }
.attach-icon-btn {
  background: rgba(168, 85, 247, 0.9);
  border: none;
  color: #fff;
  width: 40px;
  height: 40px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
}
.library-file-card {
  aspect-ratio: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  color: #d8b4fe;
  background: rgba(0, 0, 0, 0.2);
}
.library-file-card:hover { background: rgba(88, 28, 135, 0.2); }
.library-item-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 10px;
  gap: 6px;
}
.library-item-name {
  font-size: 0.75rem;
  color: rgba(255, 255, 255, 0.7);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.delete-icon-btn {
  background: transparent;
  border: none;
  color: rgba(255, 255, 255, 0.3);
  cursor: pointer;
  flex-shrink: 0;
  transition: color 0.2s ease;
}
.delete-icon-btn:hover { color: #ef4444; }

.browse-header { margin-bottom: 1rem; }
.quick-dirs { display: flex; flex-wrap: wrap; gap: 6px; }
.quick-dir-btn {
  padding: 6px 14px;
  background: rgba(255, 255, 255, 0.03);
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: 8px;
  color: rgba(255, 255, 255, 0.7);
  font-size: 0.8rem;
  cursor: pointer;
  transition: all 0.2s ease;
}
.quick-dir-btn:hover { background: rgba(88, 28, 135, 0.3); border-color: rgba(168, 85, 247, 0.4); color: #fff; }
.breadcrumb {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 14px;
  background: rgba(0, 0, 0, 0.3);
  border: 1px solid rgba(255, 255, 255, 0.06);
  border-radius: 10px;
  margin-bottom: 1rem;
}
.breadcrumb-up {
  background: rgba(255, 255, 255, 0.05);
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: 6px;
  color: rgba(255, 255, 255, 0.7);
  width: 28px;
  height: 28px;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  flex-shrink: 0;
}
.breadcrumb-up:disabled { opacity: 0.3; cursor: default; }
.breadcrumb-up:not(:disabled):hover { background: rgba(88, 28, 135, 0.3); }
.breadcrumb-path {
  font-size: 0.8rem;
  color: rgba(255, 255, 255, 0.6);
  font-family: ui-monospace, monospace;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  direction: rtl;
  text-align: left;
}
.browse-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
  gap: 8px;
}
.browse-entry {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px;
  border-radius: 10px;
  cursor: pointer;
  transition: background 0.15s ease;
  min-width: 0;
}
.browse-entry:hover { background: rgba(255, 255, 255, 0.04); }
.browse-entry-icon {
  flex-shrink: 0;
  color: #d8b4fe;
  width: 40px;
  height: 40px;
  display: flex;
  align-items: center;
  justify-content: center;
}
.browse-thumb {
  width: 40px;
  height: 40px;
  border-radius: 6px;
  overflow: hidden;
  background: rgba(0, 0, 0, 0.3);
}
.browse-thumb img { width: 100%; height: 100%; object-fit: cover; }
.browse-entry-details { min-width: 0; flex: 1; }
.browse-entry-name {
  font-size: 0.82rem;
  color: rgba(255, 255, 255, 0.85);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.browse-entry-meta {
  font-size: 0.7rem;
  color: rgba(255, 255, 255, 0.35);
  display: flex;
  gap: 8px;
}
.spinner, .spinner-small {
  border: 3px solid rgba(255, 255, 255, 0.1);
  border-top-color: #a855f7;
  border-radius: 50%;
  animation: fb-spin 0.8s linear infinite;
}
.spinner { width: 32px; height: 32px; margin: 0 auto; }
.spinner-small { width: 20px; height: 20px; border-width: 2px; }
@keyframes fb-spin { to { transform: rotate(360deg); } }
</style>
