import { Plugin } from "../types";

const SIMPLIFY_PROMPT = `# Simplify: Code Review and Cleanup

Review all changed files for reuse, quality, and efficiency. Fix any issues found.

## Phase 1: Identify Changes
Inspect the provided git diff to see what changed. If there is no diff, ask the user what files they want reviewed.

## Phase 2: Code Review Focus Areas

### 1. Code Reuse Review
- Search for existing utilities and helpers that could replace newly written code.
- Flag any new function that duplicates existing functionality and suggest using the existing one.
- Flag inline logic that could use an existing utility (e.g. hand-rolled string manipulation, manual path handling, etc.).

### 2. Code Quality Review
- **Redundant state**: state that duplicates existing state, cached values that could be derived easily.
- **Parameter sprawl**: excessive parameters on functions.
- **Copy-paste with slight variation**: near-duplicate code blocks that should be unified.
- **Leaky abstractions**: exposing internal details that should be encapsulated.
- **Stringly-typed code**: using raw strings where constants, enums or string unions exist.
- **Unnecessary comments**: explanations of WHAT the code does (identifiers should do that); delete them, keeping only non-obvious WHY comments.

### 3. Efficiency Review
- **Unnecessary work**: redundant computations, repeated file reads, duplicate API calls.
- **Missed concurrency**: independent operations run sequentially when they could run in parallel.
- **Memory**: unbounded data structures, missing cleanup, event listener leaks.

## Phase 3: Fix Issues
Aggregate findings and provide clear, actionable suggestions or direct code fixes. Confirm if the code was already clean.`;

const VERIFY_PROMPT = `# Verify: Code Change Verification

Verify that a code change does what it should by thoroughly analyzing and running tests or the app.

## Steps to Verify:
1. **Analyze target changes**: Review the git diff to understand the exact scope of the modifications.
2. **Determine side effects**: Identify all files or components that depend on the modified parts.
3. **Draft a verification checklist**: List specific test cases, edge cases, and user paths that must be tested.
4. **Execution plans**: Formulate exact command invocations (e.g., \`npm run test\`, \`cargo test\`) to run tests.
5. **Report findings**: List what was tested, what passed, and any failures or regression risks discovered.`;

const REMEMBER_PROMPT = `# Memory Review

Review the user's memory landscape and produce a clear report of proposed changes, grouped by action type. Do NOT apply changes — present proposals for user approval.

## Steps:
1. **Gather all memory layers**: Read CLAUDE.md or project instructions if they exist.
2. **Classify entries**: Determine the best destination (CLAUDE.md for project conventions, CLAUDE.local.md for personal instructions).
3. **Identify cleanup opportunities**: Scan for duplicates, outdated entries, and conflicts.
4. **Present the report**: Group by Promotions, Cleanup, Ambiguous, and No Action Needed.`;

const STUCK_PROMPT = `# Stuck: Diagnose Session Performance

The session or process appears stuck, frozen, or extremely slow. Investigate and diagnose system resources.

## What to look for:
1. **High CPU usage**: Check if Ollama or the app is pegging a CPU thread.
2. **Memory leakage**: High RSS/RAM footprint of AI or background tasks.
3. **Stuck sub-processes**: Hung compiler or test suite run.
4. **Diagnostics**: List all active node/ollama processes and sample their state.`;

const BUILTIN_PLUGINS: Plugin[] = [
  {
    id: "simplify@builtin",
    name: "Simplify",
    description: "Review changed code for reuse, quality, and efficiency, and fix any issues found (mimics Claude Code's /simplify).",
    icon: "🔧",
    enabled: true,
    systemPrompt: SIMPLIFY_PROMPT,
    command: "/simplify",
    isBuiltin: true,
    author: "Anthropic / Cerberus",
    version: "1.0.0"
  },
  {
    id: "verify@builtin",
    name: "Verify",
    description: "Verify a code change does what it should by compiling, running, or analyzing the app (mimics Claude Code's /verify).",
    icon: "✅",
    enabled: true,
    systemPrompt: VERIFY_PROMPT,
    command: "/verify",
    isBuiltin: true,
    author: "Anthropic / Cerberus",
    version: "1.0.0"
  },
  {
    id: "remember@builtin",
    name: "Remember",
    description: "Review auto-memory and propose promotions to CLAUDE.md instructions (mimics Claude Code's /remember).",
    icon: "🧠",
    enabled: false,
    systemPrompt: REMEMBER_PROMPT,
    command: "/remember",
    isBuiltin: true,
    author: "Anthropic / Cerberus",
    version: "1.0.0"
  },
  {
    id: "stuck@builtin",
    name: "Stuck",
    description: "Diagnose frozen/slow AI sessions or local Ollama processes (mimics Claude Code's /stuck).",
    icon: "⚠️",
    enabled: false,
    systemPrompt: STUCK_PROMPT,
    command: "/stuck",
    isBuiltin: true,
    author: "Anthropic / Cerberus",
    version: "1.0.0"
  }
];

const STORAGE_KEY_PLUGINS = "cerberus_plugins_config";
const STORAGE_KEY_CUSTOM = "cerberus_custom_plugins";

export function loadAllPlugins(): Plugin[] {
  try {
    const customRaw = localStorage.getItem(STORAGE_KEY_CUSTOM);
    const custom: Plugin[] = customRaw ? JSON.parse(customRaw) : [];

    const configRaw = localStorage.getItem(STORAGE_KEY_PLUGINS);
    const config: Record<string, boolean> = configRaw ? JSON.parse(configRaw) : {};

    // Combine builtins with custom, applying saved enabled state overrides
    const all = [...BUILTIN_PLUGINS, ...custom];
    return all.map(p => ({
      ...p,
      enabled: config[p.id] !== undefined ? config[p.id] : p.enabled
    }));
  } catch (e) {
    console.error("Failed to load plugins:", e);
    return BUILTIN_PLUGINS;
  }
}

export function savePluginState(pluginId: string, enabled: boolean): void {
  try {
    const configRaw = localStorage.getItem(STORAGE_KEY_PLUGINS);
    const config: Record<string, boolean> = configRaw ? JSON.parse(configRaw) : {};
    config[pluginId] = enabled;
    localStorage.setItem(STORAGE_KEY_PLUGINS, JSON.stringify(config));
  } catch (e) {
    console.error("Failed to save plugin state:", e);
  }
}

export function parseMarkdownSkill(filename: string, content: string): Plugin {
  // Parse frontmatter
  let frontmatter: Record<string, string> = {};
  let body = content;

  if (content.startsWith("---")) {
    const nextDelimiter = content.indexOf("---", 3);
    if (nextDelimiter !== -1) {
      const fmSection = content.substring(3, nextDelimiter);
      body = content.substring(nextDelimiter + 3).trim();

      const lines = fmSection.split("\n");
      for (const line of lines) {
        const colon = line.indexOf(":");
        if (colon !== -1) {
          const key = line.substring(0, colon).trim().toLowerCase();
          const val = line.substring(colon + 1).trim().replace(/^['"]|['"]$/g, ""); // strip quotes
          frontmatter[key] = val;
        }
      }
    }
  }

  const name = frontmatter.name || filename.replace(/\.md$/i, "");
  const cleanName = name.replace(/[^a-zA-Z0-9-_]/g, "");
  const id = `${cleanName.toLowerCase()}@custom`;
  const description = frontmatter.description || "Custom skill imported from markdown file.";
  const command = `/${cleanName.toLowerCase()}`;
  const icon = frontmatter.icon || "📄";

  return {
    id,
    name: name.charAt(0).toUpperCase() + name.slice(1),
    description,
    icon,
    enabled: true,
    systemPrompt: body,
    command,
    isBuiltin: false,
    author: frontmatter.author || "User",
    version: frontmatter.version || "1.0.0"
  };
}

export function addCustomPlugin(plugin: Plugin): Plugin[] {
  try {
    const customRaw = localStorage.getItem(STORAGE_KEY_CUSTOM);
    let custom: Plugin[] = customRaw ? JSON.parse(customRaw) : [];

    // Remove if already exists with same id
    custom = custom.filter(p => p.id !== plugin.id);
    custom.push(plugin);

    localStorage.setItem(STORAGE_KEY_CUSTOM, JSON.stringify(custom));

    // Also enable it by default
    savePluginState(plugin.id, true);

    return loadAllPlugins();
  } catch (e) {
    console.error("Failed to add custom plugin:", e);
    return loadAllPlugins();
  }
}

export function deleteCustomPlugin(pluginId: string): Plugin[] {
  try {
    const customRaw = localStorage.getItem(STORAGE_KEY_CUSTOM);
    if (customRaw) {
      let custom: Plugin[] = JSON.parse(customRaw);
      custom = custom.filter(p => p.id !== pluginId);
      localStorage.setItem(STORAGE_KEY_CUSTOM, JSON.stringify(custom));
    }

    const configRaw = localStorage.getItem(STORAGE_KEY_PLUGINS);
    if (configRaw) {
      const config: Record<string, boolean> = JSON.parse(configRaw);
      delete config[pluginId];
      localStorage.setItem(STORAGE_KEY_PLUGINS, JSON.stringify(config));
    }

    return loadAllPlugins();
  } catch (e) {
    console.error("Failed to delete custom plugin:", e);
    return loadAllPlugins();
  }
}
