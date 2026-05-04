use serde::{Deserialize, Serialize};
use sysinfo::System;
use tauri::{ipc::Channel, Manager};
use tokio::sync::{watch, Mutex};

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttft_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tps: Option<f64>,
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
async fn list_allowed_models(api_key: String) -> Result<Vec<ollama::AllowedModel>, String> {
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

struct PullState(Mutex<Option<watch::Sender<bool>>>);
struct ChatState(Mutex<Option<watch::Sender<bool>>>);

/// Stream `ollama pull <name>` progress to the frontend.
#[tauri::command]
async fn pull_model(
    name: String,
    quant: Option<String>,
    on_event: Channel<ollama::PullProgress>,
    state: tauri::State<'_, PullState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let (tx, rx) = watch::channel(false);
    *state.0.lock().await = Some(tx);
    let app_dir = app.path().home_dir().map(|p| p.join(".CerberusAI")).unwrap_or_else(|_| std::path::PathBuf::from("."));
    let result = ollama::pull_model(name, quant, app_dir, on_event, rx).await;
    *state.0.lock().await = None;
    result.map_err(|e| e.to_string())
}

/// Cancel an in-progress model download.
#[tauri::command]
async fn cancel_pull(state: tauri::State<'_, PullState>) -> Result<(), String> {
    if let Some(tx) = state.0.lock().await.take() {
        let _ = tx.send(true);
    }
    Ok(())
}

/// Stream a chat completion from the user's local Ollama.
#[tauri::command]
async fn chat_stream(
    model: String,
    messages: Vec<ChatMessage>,
    on_event: Channel<ChatStreamChunk>,
    state: tauri::State<'_, ChatState>,
) -> Result<(), String> {
    let (tx, rx) = watch::channel(false);
    *state.0.lock().await = Some(tx);
    let result = ollama::stream_chat_local(model, messages, on_event, rx).await;
    *state.0.lock().await = None;
    result.map_err(|e| e.to_string())
}

/// Stop an ongoing chat completion by aborting the HTTP stream.
#[tauri::command]
async fn cancel_chat(state: tauri::State<'_, ChatState>) -> Result<(), String> {
    if let Some(tx) = state.0.lock().await.take() {
        let _ = tx.send(true);
    }
    Ok(())
}

/// List all downloaded raw `.gguf` files kept in the local models folder.
#[tauri::command]
async fn list_local_ggufs(app: tauri::AppHandle) -> Result<Vec<ollama::GgufFile>, String> {
    let app_dir = app.path().home_dir().map(|p| p.join(".CerberusAI")).unwrap_or_else(|_| std::path::PathBuf::from("."));
    ollama::list_local_ggufs(app_dir).await.map_err(|e| e.to_string())
}

/// Delete a specific downloaded `.gguf` file to free up disk space.
#[tauri::command]
async fn delete_local_gguf(filename: String, app: tauri::AppHandle) -> Result<(), String> {
    let app_dir = app.path().home_dir().map(|p| p.join(".CerberusAI")).unwrap_or_else(|_| std::path::PathBuf::from("."));
    ollama::delete_local_gguf(filename, app_dir).await.map_err(|e| e.to_string())
}

/// Safely move a `.gguf` file to an arbitrary location.
#[tauri::command]
async fn move_local_gguf(filename: String, destination: String, app: tauri::AppHandle) -> Result<(), String> {
    let app_dir = app.path().home_dir().map(|p| p.join(".CerberusAI")).unwrap_or_else(|_| std::path::PathBuf::from("."));
    ollama::move_local_gguf(filename, destination, app_dir).await.map_err(|e| e.to_string())
}

/// Import a `.gguf` file from anywhere on disk into the local Ollama instance.
#[tauri::command]
async fn import_local_gguf(source_path: String, model_name: String, app: tauri::AppHandle) -> Result<String, String> {
    let app_dir = app.path().home_dir().map(|p| p.join(".CerberusAI")).unwrap_or_else(|_| std::path::PathBuf::from("."));
    ollama::import_local_gguf(source_path, model_name, app_dir).await.map_err(|e| e.to_string())
}

/// Activate a `.gguf` file that is already inside the local managed models folder.
#[tauri::command]
async fn activate_managed_gguf(filename: String, model_name: String, app: tauri::AppHandle) -> Result<String, String> {
    let app_dir = app.path().home_dir().map(|p| p.join(".CerberusAI")).unwrap_or_else(|_| std::path::PathBuf::from("."));
    ollama::activate_managed_gguf(filename, model_name, app_dir).await.map_err(|e| e.to_string())
}

/// Delete a model from the local Ollama instance.
#[tauri::command]
async fn delete_ollama_model(name: String) -> Result<(), String> {
    ollama::delete_ollama_model(&name).await.map_err(|e| e.to_string())
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
    let cpu_cores = System::physical_core_count().unwrap_or(sys.cpus().len());

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
    std::process::Command::new("powershell.exe")
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-Command")
        .arg("irm https://cerberusai.dev/get | iex")
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Remove any stale temp files left by interrupted downloads.
    let tmp = std::env::temp_dir();
    if let Ok(entries) = std::fs::read_dir(&tmp) {
        for entry in entries.flatten() {
            let n = entry.file_name();
            let s = n.to_string_lossy();
            if s.starts_with("cerberus-") && s.ends_with(".gguf") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(PullState(Mutex::new(None)))
        .manage(ChatState(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            check_api,
            list_allowed_models,
            check_for_update,
            check_local_ollama,
            list_models,
            pull_model,
            cancel_pull,
            chat_stream,
            cancel_chat,
            detect_hardware,
            update_app,
            list_local_ggufs,
            delete_local_gguf,
            move_local_gguf,
            import_local_gguf,
            activate_managed_gguf,
            delete_ollama_model,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
