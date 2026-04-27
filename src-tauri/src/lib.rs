use serde::{Deserialize, Serialize};
use sysinfo::System;
use tauri::ipc::Channel;

mod hardware;
mod ollama;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ChatStreamChunk {
    pub delta: String,
    pub done: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GpuInfo {
    pub name: String,
    pub vendor: String,
    pub vram_mb: Option<u64>,
    pub driver: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct HardwareInfo {
    pub os: String,
    pub os_version: String,
    pub cpu_brand: String,
    pub cpu_cores: usize,
    pub total_ram_mb: u64,
    pub gpus: Vec<GpuInfo>,
}

// ─── Cloud (api.cerberusai.dev) ───────────────────────────────────────────
// Only the API-key gate hits the cloud. Inference stays local.

/// Verify the user's Cerberus API key against api.cerberusai.dev.
#[tauri::command]
async fn check_api(api_key: String) -> Result<String, String> {
    ollama::verify_key(&api_key).await.map_err(|e| e.to_string())
}

/// Fetch the server-side allowlist of models from llm.cerberusai.dev.
/// Returned ids are qualified for `ollama pull`.
#[tauri::command]
async fn list_allowed_models(api_key: String) -> Result<Vec<String>, String> {
    ollama::list_allowed(&api_key).await.map_err(|e| e.to_string())
}

/// Compare the bundled app version against the latest GitHub release.
#[tauri::command]
async fn check_for_update() -> Result<ollama::UpdateInfo, String> {
    ollama::check_update(env!("CARGO_PKG_VERSION"))
        .await
        .map_err(|e| e.to_string())
}

// ─── Local Ollama ─────────────────────────────────────────────────────────

/// Returns local Ollama daemon status (running + version, or error).
#[tauri::command]
async fn check_local_ollama() -> ollama::LocalStatus {
    ollama::local_status().await
}

/// List models actually pulled into the user's local Ollama.
#[tauri::command]
async fn list_models() -> Result<Vec<ollama::ModelInfo>, String> {
    ollama::list_local().await.map_err(|e| e.to_string())
}

/// Stream `ollama pull <name>` progress to the frontend.
#[tauri::command]
async fn pull_model(
    name: String,
    on_event: Channel<ollama::PullProgress>,
) -> Result<(), String> {
    ollama::pull_model(name, on_event)
        .await
        .map_err(|e| e.to_string())
}

/// Stream a chat completion from the user's local Ollama.
#[tauri::command]
async fn chat_stream(
    model: String,
    messages: Vec<ChatMessage>,
    on_event: Channel<ChatStreamChunk>,
) -> Result<(), String> {
    ollama::stream_chat_local(model, messages, on_event)
        .await
        .map_err(|e| e.to_string())
}

// ─── Hardware ─────────────────────────────────────────────────────────────

#[tauri::command]
fn detect_hardware() -> HardwareInfo {
    let mut sys = System::new_all();
    sys.refresh_all();

    let cpu_brand = sys
        .cpus()
        .first()
        .map(|c| c.brand().trim().to_string())
        .unwrap_or_else(|| "Unknown CPU".into());
    let cpu_cores = sys.physical_core_count().unwrap_or(sys.cpus().len());

    HardwareInfo {
        os: System::name().unwrap_or_else(|| "unknown".into()),
        os_version: System::os_version().unwrap_or_else(|| "unknown".into()),
        cpu_brand,
        cpu_cores,
        total_ram_mb: sys.total_memory() / 1024 / 1024,
        gpus: hardware::detect_gpus(),
    }
}

#[tauri::command]
async fn update_app(force: Option<bool>) -> Result<(), String> {
    // Re-check before spawning: if the GitHub `latest` release isn't actually newer
    // than the bundled version, refuse — otherwise the bootstrapper would happily
    // reinstall an older artifact and downgrade the user.
    if !force.unwrap_or(false) {
        let info = ollama::check_update(env!("CARGO_PKG_VERSION"))
            .await
            .map_err(|e| e.to_string())?;
        if !info.available {
            return Err(format!(
                "no update available (installed v{}, latest v{})",
                info.current, info.latest
            ));
        }
    }
    std::process::Command::new("powershell")
        .arg("-Command")
        .arg("irm https://cerberusai.dev/get | iex")
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            check_api,
            list_allowed_models,
            check_for_update,
            check_local_ollama,
            list_models,
            pull_model,
            chat_stream,
            detect_hardware,
            update_app,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
