use crate::{ChatMessage, ChatStreamChunk};
use std::path::PathBuf;
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
        app_dir: PathBuf,
    ) -> Result<(), anyhow::Error> {
        let is_gguf_file = model.to_lowercase().ends_with(".gguf") || model.contains('/') || model.contains('\\');

        if is_gguf_file || provider == "llama_cpp" {
            let model_path = if std::path::Path::new(&model).is_absolute() {
                PathBuf::from(&model)
            } else {
                app_dir.join("models").join(&model)
            };
            return crate::llama_engine::stream_chat_llama(
                model_path,
                messages,
                tools,
                on_event,
                cancel,
                app_dir,
            )
            .await;
        }

        match provider {
            "ollama" => {
                // Try Ollama first if daemon is active
                let local = crate::model_manager::local_status().await;
                if local.running {
                    crate::model_manager::stream_chat_local(model, messages, tools, on_event, cancel).await
                } else {
                    // Fall back to llama_engine if user has a matching .gguf file in app_dir/models
                    let gguf_name = format!("{}.gguf", model.to_lowercase());
                    let candidate = app_dir.join("models").join(&gguf_name);
                    if tokio::fs::metadata(&candidate).await.is_ok() {
                        crate::llama_engine::stream_chat_llama(
                            candidate,
                            messages,
                            tools,
                            on_event,
                            cancel,
                            app_dir,
                        )
                        .await
                    } else {
                        Err(anyhow::anyhow!(
                            "Ollama daemon is not running and no local `{}` GGUF file was found in models.",
                            model
                        ))
                    }
                }
            }
            "llama_cpp" => {
                let candidate = app_dir.join("models").join(&model);
                crate::llama_engine::stream_chat_llama(
                    candidate,
                    messages,
                    tools,
                    on_event,
                    cancel,
                    app_dir,
                )
                .await
            }
            _ => Err(anyhow::anyhow!("Unsupported provider: {}", provider)),
        }
    }
}
