# Cerberus AI Desktop (v0.5.0)

Local-first, independent desktop AI chat shell built with Tauri, Rust, Vue 3, and native `llama.cpp` engine.

![Cerberus chat dashboard](assets/readme/chat-dashboard.png)

Cerberus AI provides a 100% private, standalone desktop chat environment. Run local GGUF models directly on your hardware without needing external services, cloud API keys, or third-party background daemons.

> [!WARNING]
> **Beta Release Notice**: Cerberus AI Desktop v0.5.0 is currently in active Beta. Some features may experience intermittent issues. If you encounter any bugs, please report them on the [GitHub Issues Page](https://github.com/tjcrims0nx/CerberusAI-Desktop/issues) so they can be reviewed and resolved.

---

## ⚡ What's New in v0.5.0

- **Standalone Native `llama-server` Engine**: Built-in, high-performance `llama.cpp` integration with Vulkan GPU acceleration and FlashAttention (`-fa auto`) for 3x faster prompt processing. Zero setup required.
- **Direct HuggingFace GGUF Search & Parallel Pulls**: Search open-source GGUF models directly from HuggingFace. Downloads use parallel 8-stream chunked requests with automatic resume support.
- **Accurate Model & File Size Metadata**: Direct parsing of HuggingFace LFS file sizes (`💾 4.2 GB`, `💾 8.5 GB`) across search results and local GGUF cards.
- **Live Download Progress & In-Modal Banners**: Real-time progress reporting (speed MB/s, ETA, transferred size, active status) floating above all overlays and inside the Model Manager.
- **Dynamic Model Self-Identity**: Local models automatically recognize their true model architecture and identity (`Qwen3.5`, `Llama 3.3`, `DeepSeek R1/V3`, etc.) instead of generic prompt strings.
- **Native Windows 11 Fluent Glass UI**: Standardized design system with Acrylic/Mica glass-morphism, smooth `--radius-xl` rounded corners, and purple ambient glow.
- **Model Context Protocol (MCP) Support**: Built-in MCP plugin system to connect local tools and skills directly to language models.

---

## 📦 Latest Stable Release

- **Current Stable:** [v0.5.0](https://github.com/tjcrims0nx/CerberusAI-Desktop/releases/tag/v0.5.0)
- **Windows Installer (NSIS):** `Cerberus-Setup.exe`
- **Windows Installer (MSI):** `Cerberus_0.5.0_x64_en-US.msi`
- **Checksums:** `SHA256SUMS.txt`

---

## 📥 Installation

### Download & Install

Download the latest installer (`Cerberus-Setup.exe` or `Cerberus_0.5.0_x64_en-US.msi`) directly from the [GitHub Releases page](https://github.com/tjcrims0nx/CerberusAI-Desktop/releases).

---

## 🖼️ Interface Showcase

### Chat Dashboard
![Cerberus chat dashboard](assets/readme/chat-dashboard.png)

### Model Manager & HuggingFace Pulls
![Cerberus model manager](assets/readme/model-manager.png)

### MCP Plugin Manager
![Cerberus MCP plugins](assets/readme/mcp-plugins.png)

---

## ✨ Core Features

### 🖥️ 100% Local & Private
- Native desktop shell powered by Rust & Tauri.
- Direct execution via built-in `llama-server` engine.
- Hardware monitoring displaying real-time CPU, RAM, and VRAM usage.
- All prompts, chats, and models stay 100% on your machine.

### 📦 GGUF Model Management
- Browse & search HuggingFace open-source GGUF models.
- Parallel multi-threaded chunked downloads with resume support.
- One-click import for custom local `.gguf` files.
- Active model switching and real-time disk usage analytics.

### 🔌 Model Context Protocol (MCP)
- Built-in MCP Plugin Manager.
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
[github.com/tjcrims0nx/CerberusAI-Desktop/releases](https://github.com/tjcrims0nx/CerberusAI-Desktop/releases)
