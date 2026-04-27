# Cerberus Desktop

Local-first chat dashboard for uncensored language models. Tauri (Rust) + Vue 3.

Inference runs on the user's machine via [Ollama](https://ollama.com). The app is gated behind a Cerberus API key validated against `https://api.cerberusai.dev/v1/models`.

## Layout

```
src/                  Vue 3 frontend (single-screen chat dashboard)
src-tauri/            Rust core
  src/lib.rs            Tauri commands: check_ollama, list_models, chat_stream, detect_hardware
  src/ollama.rs         reqwest client → 127.0.0.1:11434, NDJSON streaming
  src/hardware.rs       DXGI GPU enumeration (Windows)
  tauri.conf.json       Bundle config (MSI + NSIS, embedded WebView2 bootstrapper)
  capabilities/         Per-window permission set
deploy/install.ps1    Windows one-shot installer (WebView2 + Ollama + model + app)
public/cerberus.svg   Source for the icon set
```

## Development

```powershell
npm install
npm run tauri:dev
```

Requires Node 18+, Rust 1.77+, and Microsoft Edge WebView2 Runtime (preinstalled on Win 11).

## Production build

```powershell
npm run tauri:build
```

Outputs an `.msi` and `.exe` (NSIS) under `src-tauri/target/release/bundle/`.

## End-user install

```powershell
iwr -useb https://cerberusai.dev/install.ps1 | iex
```

`install.ps1` flags:
- `-Check` &mdash; detection-only report (WebView2, Ollama, models, GPU)
- `-Model <tag>` &mdash; override default `qwen2.5:3b` (use `skip` to skip the pull)
- `-ReleaseTag <tag>` &mdash; pin to a specific GitHub release (default `latest`)
- `-Silent` &mdash; unattended mode

## Notes

- The bundle config uses `webviewInstallMode: downloadBootstrapper` so the MSI/NSIS installer also pulls WebView2 if missing.
- API key is verified once on entry, then re-verified on each launch. Sign-out clears the key from `localStorage` and closes the app.
- Hardware-aware model picking lives in `pickRecommendedModel()` &mdash; biases toward smaller models when detected VRAM is &lt; 8 GB.
- API gateway must allow CORS from the Tauri origin (`tauri://localhost` on Windows) for the verify call to succeed.
