<template>
  <div class="gate-overlay model-manager-overlay">
    <div class="manager-panel">
      <!-- Header -->
      <div class="manager-header">
        <div class="manager-title-row">
          <div class="manager-glyph">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="#fff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 22h14a2 2 0 0 0 2-2V7.5L14.5 2H6a2 2 0 0 0-2 2v4"></path><polyline points="14 2 14 8 20 8"></polyline><path d="M2 15h10"></path><path d="M9 18l3-3-3-3"></path></svg>
          </div>
          <div>
            <h2 class="manager-title">MODEL MANAGER</h2>
            <p class="manager-subtitle">Manage downloaded models & remote pulls</p>
          </div>
          <button class="manager-close" @click="$emit('close')">✕</button>
        </div>

        <!-- Search bar -->
        <div class="manager-search-row">
          <input
            v-model="localManagerSearch"
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
            :class="{ active: localManagerTab === 'ollama' }"
            @click="localManagerTab = 'ollama'"
          >
            OLLAMA MODELS
            <span class="manager-tab-count">{{ models.length }}</span>
          </button>
          <button
            class="manager-tab"
            :class="{ active: localManagerTab === 'cloud' }"
            @click="localManagerTab = 'cloud'"
          >
            CLOUD CATALOG
            <span class="manager-tab-count">{{ allowedModels.length }}</span>
          </button>
          <button
            class="manager-tab"
            :class="{ active: localManagerTab === 'files' }"
            @click="localManagerTab = 'files'"
          >
            RAW FILES
            <span class="manager-tab-count">{{ localGgufs.length }}</span>
          </button>
        </div>
      </div>

      <!-- Ollama Models Tab -->
      <div v-if="localManagerTab === 'ollama'" class="manager-body">
        <div class="manager-disk-bar">
          <span class="manager-disk-label">TOTAL DISK USAGE</span>
          <span class="manager-disk-value">{{ formatBytes(totalOllamaSize) }}</span>
        </div>

        <div v-if="filteredOllamaModels.length === 0" class="manager-empty">
          <template v-if="localManagerSearch">No models matching "{{ localManagerSearch }}"</template>
          <template v-else>No models installed in Ollama yet.<br/>Pull a model from the main screen or import a .gguf file.</template>
        </div>

        <div v-else class="manager-list">
          <div v-for="m in filteredOllamaModels" :key="m.name" class="model-card">
            <div class="model-card-main">
              <div class="model-card-icon">{{ m.name.charAt(0).toUpperCase() }}</div>
              <div class="model-card-info">
                <div class="model-card-name" :title="m.name">{{ modelKey(m.name) }}</div>
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
                v-if="modelKey(selectedModel) !== modelKey(m.name)"
                @click="$emit('update:selectedModel', modelKey(m.name)); $emit('close')"
                title="Use this model"
              >USE</button>
              <span v-else class="model-active-badge">ACTIVE</span>
              <button
                class="model-action-btn danger"
                @click="$emit('deleteOllamaModel', m.name)"
                :disabled="isDeletingModel"
                title="Remove from Ollama (keeps your raw .gguf files safe)"
              >UNREGISTER</button>
            </div>
          </div>
        </div>
      </div>

      <!-- Raw Files Tab -->
      <div v-if="localManagerTab === 'files'" class="manager-body">
        <div class="manager-disk-bar">
          <span class="manager-disk-label">RAW GGUF FILES</span>
          <span class="manager-disk-value">{{ formatBytes(totalGgufSize) }}</span>
        </div>

        <p class="manager-hint">
          Downloaded <code>.gguf</code> installer files. You can safely delete these after a model has been imported into Ollama.
        </p>

        <div v-if="filteredGgufs.length === 0" class="manager-empty">
          <template v-if="localManagerSearch">No files matching "{{ localManagerSearch }}"</template>
          <template v-else>No raw .gguf files found.</template>
        </div>

        <div v-else class="manager-list">
          <div v-for="f in filteredGgufs" :key="f.name" class="model-card">
            <div class="model-card-main">
              <div class="model-card-icon file-icon">GG</div>
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
                @click="$emit('activateGguf', f.name)"
                :disabled="isImporting || isDeletingGguf"
                title="Register this file in Ollama"
              >ACTIVATE</button>
              <button
                class="model-action-btn"
                @click="$emit('moveGguf', f.name)"
                :disabled="isDeletingGguf"
                title="Move to another location"
              >MOVE</button>
              <button
                class="model-action-btn danger"
                @click="$emit('deleteGguf', f.name)"
                :disabled="isDeletingGguf"
                title="Permanently delete this file from your hard drive"
              >TRASH FILE</button>
            </div>
          </div>
        </div>
      </div>

      <!-- Cloud Models Tab -->
      <div v-if="localManagerTab === 'cloud'" class="manager-body">
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
              <div class="model-card-icon file-icon">CL</div>
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
                <div class="quant-buttons">
                  <button
                    v-for="q in m.quants.split(',').map((s: string) => s.trim()).filter(Boolean)"
                    :key="q"
                    class="model-action-btn use"
                    :class="{
                      'fit-tight': quantFit(m.quantSizes[q]) === 'tight',
                      'fit-too-big': quantFit(m.quantSizes[q]) === 'too-big'
                    }"
                    :title="
                      m.quantSizes[q]
                        ? `${fmtSizeGb(m.quantSizes[q])}` + (
                            quantFit(m.quantSizes[q]) === 'too-big'
                              ? ` — won't fit your ${vramGb || '?'} GB GPU; will run on CPU (slow)`
                              : quantFit(m.quantSizes[q]) === 'tight'
                                ? ` — close to your ${vramGb || '?'} GB GPU limit; may offload partially`
                                : ''
                          )
                        : 'Pull ' + q
                    "
                    @click.stop="$emit('pullModel', m.name, q)"
                  >
                    PULL {{ q }}<span v-if="m.quantSizes[q]" class="quant-size">{{ fmtSizeGb(m.quantSizes[q]) }}</span>
                  </button>
                  <button
                    v-if="!m.quants"
                    class="model-action-btn use"
                    @click.stop="$emit('pullModel', m.name)"
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
        <button class="import-btn" @click="$emit('importGguf')" :disabled="isImporting || !localStatus.running">
          <span v-if="isImporting">IMPORTING…</span>
          <span v-else>⬆ IMPORT GGUF</span>
        </button>
        <button class="close-modal-btn" @click="$emit('close')">DONE</button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue';

const props = defineProps<{
  managerSearch: string;
  managerTab: string;
  models: any[];
  allowedModels: any[];
  localGgufs: any[];
  totalOllamaSize: number;
  filteredOllamaModels: any[];
  selectedModel: string;
  isDeletingModel: boolean;
  totalGgufSize: number;
  filteredGgufs: any[];
  activatedGgufs: Set<string>;
  isImporting: boolean;
  isDeletingGguf: boolean;
  allModelChoices: any[];
  pulling: any;
  vramGb: string | null;
  localStatus: any;
  formatBytes: (bytes: number) => string;
  modelKey: (name: string) => string;
  quantFit: (size: number) => string;
  fmtSizeGb: (bytes: number) => string;
}>();

const emit = defineEmits([
  'close',
  'update:managerSearch',
  'update:managerTab',
  'update:selectedModel',
  'deleteOllamaModel',
  'activateGguf',
  'moveGguf',
  'deleteGguf',
  'pullModel',
  'importGguf'
]);

const localManagerSearch = ref(props.managerSearch);
const localManagerTab = ref(props.managerTab);

watch(localManagerSearch, (val) => emit('update:managerSearch', val));
watch(() => props.managerSearch, (val) => { localManagerSearch.value = val; });

watch(localManagerTab, (val) => emit('update:managerTab', val));
watch(() => props.managerTab, (val) => { localManagerTab.value = val; });

</script>

<style scoped>
.model-manager-overlay {
  padding: 28px;
  background:
    radial-gradient(circle at 16% 10%, rgba(220, 38, 38, 0.2), transparent 34%),
    radial-gradient(circle at 86% 0%, rgba(124, 58, 237, 0.19), transparent 30%),
    radial-gradient(circle at 52% 100%, rgba(20, 184, 166, 0.1), transparent 34%),
    rgba(0, 0, 0, 0.82);
  backdrop-filter: blur(12px);
}

.manager-panel {
  width: min(1120px, 94vw);
  height: min(850px, 88vh);
  overflow: hidden;
  border: 1px solid rgba(255, 255, 255, 0.12);
  border-radius: 22px;
  background:
    linear-gradient(180deg, rgba(26, 26, 36, 0.84), rgba(9, 9, 13, 0.88)),
    radial-gradient(circle at 20% 0%, rgba(220, 38, 38, 0.1), transparent 38%);
  box-shadow: 0 34px 100px rgba(0, 0, 0, 0.58), inset 0 1px 0 rgba(255, 255, 255, 0.1);
  backdrop-filter: blur(22px);
}

.manager-header {
  padding: 20px 22px 16px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  background: linear-gradient(180deg, rgba(255, 255, 255, 0.07), rgba(255, 255, 255, 0.02));
}

.manager-title-row {
  display: flex;
  align-items: center;
  gap: 14px;
}

.manager-glyph {
  width: 42px;
  height: 42px;
  display: grid;
  place-items: center;
  border-radius: 13px;
  background: linear-gradient(135deg, #dc2626, #7c3aed);
  box-shadow: 0 14px 36px rgba(220, 38, 38, 0.28);
}

.manager-title {
  margin: 0;
  letter-spacing: 0;
  font-size: 1.06rem;
}

.manager-subtitle {
  margin: 4px 0 0;
  color: rgba(255, 255, 255, 0.58);
}

.manager-close {
  margin-left: auto;
  width: 38px;
  height: 38px;
  border-radius: 12px;
  border: 1px solid rgba(255, 255, 255, 0.1);
  color: white;
  background: rgba(255, 255, 255, 0.06);
  cursor: pointer;
  transition: transform 0.18s ease, background 0.18s ease, border-color 0.18s ease;
}

.manager-close:hover {
  transform: translateY(-1px);
  border-color: rgba(248, 113, 113, 0.42);
  background: rgba(220, 38, 38, 0.18);
}

.manager-search-row {
  margin-top: 18px;
}

.manager-search {
  width: 100%;
  height: 44px;
  padding: 0 14px;
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 13px;
  color: white;
  background: rgba(0, 0, 0, 0.3);
  transition: border-color 0.18s ease, box-shadow 0.18s ease, background 0.18s ease;
}

.manager-search:focus {
  outline: none;
  border-color: rgba(248, 113, 113, 0.56);
  background: rgba(0, 0, 0, 0.42);
  box-shadow: 0 0 0 4px rgba(248, 113, 113, 0.1), 0 0 28px rgba(124, 58, 237, 0.18);
}

.manager-tabs {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 8px;
  margin-top: 16px;
  padding: 6px;
  border-radius: 14px;
  border: 1px solid rgba(255, 255, 255, 0.08);
  background: rgba(0, 0, 0, 0.24);
}

.manager-tab {
  min-height: 42px;
  border: 1px solid transparent;
  border-radius: 11px;
  color: rgba(255, 255, 255, 0.7);
  cursor: pointer;
  background: transparent;
  font-weight: 800;
  letter-spacing: 0;
  transition: transform 0.18s ease, color 0.18s ease, border-color 0.18s ease, background 0.18s ease;
}

.manager-tab:hover {
  color: white;
  transform: translateY(-1px);
  border-color: rgba(255, 255, 255, 0.12);
  background: rgba(255, 255, 255, 0.05);
}

.manager-tab.active {
  color: white;
  border-color: rgba(248, 113, 113, 0.44);
  background: linear-gradient(135deg, rgba(220, 38, 38, 0.62), rgba(124, 58, 237, 0.5));
  box-shadow: 0 14px 32px rgba(124, 58, 237, 0.18);
}

.manager-tab-count {
  display: inline-grid;
  min-width: 24px;
  height: 22px;
  place-items: center;
  margin-left: 8px;
  padding: 0 7px;
  border-radius: 999px;
  background: rgba(0, 0, 0, 0.25);
}

.manager-body {
  height: calc(100% - 190px);
  overflow-y: auto;
  padding: 18px 22px;
}

.manager-disk-bar,
.manager-hint,
.manager-empty,
.model-card {
  border: 1px solid rgba(255, 255, 255, 0.1);
  background: linear-gradient(180deg, rgba(255, 255, 255, 0.07), rgba(255, 255, 255, 0.025));
  box-shadow: 0 18px 48px rgba(0, 0, 0, 0.28), inset 0 1px 0 rgba(255, 255, 255, 0.06);
  backdrop-filter: blur(16px);
}

.manager-disk-bar {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  align-items: center;
  padding: 13px 15px;
  border-radius: 14px;
}

.manager-disk-label {
  color: rgba(255, 255, 255, 0.58);
  font-size: 0.74rem;
  font-weight: 900;
  letter-spacing: 0.12em;
}

.manager-disk-value {
  color: #fda4af;
  font-weight: 900;
}

.manager-hint {
  margin: 14px 0;
  padding: 13px 15px;
  border-radius: 14px;
  color: rgba(255, 255, 255, 0.68);
}

.manager-empty {
  margin-top: 16px;
  padding: 34px 18px;
  border-radius: 16px;
  color: rgba(255, 255, 255, 0.62);
  text-align: center;
}

.manager-list {
  display: grid;
  gap: 12px;
  margin-top: 14px;
}

.model-card {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 15px;
  border-radius: 16px;
  transition: transform 0.18s ease, border-color 0.18s ease, box-shadow 0.18s ease;
}

.model-card:hover {
  transform: translateY(-2px);
  border-color: rgba(167, 139, 250, 0.4);
  box-shadow: 0 24px 62px rgba(0, 0, 0, 0.38), 0 0 0 1px rgba(248, 113, 113, 0.12);
}

.model-card-main {
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 13px;
}

.model-card-icon {
  width: 42px;
  height: 42px;
  display: grid;
  place-items: center;
  flex: 0 0 auto;
  border-radius: 13px;
  color: white;
  background: linear-gradient(135deg, rgba(220, 38, 38, 0.86), rgba(124, 58, 237, 0.74));
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.16);
  font-weight: 900;
}

.file-icon {
  background: linear-gradient(135deg, rgba(20, 184, 166, 0.72), rgba(124, 58, 237, 0.64));
}

.model-card-info {
  min-width: 0;
}

.model-card-name {
  overflow: hidden;
  color: white;
  font-weight: 900;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.model-card-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 7px;
}

.model-tag {
  max-width: min(58vw, 520px);
  overflow: hidden;
  padding: 4px 8px;
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: 999px;
  color: rgba(255, 255, 255, 0.7);
  background: rgba(255, 255, 255, 0.05);
  font-size: 0.74rem;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.model-card-actions {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
  flex-wrap: wrap;
}

.model-action-btn,
.import-btn,
.close-modal-btn,
.model-active-badge {
  min-height: 36px;
  padding: 8px 12px;
  border: 1px solid rgba(255, 255, 255, 0.11);
  border-radius: 11px;
  color: white;
  background: rgba(255, 255, 255, 0.05);
  font-weight: 900;
  letter-spacing: 0;
  cursor: pointer;
  transition: transform 0.18s ease, border-color 0.18s ease, box-shadow 0.18s ease, background 0.18s ease;
}

.model-action-btn:hover:not(:disabled),
.import-btn:hover:not(:disabled),
.close-modal-btn:hover {
  transform: translateY(-1px);
  border-color: rgba(248, 113, 113, 0.42);
  box-shadow: 0 12px 30px rgba(0, 0, 0, 0.32);
}

.model-action-btn:disabled,
.import-btn:disabled {
  cursor: not-allowed;
  opacity: 0.52;
}

.model-action-btn.use,
.import-btn {
  border-color: rgba(248, 113, 113, 0.42);
  background: linear-gradient(135deg, rgba(220, 38, 38, 0.82), rgba(124, 58, 237, 0.72));
}

.model-action-btn.success,
.model-active-badge {
  color: #bbf7d0;
  border-color: rgba(34, 197, 94, 0.38);
  background: rgba(34, 197, 94, 0.14);
}

.model-action-btn.danger {
  color: #fecaca;
  border-color: rgba(248, 113, 113, 0.24);
}

.model-action-btn.danger:hover:not(:disabled) {
  background: rgba(220, 38, 38, 0.18);
}

.quant-buttons {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  flex-wrap: wrap;
}

.quant-size {
  display: block;
  margin-top: 3px;
  font-size: 0.68rem;
  color: rgba(255, 255, 255, 0.72);
}

.model-action-btn.fit-tight {
  border-color: rgba(245, 158, 11, 0.54);
  background: linear-gradient(135deg, rgba(245, 158, 11, 0.78), rgba(124, 58, 237, 0.52));
}

.model-action-btn.fit-too-big {
  border-color: rgba(248, 113, 113, 0.62);
  background: linear-gradient(135deg, rgba(220, 38, 38, 0.76), rgba(76, 29, 149, 0.62));
}

.model-action-btn.use:not(.fit-tight):not(.fit-too-big) {
  border-color: rgba(34, 197, 94, 0.4);
}

.manager-footer {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  padding: 16px 22px 20px;
  border-top: 1px solid rgba(255, 255, 255, 0.08);
  background: rgba(0, 0, 0, 0.18);
}

@media (max-width: 760px) {
  .model-manager-overlay {
    padding: 12px;
  }

  .manager-panel {
    width: 100%;
    height: 92vh;
    border-radius: 18px;
  }

  .manager-tabs {
    grid-template-columns: 1fr;
  }

  .manager-body {
    height: calc(100% - 280px);
    padding: 14px;
  }

  .model-card {
    align-items: stretch;
    flex-direction: column;
  }

  .model-card-actions,
  .quant-buttons {
    justify-content: stretch;
  }

  .model-action-btn {
    flex: 1;
  }
}
</style>
