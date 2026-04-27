use crate::{ChatMessage, ChatStreamChunk};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tauri::ipc::Channel;

const OLLAMA_BASE: &str = "http://127.0.0.1:11434";

fn client() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(60 * 30))
        .build()
}

#[derive(Debug, Deserialize)]
struct VersionResp {
    version: String,
}

pub async fn version() -> Result<String, anyhow::Error> {
    let c = client()?;
    let r = c
        .get(format!("{OLLAMA_BASE}/api/version"))
        .send()
        .await?
        .error_for_status()?
        .json::<VersionResp>()
        .await?;
    Ok(r.version)
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
    models: Vec<ModelInfo>,
}

pub async fn list() -> Result<Vec<ModelInfo>, anyhow::Error> {
    let c = client()?;
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
struct ChatReq<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    stream: bool,
}

#[derive(Deserialize)]
struct ChatStreamLine {
    #[serde(default)]
    message: Option<ChatStreamLineMsg>,
    #[serde(default)]
    done: bool,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Deserialize)]
struct ChatStreamLineMsg {
    #[serde(default)]
    content: String,
}

pub async fn stream_chat(
    model: String,
    messages: Vec<ChatMessage>,
    out: Channel<ChatStreamChunk>,
) -> Result<(), anyhow::Error> {
    let c = client()?;
    let body = ChatReq {
        model: &model,
        messages: &messages,
        stream: true,
    };

    let resp = c
        .post(format!("{OLLAMA_BASE}/api/chat"))
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        let err = format!("ollama returned {status}: {text}");
        let _ = out.send(ChatStreamChunk {
            delta: String::new(),
            done: true,
            error: Some(err.clone()),
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
            let line = &line[..line.len() - 1];
            if line.is_empty() {
                continue;
            }
            let parsed: Result<ChatStreamLine, _> = serde_json::from_slice(line);
            match parsed {
                Ok(p) => {
                    if let Some(err) = p.error {
                        let _ = out.send(ChatStreamChunk {
                            delta: String::new(),
                            done: true,
                            error: Some(err),
                        });
                        return Ok(());
                    }
                    let delta = p.message.map(|m| m.content).unwrap_or_default();
                    let _ = out.send(ChatStreamChunk {
                        delta,
                        done: p.done,
                        error: None,
                    });
                    if p.done {
                        return Ok(());
                    }
                }
                Err(e) => {
                    log::warn!("ollama: skipping unparseable line: {e}");
                }
            }
        }
    }

    let _ = out.send(ChatStreamChunk {
        delta: String::new(),
        done: true,
        error: None,
    });
    Ok(())
}
