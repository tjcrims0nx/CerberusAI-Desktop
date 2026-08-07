//! Where HELIX keeps its data on disk.
//!
//! Every model, the llama.cpp engine, installed plugins, and the encrypted
//! database live under one directory in the user's home. That path used to be
//! spelled out inline at thirteen call sites, which is how a rename gets done
//! nine-tenths of the way and leaves the rest pointing at a directory that no
//! longer exists. It is defined once here instead.

use std::path::{Path, PathBuf};

/// The app directory's name inside the user's home.
pub const APP_DIR_NAME: &str = ".HELIX";

/// What the app directory was called before the HELIX rename.
///
/// Only [`crate::migrate`] should need this: it moves the old directory across
/// on first launch. Everything else works from [`APP_DIR_NAME`].
pub const LEGACY_APP_DIR_NAME: &str = ".CerberusAI";

/// The app directory under `home`.
pub fn app_dir_in(home: &Path) -> PathBuf {
    home.join(APP_DIR_NAME)
}

/// The app directory, resolved through Tauri's path API.
///
/// Generic over `Manager` so it accepts both the `AppHandle` the commands are
/// handed and the `&mut App` that `setup` gets.
///
/// Falls back to the current directory when the home directory cannot be
/// resolved, matching what every call site did individually before.
pub fn app_dir<R: tauri::Runtime, M: tauri::Manager<R>>(app: &M) -> PathBuf {
    app.path()
        .home_dir()
        .map(|home| app_dir_in(&home))
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// The app directory, resolved without a Tauri handle.
///
/// For code that runs outside the app's lifecycle — first-run tuning, the
/// startup migration, tests.
pub fn app_dir_from_home() -> Option<PathBuf> {
    dirs::home_dir().as_deref().map(app_dir_in)
}
