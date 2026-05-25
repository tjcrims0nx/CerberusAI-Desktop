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
        <div v-for="plugin in plugins" :key="plugin.id" class="plugin-card">
          <div class="plugin-info">
            <div class="plugin-title-row">
              <h3>{{ plugin.name }}</h3>
              <span :class="['status', isPluginActive(plugin.id) ? 'active' : 'inactive']">
                {{ isPluginActive(plugin.id) ? 'Connected' : 'Offline' }}
              </span>
            </div>
            <p class="command">
              <code v-if="plugin.url">{{ plugin.url }}</code>
              <code v-else>{{ plugin.command }} {{ plugin.args ? plugin.args.join(' ') : '' }}</code>
            </p>
          </div>

          <div class="plugin-actions">
            <button @click="togglePlugin(plugin)" :class="plugin.enabled ? 'btn-danger' : 'btn-success'">
              {{ plugin.enabled ? 'Disable' : 'Enable' }}
            </button>
            <button @click="removePlugin(plugin.id)" class="btn-secondary">Remove</button>
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
          <span>Fetched through Cerberus Cloud Skills. Installs auto-convert compatible MCP servers and plain SKILL.md repos for Cerberus.</span>
        </div>
        <button class="btn-primary" @click="loadAwesomeSkills" :disabled="awesomeLoading">
          {{ awesomeLoading ? 'Refreshing' : 'Refresh' }}
        </button>
      </div>

      <div v-if="awesomeError" class="notice error">{{ awesomeError }}</div>
      <div v-else-if="awesomeLoading" class="notice">Loading available MCP skills...</div>
      <div v-else-if="awesomeSkills.length === 0" class="empty-panel">
        <strong>No skills returned.</strong>
        <span>Check that your cloud skills MCP server is reachable and exposes <code>list_awesome_skills</code>.</span>
      </div>

      <div v-else class="awesome-grid">
        <article v-for="skill in awesomeSkills" :key="skill.url || skill.name" class="skill-card">
          <div>
            <h3>{{ skill.name }}</h3>
            <p>{{ skill.description || 'MCP skill from awesome-skills.com' }}</p>
          </div>
          <code>{{ skill.url }}</code>
          <button
            class="btn-primary install-btn"
            @click="installAwesomeSkill(skill)"
            :disabled="installingUrl === skill.url"
          >
            {{ installingUrl === skill.url ? 'Converting' : 'Install' }}
          </button>
        </article>
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, watch } from 'vue';
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

const pluginManager = new PluginManager();
pluginManager.setApiKey(props.apiKey);

const plugins = ref<PluginConfig[]>([]);
const activePlugins = ref<string[]>([]);
const activeTab = ref<'local' | 'awesome'>('local');
const awesomeSkills = ref<AwesomeSkill[]>([]);
const awesomeLoading = ref(false);
const awesomeError = ref('');
const installingUrl = ref('');

const newPlugin = ref({
  name: '',
  command: ''
});
const newPluginArgs = ref('');
const mcpConfigPath = ref('C:/Users/tjcri/claude-code/.mcp.json');

watch(() => props.apiKey, (key) => {
  pluginManager.setApiKey(key);
});

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
    await pluginManager.loadPlugins(plugins.value);
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

async function ensureCloudSkills() {
  let cloud = plugins.value.find((p) => p.id === 'cerberus_cloud_skills');
  if (!cloud) {
    cloud = (await pluginManager.discoverPlugins())[0];
    plugins.value.push(cloud);
    savePlugins();
  }
  if (!pluginManager.activePlugins.includes(cloud.id)) {
    await pluginManager.startPlugin(cloud);
    updateActivePlugins();
  }
  return cloud;
}

function toolText(result: any): string {
  if (!result?.content) return '';
  return result.content
    .map((part: any) => typeof part.text === 'string' ? part.text : '')
    .filter(Boolean)
    .join('\n');
}

function parseAwesomeSkills(text: string): AwesomeSkill[] {
  const matches = [...text.matchAll(/-\s+\*\*(.*?)\*\*:\s*([\s\S]*?)\n\s*URL:\s*(\S+)/g)];
  if (matches.length > 0) {
    return matches.map((match) => ({
      name: match[1].trim(),
      description: match[2].trim(),
      url: match[3].trim()
    }));
  }

  return text
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => /^https:\/\/github\.com\//i.test(line))
    .map((url) => ({
      name: url.split('/').filter(Boolean).slice(-1)[0] || url,
      description: '',
      url
    }));
}

async function loadAwesomeSkills() {
  awesomeLoading.value = true;
  awesomeError.value = '';
  try {
    const cloud = await ensureCloudSkills();
    const result = await pluginManager.callTool(cloud.id, 'list_awesome_skills', {});
    awesomeSkills.value = parseAwesomeSkills(toolText(result));
  } catch (e: any) {
    awesomeError.value = e?.message || 'Failed to load Awesome-Skills directory.';
  } finally {
    awesomeLoading.value = false;
  }
}

async function installAwesomeSkill(skill: AwesomeSkill) {
  installingUrl.value = skill.url;
  awesomeError.value = '';
  try {
    const cloud = await ensureCloudSkills();
    await pluginManager.callTool(cloud.id, 'install_awesome_skill', {
      name: skill.name,
      url: skill.url
    });
  } catch (e: any) {
    awesomeError.value = e?.message || `Failed to install ${skill.name}.`;
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
};

const updateActivePlugins = () => {
  activePlugins.value = pluginManager.activePlugins;
};

const isPluginActive = (id: string) => {
  return activePlugins.value.includes(id);
};

const togglePlugin = async (plugin: PluginConfig) => {
  plugin.enabled = !plugin.enabled;
  savePlugins();

  if (plugin.enabled) {
    await pluginManager.startPlugin(plugin);
  } else {
    await pluginManager.stopPlugin(plugin.id);
  }
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

  await pluginManager.startPlugin(config);
  updateActivePlugins();

  newPlugin.value = { name: '', command: '' };
  newPluginArgs.value = '';
};

const removePlugin = async (id: string) => {
  await pluginManager.stopPlugin(id);
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
  background:
    radial-gradient(circle at 12% 0%, rgba(185, 28, 28, 0.22), transparent 30%),
    radial-gradient(circle at 88% 20%, rgba(124, 58, 237, 0.2), transparent 28%),
    linear-gradient(180deg, rgba(8, 8, 12, 0.35), rgba(8, 8, 12, 0.82));
}

.plugin-topbar,
.directory-toolbar,
.add-plugin,
.plugin-card,
.skill-card,
.empty-panel,
.notice {
  border: 1px solid rgba(255, 255, 255, 0.1);
  background: linear-gradient(180deg, rgba(28, 28, 38, 0.78), rgba(12, 12, 18, 0.72));
  box-shadow: 0 20px 55px rgba(0, 0, 0, 0.34), inset 0 1px 0 rgba(255, 255, 255, 0.08);
  backdrop-filter: blur(18px);
}

.plugin-topbar {
  position: sticky;
  top: 0;
  z-index: 2;
  display: flex;
  justify-content: space-between;
  gap: 18px;
  align-items: center;
  padding: 16px;
  border-radius: 14px;
}

.plugin-eyebrow {
  margin: 0 0 5px;
  color: #fb7185;
  font-size: 0.72rem;
  font-weight: 800;
  letter-spacing: 0.14em;
  text-transform: uppercase;
}

h2,
h3 {
  margin: 0;
  letter-spacing: 0;
}

.plugin-tabs {
  display: grid;
  grid-template-columns: repeat(2, minmax(120px, 1fr));
  gap: 6px;
  padding: 5px;
  border-radius: 12px;
  background: rgba(0, 0, 0, 0.26);
  border: 1px solid rgba(255, 255, 255, 0.08);
}

.plugin-tabs button,
button {
  min-height: 38px;
  padding: 9px 14px;
  border: 1px solid rgba(255, 255, 255, 0.11);
  border-radius: 10px;
  cursor: pointer;
  color: var(--text-primary);
  background: rgba(255, 255, 255, 0.04);
  font-weight: 800;
  letter-spacing: 0;
  transition: transform 0.18s ease, border-color 0.18s ease, box-shadow 0.18s ease, background 0.18s ease;
}

button:hover:not(:disabled) {
  transform: translateY(-1px);
  border-color: rgba(248, 113, 113, 0.45);
  box-shadow: 0 12px 26px rgba(0, 0, 0, 0.3);
}

button:disabled {
  cursor: wait;
  opacity: 0.55;
}

.plugin-tabs button.active {
  border-color: rgba(248, 113, 113, 0.5);
  background: linear-gradient(135deg, rgba(220, 38, 38, 0.72), rgba(124, 58, 237, 0.66));
  box-shadow: 0 10px 28px rgba(185, 28, 28, 0.24);
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
  padding: 18px;
  border-radius: 14px;
  transition: transform 0.18s ease, border-color 0.18s ease, box-shadow 0.18s ease;
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
  gap: 10px;
  flex-wrap: wrap;
}

.command {
  max-width: min(62vw, 560px);
  margin: 8px 0 0;
  color: var(--text-muted);
  font-size: 0.84rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.status {
  font-size: 0.68rem;
  padding: 4px 9px;
  border-radius: 999px;
  font-weight: 800;
  text-transform: uppercase;
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
}

.add-plugin,
.directory-toolbar,
.empty-panel,
.notice {
  margin-top: 16px;
  padding: 18px;
  border-radius: 14px;
}

.directory-toolbar {
  display: flex;
  justify-content: space-between;
  gap: 18px;
  align-items: center;
  margin-top: 0;
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
  border-radius: 10px;
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
  grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
  margin-top: 16px;
}

.skill-card {
  min-height: 220px;
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  gap: 14px;
  padding: 18px;
  border-radius: 14px;
  transition: transform 0.18s ease, border-color 0.18s ease, box-shadow 0.18s ease;
}

.skill-card p {
  color: var(--text-secondary);
  line-height: 1.45;
}

code {
  color: #c4b5fd;
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
  .import-section {
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
