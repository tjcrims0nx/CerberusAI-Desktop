//! First-run Ollama runtime tuning.
//!
//! On Windows, Ollama reads environment variables from the user's environment
//! at startup. Without sane defaults — particularly `OLLAMA_KEEP_ALIVE` — every
//! prompt after a brief idle pays a multi-second cold-load penalty, which
//! routinely trips the desktop's stream timeout and surfaces as
//! "No response from model — it may still be loading."
//!
//! This module sets a small set of safe defaults the very first time Cerberus
//! Desktop launches on a given machine, then records that fact in the user's
//! config directory so we never touch their environment again. Users who set
//! the variables themselves are detected and respected (we don't overwrite
//! existing values, only fill in missing ones).

use std::path::PathBuf;

/// Marker file so this only runs once per machine. Lives next to chat history
/// in the standard Cerberus app dir so uninstall + reinstall starts fresh.
fn marker_path() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    Some(home.join(".CerberusAI").join(".ollama-tuned"))
}

/// Returns true if the marker indicates we've already tuned. We don't read
/// the contents, only existence — keeping the format opaque means we can
/// expand to a TOML record later without invalidating older markers.
fn already_tuned() -> bool {
    marker_path().map(|p| p.exists()).unwrap_or(false)
}

fn write_marker() {
    if let Some(p) = marker_path() {
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(
            &p,
            format!(
                "tuned by cerberus-desktop v{} at {}",
                env!("CARGO_PKG_VERSION"),
                chrono::Utc::now().to_rfc3339()
            ),
        );
    }
}

/// (name, recommended value) pairs. Order doesn't matter.
const TUNING: &[(&str, &str)] = &[
    // Keep models resident in RAM/VRAM for 30 minutes between prompts so the
    // user doesn't pay a cold-load on every response.
    ("OLLAMA_KEEP_ALIVE", "30m"),
    // Allow 2 concurrent generations per loaded model.
    ("OLLAMA_NUM_PARALLEL", "2"),
    // Flash attention reduces attention memory bandwidth ~30%.
    ("OLLAMA_FLASH_ATTENTION", "1"),
    // q8_0 KV cache halves KV memory at imperceptible quality cost
    // (requires flash attention, which we just turned on).
    ("OLLAMA_KV_CACHE_TYPE", "q8_0"),
];

/// Apply tuning if we haven't already on this machine. Safe to call on every
/// launch — it's idempotent and quick.
///
/// Returns true if any value changed (the caller may want to bounce Ollama).
#[cfg(windows)]
pub fn apply_first_run_tuning() -> bool {
    if already_tuned() {
        return false;
    }

    let mut changed = false;
    for (name, value) in TUNING {
        match std::env::var(name) {
            Ok(existing) if !existing.is_empty() => {
                log::info!("[tuning] {name} already set to {existing}; leaving alone");
            }
            _ => {
                if set_user_env_var(name, value) {
                    log::info!("[tuning] set {name}={value}");
                    changed = true;
                } else {
                    log::warn!("[tuning] failed to set {name}");
                }
            }
        }
    }

    write_marker();
    changed
}

#[cfg(not(windows))]
pub fn apply_first_run_tuning() -> bool {
    // On macOS / Linux Ollama is started by launchd / systemd which doesn't
    // read user env, so this style of fix wouldn't apply. We still write the
    // marker so we don't pester users on every launch.
    if !already_tuned() {
        write_marker();
    }
    false
}

/// Set a *user-level* environment variable on Windows by writing to the
/// `HKCU\Environment` registry key, then broadcasting WM_SETTINGCHANGE so
/// new processes pick it up. This mirrors what the installer's
/// `[Environment]::SetEnvironmentVariable($name, $value, "User")` does.
#[cfg(windows)]
fn set_user_env_var(name: &str, value: &str) -> bool {
    use winreg::{enums::*, RegKey};

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let env = match hkcu.open_subkey_with_flags("Environment", KEY_WRITE) {
        Ok(e) => e,
        Err(e) => {
            log::warn!("could not open HKCU\\Environment for write: {e}");
            return false;
        }
    };
    if let Err(e) = env.set_value(name, &value) {
        log::warn!("could not set {name}: {e}");
        return false;
    }

    // Broadcast WM_SETTINGCHANGE so already-running processes (including the
    // Ollama tray app on next start) see the new value without a logoff.
    broadcast_environment_change();
    true
}

#[cfg(windows)]
fn broadcast_environment_change() {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::Foundation::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    let env: Vec<u16> = OsStr::new("Environment")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let mut result: usize = 0;
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0 as WPARAM,
            env.as_ptr() as LPARAM,
            SMTO_ABORTIFHUNG,
            5000,
            &mut result,
        );
    }
}
