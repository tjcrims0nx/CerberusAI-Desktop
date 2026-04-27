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
pub const REGISTRY_HOST: &str = "registry.cerberusai.dev";
const REGISTRY_DEFAULT_NAMESPACE: &str = "library";
const OLLAMA_BASE: &str = "http://127.0.0.1:11434";

/// Turn a model id from `/v1/models` into the fully-qualified name that
/// `ollama pull` expects. If the id already contains `/` it's assumed to be
/// pre-qualified and is returned as-is (with `:latest` appended if no tag).
pub fn qualify_model_id(id: &str) -> String {
    let with_tag = if id.contains(':') {
        id.to_string()
    } else {
        format!("{id}:latest")
    };
    if with_tag.contains('/') {
        with_tag
    } else {
        format!("{REGISTRY_HOST}/{REGISTRY_DEFAULT_NAMESPACE}/{with_tag}")
    }
}

fn http() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(60 * 60)) // long for first model pull
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
    let c = http()?;
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

#[derive(Debug, Deserialize)]
struct OpenAiModelEntry {
    id: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiModelsResp {
    #[serde(default)]
    data: Vec<OpenAiModelEntry>,
}

/// Fetch the OpenAI-style model list from llm.cerberusai.dev and return each
/// model id qualified for `ollama pull`.
pub async fn list_allowed(api_key: &str) -> Result<Vec<String>, anyhow::Error> {
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
    Ok(body.data.into_iter().map(|m| qualify_model_id(&m.id)).collect())
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

#[derive(Serialize)]
struct PullReq<'a> {
    name: &'a str,
    stream: bool,
}

#[derive(Deserialize)]
struct PullStatus {
    #[serde(default)]
    status: String,
    #[serde(default)]
    completed: Option<u64>,
    #[serde(default)]
    total: Option<u64>,
    #[serde(default)]
    error: Option<String>,
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

/// Stream `ollama pull <name>` progress to the frontend.
pub async fn pull_model(
    name: String,
    out: Channel<PullProgress>,
) -> Result<(), anyhow::Error> {
    let c = http()?;
    let body = PullReq { name: &name, stream: true };
    let resp = c
        .post(format!("{OLLAMA_BASE}/api/pull"))
        .json(&body)
        .send()
        .await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let _ = out.send(PullProgress {
            status: format!("error: ollama returned {status}: {text}"),
            completed: None, total: None, done: true,
            error: Some(format!("HTTP {status}")),
        });
        return Err(anyhow::anyhow!("ollama pull HTTP {status}"));
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
            match serde_json::from_slice::<PullStatus>(line) {
                Ok(p) => {
                    let done = p.status == "success" || p.error.is_some();
                    let _ = out.send(PullProgress {
                        status: p.status,
                        completed: p.completed,
                        total: p.total,
                        done,
                        error: p.error,
                    });
                    if done { return Ok(()); }
                }
                Err(e) => log::warn!("ollama pull: skipping unparseable line: {e}"),
            }
        }
    }

    let _ = out.send(PullProgress {
        status: "complete".into(),
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

#[cfg(test)]
mod tests {
    use super::qualify_model_id;

    #[test]
    fn short_id_gets_registry_namespace_and_latest() {
        assert_eq!(
            qualify_model_id("cerberus-4b-v2-abliterated"),
            "registry.cerberusai.dev/library/cerberus-4b-v2-abliterated:latest"
        );
    }

    #[test]
    fn short_id_with_tag_keeps_tag() {
        assert_eq!(
            qualify_model_id("cerberus-4b:q4"),
            "registry.cerberusai.dev/library/cerberus-4b:q4"
        );
    }

    #[test]
    fn fully_qualified_id_passes_through() {
        let id = "registry.cerberusai.dev/cerberusai/cerberus-4b:latest";
        assert_eq!(qualify_model_id(id), id);
    }

    #[test]
    fn qualified_without_tag_gets_latest() {
        assert_eq!(
            qualify_model_id("registry.cerberusai.dev/cerberusai/cerberus-4b"),
            "registry.cerberusai.dev/cerberusai/cerberus-4b:latest"
        );
    }
}
