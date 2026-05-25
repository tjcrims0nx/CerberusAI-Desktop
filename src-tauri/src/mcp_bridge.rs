use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tauri::{AppHandle, Emitter, State};

// State to hold active MCP child processes
pub struct McpState {
    pub processes: Mutex<HashMap<String, Arc<Mutex<Child>>>>,
}

impl McpState {
    pub fn new() -> Self {
        Self {
            processes: Mutex::new(HashMap::new()),
        }
    }
}

#[derive(Clone, serde::Serialize)]
struct McpMessagePayload {
    plugin_id: String,
    message: String,
}

#[derive(Clone, serde::Serialize)]
struct McpErrorPayload {
    plugin_id: String,
    error: String,
}

#[derive(serde::Deserialize)]
struct McpConfigFile {
    #[serde(rename = "mcpServers")]
    mcp_servers: HashMap<String, McpServerConfig>,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct McpServerConfig {
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub env: Option<HashMap<String, String>>,
    pub url: Option<String>,
}

#[derive(serde::Serialize)]
pub struct DiscoveredPlugin {
    pub id: String,
    pub name: String,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub env: Option<HashMap<String, String>>,
    pub url: Option<String>,
}

/// Reads an .mcp.json file and returns the list of configured MCP servers.
#[tauri::command]
pub async fn load_mcp_config(path: String) -> Result<Vec<DiscoveredPlugin>, String> {
    let content = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| format!("Failed to read {}: {}", path, e))?;

    let config: McpConfigFile = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse JSON in {}: {}", path, e))?;

    let mut plugins = Vec::new();
    for (name, server_config) in config.mcp_servers {
        plugins.push(DiscoveredPlugin {
            id: format!("mcp_{}", name),
            name: name,
            command: server_config.command,
            args: server_config.args,
            env: server_config.env,
            url: server_config.url,
        });
    }

    Ok(plugins)
}

/// Spawns an MCP server process as a sidecar/plugin and hooks up stdio to Tauri events.
#[tauri::command]
pub async fn spawn_mcp_server(
    app: AppHandle,
    state: State<'_, McpState>,
    plugin_id: String,
    command: String,
    args: Vec<String>,
    env: Option<HashMap<String, String>>,
) -> Result<(), String> {
    let mut cmd = Command::new(&command);
    cmd.args(args)
       .stdin(Stdio::piped())
       .stdout(Stdio::piped())
       .stderr(Stdio::piped());

    if let Some(envs) = env {
        cmd.envs(envs);
    }

    let mut child = cmd.spawn()
        .map_err(|e| format!("Failed to spawn {}: {}", command, e))?;

    let stdout = child.stdout.take().expect("Failed to open stdout");
    let stderr = child.stderr.take().expect("Failed to open stderr");

    let mut processes = state.processes.lock().await;
    processes.insert(plugin_id.clone(), Arc::new(Mutex::new(child)));

    let plugin_id_out = plugin_id.clone();
    let plugin_id_err = plugin_id.clone();

    // Spawn a task to read stdout and emit to frontend
    let app_handle_out = app.clone();
    tokio::spawn(async move {
        let mut reader = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let _ = app_handle_out.emit(
                "mcp-stdout",
                McpMessagePayload {
                    plugin_id: plugin_id_out.clone(),
                    message: line,
                },
            );
        }
    });

    // Spawn a task to read stderr and emit to frontend
    let app_handle_err = app.clone();
    tokio::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            let _ = app_handle_err.emit(
                "mcp-stderr",
                McpErrorPayload {
                    plugin_id: plugin_id_err.clone(),
                    error: line,
                },
            );
        }
    });

    Ok(())
}

/// Writes a JSON-RPC message to the stdin of the specified MCP server process.
#[tauri::command]
pub async fn send_mcp_message(
    state: State<'_, McpState>,
    plugin_id: String,
    message: String,
) -> Result<(), String> {
    let processes = state.processes.lock().await;

    if let Some(child_arc) = processes.get(&plugin_id) {
        let mut child = child_arc.lock().await;
        if let Some(stdin) = child.stdin.as_mut() {
            let payload = format!("{}\n", message);
            stdin
                .write_all(payload.as_bytes())
                .await
                .map_err(|e| format!("Failed to write to stdin: {}", e))?;
            stdin.flush().await.map_err(|e| format!("Failed to flush stdin: {}", e))?;
            Ok(())
        } else {
            Err(format!("Process {} has no stdin", plugin_id))
        }
    } else {
        Err(format!("Process {} not found", plugin_id))
    }
}

/// Kills an MCP server process.
#[tauri::command]
pub async fn kill_mcp_server(
    state: State<'_, McpState>,
    plugin_id: String,
) -> Result<(), String> {
    let mut processes = state.processes.lock().await;

    if let Some(child_arc) = processes.remove(&plugin_id) {
        let mut child = child_arc.lock().await;
        child.kill().await.map_err(|e| format!("Failed to kill process: {}", e))?;
        Ok(())
    } else {
        Err(format!("Process {} not found", plugin_id))
    }
}
