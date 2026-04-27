use serde::{Deserialize, Serialize};
use sysinfo::System;
use tauri::ipc::Channel;

mod ollama;
mod hardware;

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

#[tauri::command]
async fn check_ollama() -> Result<String, String> {
    ollama::version().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_models() -> Result<Vec<ollama::ModelInfo>, String> {
    ollama::list().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn chat_stream(
    model: String,
    messages: Vec<ChatMessage>,
    on_event: Channel<ChatStreamChunk>,
) -> Result<(), String> {
    ollama::stream_chat(model, messages, on_event)
        .await
        .map_err(|e| e.to_string())
}

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            check_ollama,
            list_models,
            chat_stream,
            detect_hardware,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
