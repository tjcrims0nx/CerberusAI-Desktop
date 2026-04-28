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

fn http() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60 * 60)) // long for first model pull
        .build()
}

fn http_short() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .build()
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
    s.trim().trim_start_matches('v')
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
    let latest = body.tag_name.trim_start_matches('v').to_string();
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

#[derive(Deserialize)]
struct OllamaStatusLine {
    #[serde(default)]
    status: String,
    #[serde(default)]
    completed: Option<u64>,
    #[serde(default)]
    total: Option<u64>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Serialize)]
struct CreateReq<'a> {
    name: &'a str,
    modelfile: &'a str,
    stream: bool,
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
    out: Channel<PullProgress>,
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
    let chosen = entries
        .into_iter()
        .filter(|e| e.kind == "file" && e.name.ends_with(".gguf"))
        .min_by_key(|e| e.size)
        .ok_or_else(|| anyhow::anyhow!("no .gguf found for {name}"))?;

    let total = chosen.size;
    let url = format!("{LLM_FILES_BASE}/models/{name}/{}", chosen.name);

    // 2. Parallel chunked download — 8 simultaneous connections.
    let safe_name = name.replace(['/', '\\', ':'], "_");
    let temp_path = std::env::temp_dir().join(format!("cerberus-{safe_name}-{}", chosen.name));

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

    for i in 0..CHUNKS {
        let byte_start = i * chunk_size;
        if byte_start >= total { break; }
        let byte_end = ((i + 1) * chunk_size).min(total) - 1;
        let dl_url = url.clone();
        let dl_path = temp_path.clone();
        let dl_done = completed.clone();

        handles.push(tokio::spawn(async move {
            let client = reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(15))
                .timeout(Duration::from_secs(3600))
                .build()?;
            let resp = client
                .get(&dl_url)
                .header("Range", format!("bytes={byte_start}-{byte_end}"))
                .send()
                .await?;
            let status = resp.status();
            // 206 Partial Content is the success code for range requests.
            if status.as_u16() != 206 && !status.is_success() {
                return Err(anyhow::anyhow!("chunk {i} HTTP {status}"));
            }
            let mut stream = resp.bytes_stream();
            // Each task opens its own file handle so seeks don't interfere.
            let mut f = tokio::fs::OpenOptions::new()
                .write(true).open(&dl_path).await?;
            f.seek(std::io::SeekFrom::Start(byte_start)).await?;
            while let Some(chunk) = stream.next().await {
                let bytes = chunk?;
                dl_done.fetch_add(bytes.len() as u64, Ordering::Relaxed);
                f.write_all(&bytes).await?;
            }
            f.flush().await?;
            Ok(())
        }));
    }

    // Report progress every 500 ms until all chunks finish.
    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let current = completed.load(Ordering::Relaxed);
        let _ = out.send(PullProgress {
            status: "downloading".into(),
            completed: Some(current),
            total: Some(total),
            done: false, error: None,
        });
        if handles.iter().all(|h| h.is_finished()) { break; }
    }

    let mut errors: Vec<String> = Vec::new();
    for h in handles {
        match h.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => errors.push(e.to_string()),
            Err(e) => errors.push(format!("task panic: {e}")),
        }
    }
    if !errors.is_empty() {
        let msg = errors.join("; ");
        let _ = tokio::fs::remove_file(&temp_path).await;
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

    // 3. Hand the GGUF to local Ollama via /api/create.
    let _ = out.send(PullProgress {
        status: "importing into ollama".into(),
        completed: None, total: None, done: false, error: None,
    });
    let modelfile = format!("FROM {}", temp_path.display());
    let body = CreateReq { name: &name, modelfile: &modelfile, stream: true };
    let resp = c
        .post(format!("{OLLAMA_BASE}/api/create"))
        .json(&body)
        .send()
        .await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let _ = tokio::fs::remove_file(&temp_path).await;
        let _ = out.send(PullProgress {
            status: format!("error: ollama returned {status}: {text}"),
            completed: None, total: None, done: true,
            error: Some(format!("HTTP {status}")),
        });
        return Err(anyhow::anyhow!("ollama create HTTP {status}"));
    }

    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::with_capacity(4096);
    while let Some(chunk) = stream.next().await {
        let bytes = chunk?;
        buf.extend_from_slice(&bytes);
        loop {
            let Some(nl) = buf.iter().position(|b| *b == b'\n') else { break };
            let line: Vec<u8> = buf.drain(..=nl).collect();
            let line = &line[..line.len().saturating_sub(1)];
            if line.is_empty() { continue; }
            match serde_json::from_slice::<OllamaStatusLine>(line) {
                Ok(p) => {
                    let done_now = p.status == "success" || p.error.is_some();
                    let _ = out.send(PullProgress {
                        status: p.status,
                        completed: p.completed,
                        total: p.total,
                        done: done_now,
                        error: p.error,
                    });
                    if done_now {
                        let _ = tokio::fs::remove_file(&temp_path).await;
                        return Ok(());
                    }
                }
                Err(e) => log::warn!("ollama create: skipping unparseable line: {e}"),
            }
        }
    }

    let _ = tokio::fs::remove_file(&temp_path).await;
    let _ = out.send(PullProgress {
        status: "success".into(),
        completed: None, total: None, done: true, error: None,
    });
    Ok(())
}

// ─── Local Ollama: chat streaming ─────────────────────────────────────────

#[derive(Serialize)]
struct LocalChatReq<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    stream: bool,
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
}

/// Stream a chat completion from the user's local Ollama.
/// Output goes to `out` as `ChatStreamChunk { delta, done, error }`.
pub async fn stream_chat_local(
    model: String,
    messages: Vec<ChatMessage>,
    out: Channel<ChatStreamChunk>,
) -> Result<(), anyhow::Error> {
    let c = http()?;
    let body = LocalChatReq { model: &model, messages: &messages, stream: true };

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
                delta: String::new(), done: true, error: Some(err.clone()),
            });
            return Err(anyhow::anyhow!(err));
        }
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let err = format!("ollama returned {status}: {text}");
        let _ = out.send(ChatStreamChunk {
            delta: String::new(), done: true, error: Some(err.clone()),
        });
        return Err(anyhow::anyhow!(err));
    }

    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::with_capacity(4096);

    while let Some(chunk) = stream.next().await {
        let bytes = chunk?;
        buf.extend_from_slice(&bytes);
        loop {
            let Some(nl) = buf.iter().position(|b| *b == b'\n') else { break };
            let line: Vec<u8> = buf.drain(..=nl).collect();
            let line = &line[..line.len().saturating_sub(1)];
            if line.is_empty() { continue; }
            match serde_json::from_slice::<LocalChatLine>(line) {
                Ok(p) => {
                    if let Some(err) = p.error {
                        let _ = out.send(ChatStreamChunk {
                            delta: String::new(), done: true, error: Some(err),
                        });
                        return Ok(());
                    }
                    let delta = p.message.map(|m| m.content).unwrap_or_default();
                    let _ = out.send(ChatStreamChunk {
                        delta, done: p.done, error: None,
                    });
                    if p.done { return Ok(()); }
                }
                Err(e) => log::warn!("ollama chat: skipping unparseable line: {e}"),
            }
        }
    }

    let _ = out.send(ChatStreamChunk {
        delta: String::new(), done: true, error: None,
    });
    Ok(())
}
