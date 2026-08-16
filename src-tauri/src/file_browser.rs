//! File & picture management: a native filesystem browser plus a persistent
//! "Library" of reusable attachments.
//!
//! Two halves, exposed as Tauri commands:
//!
//! * **Browse** — `fb_quick_dirs` / `fb_list_dir` / `fb_read_base64` /
//!   `fb_read_text` let the UI walk the real filesystem (Downloads, Documents,
//!   …), preview images, and pull a file's bytes in without copying anything.
//!
//! * **Library** — `library_*` keep a curated set of images and files under
//!   `~/.HELIX/library/` so an attachment used once can be re-attached in any
//!   later chat. Blobs live in `library/blobs/<id>.<ext>`; a single
//!   `library/index.json` records the metadata. Nothing here touches the
//!   encrypted KV store — these are user documents, not secrets, and keeping
//!   them as plain files means a base64 round-trip per attach instead of
//!   bloating helix.db.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Hard ceiling on any single file we will read into memory as base64/text.
/// Chat attachments are images and documents, not disk images — 25 MiB is
/// generous for that and stops one stray click on a multi-gigabyte file from
/// hanging the app.
const MAX_READ_BYTES: u64 = 25 * 1024 * 1024;

/// Extensions we treat as previewable images.
const IMAGE_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "ico", "avif",
];

fn is_image_ext(ext: &str) -> bool {
    IMAGE_EXTS.contains(&ext.to_ascii_lowercase().as_str())
}

fn ext_of(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

// ---------------------------------------------------------------------------
// Browse
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct QuickDir {
    label: String,
    path: String,
}

/// The handful of well-known folders we surface as one-click shortcuts.
/// Any that don't resolve on this OS are simply omitted.
#[tauri::command]
pub fn fb_quick_dirs() -> Vec<QuickDir> {
    let mut out = Vec::new();
    let mut push = |label: &str, dir: Option<PathBuf>| {
        if let Some(p) = dir {
            if p.is_dir() {
                out.push(QuickDir {
                    label: label.to_string(),
                    path: p.to_string_lossy().to_string(),
                });
            }
        }
    };
    push("Home", dirs::home_dir());
    push("Desktop", dirs::desktop_dir());
    push("Documents", dirs::document_dir());
    push("Downloads", dirs::download_dir());
    push("Pictures", dirs::picture_dir());
    out
}

#[derive(Serialize)]
pub struct FsEntry {
    name: String,
    path: String,
    is_dir: bool,
    is_image: bool,
    size: u64,
    ext: String,
}

#[derive(Serialize)]
pub struct DirListing {
    /// The (canonical, best-effort) directory that was listed.
    path: String,
    /// Parent directory, or `None` at a filesystem root.
    parent: Option<String>,
    entries: Vec<FsEntry>,
}

/// List a directory. When `path` is empty we start at the user's home.
///
/// Entries are sorted folders-first, then case-insensitively by name. Hidden
/// entries (dotfiles / Windows hidden isn't distinguished here) are included —
/// this is a file picker, not a shell.
#[tauri::command]
pub async fn fb_list_dir(path: String) -> Result<DirListing, String> {
    let dir = if path.trim().is_empty() {
        dirs::home_dir().ok_or_else(|| "Cannot resolve home directory".to_string())?
    } else {
        PathBuf::from(&path)
    };

    let meta = tokio::fs::metadata(&dir)
        .await
        .map_err(|e| format!("Cannot open {}: {e}", dir.display()))?;
    if !meta.is_dir() {
        return Err(format!("{} is not a directory", dir.display()));
    }

    let mut rd = tokio::fs::read_dir(&dir)
        .await
        .map_err(|e| format!("Cannot read {}: {e}", dir.display()))?;

    let mut entries: Vec<FsEntry> = Vec::new();
    while let Some(item) = rd
        .next_entry()
        .await
        .map_err(|e| format!("Error reading directory: {e}"))?
    {
        let p = item.path();
        // A broken symlink or a permission error on one entry shouldn't sink
        // the whole listing — skip what we can't stat.
        let ft = match item.file_type().await {
            Ok(ft) => ft,
            Err(_) => continue,
        };
        let is_dir = ft.is_dir();
        let size = if is_dir {
            0
        } else {
            item.metadata().await.map(|m| m.len()).unwrap_or(0)
        };
        let ext = ext_of(&p);
        entries.push(FsEntry {
            name: item.file_name().to_string_lossy().to_string(),
            path: p.to_string_lossy().to_string(),
            is_dir,
            is_image: !is_dir && is_image_ext(&ext),
            size,
            ext,
        });
    }

    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    let parent = dir
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .filter(|s| !s.is_empty());

    Ok(DirListing {
        path: dir.to_string_lossy().to_string(),
        parent,
        entries,
    })
}

async fn read_capped(path: &str) -> Result<Vec<u8>, String> {
    let meta = tokio::fs::metadata(path)
        .await
        .map_err(|e| format!("Cannot open {path}: {e}"))?;
    if meta.len() > MAX_READ_BYTES {
        return Err(format!(
            "File is too large ({:.1} MB). Limit is {} MB.",
            meta.len() as f64 / (1024.0 * 1024.0),
            MAX_READ_BYTES / (1024 * 1024)
        ));
    }
    tokio::fs::read(path)
        .await
        .map_err(|e| format!("Cannot read {path}: {e}"))
}

/// Read a file and return its raw base64 (no data-URL prefix). Used for image
/// thumbnails and for attaching an image to a message.
#[tauri::command]
pub async fn fb_read_base64(path: String) -> Result<String, String> {
    let bytes = read_capped(&path).await?;
    Ok(STANDARD.encode(bytes))
}

/// Read a file as UTF-8 text. Used for attaching a document inline.
#[tauri::command]
pub async fn fb_read_text(path: String) -> Result<String, String> {
    let bytes = read_capped(&path).await?;
    String::from_utf8(bytes).map_err(|_| "File is not valid UTF-8 text".to_string())
}

// ---------------------------------------------------------------------------
// Library
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone)]
pub struct LibraryItem {
    id: String,
    name: String,
    /// "image" or "file".
    kind: String,
    size: u64,
    /// Unix seconds. Sortable; the UI shows newest first.
    added_at: i64,
    ext: String,
}

fn library_dir() -> Result<PathBuf, String> {
    let base = crate::paths::app_dir_from_home()
        .ok_or_else(|| "Cannot resolve home directory".to_string())?;
    Ok(base.join("library"))
}

fn blobs_dir() -> Result<PathBuf, String> {
    Ok(library_dir()?.join("blobs"))
}

fn index_path() -> Result<PathBuf, String> {
    Ok(library_dir()?.join("index.json"))
}

async fn read_index() -> Result<Vec<LibraryItem>, String> {
    let p = index_path()?;
    match tokio::fs::read(&p).await {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|e| format!("Library index is corrupt: {e}")),
        // No index yet is the empty library, not an error.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("Cannot read library index: {e}")),
    }
}

async fn write_index(items: &[LibraryItem]) -> Result<(), String> {
    let dir = library_dir()?;
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("Cannot create library dir: {e}"))?;
    let json = serde_json::to_vec_pretty(items)
        .map_err(|e| format!("Cannot serialize library index: {e}"))?;
    tokio::fs::write(index_path()?, json)
        .await
        .map_err(|e| format!("Cannot write library index: {e}"))
}

fn new_id() -> String {
    // rand 0.10 moved `random()` off `Rng` (now an alias for the old `RngCore`)
    // and onto `RngExt`.
    use rand::RngExt;
    let bytes: [u8; 12] = rand::rng().random();
    hex::encode(bytes)
}

fn kind_for_ext(ext: &str) -> &'static str {
    if is_image_ext(ext) {
        "image"
    } else {
        "file"
    }
}

fn blob_path_for(id: &str, ext: &str) -> Result<PathBuf, String> {
    let name = if ext.is_empty() {
        id.to_string()
    } else {
        format!("{id}.{ext}")
    };
    Ok(blobs_dir()?.join(name))
}

/// Newest-first list of everything in the library.
#[tauri::command]
pub async fn library_list() -> Result<Vec<LibraryItem>, String> {
    let mut items = read_index().await?;
    items.sort_by_key(|b| std::cmp::Reverse(b.added_at));
    Ok(items)
}

/// Save raw base64 content into the library. Used when the user uploads a file
/// through the Library tab, or promotes a browsed file into the library.
#[tauri::command]
pub async fn library_save(
    name: String,
    data_base64: String,
) -> Result<LibraryItem, String> {
    let bytes = STANDARD
        .decode(data_base64.as_bytes())
        .map_err(|e| format!("Invalid base64 content: {e}"))?;
    if bytes.len() as u64 > MAX_READ_BYTES {
        return Err(format!(
            "File is too large. Limit is {} MB.",
            MAX_READ_BYTES / (1024 * 1024)
        ));
    }
    let ext = ext_of(Path::new(&name));
    persist_blob(&name, &ext, &bytes).await
}

/// Copy a file already on disk into the library.
#[tauri::command]
pub async fn library_import_path(path: String) -> Result<LibraryItem, String> {
    let bytes = read_capped(&path).await?;
    let p = PathBuf::from(&path);
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());
    let ext = ext_of(&p);
    persist_blob(&name, &ext, &bytes).await
}

async fn persist_blob(name: &str, ext: &str, bytes: &[u8]) -> Result<LibraryItem, String> {
    tokio::fs::create_dir_all(blobs_dir()?)
        .await
        .map_err(|e| format!("Cannot create library blobs dir: {e}"))?;

    let id = new_id();
    let blob = blob_path_for(&id, ext)?;
    tokio::fs::write(&blob, bytes)
        .await
        .map_err(|e| format!("Cannot write library blob: {e}"))?;

    let item = LibraryItem {
        id,
        name: name.to_string(),
        kind: kind_for_ext(ext).to_string(),
        size: bytes.len() as u64,
        added_at: chrono::Utc::now().timestamp(),
        ext: ext.to_string(),
    };

    let mut items = read_index().await?;
    items.push(item.clone());
    write_index(&items).await?;
    Ok(item)
}

async fn resolve_blob(id: &str) -> Result<(LibraryItem, PathBuf), String> {
    let items = read_index().await?;
    let item = items
        .into_iter()
        .find(|i| i.id == id)
        .ok_or_else(|| "Library item not found".to_string())?;
    let blob = blob_path_for(&item.id, &item.ext)?;
    Ok((item, blob))
}

/// Base64 of a library item's bytes — for attaching a stored image.
#[tauri::command]
pub async fn library_read_base64(id: String) -> Result<String, String> {
    let (_, blob) = resolve_blob(&id).await?;
    let bytes = tokio::fs::read(&blob)
        .await
        .map_err(|e| format!("Cannot read library blob: {e}"))?;
    Ok(STANDARD.encode(bytes))
}

/// UTF-8 text of a library item — for attaching a stored document inline.
#[tauri::command]
pub async fn library_read_text(id: String) -> Result<String, String> {
    let (_, blob) = resolve_blob(&id).await?;
    let bytes = tokio::fs::read(&blob)
        .await
        .map_err(|e| format!("Cannot read library blob: {e}"))?;
    String::from_utf8(bytes).map_err(|_| "File is not valid UTF-8 text".to_string())
}

/// Remove a library item: its blob and its index entry. Deleting something
/// that's already gone is a success, not an error.
#[tauri::command]
pub async fn library_delete(id: String) -> Result<(), String> {
    let mut items = read_index().await?;
    if let Some(pos) = items.iter().position(|i| i.id == id) {
        let item = items.remove(pos);
        let blob = blob_path_for(&item.id, &item.ext)?;
        // Best-effort blob removal; a missing blob still means "gone".
        let _ = tokio::fs::remove_file(&blob).await;
        write_index(&items).await?;
    }
    Ok(())
}
