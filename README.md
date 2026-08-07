# HELIX Desktop (v0.6.0)

Local-first, 100% private desktop AI chat shell built with Tauri, Rust, Vue 3, and native `llama.cpp` engine.

HELIX provides a standalone, private desktop chat environment. Run local GGUF models directly on your hardware without needing external cloud services, API keys, or third-party background daemons.

> [!NOTE]
> **Release Notice**: HELIX Desktop v0.6.0 is an active local-first release. If you encounter any bugs or feature requests, please report them on the [GitHub Issues Page](https://github.com/tjcrims0nx/Helix/issues).

---

## ⚡ What's New in v0.6.0

- **💻 Professional Code & File Creation**:
  - Auto-generated language tags (`PYTHON`, `TYPESCRIPT`, `RUST`, `HTML`, `CSS`, `VUE`, `JSON`, `POWERSHELL`, `SQL`, `BASH`) and file extension suggestions (`script.py`, `Component.vue`, `index.html`).
  - One-click **Copy** button on code blocks with animated feedback (`Copied ✓`).
  - One-click **Download / Save File** button prompting native Windows File Save Dialog via Tauri (`save_text_file`) or instant blob download.

- **🔌 In-Chat `/mcp` Slash Commands & Composer Menu**:
  - `/mcp list`: Live status overview of all configured MCP plugins, connection state (🟢 Connected / 🔴 Offline), and endpoints.
  - `/mcp enable <id>` & `/mcp disable <id>`: Activate or deactivate plugins directly inside chat prompts.
  - `/mcp open`: Instantly open the Plugin Manager modal.
  - Quick **MCP Plugins** launcher added inside the composer `+` Attach Menu.

- **🌐 Dual Stdio & SSE MCP Plugin Support**:
  - Command-based (`stdio`) and URL-based (`SSE`) MCP plugin forms with live toggle tabs in the Plugin Manager.
  - Real-time **Connected / Offline** badge sync linked directly to active sidecar processes.
  - Isolated plugin startup error handling so one failing plugin does not block others.

- **📋 Universal Message Copying**:
  - One-click Copy buttons added to **both User prompts and Assistant responses**.

- **🎨 Dark Metallic Glass Design**:
  - Standardized Windows 11 Fluent Glassmorphism with Acrylic/Mica glass visual hierarchy across sidebars, action cards, model manager, and chat bubbles.
  - Persistent welcome dashboard retaining quick suggestion chips on empty chats.

- **⚡ Standalone Native `llama-server` Engine**:
  - Built-in `llama.cpp` integration with Vulkan GPU acceleration and FlashAttention (`-fa auto`) for 3x faster prompt processing. Zero setup required.

- **📦 HuggingFace GGUF Search & Parallel Chunk Downloads**:
  - Direct search for open-source GGUF models on HuggingFace.
  - Multi-threaded 8-stream parallel downloads with resume support and accurate LFS file size metadata (`💾 4.2 GB`, `💾 8.5 GB`).

---

## 📦 Latest Stable Release

- **Current Stable:** [v0.6.0](https://github.com/tjcrims0nx/Helix/releases/tag/v0.6.0)
- **Windows Installer (NSIS):** `HELIX-Setup.exe`
- **Windows Installer (MSI):** `HELIX_0.6.0_x64_en-US.msi`
- **Checksums:** `SHA256SUMS.txt`

---

## 📥 Installation

### Download & Install

Download the latest installer (`HELIX-Setup.exe` or `HELIX_0.6.0_x64_en-US.msi`) directly from the [GitHub Releases page](https://github.com/tjcrims0nx/Helix/releases).

---

## 🖼️ Interface Showcase

### Chat Dashboard
![HELIX chat dashboard](assets/readme/chat-dashboard.png)

### Model Manager & HuggingFace Pulls
![HELIX model manager](assets/readme/model-manager.png)

### MCP Plugin Manager
![HELIX MCP plugins](assets/readme/mcp-plugins.png)

---

## ✨ Core Features

### 🖥️ 100% Local & Private
- Native desktop shell powered by Rust & Tauri 2.
- Direct execution via built-in `llama-server` engine.
- Hardware monitoring displaying real-time CPU, RAM, and VRAM usage.
- All prompts, chats, and models stay 100% on your machine.

### 📦 GGUF Model Management
- Browse & search HuggingFace open-source GGUF models.
- Parallel multi-threaded chunked downloads with resume support.
- One-click import for custom local `.gguf` files.
- Active model switching and real-time disk usage analytics.

### 🔌 Model Context Protocol (MCP)
- Built-in MCP Plugin Manager supporting stdio binaries & SSE URLs.
- In-chat `/mcp` slash commands for instant plugin management.
- Connect filesystem tools, Web APIs, and custom agent skills.
- Automatic tool calling loop with reasoning trace.

---

## 💻 Development

### Prerequisites
- Node.js (v18+)
- Rust (v1.77+)
- Tauri CLI (`npm install -g @tauri-apps/cli`)

### Run Locally

```bash
# Install dependencies
npm install

# Launch app in development mode
npm run tauri:dev
```

### Production Build

```bash
# Build desktop installer executable
npm run tauri:build
```

---

## 🛠️ Technology Stack

- **Desktop Framework**: Tauri 2 (Rust)
- **Frontend Logic**: Vue 3, Vite, TypeScript
- **Local Engine**: `llama-server` (Vulkan GPU offload + FlashAttention)
- **Design System**: Windows 11 Fluent Glassmorphism (CSS Tokens)
- **Distribution**: GitHub Releases & Actions Automation

---

## 📜 License & Releases

Stable releases and changelogs are published automatically at:
[github.com/tjcrims0nx/Helix/releases](https://github.com/tjcrims0nx/Helix/releases)
