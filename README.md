# Cerberus AI — Local-First Chat Dashboard

![Cerberus UI](file:///C:/Users/tjcri/.gemini/antigravity/brain/b3f02429-5114-47ea-bdd1-f43b176a8bbb/main_chat_interface_1777312310653.png)

Cerberus is a powerful, local-first chat dashboard designed for uncensored and private interactions with language models. It runs entirely on your machine via Ollama, ensuring your data never leaves your local environment.

> [!IMPORTANT]
> **API Key Required:** An active API key from [cerberusai.dev](https://cerberusai.dev) is **REQUIRED** to utilize this software and unlock the chat interface.

## Features

- **Local-First Privacy**: Your chats and data stay on your machine.
- **Uncensored Models**: Full support for uncensored language models without restrictions.
- **Dynamic Quantization**: (New in v0.1.2) automatically selects and downloads the smallest available quantization for any given model to optimize performance and disk space.
- **Direct-GGUF Flow**: Blazingly fast model pulls directly from our high-speed mirrors.
- **Modern UI**: Sleek, glassmorphic design built with Vue 3 and Tauri.

## Getting Started

1. **Install Ollama**: Ensure [Ollama](https://ollama.com) is installed and running on your machine.
2. **Get an API Key**: Sign up at [cerberusai.dev](https://cerberusai.dev) to obtain your unique API key.
3. **Download Cerberus**: Get the latest installer from our [releases page](https://github.com/tjcrims0nx/CerberusAI-Desktop/releases).
4. **Unlock and Chat**: Enter your API key in the app and start chatting locally!

## Development

Cerberus is built using:
- **Frontend**: Vue 3, Vite, Tailwind CSS (optional)
- **Backend**: Rust, Tauri
- **Models**: GGUF via Ollama

To run locally for development:
```bash
npm install
npm run dev
```

To build for production:
```bash
npm run tauri:build
```
