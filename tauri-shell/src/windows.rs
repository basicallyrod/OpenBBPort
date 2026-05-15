//! Window management helpers.
//!
//! - [`apply_close_to_tray`] — intercept the main window's close button to
//!   hide it instead of destroying it, so the app keeps running in the
//!   tray.
//! - [`open_or_focus_window`] — generic helper for re-openable child
//!   windows (logs windows etc.). Re-show if it already exists; otherwise
//!   build a fresh `WebviewWindow` with a `prevent_close + hide` listener.
//! - [`open_external_url_window`] — pop a Tauri webview pointed at an
//!   arbitrary HTTPS URL (e.g. docs, Jupyter UI).
//!
//! Note: `open_external_url_window` is the highest-severity surface in
//! the original Tauri reference. The current default capability list
//! does NOT extend `fs:*` / `shell:*` to popped windows — see
//! `capabilities/logs-window.json`. If you allow arbitrary URLs, audit
//! the window label glob in your capability files.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};

#[cfg(target_os = "macos")]
fn macos_window_styling(_window: &WebviewWindow) {
    // The original reference set the NSWindow background to opaque black
    // and configured a transparent title bar via Obj-C. Hook the same
    // here if you want pixel-perfect parity. Left as a no-op so this
    // crate builds without objc2 setup on non-mac hosts.
}

#[cfg(not(target_os = "macos"))]
fn macos_window_styling(_window: &WebviewWindow) {}

/// Wire close-to-tray on a window. Drop the returned guard to detach.
pub fn apply_close_to_tray(window: WebviewWindow) {
    let w = window.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            let _ = w.hide();
            api.prevent_close();
        }
    });
}

#[derive(Debug, Clone)]
pub struct ChildWindowConfig {
    pub label: String,
    pub url: WebviewUrl,
    pub title: String,
    pub width: f64,
    pub height: f64,
    pub min_width: Option<f64>,
    pub min_height: Option<f64>,
    pub resizable: bool,
    pub center: bool,
    /// If true, intercept close and hide rather than destroy.
    pub hide_on_close: bool,
}

impl ChildWindowConfig {
    pub fn logs(label: impl Into<String>, route: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            url: WebviewUrl::App(route.into().into()),
            title: title.into(),
            width: 1000.0,
            height: 600.0,
            min_width: Some(600.0),
            min_height: Some(200.0),
            resizable: true,
            center: true,
            hide_on_close: true,
        }
    }

    pub fn external(label: impl Into<String>, url: url::Url, title: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            url: WebviewUrl::External(url),
            title: title.into(),
            width: 1200.0,
            height: 800.0,
            min_width: None,
            min_height: None,
            resizable: true,
            center: true,
            hide_on_close: false,
        }
    }
}

/// Re-open an existing window by label, or create a new one.
pub fn open_or_focus_window(
    app: &AppHandle,
    config: ChildWindowConfig,
) -> tauri::Result<WebviewWindow> {
    if let Some(existing) = app.get_webview_window(&config.label) {
        let _ = existing.show();
        let _ = existing.set_focus();
        return Ok(existing);
    }

    let mut builder = WebviewWindowBuilder::new(app, &config.label, config.url.clone())
        .title(&config.title)
        .inner_size(config.width, config.height)
        .resizable(config.resizable);
    if let (Some(w), Some(h)) = (config.min_width, config.min_height) {
        builder = builder.min_inner_size(w, h);
    }
    if config.center {
        builder = builder.center();
    }

    let window = builder.build()?;
    macos_window_styling(&window);

    if config.hide_on_close {
        let w = window.clone();
        window.on_window_event(move |event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = w.hide();
                api.prevent_close();
            }
        });
    }

    Ok(window)
}

/// Convenience for the per-process logs windows: builds a label like
/// `<prefix>-<id>` and routes to `/<route>?<id_key>=<id>`.
pub fn open_logs_window(
    app: &AppHandle,
    label_prefix: &str,
    id: &str,
    route_path: &str,
    id_key: &str,
    title: &str,
) -> tauri::Result<WebviewWindow> {
    let label = format!("{label_prefix}-{id}");
    let route = format!("{route_path}?{id_key}={}", urlencode(id));
    open_or_focus_window(
        app,
        ChildWindowConfig::logs(label, route, title),
    )
}

/// Open an arbitrary external URL in a new Tauri webview window. Note
/// the capability scope must explicitly include the label glob used
/// (default: `url-*`).
pub fn open_external_url_window(
    app: &AppHandle,
    url: &str,
    title: &str,
) -> tauri::Result<WebviewWindow> {
    let parsed = url::Url::parse(url)
        .map_err(|e| tauri::Error::Anyhow(anyhow::anyhow!("invalid URL: {e}")))?;
    let label = format!(
        "url-{}",
        chrono::Utc::now().timestamp_millis()
    );
    open_or_focus_window(app, ChildWindowConfig::external(label, parsed, title))
}

fn urlencode(s: &str) -> String {
    // tiny URL-component encoder — adequate for ids/env names
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            other => {
                let mut b = [0u8; 4];
                other
                    .encode_utf8(&mut b)
                    .bytes()
                    .map(|b| format!("%{:02X}", b))
                    .collect()
            }
        })
        .collect()
}
