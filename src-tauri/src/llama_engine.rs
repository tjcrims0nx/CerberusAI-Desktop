//! Standalone llama.cpp engine manager.
//! Runs local GGUF models independently via `llama-server` on localhost:11435
//! without needing Ollama or any external services.
//!
//! On first use the engine automatically downloads a pre-built llama-server
//! binary from the llama.cpp GitHub releases, extracts it into
//! `~/.CerberusAI/bin/`, and manages the process lifecycle.

use crate::{ChatMessage, ChatStreamChunk};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex as StdMutex;
use std::time::Duration;
use tauri::ipc::Channel;

static ACTIVE_SERVER: StdMutex<Option<ActiveLlamaServer>> = StdMutex::new(None);

struct ActiveLlamaServer {
    model_path: PathBuf,
    child: Child,
}

const SERVER_PORT: u16 = 11435;
const SERVER_URL: &str = "http://127.0.0.1:11435";

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}

fn pick_asset_suffix() -> &'static str {
    if cfg!(target_os = "windows") {
        if cfg!(target_arch = "x86_64") {
            // Vulkan build works on NVIDIA + AMD + Intel on Windows
            "bin-win-vulkan-x64.zip"
        } else {
            "bin-win-arm64.zip"
        }
    } else if cfg!(target_os = "macos") {
        "bin-macos-arm64.zip"
    } else {
        // Linux x86_64
        "bin-ubuntu-x64.zip"
    }
}

async fn fetch_latest_release_info(client: &reqwest::Client) -> Result<(String, String, String), anyhow::Error> {
    let suffix = pick_asset_suffix();
    let api_url = "https://api.github.com/repos/ggml-org/llama.cpp/releases/latest";
    let resp = client
        .get(api_url)
        .header("User-Agent", "CerberusAI-Desktop")
        .header("Accept", "application/json")
        .send()
        .await;

    if let Ok(r) = resp {
        if r.status().is_success() {
            if let Ok(release) = r.json::<GithubRelease>().await {
                let tag = release.tag_name;
                if let Some(asset) = release.assets.into_iter().find(|a| a.name.ends_with(suffix)) {
                    return Ok((tag, asset.name, asset.browser_download_url));
                }
            }
        }
    }

    // Fallback URL if GitHub API rate-limit or offline
    let tag = "b4800".to_string();
    let asset_name = format!("llama-{tag}-{suffix}");
    let dl_url = format!("https://github.com/ggml-org/llama.cpp/releases/download/{tag}/{asset_name}");
    Ok((tag, asset_name, dl_url))
}

/// Download and extract llama-server into `app_dir/bin/`.
/// Returns the path to the extracted `llama-server` binary.
async fn download_llama_server(app_dir: &Path) -> Result<PathBuf, anyhow::Error> {
    let bin_dir = app_dir.join("bin");
    tokio::fs::create_dir_all(&bin_dir).await?;

    let bin_name = if cfg!(windows) { "llama-server.exe" } else { "llama-server" };
    let target_bin = bin_dir.join(bin_name);
    let version_file = bin_dir.join("version.txt");

    let current_version = tokio::fs::read_to_string(&version_file)
        .await
        .unwrap_or_default();

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(600))
        .build()?;

    let (tag_name, asset_name, download_url) = fetch_latest_release_info(&client).await?;

    // Already downloaded with matching release version?
    if !current_version.trim().is_empty()
        && current_version.trim() == tag_name
        && tokio::fs::metadata(&target_bin).await.is_ok()
    {
        return Ok(target_bin);
    }

    // Stop active process before replacing binary files
    stop_server();
    let _ = tokio::fs::remove_file(&target_bin).await;

    log::info!("Downloading llama-server release {} from {}...", tag_name, download_url);

    let resp = client
        .get(&download_url)
        .header("User-Agent", "CerberusAI-Desktop")
        .send()
        .await?;

    if !resp.status().is_success() {
        return Err(anyhow::anyhow!(
            "Failed to download llama-server: HTTP {} from {}",
            resp.status(),
            download_url
        ));
    }

    let bytes = resp.bytes().await?;
    let zip_path = bin_dir.join(&asset_name);
    tokio::fs::write(&zip_path, &bytes).await?;

    // Extract the zip
    let zip_path_clone = zip_path.clone();
    let bin_dir_clone = bin_dir.clone();
    tokio::task::spawn_blocking(move || -> Result<(), anyhow::Error> {
        let file = std::fs::File::open(&zip_path_clone)?;
        let mut archive = zip::ZipArchive::new(file)?;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let outpath = match entry.enclosed_name() {
                Some(p) => p.to_owned(),
                None => continue,
            };

            let fname = outpath.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
            let dominated = fname.to_lowercase();
            let dominated_str = dominated.as_str();
            let dominated_bytes = dominated_str.as_bytes();
            let dominated_path = std::path::Path::new(dominated_str);

            let dominated_ext = dominated_path.extension().map(|e| e.to_str().unwrap_or("")).unwrap_or("");

            let dominated_is_server = dominated.starts_with("llama-server");
            let dominated_is_dll = dominated_ext == "dll" || dominated_ext == "so" || dominated_ext == "dylib";

            if entry.is_dir() {
                continue;
            }
            if !dominated_is_server && !dominated_is_dll {
                if !(dominated_bytes.len() > 0 && dominated_is_dll) {
                    continue;
                }
            }

            let dest = bin_dir_clone.join(&fname);
            let mut outfile = std::fs::File::create(&dest)?;
            std::io::copy(&mut entry, &mut outfile)?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))?;
            }
        }
        Ok(())
    })
    .await??;

    // Clean up zip
    let _ = tokio::fs::remove_file(&zip_path).await;

    if tokio::fs::metadata(&target_bin).await.is_ok() {
        let _ = tokio::fs::write(&version_file, &tag_name).await;
        log::info!("llama-server {} installed successfully to {}", tag_name, target_bin.display());
        Ok(target_bin)
    } else {
        Err(anyhow::anyhow!(
            "Failed to extract llama-server binary from {}. Expected binary at: {}",
            asset_name,
            target_bin.display()
        ))
    }
}

/// Find or auto-download `llama-server` binary.
pub async fn find_or_download_llama_server(app_dir: &Path) -> Result<PathBuf, anyhow::Error> {
    download_llama_server(app_dir).await
}

fn spawn_llama_server(server_bin: &Path, model_path: &Path, ngl: u32) -> Result<Child, anyhow::Error> {
    let mut cmd = Command::new(server_bin);
    cmd.arg("-m")
       .arg(model_path)
       .arg("--port")
       .arg(SERVER_PORT.to_string())
       .arg("-c")
       .arg("4096")
       .arg("-ngl")
       .arg(ngl.to_string())
       .arg("-fa")
       .arg("auto")
       .stdout(Stdio::piped())
       .stderr(Stdio::piped());

    cmd.spawn().map_err(|e| anyhow::anyhow!("Failed to start llama-server: {e}"))
}

/// Ensure a `llama-server` process is running for the specified `.gguf` file.
pub async fn ensure_server(model_path: &Path, app_dir: &Path) -> Result<(), anyhow::Error> {
    if !model_path.exists() {
        return Err(anyhow::anyhow!(
            "Model file not found at path: {}. Please pull or import the model first.",
            model_path.display()
        ));
    }

    // Check if server is already running with the exact same model
    {
        let mut lock = ACTIVE_SERVER.lock().unwrap();
        if let Some(active) = lock.as_mut() {
            if active.model_path == model_path {
                match active.child.try_wait() {
                    Ok(None) => return Ok(()), // Still running fine
                    _ => {
                        lock.take(); // Dead process, clean up
                    }
                }
            } else {
                // Different model requested — kill previous server process
                let _ = active.child.kill();
                let _ = active.child.wait();
                lock.take();
            }
        }
    }

    // Find or auto-download binary
    let server_bin = find_or_download_llama_server(app_dir).await?;

    // On Windows, unblock file (remove Zone.Identifier stream) to bypass App Control / SmartScreen blocks
    #[cfg(windows)]
    {
        let zone_file = format!("{}:Zone.Identifier", server_bin.display());
        let _ = std::fs::remove_file(zone_file);
    }

    // Try starting with GPU offload (-ngl 99) first, then fallback to CPU (-ngl 0) if GPU fails
    let ngl_options = [99, 0];
    let mut last_error = String::new();

    for &ngl in &ngl_options {
        let mut child = match spawn_llama_server(&server_bin, model_path, ngl) {
            Ok(c) => c,
            Err(e) => {
                last_error = e.to_string();
                continue;
            }
        };

        // Wait for server health endpoint to respond
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()?;

        let start = std::time::Instant::now();
        let timeout = Duration::from_secs(120); // 120s timeout for cold model loading
        let mut process_crashed = false;

        while start.elapsed() < timeout {
            tokio::time::sleep(Duration::from_millis(500)).await;

            // Check if child process crashed
            match child.try_wait() {
                Ok(Some(status)) => {
                    process_crashed = true;
                    let mut stderr_output = String::new();
                    if let Some(mut stderr_pipe) = child.stderr.take() {
                        use std::io::Read;
                        let _ = stderr_pipe.read_to_string(&mut stderr_output);
                    }
                    let err_msg = stderr_output.trim();
                    last_error = if err_msg.is_empty() {
                        format!("llama-server exited with status {status}")
                    } else {
                        format!("llama-server exited with status {status}: {err_msg}")
                    };
                    break;
                }
                Err(e) => {
                    process_crashed = true;
                    last_error = format!("Error checking llama-server: {e}");
                    break;
                }
                Ok(None) => {} // Still running/loading
            }

            if let Ok(resp) = client.get(format!("{SERVER_URL}/health")).send().await {
                if resp.status().is_success() {
                    let mut lock = ACTIVE_SERVER.lock().unwrap();
                    *lock = Some(ActiveLlamaServer {
                        model_path: model_path.to_path_buf(),
                        child,
                    });
                    return Ok(());
                }
            }
        }

        if !process_crashed {
            let _ = child.kill();
            let _ = child.wait();
            last_error = "`llama-server` timed out while loading model into memory".to_string();
        }
    }

    Err(anyhow::anyhow!("Failed to start llama-server engine: {last_error}"))
}

#[derive(Serialize)]
struct OpenAiMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct OpenAiChatReq<'a> {
    messages: Vec<OpenAiMessage<'a>>,
    stream: bool,
    temperature: f32,
}

#[derive(Deserialize)]
struct StreamDelta {
    content: Option<String>,
}

#[derive(Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
}

/// Stream chat completions from the local `llama-server` endpoint.
pub async fn stream_chat_llama(
    model_path: PathBuf,
    messages: Vec<ChatMessage>,
    on_event: Channel<ChatStreamChunk>,
    mut cancel: tokio::sync::watch::Receiver<bool>,
    app_dir: PathBuf,
) -> Result<(), anyhow::Error> {
    // Ensure llama-server is running for this GGUF file
    ensure_server(&model_path, &app_dir).await?;

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(600))
        .build()?;

    let api_messages: Vec<OpenAiMessage> = messages
        .iter()
        .map(|m| OpenAiMessage {
            role: &m.role,
            content: &m.content,
        })
        .collect();

    let body = OpenAiChatReq {
        messages: api_messages,
        stream: true,
        temperature: 0.7,
    };

    let resp = client
        .post(format!("{SERVER_URL}/v1/chat/completions"))
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let err_text = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("llama-server returned HTTP {status}: {err_text}"));
    }

    let mut stream = resp.bytes_stream();
    let mut buffer = String::new();

    loop {
        tokio::select! {
            _ = cancel.changed() => {
                if *cancel.borrow() {
                    let _ = on_event.send(ChatStreamChunk {
                        delta: String::new(),
                        done: true,
                        error: None,
                        ttft_ms: None,
                        tps: None,
                        tool_calls: None,
                    });
                    return Ok(());
                }
            }
            item = stream.next() => {
                match item {
                    Some(Ok(bytes)) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));
                        while let Some(pos) = buffer.find('\n') {
                            let line = buffer[..pos].trim().to_string();
                            buffer.drain(..=pos);

                            if line.is_empty() || line.starts_with(':') {
                                continue;
                            }
                            if let Some(data) = line.strip_prefix("data: ") {
                                let data = data.trim();
                                if data == "[DONE]" {
                                    let _ = on_event.send(ChatStreamChunk {
                                        delta: String::new(),
                                        done: true,
                                        error: None,
                                        ttft_ms: None,
                                        tps: None,
                                        tool_calls: None,
                                    });
                                    return Ok(());
                                }

                                if let Ok(chunk) = serde_json::from_str::<StreamChunk>(data) {
                                    if let Some(choice) = chunk.choices.first() {
                                        if let Some(delta) = &choice.delta.content {
                                            if !delta.is_empty() {
                                                let _ = on_event.send(ChatStreamChunk {
                                                    delta: delta.clone(),
                                                    done: false,
                                                    error: None,
                                                    ttft_ms: None,
                                                    tps: None,
                                                    tool_calls: None,
                                                });
                                            }
                                        }
                                        if choice.finish_reason.is_some() {
                                            let _ = on_event.send(ChatStreamChunk {
                                                delta: String::new(),
                                                done: true,
                                                error: None,
                                                ttft_ms: None,
                                                tps: None,
                                                tool_calls: None,
                                            });
                                            return Ok(());
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Some(Err(e)) => return Err(e.into()),
                    None => break,
                }
            }
        }
    }

    let _ = on_event.send(ChatStreamChunk {
        delta: String::new(),
        done: true,
        error: None,
        ttft_ms: None,
        tps: None,
        tool_calls: None,
    });
    Ok(())
}

// ─── Engine lifecycle ──────────────────────────────────────────────────────

#[derive(Debug, Serialize, Clone)]
pub struct EngineStatus {
    pub ready: bool,
    pub binary_found: bool,
    pub active_model: Option<String>,
}

/// Check the current state of the llama engine.
pub async fn engine_status(app_dir: &Path) -> EngineStatus {
    let bin_name = if cfg!(windows) { "llama-server.exe" } else { "llama-server" };
    let app_bin = app_dir.join("bin").join(bin_name);
    let binary_found = tokio::fs::metadata(&app_bin).await.is_ok();

    let lock = ACTIVE_SERVER.lock().unwrap();
    let active_model = lock.as_ref().map(|a| a.model_path.display().to_string());
    let ready = lock.is_some();

    EngineStatus {
        ready,
        binary_found,
        active_model,
    }
}

/// Stop any running llama-server process.
pub fn stop_server() {
    let mut lock = ACTIVE_SERVER.lock().unwrap();
    if let Some(mut active) = lock.take() {
        let _ = active.child.kill();
        let _ = active.child.wait();
    }
}
