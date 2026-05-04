//! Cerberus chat backend.
//!
//! Architecture:
//!   * The user's API key is verified against https://api.cerberusai.dev/v1/models
//!     (cloud auth gate — `verify_key`).
//!   * All chat inference happens on the user's own machine via the local Ollama
//!     daemon at http://127.0.0.1:11434  (`list_local`, `stream_chat_local`,
//!     `pull_model`, `local_status`).

use crate::{ChatMessage, ChatStreamChunk};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tauri::ipc::Channel;

const CLOUD_API_BASE: &str = "https://api.cerberusai.dev";
const LLM_FILES_BASE: &str = "https://llm.cerberusai.dev";
const OLLAMA_BASE: &str = "http://127.0.0.1:11434";

use std::sync::OnceLock;

static HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
static HTTP_SHORT_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn http() -> Result<reqwest::Client, reqwest::Error> {
    if let Some(c) = HTTP_CLIENT.get() {
        return Ok(c.clone());
    }
    let c = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60 * 60)) // long for first model pull
        .build()?;
    let _ = HTTP_CLIENT.set(c.clone());
    Ok(c)
}

fn http_short() -> Result<reqwest::Client, reqwest::Error> {
    if let Some(c) = HTTP_SHORT_CLIENT.get() {
        return Ok(c.clone());
    }
    let c = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .build()?;
    let _ = HTTP_SHORT_CLIENT.set(c.clone());
    Ok(c)
}

// ─── Cloud: API-key verification only ──────────────────────────────────────

/// Verify the API key against api.cerberusai.dev.
/// Returns `"ok"` on success, or an error with the upstream status / network detail.
pub async fn verify_key(api_key: &str) -> Result<String, anyhow::Error> {
    let c = http()?;
    let r = c
        .get(format!("{CLOUD_API_BASE}/v1/models"))
        .header("Authorization", format!("Bearer {api_key}"))
        .send()
        .await?;
    if r.status().is_success() {
        Ok("ok".to_string())
    } else if r.status().as_u16() == 401 || r.status().as_u16() == 403 {
        Err(anyhow::anyhow!("invalid API key (HTTP {})", r.status()))
    } else {
        Err(anyhow::anyhow!("API returned status {}", r.status()))
    }
}

// ─── Cloud: GitHub release-based update check ──────────────────────────────

const RELEASES_LATEST_URL: &str =
    "https://api.github.com/repos/tjcrims0nx/CerberusAI-Desktop/releases/latest";

#[derive(Debug, Deserialize)]
struct GitHubReleaseResp {
    tag_name: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub available: bool,
}

fn parse_semver(s: &str) -> Vec<u64> {
    s.trim()
        .trim_start_matches(|c| c == 'v' || c == 'V')
        .split(|c: char| c == '.' || c == '-' || c == '+')
        .filter_map(|p| p.parse::<u64>().ok())
        .collect()
}

pub async fn check_update(current: &str) -> Result<UpdateInfo, anyhow::Error> {
    let c = http_short()?;
    let r = c
        .get(RELEASES_LATEST_URL)
        .header("User-Agent", "CerberusDesktop")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?;
    if !r.status().is_success() {
        return Err(anyhow::anyhow!("GitHub API returned {}", r.status()));
    }
    let body = r.json::<GitHubReleaseResp>().await?;
    let latest = body.tag_name.trim_start_matches(|c| c == 'v' || c == 'V').to_string();
    let available = parse_semver(&latest) > parse_semver(current);
    Ok(UpdateInfo {
        current: current.to_string(),
        latest,
        available,
    })
}

// ─── Cloud: server-side model allowlist ────────────────────────────────────

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AllowedModel {
    pub id: String,
    pub description: String,
    pub quants: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiModelEntry {
    id: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    quants: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiModelsResp {
    #[serde(default)]
    data: Vec<OpenAiModelEntry>,
}

/// Fetch the OpenAI-style model list from api.cerberusai.dev. The model id
/// (e.g. `Arbiter-GL9b`) is also the directory name on llm.cerberusai.dev
/// and the name we'll use for the local Ollama model.
pub async fn list_allowed(api_key: &str) -> Result<Vec<AllowedModel>, anyhow::Error> {
    let c = http()?;
    let r = c
        .get(format!("{CLOUD_API_BASE}/v1/models"))
        .header("Authorization", format!("Bearer {api_key}"))
        .send()
        .await?;
    if !r.status().is_success() {
        let status = r.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(anyhow::anyhow!("invalid API key (HTTP {status})"));
        }
        return Err(anyhow::anyhow!("models API returned status {status}"));
    }
    let body = r.json::<OpenAiModelsResp>().await?;
    Ok(body.data.into_iter().map(|m| AllowedModel {
        id: m.id,
        description: m.description,
        quants: m.quants,
    }).collect())
}

// ─── Local Ollama: status + model management ──────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
pub struct LocalStatus {
    pub running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct VersionResp {
    version: String,
}

pub async fn local_status() -> LocalStatus {
    let c = match http() {
        Ok(c) => c,
        Err(e) => return LocalStatus { running: false, version: None, error: Some(e.to_string()) },
    };
    match c.get(format!("{OLLAMA_BASE}/api/version")).send().await {
        Ok(r) if r.status().is_success() => match r.json::<VersionResp>().await {
            Ok(v) => LocalStatus { running: true, version: Some(v.version), error: None },
            Err(e) => LocalStatus { running: false, version: None, error: Some(e.to_string()) },
        },
        Ok(r) => LocalStatus {
            running: false,
            version: None,
            error: Some(format!("ollama returned {}", r.status())),
        },
        Err(e) => LocalStatus { running: false, version: None, error: Some(e.to_string()) },
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModelDetails {
    #[serde(default)]
    pub parameter_size: Option<String>,
    #[serde(default)]
    pub quantization_level: Option<String>,
    #[serde(default)]
    pub family: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModelInfo {
    pub name: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub modified_at: String,
    #[serde(default)]
    pub details: Option<ModelDetails>,
}

#[derive(Debug, Deserialize)]
struct TagsResp {
    #[serde(default)]
    models: Vec<ModelInfo>,
}

/// Models actually pulled into the user's local Ollama instance.
pub async fn list_local() -> Result<Vec<ModelInfo>, anyhow::Error> {
    let c = http()?;
    let r = c
        .get(format!("{OLLAMA_BASE}/api/tags"))
        .send()
        .await?
        .error_for_status()?
        .json::<TagsResp>()
        .await?;
    Ok(r.models)
}

#[derive(Deserialize)]
struct DirEntry {
    name: String,
    #[serde(default)]
    size: u64,
    #[serde(default, rename = "type")]
    kind: String,
}



#[derive(Debug, Serialize, Clone)]
pub struct PullProgress {
    pub status: String,
    pub completed: Option<u64>,
    pub total: Option<u64>,
    pub done: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Download the smallest GGUF for `name` from llm.cerberusai.dev and import
/// it into the user's local Ollama via `/api/create`. Progress is streamed
/// to `out`: byte-progress during download, then status messages from
/// Ollama while the model is imported.
pub async fn pull_model(
    name: String,
    quant: Option<String>,
    app_dir: std::path::PathBuf,
    out: Channel<PullProgress>,
    mut cancel: tokio::sync::watch::Receiver<bool>,
) -> Result<(), anyhow::Error> {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tokio::io::{AsyncSeekExt, AsyncWriteExt};

    let c = http()?;

    // 1. Pick the smallest .gguf in the model's directory.
    let _ = out.send(PullProgress {
        status: "looking up model".into(),
        completed: None, total: None, done: false, error: None,
    });
    let listing_url = format!("{LLM_FILES_BASE}/api/models/{name}/");
    let resp = c.get(&listing_url).send().await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let _ = out.send(PullProgress {
            status: format!("error: listing returned {status}"),
            completed: None, total: None, done: true,
            error: Some(format!("HTTP {status}")),
        });
        return Err(anyhow::anyhow!("listing HTTP {status}"));
    }
    let entries = resp.json::<Vec<DirEntry>>().await?;
    
    // Filter to .gguf files
    let mut valid_entries: Vec<DirEntry> = entries
        .into_iter()
        .filter(|e| e.kind == "file" && e.name.ends_with(".gguf"))
        .collect();

    if valid_entries.is_empty() {
        return Err(anyhow::anyhow!("no .gguf found for {name}"));
    }

    // Apply quant filter if provided
    if let Some(q) = quant {
        let q_lower = q.to_lowercase();
        let matches: Vec<DirEntry> = valid_entries
            .into_iter()
            .filter(|e| e.name.to_lowercase().contains(&q_lower))
            .collect();
            
        if matches.is_empty() {
            let msg = format!("no .gguf matching quant '{}' found for {}", q, name);
            let _ = out.send(PullProgress {
                status: format!("error: {}", msg),
                completed: None, total: None, done: true,
                error: Some(msg.clone()),
            });
            return Err(anyhow::anyhow!(msg));
        }
        valid_entries = matches;
    }

    let chosen = valid_entries
        .into_iter()
        .min_by_key(|e| e.size)
        .unwrap();

    let total = chosen.size;
    let url = format!("{LLM_FILES_BASE}/models/{name}/{}", chosen.name);

    // 2. Parallel chunked download — 8 simultaneous connections.
    let safe_name = name.replace(['/', '\\', ':'], "_");
    
    let models_dir = app_dir.join("models");
    if let Err(e) = tokio::fs::create_dir_all(&models_dir).await {
        let msg = format!("failed to create models directory: {e}");
        let _ = out.send(PullProgress {
            status: format!("error: {}", msg),
            completed: None, total: None, done: true, error: Some(msg.clone()),
        });
        return Err(anyhow::anyhow!(msg));
    }
    
    let temp_path = models_dir.join(format!("{safe_name}-{}", chosen.name));

    // Pre-allocate the file so all workers can seek+write concurrently.
    {
        let f = tokio::fs::OpenOptions::new()
            .write(true).create(true).truncate(true)
            .open(&temp_path).await?;
        f.set_len(total).await?;
    }

    const CHUNKS: u64 = 8;
    let chunk_size = (total + CHUNKS - 1) / CHUNKS;
    let completed = Arc::new(AtomicU64::new(0));
    let mut handles: Vec<tokio::task::JoinHandle<Result<(), anyhow::Error>>> = Vec::new();

    // One shared client so all 8 workers reuse TLS sessions and the connection pool.
    let chunk_client = Arc::new(
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(3600))
            .build()?
    );

    for i in 0..CHUNKS {
        let byte_start = i * chunk_size;
        if byte_start >= total { break; }
        let byte_end = ((i + 1) * chunk_size).min(total) - 1;
        let dl_url = url.clone();
        let dl_path = temp_path.clone();
        let dl_done = completed.clone();
        let mut dl_cancel = cancel.clone();
        let client = chunk_client.clone();

        handles.push(tokio::spawn(async move {
            let resp = client
                .get(&dl_url)
                .header("Range", format!("bytes={byte_start}-{byte_end}"))
                .send()
                .await?;
            let status = resp.status();
            if status.as_u16() != 206 && !status.is_success() {
                return Err(anyhow::anyhow!("chunk {i} HTTP {status}"));
            }
            let mut stream = resp.bytes_stream();
            let mut f = tokio::fs::OpenOptions::new()
                .write(true).open(&dl_path).await?;
            f.seek(std::io::SeekFrom::Start(byte_start)).await?;
            loop {
                tokio::select! {
                    biased;
                    _ = dl_cancel.changed() => {
                        if *dl_cancel.borrow() {
                            return Err(anyhow::anyhow!("cancelled"));
                        }
                    }
                    chunk = stream.next() => {
                        match chunk {
                            None => break,
                            Some(Err(e)) => return Err(e.into()),
                            Some(Ok(bytes)) => {
                                dl_done.fetch_add(bytes.len() as u64, Ordering::Relaxed);
                                f.write_all(&bytes).await?;
                            }
                        }
                    }
                }
            }
            f.flush().await?;
            Ok(())
        }));
    }

    // Report progress every 500 ms; stop on cancel or when all chunks finish.
    let mut cancelled = false;
    loop {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(500)) => {
                let current = completed.load(Ordering::Relaxed);
                let _ = out.send(PullProgress {
                    status: "downloading".into(),
                    completed: Some(current),
                    total: Some(total),
                    done: false, error: None,
                });
                if handles.iter().all(|h| h.is_finished()) { break; }
            }
            _ = cancel.changed() => {
                if *cancel.borrow() {
                    // Only treat as cancelled if chunks are still running.
                    // If all finished before this signal arrived, let the
                    // normal completion path handle the result.
                    cancelled = !handles.iter().all(|h| h.is_finished());
                    break;
                }
            }
        }
    }
    let mut errors: Vec<String> = Vec::new();
    for h in &handles { h.abort(); }
    for h in handles {
        match h.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) if e.to_string() == "cancelled" => {}
            Ok(Err(e)) => errors.push(e.to_string()),
            Err(_) => {} // aborted or panicked — ignore
        }
    }

    if cancelled || !errors.is_empty() {
        let _ = tokio::fs::remove_file(&temp_path).await;
        if cancelled {
            let _ = out.send(PullProgress {
                status: "cancelled".into(),
                completed: None, total: None, done: true, error: None,
            });
            return Ok(());
        }
        let msg = errors.join("; ");
        let _ = out.send(PullProgress {
            status: format!("error: {msg}"),
            completed: None, total: None, done: true,
            error: Some(msg.clone()),
        });
        return Err(anyhow::anyhow!("download errors: {msg}"));
    }

    let _ = out.send(PullProgress {
        status: "downloading".into(),
        completed: Some(total),
        total: Some(total),
        done: false, error: None,
    });

    // 3. Hand the GGUF to local Ollama via the CLI `ollama create` command.
    let _ = out.send(PullProgress {
        status: "importing into ollama (this may take a minute)...".into(),
        completed: None, total: None, done: false, error: None,
    });
    
    let modelfile_path = temp_path.with_extension("Modelfile");
    let path_str = temp_path.to_string_lossy().replace('\\', "/");
    
    // Ollama automatically extracts the correct chat template and stop tokens
    // directly from the GGUF file's metadata. Do not hardcode ChatML.
    let modelfile_content = format!("FROM \"{}\"\n", path_str);
    
    if let Err(e) = tokio::fs::write(&modelfile_path, modelfile_content).await {
        let msg = format!("failed to write Modelfile: {e}");
        let _ = out.send(PullProgress {
            status: format!("error: {msg}"),
            completed: None, total: None, done: true, error: Some(msg.clone()),
        });
        return Err(anyhow::anyhow!(msg));
    }

    let mut child = match tokio::process::Command::new("ollama")
        .arg("create")
        .arg(&name)
        .arg("-f")
        .arg(&modelfile_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn() 
    {
        Ok(c) => c,
        Err(e) => {
            let _ = tokio::fs::remove_file(&modelfile_path).await;
            let msg = format!("Failed to start `ollama` CLI: {e}. Is Ollama in your PATH?");
            let _ = out.send(PullProgress {
                status: format!("error: {}", msg),
                completed: None, total: None, done: true, error: Some(msg.clone()),
            });
            return Err(anyhow::anyhow!(msg));
        }
    };

    let status = child.wait().await?;
    let _ = tokio::fs::remove_file(&modelfile_path).await;

    if !status.success() {
        let msg = format!("ollama create failed with status {status}");
        let _ = out.send(PullProgress {
            status: format!("error: {}", msg),
            completed: None, total: None, done: true, error: Some(msg.clone()),
        });
        return Err(anyhow::anyhow!(msg));
    }

    let _ = out.send(PullProgress {
        status: "success".into(),
        completed: None, total: None, done: true, error: None,
    });
    Ok(())
}

// ─── Local Ollama: chat streaming ─────────────────────────────────────────

#[derive(Serialize)]
struct LocalChatOptions {
    num_ctx: u32,
    num_predict: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_batch: Option<u32>,
}

#[derive(Serialize)]
struct LocalChatReq<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    stream: bool,
    options: LocalChatOptions,
    keep_alive: &'a str,
}

#[derive(Deserialize)]
struct LocalChatLineMsg {
    #[serde(default)]
    content: String,
}

#[derive(Deserialize)]
struct LocalChatLine {
    #[serde(default)]
    message: Option<LocalChatLineMsg>,
    #[serde(default)]
    done: bool,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    eval_count: Option<u32>,
    #[serde(default)]
    eval_duration: Option<u64>,
}

/// Stream a chat completion from the user's local Ollama.
/// Output goes to `out` as `ChatStreamChunk { delta, done, error }`.
/// When `cancel_rx` fires, we drop the HTTP stream, which closes the
/// connection and immediately stops Ollama from burning CPU.
pub async fn stream_chat_local(
    model: String,
    messages: Vec<ChatMessage>,
    out: Channel<ChatStreamChunk>,
    mut cancel_rx: tokio::sync::watch::Receiver<bool>,
) -> Result<(), anyhow::Error> {
    let c = http()?;
    let body = LocalChatReq { 
        model: &model, 
        messages: &messages, 
        stream: true,
        options: LocalChatOptions { num_ctx: 2048, num_predict: 2048, num_batch: Some(512) },
        keep_alive: "10m",
    };

    let resp = c
        .post(format!("{OLLAMA_BASE}/api/chat"))
        .json(&body)
        .send()
        .await;

    let resp = match resp {
        Ok(r) => r,
        Err(e) => {
            let err = if e.is_connect() {
                "Local Ollama isn't running on 127.0.0.1:11434. Start it with `ollama serve`.".to_string()
            } else {
                e.to_string()
            };
            let _ = out.send(ChatStreamChunk {
                delta: String::new(), done: true, error: Some(err.clone()), ttft_ms: None, tps: None,
            });
            return Err(anyhow::anyhow!(err));
        }
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let err = format!("ollama returned {status}: {text}");
        let _ = out.send(ChatStreamChunk {
            delta: String::new(), done: true, error: Some(err.clone()), ttft_ms: None, tps: None,
        });
        return Err(anyhow::anyhow!(err));
    }

    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::with_capacity(4096);

    // 5-minute inactivity timeout: if no data arrives for this long, bail out
    // so the UI doesn't appear frozen forever.
    let inactivity_timeout = Duration::from_secs(300);

    let start_time = std::time::Instant::now();
    let mut ttft_ms = None;

    loop {
        // Race: either we get the next chunk, or the user presses stop.
        let chunk = tokio::select! {
            biased;
            _ = cancel_rx.changed() => {
                // User pressed stop — drop the stream to close the HTTP
                // connection so Ollama immediately stops generating.
                drop(stream);
                let _ = out.send(ChatStreamChunk {
                    delta: String::new(),
                    done: true,
                    error: None,
                    ttft_ms: None, tps: None,
                });
                return Ok(());
            }
            c = tokio::time::timeout(inactivity_timeout, stream.next()) => c,
        };

        match chunk {
            Err(_elapsed) => {
                // No data from Ollama for 5 minutes — report and exit.
                let _ = out.send(ChatStreamChunk {
                    delta: String::new(),
                    done: true,
                    error: Some("Ollama stopped responding (timeout). Try sending your message again.".into()),
                    ttft_ms: None, tps: None,
                });
                return Ok(());
            }
            Ok(None) => break, // stream ended
            Ok(Some(Err(e))) => return Err(e.into()),
            Ok(Some(Ok(bytes))) => {
                buf.extend_from_slice(&bytes);
                loop {
                    let Some(nl) = buf.iter().position(|b| *b == b'\n') else { break };
                    let line: Vec<u8> = buf.drain(..=nl).collect();
                    // Strip trailing \n and \r (handle both LF and CRLF)
                    let mut end = line.len();
                    while end > 0 && (line[end - 1] == b'\n' || line[end - 1] == b'\r') {
                        end -= 1;
                    }
                    let line = &line[..end];
                    if line.is_empty() { continue; }
                    match serde_json::from_slice::<LocalChatLine>(line) {
                        Ok(p) => {
                            if let Some(err) = p.error {
                                let _ = out.send(ChatStreamChunk {
                                    delta: String::new(), done: true, error: Some(err),
                                    ttft_ms: None, tps: None,
                                });
                                return Ok(());
                            }
                            let delta = p.message.map(|m| m.content).unwrap_or_default();
                            if ttft_ms.is_none() && !delta.is_empty() {
                                ttft_ms = Some(start_time.elapsed().as_millis() as u64);
                            }
                            
                            let mut chunk_tps = None;
                            if p.done {
                                if let (Some(count), Some(dur)) = (p.eval_count, p.eval_duration) {
                                    if dur > 0 {
                                        chunk_tps = Some((count as f64) / ((dur as f64) / 1_000_000_000.0));
                                    }
                                }
                            }
                            
                            let _ = out.send(ChatStreamChunk {
                                delta, done: p.done, error: None,
                                ttft_ms,
                                tps: chunk_tps,
                            });
                            if p.done { return Ok(()); }
                        }
                        Err(e) => log::warn!("ollama chat: skipping unparseable line: {e}"),
                    }
                }
            }
        }
    }

    let _ = out.send(ChatStreamChunk {
        delta: String::new(), done: true, error: None, ttft_ms: None, tps: None,
    });
    Ok(())
}

// ─── Local Filesystem: GGUF File Management ──────────────────────────────

#[derive(Debug, Serialize, Clone)]
pub struct GgufFile {
    pub name: String,
    pub size: u64,
}

fn find_ggufs_sync(dir: &std::path::Path, base_dir: &std::path::Path, files: &mut Vec<GgufFile>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("gguf") {
                if let Ok(meta) = entry.metadata() {
                    let rel_path = path.strip_prefix(base_dir).unwrap_or(&path);
                    let name = rel_path.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/");
                    files.push(GgufFile {
                        name,
                        size: meta.len(),
                    });
                }
            } else if path.is_dir() {
                find_ggufs_sync(&path, base_dir, files);
            }
        }
    }
}

/// List all downloaded `.gguf` files recursively in the `models` directory.
pub async fn list_local_ggufs(app_dir: std::path::PathBuf) -> Result<Vec<GgufFile>, anyhow::Error> {
    let models_dir = app_dir.join("models");
    
    if !models_dir.exists() {
        return Ok(Vec::new());
    }

    let files = tokio::task::spawn_blocking(move || {
        let mut files = Vec::new();
        find_ggufs_sync(&models_dir, &models_dir, &mut files);
        files
    }).await?;
    
    Ok(files)
}

/// Securely delete a `.gguf` file from the `models` directory.
pub async fn delete_local_gguf(filename: String, app_dir: std::path::PathBuf) -> Result<(), anyhow::Error> {
    // Only allow deleting .gguf files to prevent arbitrary file deletion, and block directory traversal
    if !filename.ends_with(".gguf") || filename.contains("..") {
        return Err(anyhow::anyhow!("Invalid filename"));
    }
    
    let models_dir = app_dir.join("models");
    let file_path = models_dir.join(&filename);
    
    if file_path.exists() {
        tokio::fs::remove_file(file_path).await?;
        
        // Try to clean up empty parent directories if any
        if let Some(parent) = std::path::Path::new(&filename).parent() {
            let _ = tokio::fs::remove_dir(models_dir.join(parent)).await; // Will fail silently if not empty, which is intended
        }
    } else {
        return Err(anyhow::anyhow!("File not found"));
    }
    
    Ok(())
}

/// Safely move a `.gguf` file to an arbitrary location on the hard drive.
pub async fn move_local_gguf(filename: String, destination: String, app_dir: std::path::PathBuf) -> Result<(), anyhow::Error> {
    if !filename.ends_with(".gguf") || filename.contains("..") {
        return Err(anyhow::anyhow!("Invalid source filename"));
    }

    let models_dir = app_dir.join("models");
    let source_path = models_dir.join(&filename);

    if !source_path.exists() {
        return Err(anyhow::anyhow!("Source file not found"));
    }

    // Attempt to copy the file to the new destination.
    // If successful, remove the original file. This handles cross-drive moves securely.
    tokio::fs::copy(&source_path, &destination).await?;
    tokio::fs::remove_file(&source_path).await?;
    
    // Try to clean up empty parent directories if any
    if let Some(parent) = std::path::Path::new(&filename).parent() {
        let _ = tokio::fs::remove_dir(models_dir.join(parent)).await; // Will fail silently if not empty, which is intended
    }
    
    Ok(())
}

/// Import an arbitrary `.gguf` file from the user's filesystem into Ollama.
/// The file is moved into the local models directory, a Modelfile is created,
/// and `ollama create <model_name> -f <Modelfile>` is run to register it.
pub async fn import_local_gguf(
    source_path: String,
    model_name: String,
    app_dir: std::path::PathBuf,
) -> Result<String, anyhow::Error> {
    let src = std::path::Path::new(&source_path);
    if !src.exists() {
        return Err(anyhow::anyhow!("File not found: {}", source_path));
    }
    if src.extension().and_then(|s| s.to_str()) != Some("gguf") {
        return Err(anyhow::anyhow!("Only .gguf files can be imported"));
    }

    let models_dir = app_dir.join("models");
    tokio::fs::create_dir_all(&models_dir).await?;

    let filename = src.file_name().unwrap_or_default();
    let dest_path = models_dir.join(filename);

    // Move the file into the managed models directory if it's not already there
    if src.canonicalize().unwrap_or_default() != dest_path.canonicalize().unwrap_or_default() {
        if let Err(_) = tokio::fs::rename(src, &dest_path).await {
            // Fallback to copy+delete if rename fails (e.g., cross-drive move)
            tokio::fs::copy(src, &dest_path).await?;
            tokio::fs::remove_file(src).await?;
        }
    }

    // Normalise the path for the Modelfile
    let path_str = dest_path.to_string_lossy().replace('\\', "/");

    let modelfile_path = models_dir.join(format!("{}.Modelfile", model_name));

    // Ollama automatically extracts the correct chat template and stop tokens
    // directly from the GGUF file's metadata. Do not hardcode ChatML.
    let modelfile_content = format!("FROM \"{}\"\n", path_str);

    tokio::fs::write(&modelfile_path, &modelfile_content).await?;

    let output = tokio::process::Command::new("ollama")
        .arg("create")
        .arg(&model_name)
        .arg("-f")
        .arg(&modelfile_path)
        .output()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to start `ollama` CLI: {e}. Is Ollama in your PATH?"))?;

    let _ = tokio::fs::remove_file(&modelfile_path).await;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("ollama create failed: {}", stderr));
    }

    Ok(format!("Successfully imported {} as '{}'", source_path, model_name))
}

/// Activate a `.gguf` file that is already stored in the managed models directory.
pub async fn activate_managed_gguf(
    filename: String,
    model_name: String,
    app_dir: std::path::PathBuf,
) -> Result<String, anyhow::Error> {
    if !filename.ends_with(".gguf") || filename.contains("..") {
        return Err(anyhow::anyhow!("Invalid filename"));
    }

    let models_dir = app_dir.join("models");
    let dest_path = models_dir.join(&filename);

    if !dest_path.exists() {
        return Err(anyhow::anyhow!("File not found in managed storage"));
    }

    let path_str = dest_path.to_string_lossy().replace('\\', "/");
    let modelfile_path = models_dir.join(format!("{}.Modelfile", model_name));
    let modelfile_content = format!("FROM \"{}\"\n", path_str);

    tokio::fs::write(&modelfile_path, &modelfile_content).await?;

    let output = tokio::process::Command::new("ollama")
        .arg("create")
        .arg(&model_name)
        .arg("-f")
        .arg(&modelfile_path)
        .output()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to start `ollama` CLI: {e}. Is Ollama in your PATH?"))?;

    let _ = tokio::fs::remove_file(&modelfile_path).await;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("ollama create failed: {}", stderr));
    }

    Ok(format!("Successfully activated {} as '{}'", filename, model_name))
}

/// Delete a model from the local Ollama instance via the HTTP API.
pub async fn delete_ollama_model(name: &str) -> Result<(), anyhow::Error> {
    let c = http()?;
    let resp = c
        .delete(format!("{OLLAMA_BASE}/api/delete"))
        .json(&serde_json::json!({ "name": name }))
        .send()
        .await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("ollama delete returned {status}: {text}"));
    }
    Ok(())
}
