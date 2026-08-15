//! HELIX chat backend.
//!
//! Architecture:
//!   * All chat inference happens on the user's own machine via the local Ollama
//!     daemon at http://127.0.0.1:11434  (`list_local`, `stream_chat_local`,
//!     `pull_model`, `local_status`).
//!   * Models are discovered/downloaded from HuggingFace (`search_huggingface`,
//!     `pull_model`) and can also be run directly from a `.gguf` file by
//!     `llama_engine`.

use crate::proc::NoWindow;
use crate::{ChatMessage, ChatStreamChunk, ToolCallChunk, ToolCallFunction};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tauri::ipc::Channel;


const OLLAMA_BASE: &str = "http://127.0.0.1:11434";

use std::sync::OnceLock;

static HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
static HTTP_SHORT_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn http() -> Result<reqwest::Client, reqwest::Error> {
    if let Some(c) = HTTP_CLIENT.get() {
        return Ok(c.clone());
    }
    let c = reqwest::Client::builder()
        .user_agent(concat!("HELIX-Desktop/", env!("CARGO_PKG_VERSION")))
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
        .user_agent(concat!("HELIX-Desktop/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10))
        .build()?;
    let _ = HTTP_SHORT_CLIENT.set(c.clone());
    Ok(c)
}

/// Best-effort lookup of the `ollama` CLI on PATH. Returns the resolved
/// path if found, so callers can fail fast with a clear message before
/// kicking off a multi-GB download.
async fn which_ollama() -> Option<std::path::PathBuf> {
    let bin = if cfg!(windows) { "ollama.exe" } else { "ollama" };
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(bin);
        if tokio::fs::metadata(&candidate).await.is_ok() {
            return Some(candidate);
        }
    }
    None
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub available: bool,
    pub release_url: String,
}

#[derive(Debug, Deserialize)]
struct GitHubReleaseTag {
    tag_name: String,
    html_url: String,
}

pub async fn check_update(current: &str) -> Result<UpdateInfo, anyhow::Error> {
    let client = http()?;
    let resp = client
        .get("https://api.github.com/repos/tjcrims0nx/Helix/releases/latest")
        .header("User-Agent", "HELIX-Desktop-App")
        .header("Accept", "application/json")
        .send()
        .await;

    let (latest_tag, release_url) = match resp {
        Ok(r) if r.status().is_success() => {
            if let Ok(rel) = r.json::<GitHubReleaseTag>().await {
                (rel.tag_name, rel.html_url)
            } else {
                (format!("v{current}"), "https://github.com/tjcrims0nx/Helix/releases/latest".to_string())
            }
        }
        _ => (format!("v{current}"), "https://github.com/tjcrims0nx/Helix/releases/latest".to_string()),
    };

    let latest_clean = latest_tag.trim_start_matches('v').trim().to_string();
    let current_clean = current.trim_start_matches('v').trim().to_string();

    let available = is_version_newer(&latest_clean, &current_clean);

    Ok(UpdateInfo {
        current: current_clean,
        latest: latest_clean,
        available,
        release_url,
    })
}

fn is_version_newer(latest: &str, current: &str) -> bool {
    let parse_ver = |v: &str| -> Vec<u64> {
        v.split('.')
            .map(|p| p.parse::<u64>().unwrap_or(0))
            .collect()
    };
    let l = parse_ver(latest);
    let c = parse_ver(current);
    l > c
}

#[derive(Debug, Serialize, Clone)]
pub struct UpdateProgress {
    pub status: String,
    pub completed: Option<u64>,
    pub total: Option<u64>,
    pub done: bool,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitHubReleaseInfo {
    assets: Vec<GitHubAssetInfo>,
}

#[derive(Debug, Deserialize)]
struct GitHubAssetInfo {
    name: String,
    browser_download_url: String,
}

pub async fn download_and_install_update(
    tag: String,
    out: tauri::ipc::Channel<UpdateProgress>,
) -> Result<(), anyhow::Error> {
    use tokio::io::AsyncWriteExt;
    use futures_util::StreamExt;

    let tag_clean = if tag.starts_with('v') { tag } else { format!("v{tag}") };
    let client = http()?;

    let _ = out.send(UpdateProgress {
        status: format!("Checking release assets for {tag_clean}..."),
        completed: None,
        total: None,
        done: false,
        error: None,
    });

    let url = format!("https://api.github.com/repos/tjcrims0nx/Helix/releases/tags/{tag_clean}");
    let resp = client
        .get(&url)
        .header("User-Agent", "HELIX-Desktop-App")
        .header("Accept", "application/json")
        .send()
        .await?;

    if !resp.status().is_success() {
        let msg = format!("Failed to fetch release details for {tag_clean}: HTTP {}", resp.status());
        let _ = out.send(UpdateProgress {
            status: format!("error: {msg}"),
            completed: None, total: None, done: true, error: Some(msg.clone()),
        });
        return Err(anyhow::anyhow!(msg));
    }

    let rel_info = resp.json::<GitHubReleaseInfo>().await?;

    let asset = rel_info
        .assets
        .into_iter()
        .find(|a| a.name.ends_with(".exe") || a.name.ends_with(".msi"))
        .ok_or_else(|| anyhow::anyhow!("No installer file (.exe or .msi) found in release assets for {tag_clean}"))?;

    let _ = out.send(UpdateProgress {
        status: format!("Connecting to download {}...", asset.name),
        completed: Some(0),
        total: None,
        done: false,
        error: None,
    });

    let download_resp = client
        .get(&asset.browser_download_url)
        .header("User-Agent", "HELIX-Desktop-App")
        .send()
        .await?;

    if !download_resp.status().is_success() {
        let msg = format!("Failed to download {}: HTTP {}", asset.name, download_resp.status());
        let _ = out.send(UpdateProgress {
            status: format!("error: {msg}"),
            completed: None, total: None, done: true, error: Some(msg.clone()),
        });
        return Err(anyhow::anyhow!(msg));
    }

    let total_bytes = download_resp.content_length().unwrap_or(0);
    let temp_dir = std::env::temp_dir();
    let installer_path = temp_dir.join(&asset.name);

    let mut file = tokio::fs::File::create(&installer_path).await?;
    let mut downloaded: u64 = 0;
    let mut stream = download_resp.bytes_stream();

    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;

        let _ = out.send(UpdateProgress {
            status: format!("Downloading {}...", asset.name),
            completed: Some(downloaded),
            total: if total_bytes > 0 { Some(total_bytes) } else { None },
            done: false,
            error: None,
        });
    }

    file.flush().await?;
    drop(file);

    let _ = out.send(UpdateProgress {
        status: "Launching installer...".into(),
        completed: Some(downloaded),
        total: if total_bytes > 0 { Some(total_bytes) } else { None },
        done: true,
        error: None,
    });

    #[cfg(target_os = "windows")]
    {
        if asset.name.ends_with(".msi") {
            std::process::Command::new("msiexec")
                .args(["/i", &installer_path.to_string_lossy(), "/passive"])
                .spawn()?;
        } else {
            std::process::Command::new(&installer_path)
                .spawn()?;
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        std::process::Command::new(&installer_path).spawn()?;
    }

    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    std::process::exit(0);
}

// ─── Cloud: GitHub release-based update check ──────────────────────────────



/// Extract the quant label out of a GGUF filename like
/// "qwen-3.6-annihilated-Q4_K_M.gguf" -> "Q4_K_M". Returns None if no recognizable
/// quant suffix is found.
fn extract_quant(filename: &str) -> Option<String> {
    let stem = filename.strip_suffix(".gguf").unwrap_or(filename);
    // Walk segments separated by '-' or '_' and pick the last one that looks
    // like a quant label (Q\d, IQ\d, f\d, F\d, mostly).
    let last_dash = stem.rfind('-')?;
    let candidate = &stem[last_dash + 1..];
    let lower = candidate.to_lowercase();
    let looks_like_quant = lower.starts_with('q')
        || lower.starts_with("iq")
        || lower == "f16"
        || lower == "f32"
        || lower == "bf16";
    if looks_like_quant {
        Some(candidate.to_string())
    } else {
        // Some filenames use compound suffixes like "Q4_K_M" — re-check the
        // last two segments joined.
        let prev_dash = stem[..last_dash].rfind('-')?;
        let combined = &stem[prev_dash + 1..];
        let lower = combined.to_lowercase();
        if lower.starts_with('q') || lower.starts_with("iq") {
            Some(combined.to_string())
        } else {
            None
        }
    }
}

// ─── HuggingFace: public model search & file listing ───────────────────────

#[derive(Debug, Serialize, Clone)]
pub struct HfSearchResult {
    pub repo_id: String,
    pub author: String,
    pub model_name: String,
    pub downloads: u64,
    pub likes: u64,
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct HfApiModel {
    #[serde(rename = "modelId", default)]
    model_id: String,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    likes: u64,
    #[serde(default)]
    tags: Vec<String>,
}

/// Search HuggingFace for GGUF models. No auth required.
pub async fn search_huggingface(query: &str) -> Result<Vec<HfSearchResult>, anyhow::Error> {
    let c = http_short()?;
    let q = query.trim();
    let url = if q.is_empty() {
        "https://huggingface.co/api/models?filter=gguf&sort=downloads&direction=-1&limit=50".to_string()
    } else {
        format!(
            "https://huggingface.co/api/models?search={}&filter=gguf&sort=downloads&direction=-1&limit=50",
            urlencoding::encode(q)
        )
    };
    let r = c
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await?;
    if !r.status().is_success() {
        return Err(anyhow::anyhow!("HuggingFace API returned {}", r.status()));
    }
    let models = r.json::<Vec<HfApiModel>>().await?;
    let results = models
        .into_iter()
        .map(|m| {
            let parts: Vec<&str> = m.model_id.splitn(2, '/').collect();
            let (author, model_name) = if parts.len() == 2 {
                (parts[0].to_string(), parts[1].to_string())
            } else {
                (String::new(), m.model_id.clone())
            };
            HfSearchResult {
                repo_id: m.model_id,
                author: m.author.unwrap_or(author),
                model_name,
                downloads: m.downloads,
                likes: m.likes,
                tags: m.tags,
            }
        })
        .collect();
    Ok(results)
}

#[derive(Debug, Serialize, Clone)]
pub struct HfGgufFile {
    pub filename: String,
    pub size: u64,
    pub quant_label: String,
}

#[derive(Debug, Deserialize)]
struct HfLfsInfo {
    #[serde(default)]
    size: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct HfRepoSibling {
    #[serde(rename = "rfilename")]
    filename: String,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    lfs: Option<HfLfsInfo>,
}

#[derive(Debug, Deserialize)]
struct HfRepoInfo {
    #[serde(default)]
    siblings: Vec<HfRepoSibling>,
}

#[derive(Debug, Deserialize)]
struct HfTreeItem {
    path: String,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    lfs: Option<HfLfsInfo>,
}

/// List all GGUF files in a HuggingFace repo with their sizes and quant labels.
pub async fn list_huggingface_files(repo_id: &str) -> Result<Vec<HfGgufFile>, anyhow::Error> {
    let c = http()?;
    let url = format!("https://huggingface.co/api/models/{repo_id}");
    let r = c
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await?;
    if !r.status().is_success() {
        return Err(anyhow::anyhow!("HuggingFace API returned {}", r.status()));
    }
    let info = r.json::<HfRepoInfo>().await?;
    let mut files: Vec<HfGgufFile> = info
        .siblings
        .into_iter()
        .filter(|s| s.filename.to_lowercase().ends_with(".gguf"))
        .map(|s| {
            let quant = extract_quant(&s.filename).unwrap_or_else(|| "unknown".to_string());
            let sz = s.size.or_else(|| s.lfs.as_ref().and_then(|l| l.size)).unwrap_or(0);
            HfGgufFile {
                filename: s.filename,
                size: sz,
                quant_label: quant,
            }
        })
        .collect();

    // If file sizes were missing in the model metadata, query the repo tree endpoint
    if files.iter().any(|f| f.size == 0) {
        let tree_url = format!("https://huggingface.co/api/models/{repo_id}/tree/main");
        if let Ok(tr) = c.get(&tree_url).header("Accept", "application/json").send().await {
            if tr.status().is_success() {
                if let Ok(tree_items) = tr.json::<Vec<HfTreeItem>>().await {
                    let map: std::collections::HashMap<String, u64> = tree_items
                        .into_iter()
                        .map(|item| {
                            let sz = item.size.or_else(|| item.lfs.as_ref().and_then(|l| l.size)).unwrap_or(0);
                            (item.path, sz)
                        })
                        .collect();
                    for file in &mut files {
                        if file.size == 0 {
                            if let Some(&sz) = map.get(&file.filename) {
                                file.size = sz;
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(files)
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
pub async fn list_local(app_dir: std::path::PathBuf) -> Result<Vec<ModelInfo>, anyhow::Error> {
    let mut out: Vec<ModelInfo> = Vec::new();

    // 1. Fetch Ollama models if daemon is running
    if let Ok(c) = http() {
        if let Ok(r) = c.get(format!("{OLLAMA_BASE}/api/tags")).send().await {
            if r.status().is_success() {
                if let Ok(tags) = r.json::<TagsResp>().await {
                    out.extend(tags.models);
                }
            }
        }
    }

    // 2. Scan local models folder for GGUF files
    if let Ok(local_ggufs) = list_local_ggufs(app_dir).await {
        for f in local_ggufs {
            if is_projector_gguf(&f.name) {
                continue;
            }
            let model_name = f.name.clone();
            if !out.iter().any(|m| m.name.eq_ignore_ascii_case(&model_name)) {
                let quant = extract_quant(&f.name);
                out.push(ModelInfo {
                    name: model_name,
                    size: f.size,
                    modified_at: String::new(),
                    details: Some(ModelDetails {
                        parameter_size: None,
                        quantization_level: quant,
                        family: Some("GGUF".to_string()),
                    }),
                });
            }
        }
    }

    Ok(out)
}



/// Persisted resume metadata kept next to the partially-downloaded GGUF.
/// On every chunk completion we rewrite this file. If the app is killed
/// mid-download, the next pull_model invocation reads this and resumes only
/// the unfinished chunks.
#[derive(Debug, Serialize, Deserialize)]
struct ResumeSidecar {
    /// Public URL the bytes came from (used to invalidate stale state when
    /// the chosen quant or filename changes between runs).
    url: String,
    /// Total expected bytes per the server's Content-Length.
    total: u64,
    /// Boolean "is this chunk fully written" flag for each of the CHUNKS slices.
    /// Index N corresponds to byte range [N*chunk_size, (N+1)*chunk_size).
    completed_chunks: Vec<bool>,
    chunk_size: u64,
}

#[derive(Debug, Serialize, Clone, Default)]
pub struct PullProgress {
    pub status: String,
    pub completed: Option<u64>,
    pub total: Option<u64>,
    pub done: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Instantaneous transfer rate (bytes/sec) over the last sample window.
    /// Frontend can format this as MB/s for the user.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_per_second: Option<u64>,
    /// Estimated remaining seconds based on `bytes_per_second`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eta_seconds: Option<u64>,
    /// Set on the first event of a resumed download so the UI can show a hint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resumed: Option<bool>,
}

/// Download the smallest GGUF for `name` from the configured model index and import
/// it into the user's local Ollama via `/api/create`. Progress is streamed
/// to `out`: byte-progress during download, then status messages from
/// Ollama while the model is imported.
pub async fn pull_model(
    repo_id: String,
    filename: String,
    app_dir: std::path::PathBuf,
    out: Channel<PullProgress>,
    mut cancel: tokio::sync::watch::Receiver<bool>,
) -> Result<(), anyhow::Error> {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tokio::io::{AsyncSeekExt, AsyncWriteExt};

    let clean_filename = std::path::Path::new(&filename)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&filename)
        .to_string();

    // Derive the Ollama model name from the filename (strip .gguf, lowercase).
    let ollama_model_name = clean_filename
        .strip_suffix(".gguf")
        .or_else(|| clean_filename.strip_suffix(".GGUF"))
        .unwrap_or(&clean_filename)
        .to_lowercase();

    let c = http()?;

    // Construct the HuggingFace download URL.
    let url = format!("https://huggingface.co/{repo_id}/resolve/main/{filename}");

    // Get file size via GET Range: bytes=0-0 (HF LFS redirects to S3 pre-signed URLs, which fail on HEAD requests with 403).
    let _ = out.send(PullProgress {
        status: "looking up model".into(),
        completed: None, total: None, done: false, error: None,
        bytes_per_second: None, eta_seconds: None, resumed: None,
    });

    let mut total: u64 = 0;
    if let Ok(resp) = c.get(&url).header("Range", "bytes=0-0").send().await {
        if resp.status().is_success() || resp.status().as_u16() == 206 {
            if let Some(cr) = resp.headers().get("content-range").and_then(|v| v.to_str().ok()) {
                if let Some(tot_str) = cr.split('/').nth(1) {
                    total = tot_str.trim().parse::<u64>().unwrap_or(0);
                }
            }
            if total == 0 {
                total = resp
                    .headers()
                    .get("content-length")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(0);
            }
        }
    }

    // Fallback to HEAD request if GET range didn't yield file size
    if total == 0 {
        if let Ok(head_resp) = c.head(&url).send().await {
            if head_resp.status().is_success() {
                total = head_resp
                    .headers()
                    .get("content-length")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(0);
            }
        }
    }

    if total == 0 {
        let msg = format!("Could not determine file size for {repo_id}/{filename}");
        let _ = out.send(PullProgress {
            status: format!("error: {msg}"),
            completed: None, total: None, done: true, error: Some(msg.clone()),
            ..Default::default()
        });
        return Err(anyhow::anyhow!(msg));
    }

    // 2. Parallel chunked download — 8 simultaneous connections.
    let safe_name = repo_id.replace(['/', '\\', ':'], "_");
    
    let models_dir = app_dir.join("models");
    if let Err(e) = tokio::fs::create_dir_all(&models_dir).await {
        let msg = format!("failed to create models directory: {e}");
        let _ = out.send(PullProgress {
            status: format!("error: {}", msg),
            completed: None, total: None, done: true, error: Some(msg.clone()),
            ..Default::default()
        });
        return Err(anyhow::anyhow!(msg));
    }
    
    let temp_path = models_dir.join(format!("{safe_name}-{}", clean_filename));
    let sidecar_path = temp_path.with_extension("part.json");

    let range_check = c.get(&url).header("Range", "bytes=0-0");
    let range_supported = range_check
        .send()
        .await
        .map(|resp| resp.status().as_u16() == 206)
        .unwrap_or(false);
    let chunks: u64 = if range_supported { 8 } else { 1 };
    let chunk_size = total.div_ceil(chunks);

    // Resume support — if a sidecar matches this URL & total, reuse the file
    // and skip already-completed chunks. Otherwise start fresh.
    let mut completed_chunks: Vec<bool> = vec![false; chunks as usize];
    let mut resumed_from_disk = false;
    if let Ok(bytes) = tokio::fs::read(&sidecar_path).await {
        if let Ok(side) = serde_json::from_slice::<ResumeSidecar>(&bytes) {
            let file_ok = match tokio::fs::metadata(&temp_path).await {
                Ok(meta) => meta.len() == side.total,
                Err(_) => false,
            };
            if file_ok && side.url == url && side.total == total && side.chunk_size == chunk_size
                && side.completed_chunks.len() == chunks as usize
            {
                completed_chunks = side.completed_chunks;
                resumed_from_disk = completed_chunks.iter().any(|c| *c);
            }
        }
    }

    if !resumed_from_disk {
        // Fresh download — wipe any stale temp file & sidecar then preallocate.
        let _ = tokio::fs::remove_file(&sidecar_path).await;
        let f = tokio::fs::OpenOptions::new()
            .write(true).create(true).truncate(true)
            .open(&temp_path).await?;
        f.set_len(total).await?;
    } else {
        let already: u64 = completed_chunks.iter().enumerate()
            .filter(|(_, c)| **c)
            .map(|(i, _)| {
                let s = i as u64 * chunk_size;
                let e = ((i as u64 + 1) * chunk_size).min(total);
                e - s
            }).sum();
        let _ = out.send(PullProgress {
            status: format!("resuming previous download ({already} bytes already on disk)"),
            completed: Some(already),
            total: Some(total),
            done: false, error: None,
            bytes_per_second: None, eta_seconds: None,
            resumed: Some(true),
        });
    }

    // Sum bytes already on disk so the progress counter starts at the right place.
    let already_bytes: u64 = completed_chunks.iter().enumerate()
        .filter(|(_, c)| **c)
        .map(|(i, _)| {
            let s = i as u64 * chunk_size;
            let e = ((i as u64 + 1) * chunk_size).min(total);
            e - s
        }).sum::<u64>()
        .min(total);
    let completed = Arc::new(AtomicU64::new(already_bytes));
    let chunk_done_flags = Arc::new(tokio::sync::Mutex::new(completed_chunks.clone()));
    let mut handles: Vec<tokio::task::JoinHandle<Result<(), anyhow::Error>>> = Vec::new();

    // One shared client so all 8 workers reuse TLS sessions and the connection pool.
    let chunk_client = Arc::new(
        reqwest::Client::builder()
            .user_agent(concat!("HELIX-Desktop/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(3600))
            .build()?
    );

    for i in 0..chunks {
        let byte_start = i * chunk_size;
        if byte_start >= total { break; }
        if completed_chunks[i as usize] {
            // Already fully written from a prior run — skip.
            continue;
        }
        let byte_end = ((i + 1) * chunk_size).min(total) - 1;
        let dl_url = url.clone();
        let dl_path = temp_path.clone();
        let dl_done = completed.clone();
        let mut dl_cancel = cancel.clone();
        let client = chunk_client.clone();
        let sidecar = sidecar_path.clone();
        let flags = chunk_done_flags.clone();
        let total_clone = total;
        let chunk_size_clone = chunk_size;
        let url_clone = url.clone();

        handles.push(tokio::spawn(async move {
            let req = client
                .get(&dl_url)
                .header("Range", format!("bytes={byte_start}-{byte_end}"));
            let resp = req.send().await?;
            let status = resp.status();
            if status.as_u16() != 206 && !(chunk_size_clone == total_clone && status.is_success()) {
                return Err(anyhow::anyhow!("chunk {i} HTTP {status}"));
            }
            let mut stream = resp.bytes_stream();
            let mut f = tokio::fs::OpenOptions::new()
                .write(true).open(&dl_path).await?;
            f.seek(std::io::SeekFrom::Start(byte_start)).await?;
            // Per-chunk inactivity timeout: if the upstream sends no bytes for
            // STALL_TIMEOUT, fail this chunk so the outer error path can
            // surface a clean message instead of letting the user stare at a
            // frozen progress bar for the hour-long overall timeout.
            const STALL_TIMEOUT: Duration = Duration::from_secs(30);
            loop {
                tokio::select! {
                    biased;
                    _ = dl_cancel.changed() => {
                        if *dl_cancel.borrow() {
                            return Err(anyhow::anyhow!("cancelled"));
                        }
                    }
                    chunk = tokio::time::timeout(STALL_TIMEOUT, stream.next()) => {
                        match chunk {
                            Err(_) => {
                                return Err(anyhow::anyhow!(
                                    "chunk {i} stalled (no data for {}s); upstream may be down",
                                    STALL_TIMEOUT.as_secs()
                                ));
                            }
                            Ok(None) => break,
                            Ok(Some(Err(e))) => return Err(e.into()),
                            Ok(Some(Ok(bytes))) => {
                                let prev = dl_done.fetch_add(bytes.len() as u64, Ordering::Relaxed);
                                if prev.saturating_add(bytes.len() as u64) > total_clone {
                                    dl_done.store(total_clone, Ordering::Relaxed);
                                }
                                f.write_all(&bytes).await?;
                            }
                        }
                    }
                }
            }
            f.flush().await?;
            // Mark this chunk done in the sidecar so a future restart skips it.
            {
                let mut g = flags.lock().await;
                g[i as usize] = true;
                let snapshot = ResumeSidecar {
                    url: url_clone,
                    total: total_clone,
                    chunk_size: chunk_size_clone,
                    completed_chunks: g.clone(),
                };
                if let Ok(bytes) = serde_json::to_vec(&snapshot) {
                    let _ = tokio::fs::write(&sidecar, bytes).await;
                }
            }
            Ok(())
        }));
    }

    // Report progress every 500 ms; stop on cancel or when all chunks finish.
    // We track a 5-sample rolling window for byte-rate so the displayed
    // MB/s isn't jumpy from individual TCP socket bursts.
    let mut cancelled = false;
    let mut samples: std::collections::VecDeque<(std::time::Instant, u64)> =
        std::collections::VecDeque::with_capacity(8);
    samples.push_back((std::time::Instant::now(), already_bytes));
    loop {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(500)) => {
                let current = completed.load(Ordering::Relaxed).min(total);
                completed.store(current, Ordering::Relaxed);
                let now = std::time::Instant::now();
                samples.push_back((now, current));
                while samples.len() > 6 {
                    samples.pop_front();
                }
                let (bps, eta) = if let (Some((t0, b0)), Some((t1, b1))) =
                    (samples.front(), samples.back())
                {
                    let secs = t1.duration_since(*t0).as_secs_f64().max(0.001);
                    let delta = b1.saturating_sub(*b0) as f64;
                    let rate = (delta / secs).max(0.0) as u64;
                    let remaining = total.saturating_sub(current);
                    let eta = if rate > 0 {
                        Some(remaining / rate.max(1))
                    } else {
                        None
                    };
                    (Some(rate), eta)
                } else {
                    (None, None)
                };
                let _ = out.send(PullProgress {
                    status: "downloading".into(),
                    completed: Some(current),
                    total: Some(total),
                    done: false, error: None,
                    bytes_per_second: bps,
                    eta_seconds: eta,
                    resumed: None,
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
        // On cancel, keep the temp file + sidecar so the user can resume next time.
        // On hard error, wipe both so the next attempt is clean.
        if !cancelled {
            let _ = tokio::fs::remove_file(&temp_path).await;
            let _ = tokio::fs::remove_file(&sidecar_path).await;
        }
        if cancelled {
            let _ = out.send(PullProgress {
                status: "cancelled".into(),
                completed: None, total: None, done: true, error: None,
                ..Default::default()
            });
            return Ok(());
        }
        let msg = errors.join("; ");
        let _ = out.send(PullProgress {
            status: format!("error: {msg}"),
            completed: None, total: None, done: true,
            error: Some(msg.clone()),
            ..Default::default()
        });
        return Err(anyhow::anyhow!("download errors: {msg}"));
    }

    let _ = out.send(PullProgress {
        status: "downloading".into(),
        completed: Some(total),
        total: Some(total),
        done: false, error: None,
        ..Default::default()
    });

    // 3. Finalize the downloaded GGUF file.
    //    Keep it in ~/.HELIX/models/ with its original filename so llama-server
    //    can load it directly. Also attempt ollama create if Ollama is running.
    let final_path = models_dir.join(&clean_filename);
    if final_path != temp_path {
        if let Err(e) = tokio::fs::rename(&temp_path, &final_path).await {
            // rename can fail cross-device, fall back to copy+delete
            if let Err(e2) = tokio::fs::copy(&temp_path, &final_path).await {
                let msg = format!("Failed to finalize model file: rename={e}, copy={e2}");
                let _ = out.send(PullProgress {
                    status: format!("error: {msg}"),
                    completed: None, total: None, done: true, error: Some(msg.clone()),
                    ..Default::default()
                });
                return Err(anyhow::anyhow!(msg));
            }
            let _ = tokio::fs::remove_file(&temp_path).await;
        }
    }
    let _ = tokio::fs::remove_file(&sidecar_path).await;

    // On Windows, unblock file (remove Zone.Identifier stream)
    #[cfg(windows)]
    {
        let zone_file = format!("{}:Zone.Identifier", final_path.display());
        let _ = std::fs::remove_file(zone_file);
    }

    // Best-effort: if Ollama is running, register the model so it shows up there too.
    let _ = out.send(PullProgress {
        status: "registering model...".into(),
        completed: None, total: None, done: false, error: None,
        ..Default::default()
    });

    match try_register_in_ollama(&final_path, &ollama_model_name).await {
        Ok(()) => log::info!("Model also registered in Ollama as '{}'", ollama_model_name),
        Err(reason) => log::warn!(
            "Could not register in Ollama (non-fatal, model available as local GGUF): {reason}"
        ),
    }

    let _ = out.send(PullProgress {
        status: "success".into(),
        completed: None, total: None, done: true, error: None,
        ..Default::default()
    });
    Ok(())
}

// ─── Local Ollama: chat streaming ─────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OllamaFunctionDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OllamaToolDef {
    #[serde(rename = "type")]
    pub kind: String, // always "function"
    pub function: OllamaFunctionDef,
}

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
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<&'a [OllamaToolDef]>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct OllamaToolCallFunction {
    name: String,
    arguments: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct OllamaToolCall {
    function: OllamaToolCallFunction,
}

#[derive(Deserialize)]
struct LocalChatLineMsg {
    #[serde(default)]
    content: String,
    #[serde(default)]
    tool_calls: Option<Vec<OllamaToolCall>>,
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
    tools: Option<Vec<OllamaToolDef>>,
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
        tools: tools.as_deref(),
    };

    let resp = tokio::select! {
        biased;
        _ = cancel_rx.changed() => {
            // User hit stop before the request even came back. Bail out clean.
            let _ = out.send(ChatStreamChunk {
                delta: String::new(),
                done: true,
                error: None,
                ttft_ms: None, tps: None,
                tool_calls: None,
            });
            return Ok(());
        }
        r = c.post(format!("{OLLAMA_BASE}/api/chat")).json(&body).send() => r,
    };

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
                tool_calls: None,
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
            tool_calls: None,
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
                    tool_calls: None,
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
                    tool_calls: None,
                });
                return Ok(());
            }
            Ok(None) => break, // stream ended
            Ok(Some(Err(e))) => return Err(e.into()),
            Ok(Some(Ok(bytes))) => {
                buf.extend_from_slice(&bytes);
                while let Some(nl) = buf.iter().position(|b| *b == b'\n') {
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
                                    tool_calls: None,
                                });
                                return Ok(());
                            }
                            let msg = p.message;
                            let delta = msg.as_ref().map(|m| m.content.clone()).unwrap_or_default();
                            let chunk_tool_calls: Option<Vec<ToolCallChunk>> = msg
                                .as_ref()
                                .and_then(|m| m.tool_calls.as_ref())
                                .map(|tcs| {
                                    tcs.iter()
                                        .map(|tc| ToolCallChunk {
                                            id: None,
                                            function: ToolCallFunction {
                                                name: tc.function.name.clone(),
                                                arguments: tc.function.arguments.clone(),
                                            },
                                        })
                                        .collect()
                                });
                            if ttft_ms.is_none() && (!delta.is_empty() || chunk_tool_calls.is_some()) {
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
                                tool_calls: chunk_tool_calls,
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
        tool_calls: None,
    });
    Ok(())
}

// ─── Local Filesystem: GGUF File Management ──────────────────────────────

/// Whether a `.gguf` is a multimodal projector rather than a loadable model.
///
/// Vision models ship a companion `mmproj-*.gguf` holding the image encoder.
/// Its architecture is `clip`, so `llama-server` refuses it with "unsupported
/// model architecture" and exits — offering one in the model picker only
/// produces a model that can never load. llama.cpp names these files by
/// convention, which is what the check keys on.
///
/// Only the picker filters them; disk-management listings still show them so
/// they can be deleted or moved.
pub fn is_projector_gguf(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.contains("mmproj") || lower.contains("mm-proj")
}

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
        tokio::fs::remove_file(&file_path).await?;

        // Remove the sibling Modelfile older builds could leave next to the model,
        // so deleting a GGUF doesn't strand a file pointing at it.
        let _ = tokio::fs::remove_file(file_path.with_extension("Modelfile")).await;

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

/// Best-effort registration of a GGUF into Ollama.
///
/// Never fatal. A `.gguf` sitting in `~/.HELIX/models/` is already fully usable:
/// `list_models` surfaces it and `llama_engine::ensure_server` runs it on the bundled
/// `llama-server`. Ollama is a convenience, not a requirement, so callers treat the
/// `Err` string purely as a human-readable note to pass along to the user.
async fn try_register_in_ollama(
    gguf_path: &std::path::Path,
    model_name: &str,
) -> Result<(), String> {
    if which_ollama().await.is_none() {
        return Err("the `ollama` CLI was not found on PATH".to_string());
    }
    if !local_status().await.running {
        return Err("the Ollama daemon isn't running on 127.0.0.1:11434".to_string());
    }
    // `ollama create` reports a missing FROM file as "400 Bad Request: invalid model name",
    // which sends users chasing a naming problem that doesn't exist. Check it ourselves so
    // the real cause is what gets reported.
    if tokio::fs::metadata(gguf_path).await.is_err() {
        return Err(format!("{} is missing", gguf_path.display()));
    }

    // Write the Modelfile outside the models dir so a failure can never leave litter there.
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let unique = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let modelfile_path =
        std::env::temp_dir().join(format!("helix-{}-{unique}.Modelfile", std::process::id()));

    let content = format!(
        "FROM \"{}\"\n",
        gguf_path.to_string_lossy().replace('\\', "/")
    );
    tokio::fs::write(&modelfile_path, content)
        .await
        .map_err(|e| format!("could not write a temporary Modelfile: {e}"))?;

    let result = tokio::process::Command::new("ollama")
        .arg("create")
        .arg(model_name)
        .arg("-f")
        .arg(&modelfile_path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .no_window()
        .output()
        .await;

    // Clean up *before* inspecting the result, so no early return can leak the file.
    if let Err(e) = tokio::fs::remove_file(&modelfile_path).await {
        log::warn!(
            "Could not remove temporary Modelfile {}: {e}",
            modelfile_path.display()
        );
    }

    match result {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr).trim().to_string();
            Err(if stderr.is_empty() {
                format!("`ollama create` exited with {}", o.status)
            } else {
                stderr
            })
        }
        Err(e) => Err(format!("could not start the `ollama` CLI: {e}")),
    }
}

/// Remove stray `*.Modelfile` entries from the models directory.
///
/// Modelfiles are transient scratch files written to the temp dir by
/// `try_register_in_ollama`, so anything left in `models/` is litter from an older
/// build that leaked them on failure. Best-effort: errors are logged, never fatal.
async fn sweep_orphan_modelfiles(models_dir: &std::path::Path) {
    let Ok(mut entries) = tokio::fs::read_dir(models_dir).await else {
        return;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("Modelfile") {
            if let Err(e) = tokio::fs::remove_file(&path).await {
                log::warn!("Could not remove stale Modelfile {}: {e}", path.display());
            } else {
                log::info!("Removed stale Modelfile {}", path.display());
            }
        }
    }
}

/// Case-insensitive `.gguf` extension check. `pull_model` already accepts `.GGUF`
/// filenames, so imports must too.
fn has_gguf_extension(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("gguf"))
}

/// True only when both paths resolve to the same *existing* file.
///
/// Deliberately not `canonicalize().unwrap_or_default()`: a destination that doesn't
/// exist yet fails to canonicalize, and comparing two defaulted empty paths would call
/// them equal — which used to make the copy silently skip.
fn is_same_existing_file(a: &std::path::Path, b: &std::path::Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// Pick a destination in `models_dir` that won't clobber a different existing model:
/// `name.gguf`, then `name-2.gguf`, `name-3.gguf`, …
fn unique_dest_path(models_dir: &std::path::Path, filename: &str) -> std::path::PathBuf {
    let candidate = models_dir.join(filename);
    if !candidate.exists() {
        return candidate;
    }
    let as_path = std::path::Path::new(filename);
    let stem = as_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("model");
    let ext = as_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("gguf");
    for n in 2..=999 {
        let candidate = models_dir.join(format!("{stem}-{n}.{ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    models_dir.join(format!("{stem}-{}.{ext}", std::process::id()))
}

/// Copy `src` to `dest`, verifying the whole file arrived.
///
/// On any failure the partial destination is removed. The source is never modified —
/// that is the whole point: an import must not be able to lose the user's model.
async fn copy_into_models_dir(
    src: &std::path::Path,
    dest: &std::path::Path,
    expected_len: u64,
) -> Result<(), anyhow::Error> {
    let copied = match tokio::fs::copy(src, dest).await {
        Ok(n) => n,
        Err(e) => {
            let _ = tokio::fs::remove_file(dest).await;
            return Err(anyhow::anyhow!(
                "Could not copy into {}: {e}. Your original file was left untouched.",
                dest.display()
            ));
        }
    };
    if copied != expected_len {
        let _ = tokio::fs::remove_file(dest).await;
        return Err(anyhow::anyhow!(
            "Copy was incomplete ({copied} of {expected_len} bytes) — is the disk full? \
             Your original file was left untouched."
        ));
    }
    // Clear the mark-of-the-web, mirroring the download path.
    #[cfg(windows)]
    {
        let zone_file = format!("{}:Zone.Identifier", dest.display());
        let _ = std::fs::remove_file(zone_file);
    }
    Ok(())
}

/// Import an arbitrary `.gguf` file from the user's filesystem.
///
/// The file is **copied** into `~/.HELIX/models/` — the user's original is always left
/// where they put it. Registering the result in Ollama is best-effort: the copy is
/// runnable by HELIX's built-in engine either way, so a missing or sleeping Ollama
/// never fails the import.
pub async fn import_local_gguf(
    source_path: String,
    model_name: String,
    app_dir: std::path::PathBuf,
) -> Result<String, anyhow::Error> {
    let src = std::path::Path::new(&source_path);
    let src_meta = tokio::fs::metadata(src)
        .await
        .map_err(|e| anyhow::anyhow!("Cannot read {source_path}: {e}"))?;
    if !src_meta.is_file() {
        return Err(anyhow::anyhow!("Not a file: {source_path}"));
    }
    if !has_gguf_extension(src) {
        return Err(anyhow::anyhow!("Only .gguf files can be imported"));
    }

    let models_dir = app_dir.join("models");
    tokio::fs::create_dir_all(&models_dir).await?;

    // Clear litter left by older builds that leaked Modelfiles into models/.
    sweep_orphan_modelfiles(&models_dir).await;

    let filename = src
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("Unsupported filename: {source_path}"))?
        .to_string();

    // If the picked file already *is* the managed copy, don't copy it over itself.
    let already_managed = is_same_existing_file(src, &models_dir.join(&filename));
    let dest_path = if already_managed {
        models_dir.join(&filename)
    } else {
        let dest = unique_dest_path(&models_dir, &filename);
        copy_into_models_dir(src, &dest, src_meta.len()).await?;
        dest
    };

    let note = match try_register_in_ollama(&dest_path, &model_name).await {
        Ok(()) => format!("Registered in Ollama as '{model_name}'."),
        Err(reason) => {
            log::warn!("Ollama registration skipped for '{model_name}': {reason}");
            format!(
                "Ollama registration was skipped ({reason}), but the model still runs on \
                 HELIX's built-in engine."
            )
        }
    };

    let placement = if already_managed {
        "It was already in your models folder.".to_string()
    } else {
        format!(
            "Copied to {}\nYour original file was left in place.",
            dest_path.display()
        )
    };

    Ok(format!(
        "Imported {}.\n\n{placement}\n{note}",
        dest_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&filename)
    ))
}

/// Activate a `.gguf` file that is already stored in the managed models directory.
///
/// "Activate" means registering it with Ollama, which is best-effort for the same reason
/// as import: the file is already runnable by HELIX's built-in engine, so a missing or
/// sleeping Ollama is reported as a note rather than failing the action.
pub async fn activate_managed_gguf(
    filename: String,
    model_name: String,
    app_dir: std::path::PathBuf,
) -> Result<String, anyhow::Error> {
    if !has_gguf_extension(std::path::Path::new(&filename)) || filename.contains("..") {
        return Err(anyhow::anyhow!("Invalid filename"));
    }

    let models_dir = app_dir.join("models");
    let dest_path = models_dir.join(&filename);

    if !dest_path.exists() {
        return Err(anyhow::anyhow!("File not found in managed storage"));
    }

    match try_register_in_ollama(&dest_path, &model_name).await {
        Ok(()) => Ok(format!("Activated {filename} in Ollama as '{model_name}'.")),
        Err(reason) => {
            log::warn!("Ollama registration skipped for '{model_name}': {reason}");
            Ok(format!(
                "{filename} is ready to use on HELIX's built-in engine.\n\n\
                 Registering it with Ollama was skipped ({reason})."
            ))
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    /// A throwaway app dir with a `models/` subfolder. Named per test rather than
    /// randomised, and cleared on the way in, so a crashed run leaves nothing that
    /// poisons the next one. Matches how the engine and migrate tests get scratch space.
    fn scratch_app_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("helix-models-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("models")).expect("temp dir");
        dir
    }

    #[test]
    fn gguf_extension_check_is_case_insensitive() {
        // A `.GGUF` download used to be rejected outright by import.
        assert!(has_gguf_extension(Path::new("model.gguf")));
        assert!(has_gguf_extension(Path::new("model.GGUF")));
        assert!(has_gguf_extension(Path::new("model.GgUf")));
        assert!(!has_gguf_extension(Path::new("model.bin")));
        assert!(!has_gguf_extension(Path::new("model")));
    }

    #[test]
    fn unique_dest_path_avoids_clobbering_a_different_model() {
        let app_dir = scratch_app_dir("unique-dest");
        let models = app_dir.join("models");

        // Nothing there yet: keep the original name.
        assert_eq!(
            unique_dest_path(&models, "m-Q4_K_M.gguf"),
            models.join("m-Q4_K_M.gguf")
        );

        // Occupied: step aside instead of overwriting.
        std::fs::write(models.join("m-Q4_K_M.gguf"), b"first").unwrap();
        assert_eq!(
            unique_dest_path(&models, "m-Q4_K_M.gguf"),
            models.join("m-Q4_K_M-2.gguf")
        );

        std::fs::write(models.join("m-Q4_K_M-2.gguf"), b"second").unwrap();
        assert_eq!(
            unique_dest_path(&models, "m-Q4_K_M.gguf"),
            models.join("m-Q4_K_M-3.gguf")
        );
    }

    #[test]
    fn same_existing_file_is_false_when_paths_cannot_resolve() {
        let app_dir = scratch_app_dir("same-file");
        let real = app_dir.join("models").join("real.gguf");
        std::fs::write(&real, b"x").unwrap();

        assert!(is_same_existing_file(&real, &real));
        // Two nonexistent paths must NOT compare equal — the bug that made the copy
        // silently skip and left the Modelfile pointing at a file never created.
        assert!(!is_same_existing_file(
            &app_dir.join("nope-a.gguf"),
            &app_dir.join("nope-b.gguf")
        ));
        assert!(!is_same_existing_file(&real, &app_dir.join("nope-a.gguf")));
    }

    #[tokio::test]
    async fn copy_preserves_the_source_file() {
        let app_dir = scratch_app_dir("copy-preserves");
        let src = app_dir.join("downloaded.gguf");
        std::fs::write(&src, b"pretend weights").unwrap();
        let dest = app_dir.join("models").join("downloaded.gguf");

        copy_into_models_dir(&src, &dest, b"pretend weights".len() as u64)
            .await
            .expect("copy should succeed");

        // The whole point: importing must never remove the user's file.
        assert!(src.exists(), "source must survive an import");
        assert_eq!(std::fs::read(&dest).unwrap(), b"pretend weights");
    }

    #[tokio::test]
    async fn copy_leaves_no_partial_file_when_length_is_wrong() {
        let app_dir = scratch_app_dir("copy-partial");
        let src = app_dir.join("truncated.gguf");
        std::fs::write(&src, b"short").unwrap();
        let dest = app_dir.join("models").join("truncated.gguf");

        // Claim a larger expected size, as a disk-full copy would produce.
        let err = copy_into_models_dir(&src, &dest, 999_999)
            .await
            .expect_err("mismatched length must fail");

        assert!(err.to_string().contains("incomplete"), "got: {err}");
        assert!(!dest.exists(), "partial copy must be cleaned up");
        assert!(src.exists(), "source must survive a failed import");
    }

    #[tokio::test]
    async fn sweep_removes_stray_modelfiles_but_keeps_models() {
        let app_dir = scratch_app_dir("sweep");
        let models = app_dir.join("models");
        std::fs::write(models.join("keep.gguf"), b"weights").unwrap();
        std::fs::write(models.join("orphan.Modelfile"), b"FROM \"gone.gguf\"").unwrap();

        sweep_orphan_modelfiles(&models).await;

        assert!(!models.join("orphan.Modelfile").exists());
        assert!(models.join("keep.gguf").exists());
    }

    #[tokio::test]
    async fn deleting_a_gguf_also_removes_its_modelfile() {
        let app_dir = scratch_app_dir("delete-sibling");
        let models = app_dir.join("models");
        std::fs::write(models.join("m.gguf"), b"weights").unwrap();
        std::fs::write(models.join("m.Modelfile"), b"FROM \"m.gguf\"").unwrap();

        delete_local_gguf("m.gguf".to_string(), app_dir.clone())
            .await
            .expect("delete should succeed");

        assert!(!models.join("m.gguf").exists());
        assert!(!models.join("m.Modelfile").exists());
    }

    /// The regression this whole change exists for: the import must succeed and keep the
    /// user's file even when Ollama can't take the model. The payload here is not a real
    /// GGUF, so registration fails whether or not a daemon is up — the outcome must be
    /// `Ok` either way.
    #[tokio::test]
    async fn import_succeeds_and_keeps_source_even_when_ollama_cannot_register() {
        let app_dir = scratch_app_dir("import-e2e");
        let src = app_dir.join("my-model-Q4_K_M.gguf");
        std::fs::write(&src, b"not actually gguf bytes").unwrap();

        let msg = import_local_gguf(
            src.to_string_lossy().to_string(),
            "helix-import-test".to_string(),
            app_dir.clone(),
        )
        .await
        .expect("import must not fail just because Ollama is unavailable");

        assert!(src.exists(), "the user's original file must still be there");
        assert!(app_dir.join("models").join("my-model-Q4_K_M.gguf").exists());
        assert!(msg.contains("Imported"), "got: {msg}");

        // No Modelfile litter left behind in the models folder.
        let strays: Vec<_> = std::fs::read_dir(app_dir.join("models"))
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.ends_with(".Modelfile"))
            .collect();
        assert!(strays.is_empty(), "stray Modelfiles: {strays:?}");
    }

    /// A `.GGUF` (uppercase) import used to be rejected before it even started.
    #[tokio::test]
    async fn import_accepts_uppercase_extension() {
        let app_dir = scratch_app_dir("import-upper");
        let src = app_dir.join("shouty.GGUF");
        std::fs::write(&src, b"weights").unwrap();

        import_local_gguf(
            src.to_string_lossy().to_string(),
            "helix-upper-test".to_string(),
            app_dir.clone(),
        )
        .await
        .expect(".GGUF must be importable");

        assert!(app_dir.join("models").join("shouty.GGUF").exists());
    }
}
