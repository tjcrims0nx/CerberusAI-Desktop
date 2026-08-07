//! In-app web browser — live-rendered pages with screenshot & text extraction.
//!
//! Opens a sandboxed WebView2 window (no IPC capability → the remote page cannot
//! reach any app command). Navigation is driven from Rust via `eval`; the page's
//! text is pulled out through WebView2 `ExecuteScript`; the screenshot is grabbed
//! with `xcap`. Because the browser window carries no capability, none of this
//! gives the loaded site access to the filesystem or to HELIX's commands.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

const BROWSER_LABEL: &str = "helix-browser";
const BROWSER_TITLE: &str = "HELIX Browser";

/// Open or navigate the browser window to a URL. Reuses the window if already open.
#[tauri::command]
pub async fn browser_open(url: String, app: AppHandle) -> Result<(), String> {
    let parsed = tauri::Url::parse(&url).map_err(|e| format!("Invalid URL: {e}"))?;
    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        return Err("Only HTTPS and HTTP URLs are allowed.".into());
    }

    if let Some(window) = app.get_webview_window(BROWSER_LABEL) {
        let encoded = serde_json::to_string(&url).map_err(|e| format!("Failed to encode URL: {e}"))?;
        let js = format!("window.location.href = {encoded}");
        window.eval(&js).map_err(|e| format!("Failed to navigate: {e}"))?;
        let _ = window.set_focus();
    } else {
        WebviewWindowBuilder::new(&app, BROWSER_LABEL, WebviewUrl::External(parsed))
            .title(BROWSER_TITLE)
            .inner_size(1100.0, 780.0)
            // Sandbox: refuse local-file navigations; the window has no IPC
            // capability, so even a hostile page cannot call app commands.
            .on_navigation(|u| u.scheme() != "file")
            .build()
            .map_err(|e| format!("Failed to create browser window: {e}"))?;
    }
    Ok(())
}

fn eval_in_browser(app: &AppHandle, js: &str) -> Result<(), String> {
    let window = app
        .get_webview_window(BROWSER_LABEL)
        .ok_or("Browser window is not open.")?;
    window.eval(js).map_err(|e| format!("Navigation failed: {e}"))
}

/// Navigate back in the browser window's history.
#[tauri::command]
pub fn browser_back(app: AppHandle) -> Result<(), String> {
    eval_in_browser(&app, "history.back()")
}

/// Navigate forward in the browser window's history.
#[tauri::command]
pub fn browser_forward(app: AppHandle) -> Result<(), String> {
    eval_in_browser(&app, "history.forward()")
}

/// Reload the browser window's current page.
#[tauri::command]
pub fn browser_reload(app: AppHandle) -> Result<(), String> {
    eval_in_browser(&app, "location.reload()")
}

/// Read the browser window's current URL (for keeping the address bar in sync).
#[tauri::command]
pub fn browser_get_url(app: AppHandle) -> Result<String, String> {
    let window = app
        .get_webview_window(BROWSER_LABEL)
        .ok_or("Browser window is not open.")?;
    let url = window.url().map_err(|e| format!("Cannot read URL: {e}"))?;
    Ok(url.to_string())
}

/// Close the browser window.
#[tauri::command]
pub fn browser_close(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(BROWSER_LABEL) {
        window.close().map_err(|e| format!("Failed to close: {e}"))?;
    }
    Ok(())
}

#[derive(Serialize, Deserialize)]
pub struct PageText {
    pub url: String,
    pub title: String,
    pub text: String,
}

/// The JS whose JSON result WebView2 hands back. Returning an object (not a
/// pre-stringified string) means WebView2 JSON-encodes it exactly once, so the
/// result parses straight into `PageText`.
const EXTRACT_JS: &str =
    "({url: location.href, title: document.title, text: document.body ? document.body.innerText : ''})";

/// Extract the current page's URL, title and visible text via WebView2 ExecuteScript.
#[cfg(target_os = "windows")]
#[tauri::command]
pub async fn browser_extract_text(app: AppHandle) -> Result<PageText, String> {
    use std::sync::{Arc, Mutex};
    use webview2_com::ExecuteScriptCompletedHandler;
    // windows 0.61 (aliased) MUST match webview2-com-sys 0.38.2's `windows`, or
    // the PCWSTR / Result types won't line up with ExecuteScript's signature.
    use windows061::core::PCWSTR;

    let window = app
        .get_webview_window(BROWSER_LABEL)
        .ok_or("Browser window is not open.")?;

    let (tx, rx) = tokio::sync::oneshot::channel::<Result<PageText, String>>();
    let tx = Arc::new(Mutex::new(Some(tx)));

    window
        .with_webview(move |webview| {
            let controller = webview.controller();
            // Null-terminated UTF-16; must outlive the (synchronous) ExecuteScript call.
            let script: Vec<u16> = EXTRACT_JS.encode_utf16().chain(std::iter::once(0)).collect();
            let tx_handler = tx.clone();

            // Safety: COM interface calls. The completion handler is invoked on
            // the same (UI) thread once the script finishes.
            unsafe {
                let core = match controller.CoreWebView2() {
                    Ok(c) => c,
                    Err(e) => {
                        if let Some(s) = tx.lock().unwrap().take() {
                            let _ = s.send(Err(format!("Cannot access CoreWebView2: {e}")));
                        }
                        return;
                    }
                };

                let handler = ExecuteScriptCompletedHandler::create(Box::new(
                    move |error: windows061::core::Result<()>,
                          json_result: String|
                          -> windows061::core::Result<()> {
                        let outcome = match error {
                            Ok(()) => serde_json::from_str::<PageText>(&json_result)
                                .map_err(|e| format!("Could not parse page content: {e}")),
                            Err(e) => Err(format!("ExecuteScript error: {e}")),
                        };
                        if let Some(s) = tx_handler.lock().unwrap().take() {
                            let _ = s.send(outcome);
                        }
                        Ok(())
                    },
                ));

                if let Err(e) = core.ExecuteScript(PCWSTR::from_raw(script.as_ptr()), &handler) {
                    if let Some(s) = tx.lock().unwrap().take() {
                        let _ = s.send(Err(format!("ExecuteScript failed: {e}")));
                    }
                }
            }
        })
        .map_err(|e| format!("with_webview failed: {e}"))?;

    rx.await
        .map_err(|_| "The page reader was dropped before returning.".to_string())?
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
pub async fn browser_extract_text(_app: AppHandle) -> Result<PageText, String> {
    Err("Page text extraction is only supported on Windows.".into())
}

/// Capture the browser window as a PNG and return it as raw base64 (no data-URL prefix).
#[cfg(target_os = "windows")]
#[tauri::command]
pub async fn browser_screenshot(app: AppHandle) -> Result<String, String> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use std::io::Cursor;
    use xcap::Window;

    // Bring the window forward so the compositor has fresh pixels to hand GDI.
    if let Some(window) = app.get_webview_window(BROWSER_LABEL) {
        let _ = window.unminimize();
        let _ = window.set_focus();
    } else {
        return Err("Browser window is not open.".into());
    }
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;

    // xcap is blocking; keep it off the async worker.
    tokio::task::spawn_blocking(|| {
        let windows = Window::all().map_err(|e| format!("Cannot enumerate windows: {e}"))?;
        let target = windows
            .into_iter()
            .find(|w| w.title().ok().as_deref() == Some(BROWSER_TITLE))
            .ok_or("Browser window not found. Make sure it is open and not minimized.")?;

        if target.is_minimized().unwrap_or(false) {
            return Err("Browser window is minimized — restore it and try again.".into());
        }

        let rgba = target
            .capture_image()
            .map_err(|e| format!("Screenshot failed: {e}"))?;

        let dyn_img = xcap::image::DynamicImage::ImageRgba8(rgba);
        let mut png: Vec<u8> = Vec::new();
        dyn_img
            .write_to(&mut Cursor::new(&mut png), xcap::image::ImageFormat::Png)
            .map_err(|e| format!("PNG encoding failed: {e}"))?;

        Ok(STANDARD.encode(png))
    })
    .await
    .map_err(|e| format!("Screenshot task failed: {e}"))?
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
pub async fn browser_screenshot(_app: AppHandle) -> Result<String, String> {
    Err("Screenshot capture is only supported on Windows.".into())
}
