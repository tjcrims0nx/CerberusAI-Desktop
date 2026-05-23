# Cerberus AI — Local-First Chat Dashboard

![Cerberus UI](assets/screenshot.png?v=2)

Cerberus is a powerful, local-first chat dashboard designed for uncensored and private interactions with language models. It runs entirely on your machine via Ollama, ensuring your data never leaves your local environment.

> [!IMPORTANT]
> **API Key Required:** An active API key from [cerberusai.dev](https://cerberusai.dev) is **REQUIRED** to utilize this software and unlock the chat interface.

## Features

### Core
- **Local-First Privacy**: Your chats and data stay on your machine.
- **Uncensored Models**: Full support for uncensored language models without restrictions.
- **Modern UI**: Sleek, glassmorphic design built with Vue 3 and Tauri.

### Model Management
- **LM Studio-Style Model Picker**: Dropdown shows all allowlisted models with download status. Selecting an undownloaded model auto-triggers the pull.
- **Cloud Catalog Tab**: Browse all available models from the Cerberus CDN with real quantization options discovered live from the server.
- **Server-Driven Model Allowlist**: Model filtering driven by the API instead of hardcoded names.
- **Dynamic Quantization**: Automatically selects and downloads the smallest available quantization for any given model.
- **Direct-GGUF Flow**: Blazingly fast model pulls directly from our high-speed mirrors.
- **VRAM-Fit Hints**: GPU memory estimates surfaced in the cloud catalog so you know what fits before downloading.

### Downloads
- **Resumable Parallel Downloads** *(v0.3.0)*: 8-connection parallel HTTP Range requests with per-chunk sidecar tracking — interrupted downloads pick up where they left off.
- **Live Speed & ETA** *(v0.3.1)*: Real-time MB/s throughput and remaining-time estimate on the download progress bar.
- **Download Progress Bar**: Fixed top-of-window progress bar during model downloads with real-time percentage and status.
- **Cancel Downloads**: Abort in-progress model pulls with a single click; temp files are cleaned up automatically.

### Intelligence
- **Plugin & Custom Skill System** *(v0.3.2-beta)*: Built-in slash commands (`/simplify`, `/verify`, `/remember`, `/stuck`) plus support for importing custom `SKILL.md` files with YAML frontmatter.
- **Slash Suggestions Overlay** *(v0.3.2-beta)*: Floating auto-complete overlay when typing `/` in the composer — discover and invoke plugins instantly.
- **Premium Model Gating** *(v0.3.1)*: Gateway-side premium model access control wired through the desktop client.

### System
- **Smart Update Button**: Shows current version, checks GitHub for updates, pulses when a new version is available. Beta builds auto-route to the beta installer.
- **First-Run Ollama Tuning** *(v0.3.1)*: Automatic configuration of keep-alive, flash attention, and KV cache quantization on first launch (Windows).
- **Hardware Detection**: CPU, RAM, GPU, and VRAM info surfaced in the UI and used for model recommendations.

---

## One-Line Install (Windows)

### Stable

```powershell
irm https://cerberusai.dev/get | iex
```

### Beta / Pre-release

```powershell
irm https://cerberusai.dev/get-beta | iex
```

*(Or direct GitHub fallback link)*:
```powershell
irm https://raw.githubusercontent.com/tjcrims0nx/CerberusAI-Desktop/main/deploy/get-cerberus-beta.ps1 | iex
```

> [!TIP]
> The beta installer automatically finds the latest pre-release from GitHub, bootstraps WebView2 + Ollama + a starter model, and installs the Cerberus desktop app.

Or download the latest installer from our [releases page](https://github.com/tjcrims0nx/CerberusAI-Desktop/releases).

---

## Getting Started

1. **Install Ollama**: Ensure [Ollama](https://ollama.com) is installed and running on your machine.
2. **Get an API Key**: Sign up at [cerberusai.dev](https://cerberusai.dev) to obtain your unique API key.
3. **Download Cerberus**: Run the one-liner above or grab the installer from [releases](https://github.com/tjcrims0nx/CerberusAI-Desktop/releases).
4. **Unlock and Chat**: Enter your API key in the app and start chatting locally!

---

## Development

Cerberus is built using:
- **Frontend**: Vue 3, Vite
- **Backend**: Rust, Tauri
- **Models**: GGUF via Ollama

To run locally for development:
```bash
npm install
npm run tauri dev
```

To build for production:
```bash
npm run tauri build
```
