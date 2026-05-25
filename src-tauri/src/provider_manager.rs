//! Provider Manager abstracts API calls (like OpenAI, Anthropic, or Local Ollama)
//! and dynamically routes chat requests.

use crate::{ChatMessage, ChatStreamChunk};
use tauri::ipc::Channel;

pub struct ProviderManager;

impl ProviderManager {
    pub async fn route_chat(
        provider: &str,
        model: String,
        messages: Vec<ChatMessage>,
        tools: Option<Vec<crate::model_manager::OllamaToolDef>>,
        on_event: Channel<ChatStreamChunk>,
        cancel: tokio::sync::watch::Receiver<bool>,
    ) -> Result<(), anyhow::Error> {
        match provider {
            "ollama" => crate::model_manager::stream_chat_local(model, messages, tools, on_event, cancel).await,
            "airllm" => {
                let client = reqwest::Client::new();
                let payload = serde_json::json!({
                    "model": model,
                    "messages": messages,
                    "temperature": 0.7,
                    "max_tokens": 1024
                });
                let resp = client.post("http://127.0.0.1:11435/v1/chat/completions")
                    .json(&payload)
                    .send()
                    .await?;

                let json: serde_json::Value = resp.json().await?;
                let content = json["choices"][0]["message"]["content"].as_str().unwrap_or("");

                let _ = on_event.send(ChatStreamChunk {
                    delta: content.to_string(),
                    done: true,
                    error: None,
                    ttft_ms: None,
                    tps: None,
                    tool_calls: None,
                });

                Ok(())
            }
            _ => Err(anyhow::anyhow!("Unsupported provider: {}", provider)),
        }
    }
}
