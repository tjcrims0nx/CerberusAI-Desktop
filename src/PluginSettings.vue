<template>
  <div class="plugin-settings">
    <div class="plugin-topbar">
      <div>
        <p class="plugin-eyebrow">MCP control</p>
        <h2>Plugin Manager</h2>
      </div>
      <div class="plugin-tabs" role="tablist" aria-label="Plugin manager sections">
        <button :class="{ active: activeTab === 'local' }" @click="activeTab = 'local'">Local Plugins</button>
        <button :class="{ active: activeTab === 'awesome' }" @click="openAwesomeTab">Awesome-Skills</button>
      </div>
    </div>

    <section v-if="activeTab === 'local'" class="plugin-section">
      <div v-if="plugins.length === 0" class="empty-panel">
        <strong>No local plugins configured.</strong>
        <span>Add a command plugin or load an existing <code>.mcp.json</code> config.</span>
      </div>

      <div v-else class="plugin-list">
        <div
          v-for="plugin in plugins"
          :key="plugin.id"
          :class="['plugin-card', { 'awesome-plugin-card': isAwesomePlugin(plugin) }]"
        >
          <div class="plugin-info">
            <div class="plugin-title-row">
              <h3>{{ plugin.name }}</h3>
              <span v-if="isAwesomePlugin(plugin)" class="source-pill">Awesome-Skills</span>
              <span v-if="plugin.verified" class="verify-pill ok">Verified</span>
              <span v-else-if="plugin.verifyError" class="verify-pill error">Verify failed</span>
              <span :class="['status', isPluginActive(plugin.id) ? 'active' : 'inactive']">
                {{ isPluginActive(plugin.id) ? 'Connected' : 'Offline' }}
              </span>
            </div>
            <p class="command">
              <code v-if="plugin.url">{{ plugin.url }}</code>
              <code v-else-if="isAwesomePlugin(plugin)">{{ plugin.cwd || plugin.command }}</code>
              <code v-else>{{ plugin.command }} {{ plugin.args ? plugin.args.join(' ') : '' }}</code>
            </p>
            <p v-if="plugin.verifyError" class="plugin-verify-error">{{ plugin.verifyError }}</p>
          </div>

          <div class="plugin-actions">
            <button
              @click="togglePlugin(plugin)"
              :class="['power-btn', plugin.enabled ? 'active' : 'inactive']"
              :title="plugin.enabled ? 'Deactivate plugin' : 'Activate plugin'"
            >
              <span class="power-dot"></span>
              {{ plugin.enabled ? 'Deactivate' : 'Activate' }}
            </button>
            <button @click="removePlugin(plugin.id)" class="remove-btn" title="Remove plugin">Remove</button>
          </div>
        </div>
      </div>

      <div class="add-plugin">
        <div class="add-heading">
          <h3>Add New Plugin</h3>
          <span>Command-based MCP servers run locally through Tauri.</span>
        </div>
        <div class="import-section">
          <input type="text" v-model="mcpConfigPath" placeholder="Path to .mcp.json" />
          <button @click="loadFromConfig" class="btn-primary">Load Config</button>
        </div>
        <form @submit.prevent="addPlugin" class="manual-form">
          <div class="form-group">
            <label>Plugin Name</label>
            <input v-model="newPlugin.name" placeholder="Local File System" required />
          </div>
          <div class="form-group">
            <label>Command</label>
            <input v-model="newPlugin.command" placeholder="npx" required />
          </div>
          <div class="form-group wide">
            <label>Arguments</label>
            <input v-model="newPluginArgs" placeholder="-y, @modelcontextprotocol/server-filesystem, C:/path" />
          </div>
          <button type="submit" class="btn-primary add-btn">Add Plugin</button>
        </form>
      </div>
    </section>

    <section v-else class="plugin-section">
      <div class="directory-toolbar">
        <div>
          <p class="plugin-eyebrow">Remote directory</p>
          <h3>Awesome-Skills Directory</h3>
          <span>Search awesome-skills.com, pull GitHub repos, and auto-convert plain SKILL.md repos for Cerberus.</span>
        </div>
        <div class="directory-actions">
          <div class="search-box">
            <input
              v-model="awesomeQuery"
              type="search"
              placeholder="Search plugins"
              autocomplete="off"
              @focus="showSuggestions = true"
              @keydown.enter.prevent="loadAwesomeSkills"
            />
            <div v-if="showSuggestions && relatedSkills.length > 0" class="suggestion-menu">
              <button
                v-for="skill in relatedSkills"
                :key="skill.url"
                type="button"
                @mousedown.prevent="chooseRelatedSkill(skill)"
              >
                <span>{{ skill.name }}</span>
                <small>{{ skill.url.replace('https://github.com/', '') }}</small>
              </button>
            </div>
          </div>
          <button class="btn-primary" @click="loadAwesomeSkills" :disabled="awesomeLoading">
            {{ awesomeLoading ? 'Searching' : 'Search' }}
          </button>
        </div>
      </div>

      <div v-if="awesomeError" class="notice error">{{ awesomeError }}</div>
      <div v-else-if="awesomeLoading" class="notice">Loading available MCP skills...</div>
      <div v-else-if="awesomeSkills.length === 0" class="empty-panel">
        <strong>No skills returned.</strong>
        <span>Try a different search or refresh the full directory.</span>
      </div>

      <div v-else class="awesome-grid">
        <article
          v-for="skill in awesomeSkills"
          :key="skill.url || skill.name"
          :class="['skill-card', { installed: !!installedAwesomePlugin(skill) }]"
        >
          <div class="skill-main">
            <div class="skill-title-row">
              <h3>{{ skill.name }}</h3>
              <span v-if="installedAwesomePlugin(skill)?.verified" class="installed-pill">Verified</span>
              <span v-else-if="installedAwesomePlugin(skill)" class="installed-pill">Installed</span>
            </div>
            <p>{{ skill.description || 'MCP skill from awesome-skills.com' }}</p>
          </div>
          <code :title="skill.url">{{ skill.url.replace('https://github.com/', '') }}</code>
          <p v-if="installMessages[skill.url]" :class="['install-message', installMessages[skill.url].kind]">
            {{ installMessages[skill.url].text }}
          </p>
          <button
            :class="['install-btn', installedAwesomePlugin(skill) ? 'installed-action' : 'btn-primary']"
            @click="handleAwesomeSkillAction(skill)"
            :disabled="installingUrl === skill.url"
          >
            {{ awesomeSkillButtonLabel(skill) }}
          </button>
        </article>
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, onMounted, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { PluginManager, type PluginConfig } from './PluginManager';

type AwesomeSkill = {
  name: string;
  description: string;
  url: string;
};

const props = defineProps<{
  apiKey: string;
}>();

const emit = defineEmits<{
  pluginsChanged: [plugins: PluginConfig[]];
}>();

const pluginManager = new PluginManager();
pluginManager.setApiKey(props.apiKey);

const plugins = ref<PluginConfig[]>([]);
const activePlugins = ref<string[]>([]);
const activeTab = ref<'local' | 'awesome'>('local');
const awesomeSkills = ref<AwesomeSkill[]>([]);
const awesomeLoading = ref(false);
const awesomeError = ref('');
const awesomeQuery = ref('');
const showSuggestions = ref(false);
const installingUrl = ref('');
const installMessages = ref<Record<string, { kind: 'success' | 'error', text: string }>>({});

const newPlugin = ref({
  name: '',
  command: ''
});
const newPluginArgs = ref('');
const mcpConfigPath = ref('C:/Users/tjcri/claude-code/.mcp.json');

watch(() => props.apiKey, (key) => {
  pluginManager.setApiKey(key);
});

const relatedSkills = computed(() => {
  const query = awesomeQuery.value.trim().toLowerCase();
  if (!query || awesomeSkills.value.length === 0) return [];
  return awesomeSkills.value
    .map((skill) => ({ skill, score: relatedScore(skill, query) }))
    .filter((item) => item.score < 999)
    .sort((a, b) => a.score - b.score || a.skill.name.localeCompare(b.skill.name))
    .slice(0, 6)
    .map((item) => item.skill);
});

const awesomePluginId = (skill: AwesomeSkill) => {
  const slug = skill.name
    .toLowerCase()
    .replace(/[^a-z0-9-]+/g, '_')
    .replace(/^_+|_+$/g, '') || 'plugin';
  return `awesome_${slug}`;
};

onMounted(async () => {
  await loadSavedPlugins();
});

async function loadSavedPlugins() {
  try {
    const savedPlugins = await invoke<string | null>("db_get_kv", { key: 'mcp-plugins' });
    if (savedPlugins) {
      plugins.value = JSON.parse(savedPlugins);
    } else {
      plugins.value = await pluginManager.discoverPlugins();
      savePlugins();
    }
    updateActivePlugins();
  } catch (e) {
    console.warn("Failed to load MCP plugins", e);
  }
}

async function openAwesomeTab() {
  activeTab.value = 'awesome';
  if (awesomeSkills.value.length === 0 && !awesomeLoading.value) {
    await loadAwesomeSkills();
  }
}

async function loadAwesomeSkills() {
  awesomeLoading.value = true;
  awesomeError.value = '';
  showSuggestions.value = false;
  try {
    awesomeSkills.value = await invoke<AwesomeSkill[]>('search_awesome_skills', {
      query: awesomeQuery.value.trim() || null
    });
  } catch (e: any) {
    awesomeError.value = e?.message || 'Failed to load Awesome-Skills directory.';
  } finally {
    awesomeLoading.value = false;
  }
}

function normalizeText(value: string) {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, ' ').trim();
}

function editDistance(a: string, b: string) {
  const costs = Array.from({ length: b.length + 1 }, (_, index) => index);
  for (let i = 0; i < a.length; i += 1) {
    let last = i;
    costs[0] = i + 1;
    for (let j = 0; j < b.length; j += 1) {
      const old = costs[j + 1];
      costs[j + 1] = a[i] === b[j] ? last : Math.min(last, costs[j], costs[j + 1]) + 1;
      last = old;
    }
  }
  return costs[b.length] ?? 999;
}

function relatedScore(skill: AwesomeSkill, query: string) {
  const normalizedQuery = normalizeText(query);
  const haystack = normalizeText(`${skill.name} ${skill.description} ${skill.url}`);
  if (haystack.includes(normalizedQuery)) return 0;

  const tokens = haystack.split(/\s+/).filter(Boolean);
  const queryTokens = normalizedQuery.split(/\s+/).filter(Boolean);
  let score = 0;
  for (const queryToken of queryTokens) {
    const best = Math.min(
      ...tokens.map((token) => token.startsWith(queryToken) || queryToken.startsWith(token)
        ? 1
        : editDistance(queryToken, token))
    );
    if (best > Math.max(1, Math.floor(queryToken.length / 3))) return 999;
    score += best;
  }
  return score + 2;
}

function chooseRelatedSkill(skill: AwesomeSkill) {
  awesomeQuery.value = skill.name;
  awesomeSkills.value = [skill, ...awesomeSkills.value.filter((item) => item.url !== skill.url)];
  showSuggestions.value = false;
}

function installedAwesomePlugin(skill: AwesomeSkill) {
  const id = awesomePluginId(skill);
  return plugins.value.find((plugin) => plugin.id === id);
}

function awesomeSkillButtonLabel(skill: AwesomeSkill) {
  if (installingUrl.value === skill.url) return 'Converting';
  const installed = installedAwesomePlugin(skill);
  if (!installed) return 'Install';
  return isPluginActive(installed.id) ? 'Installed' : 'Activate';
}

async function handleAwesomeSkillAction(skill: AwesomeSkill) {
  const installed = installedAwesomePlugin(skill);
  if (installed) {
    if (!isPluginActive(installed.id)) {
      await activateInstalledAwesomeSkill(installed, skill.url);
    }
    return;
  }
  await installAwesomeSkill(skill);
}

async function activateInstalledAwesomeSkill(plugin: PluginConfig, sourceUrl?: string) {
  plugin.enabled = true;
  savePlugins();
  updateActivePlugins();
  if (sourceUrl) {
    installMessages.value[sourceUrl] = plugin.verified
      ? { kind: 'success', text: 'Activated and ready.' }
      : { kind: 'error', text: plugin.verifyError || `Installed, but verification failed for ${plugin.name}.` };
  }
}

async function installAwesomeSkill(skill: AwesomeSkill) {
  installingUrl.value = skill.url;
  awesomeError.value = '';
  delete installMessages.value[skill.url];
  try {
    const installed = await invoke<PluginConfig>('install_awesome_skill', {
      name: skill.name,
      url: skill.url
    });
    const existingIndex = plugins.value.findIndex((plugin) => plugin.id === installed.id);
    if (existingIndex >= 0) {
      plugins.value[existingIndex] = installed;
    } else {
      plugins.value.push(installed);
    }
    savePlugins();
    installMessages.value[skill.url] = installed.verified
      ? { kind: 'success', text: 'Downloaded, converted, and verified.' }
      : { kind: 'error', text: installed.verifyError || 'Downloaded, but verification did not pass.' };
    await activateInstalledAwesomeSkill(installed, skill.url);
  } catch (e: any) {
    installMessages.value[skill.url] = {
      kind: 'error',
      text: typeof e === 'string' ? e : (e?.message || `Failed to install ${skill.name}.`)
    };
  } finally {
    installingUrl.value = '';
  }
}

const loadFromConfig = async () => {
  if (!mcpConfigPath.value) return;
  const discovered = await pluginManager.loadFromConfigFile(mcpConfigPath.value);
  for (const config of discovered) {
    if (!plugins.value.find(p => p.id === config.id)) {
      plugins.value.push(config);
    }
  }
  savePlugins();
};

const savePlugins = () => {
  invoke("db_set_kv", { key: 'mcp-plugins', value: JSON.stringify(plugins.value) }).catch(console.error);
  emit('pluginsChanged', plugins.value);
};

const updateActivePlugins = () => {
  activePlugins.value = pluginManager.activePlugins;
};

const isPluginActive = (id: string) => {
  const plugin = plugins.value.find((item) => item.id === id);
  return !!plugin?.enabled || activePlugins.value.includes(id);
};

const isAwesomePlugin = (plugin: PluginConfig) => {
  return plugin.id.startsWith('awesome_') || !!plugin.cwd?.includes('.CerberusAI');
};

const togglePlugin = async (plugin: PluginConfig) => {
  plugin.enabled = !plugin.enabled;
  savePlugins();
  updateActivePlugins();
};

const addPlugin = async () => {
  const id = `plugin_${Date.now()}`;
  const args = newPluginArgs.value.split(',').map(s => s.trim()).filter(s => s.length > 0);

  const config: PluginConfig = {
    id,
    name: newPlugin.value.name,
    command: newPlugin.value.command,
    args,
    enabled: true
  };

  plugins.value.push(config);
  savePlugins();
  updateActivePlugins();

  newPlugin.value = { name: '', command: '' };
  newPluginArgs.value = '';
};

const removePlugin = async (id: string) => {
  plugins.value = plugins.value.filter(p => p.id !== id);
  savePlugins();
  updateActivePlugins();
};
</script>

<style scoped>
.plugin-settings {
  min-height: 100%;
  padding: 22px;
  color: var(--text-primary);
  background: transparent;
}

.plugin-topbar,
.directory-toolbar,
.add-plugin,
.plugin-card,
.skill-card,
.empty-panel,
.notice {
  border: 1px solid rgba(255, 255, 255, 0.08);
  background: rgba(255, 255, 255, 0.03);
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3);
  backdrop-filter: blur(24px) saturate(1.4);
  border-radius: 12px;
}

.plugin-topbar {
  position: sticky;
  top: 0;
  z-index: 2;
  display: flex;
  justify-content: space-between;
  gap: 18px;
  align-items: center;
  padding: 14px 16px;
}

.plugin-eyebrow {
  margin: 0 0 4px;
  color: #c084fc;
  font-size: 0.72rem;
  font-weight: 700;
  letter-spacing: 0.1em;
  text-transform: uppercase;
}

h2,
h3 {
  margin: 0;
  letter-spacing: 0.02em;
  font-weight: 700;
}

.plugin-tabs {
  display: grid;
  grid-template-columns: repeat(2, minmax(120px, 1fr));
  gap: 6px;
  padding: 4px;
  border-radius: 12px;
  background: rgba(0, 0, 0, 0.3);
  border: 1px solid rgba(255, 255, 255, 0.06);
}

.plugin-tabs button,
button {
  min-height: 36px;
  padding: 8px 14px;
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: 6px;
  cursor: pointer;
  color: var(--text-primary);
  background: rgba(255, 255, 255, 0.04);
  font-weight: 600;
  font-size: 0.84rem;
  letter-spacing: 0;
  transition: background var(--duration-fast) ease, border-color var(--duration-fast) ease, transform 60ms ease;
}

button:hover:not(:disabled) {
  background: rgba(255, 255, 255, 0.08);
  border-color: rgba(255, 255, 255, 0.12);
  transform: none;
  box-shadow: none;
}

button:active:not(:disabled) {
  transform: scale(0.97);
  background: rgba(255, 255, 255, 0.1);
}

button:disabled {
  cursor: wait;
  opacity: 0.55;
}

.plugin-tabs button.active {
  border-color: rgba(168, 85, 247, 0.35);
  background: rgba(168, 85, 247, 0.22);
  color: #fff;
  box-shadow: none;
}

.plugin-section {
  margin-top: 18px;
}

.plugin-list,
.awesome-grid {
  display: grid;
  gap: 14px;
}

.plugin-card {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 18px;
  padding: 14px;
  border-radius: 12px;
  transition: transform 0.18s ease, border-color 0.18s ease, box-shadow 0.18s ease;
}

.awesome-plugin-card {
  border-color: rgba(52, 211, 153, 0.18);
  background:
    linear-gradient(180deg, rgba(20, 184, 166, 0.08), rgba(12, 12, 18, 0.72)),
    linear-gradient(180deg, rgba(28, 28, 38, 0.78), rgba(12, 12, 18, 0.72));
}

.plugin-card:hover,
.skill-card:hover {
  transform: translateY(-2px);
  border-color: rgba(167, 139, 250, 0.42);
  box-shadow: 0 24px 62px rgba(0, 0, 0, 0.42), 0 0 0 1px rgba(248, 113, 113, 0.14);
}

.plugin-title-row {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.command {
  max-width: min(62vw, 560px);
  margin: 7px 0 0;
  color: var(--text-muted);
  font-size: 0.78rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.status {
  font-size: 0.68rem;
  padding: 4px 8px;
  border-radius: 8px;
  font-weight: 800;
  text-transform: uppercase;
}

.source-pill {
  color: #99f6e4;
  background: rgba(20, 184, 166, 0.12);
  border: 1px solid rgba(45, 212, 191, 0.24);
  border-radius: 8px;
  padding: 4px 8px;
  font-size: 0.68rem;
  font-weight: 900;
}

.verify-pill {
  border-radius: 8px;
  padding: 4px 8px;
  font-size: 0.68rem;
  font-weight: 900;
  text-transform: uppercase;
}

.verify-pill.ok {
  color: #bbf7d0;
  background: rgba(34, 197, 94, 0.12);
  border: 1px solid rgba(74, 222, 128, 0.24);
}

.verify-pill.error {
  color: #fecaca;
  background: rgba(220, 38, 38, 0.12);
  border: 1px solid rgba(248, 113, 113, 0.28);
}

.plugin-verify-error {
  margin: 7px 0 0;
  color: #fecaca;
  font-size: 0.74rem;
  line-height: 1.35;
}

.status.active {
  color: #6ee7b7;
  background: rgba(16, 185, 129, 0.14);
  border: 1px solid rgba(16, 185, 129, 0.32);
}

.status.inactive {
  color: #fca5a5;
  background: rgba(248, 113, 113, 0.1);
  border: 1px solid rgba(248, 113, 113, 0.24);
}

.plugin-actions {
  display: flex;
  gap: 8px;
  align-items: center;
  flex-shrink: 0;
}

.power-btn,
.remove-btn {
  min-height: 34px;
  padding: 7px 11px;
  border-radius: 8px;
  font-size: 0.8rem;
}

.power-btn {
  display: inline-flex;
  align-items: center;
  gap: 7px;
}

.power-btn.active {
  color: #d1fae5;
  background: rgba(5, 150, 105, 0.18);
  border-color: rgba(45, 212, 191, 0.34);
}

.power-btn.inactive {
  color: #fecaca;
  background: rgba(220, 38, 38, 0.12);
  border-color: rgba(248, 113, 113, 0.28);
}

.power-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: currentColor;
  box-shadow: 0 0 12px currentColor;
}

.remove-btn {
  color: var(--text-secondary);
  background: rgba(255, 255, 255, 0.04);
}

.add-plugin,
.directory-toolbar,
.empty-panel,
.notice {
  margin-top: 16px;
  padding: 18px;
  border-radius: 12px;
}

.directory-toolbar {
  display: flex;
  justify-content: space-between;
  gap: 18px;
  align-items: center;
  margin-top: 0;
}

.directory-actions {
  display: grid;
  grid-template-columns: minmax(180px, 260px) auto;
  gap: 10px;
  align-items: center;
}

.search-box {
  position: relative;
}

.suggestion-menu {
  position: absolute;
  z-index: 4;
  top: calc(100% + 6px);
  left: 0;
  right: 0;
  display: grid;
  gap: 4px;
  padding: 6px;
  border: 1px solid rgba(255, 255, 255, 0.12);
  border-radius: 10px;
  background: rgba(10, 10, 16, 0.96);
  box-shadow: 0 20px 44px rgba(0, 0, 0, 0.45);
}

.suggestion-menu button {
  min-height: 0;
  display: grid;
  gap: 2px;
  justify-items: start;
  padding: 8px 9px;
  border-radius: 8px;
  text-align: left;
}

.suggestion-menu small {
  max-width: 100%;
  color: var(--text-muted);
  font-size: 0.72rem;
  overflow: hidden;
  text-overflow: ellipsis;
}

.directory-toolbar span,
.add-heading span,
.empty-panel span {
  display: block;
  margin-top: 6px;
  color: var(--text-muted);
}

.import-section,
.manual-form {
  display: grid;
  gap: 12px;
}

.import-section {
  grid-template-columns: 1fr auto;
  margin: 16px 0;
}

.manual-form {
  grid-template-columns: repeat(2, minmax(0, 1fr)) auto;
  align-items: end;
}

.form-group.wide {
  grid-column: span 2;
}

.form-group label {
  display: block;
  margin-bottom: 7px;
  color: var(--text-secondary);
  font-size: 0.78rem;
  font-weight: 800;
}

input {
  width: 100%;
  min-height: 40px;
  padding: 10px 12px;
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 12px;
  background: rgba(0, 0, 0, 0.34);
  color: white;
  font-size: 0.9rem;
  transition: border-color 0.18s ease, box-shadow 0.18s ease, background 0.18s ease;
}

input:focus {
  outline: none;
  border-color: rgba(248, 113, 113, 0.55);
  box-shadow: 0 0 0 3px rgba(248, 113, 113, 0.12), 0 0 22px rgba(124, 58, 237, 0.18);
  background: rgba(0, 0, 0, 0.48);
}

.awesome-grid {
  grid-template-columns: repeat(auto-fill, minmax(210px, 1fr));
  gap: 10px;
  margin-top: 12px;
}

.skill-card {
  min-height: 154px;
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  gap: 10px;
  padding: 12px;
  border-radius: 12px;
  transition: transform 0.18s ease, border-color 0.18s ease, box-shadow 0.18s ease;
}

.skill-card.installed {
  border-color: rgba(45, 212, 191, 0.26);
  background:
    linear-gradient(180deg, rgba(20, 184, 166, 0.1), rgba(12, 12, 18, 0.72)),
    linear-gradient(180deg, rgba(28, 28, 38, 0.78), rgba(12, 12, 18, 0.72));
}

.skill-main {
  min-height: 74px;
}

.skill-title-row {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 8px;
}

.skill-card h3 {
  font-size: 0.98rem;
  line-height: 1.22;
  overflow-wrap: anywhere;
}

.installed-pill {
  flex-shrink: 0;
  color: #99f6e4;
  background: rgba(20, 184, 166, 0.12);
  border: 1px solid rgba(45, 212, 191, 0.24);
  border-radius: 7px;
  padding: 3px 6px;
  font-size: 0.62rem;
  font-weight: 900;
  text-transform: uppercase;
}

.skill-card p {
  display: -webkit-box;
  margin: 7px 0 0;
  color: var(--text-secondary);
  font-size: 0.78rem;
  line-height: 1.35;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 2;
  overflow: hidden;
}

code {
  color: #c4b5fd;
  font-size: 0.72rem;
  line-height: 1.25;
  overflow-wrap: anywhere;
}

.btn-primary {
  background: linear-gradient(135deg, #dc2626, #7c3aed);
  border-color: rgba(248, 113, 113, 0.42);
}

.btn-success {
  background: linear-gradient(135deg, #059669, #0f766e);
  border-color: rgba(45, 212, 191, 0.38);
}

.btn-danger,
.btn-secondary {
  background: rgba(255, 255, 255, 0.05);
}

.btn-danger:hover {
  color: #fecaca;
  background: rgba(220, 38, 38, 0.18);
}

.install-btn,
.add-btn {
  width: 100%;
}

.install-btn {
  min-height: 34px;
  padding: 7px 10px;
  border-radius: 8px;
  font-size: 0.82rem;
}

.installed-action {
  color: #d1fae5;
  background: rgba(5, 150, 105, 0.18);
  border-color: rgba(45, 212, 191, 0.34);
}

.installed-action:disabled {
  cursor: default;
  opacity: 0.72;
}

.install-message {
  margin: -2px 0 0;
  font-size: 0.72rem;
  line-height: 1.25;
}

.install-message.success {
  color: #99f6e4;
}

.install-message.error {
  color: #fecaca;
}

.notice.error {
  color: #fecaca;
  border-color: rgba(248, 113, 113, 0.38);
}

@media (max-width: 760px) {
  .plugin-topbar,
  .directory-toolbar,
  .plugin-card {
    align-items: stretch;
    flex-direction: column;
  }

  .manual-form,
  .import-section,
  .directory-actions {
    grid-template-columns: 1fr;
  }

  .form-group.wide {
    grid-column: auto;
  }

  .plugin-actions {
    width: 100%;
  }

  .plugin-actions button {
    flex: 1;
  }
}
</style>
