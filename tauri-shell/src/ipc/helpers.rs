//! Misc helper commands. Mostly real; a couple of working-directory
//! helpers are stubs because they assume your connector's settings shape.
//!
//! Topics covered:
//! - Path discovery (`get_home_directory`, `get_settings_directory`)
//!   resolved by `crate::path_utils`.
//! - Native file/directory pickers (`select_directory`, `select_file`)
//!   delegating to the `tauri-plugin-dialog`.
//! - Filesystem existence checks (`check_directory_exists`,
//!   `check_file_exists`) — convenience over `std::fs::metadata` so the
//!   renderer doesn't need fs capabilities for read-only probes.
//! - Window helpers (`open_url_in_window`, `open_workspace_in_browser`,
//!   `open_logs_window`) thin wrappers over `crate::windows`.
//! - The stubbed `get_working_directory`/`save_working_directory` pair —
//!   wire these to your settings file once the schema is decided.
//!
//! Security note: `open_url_in_window` must NEVER receive a URL that
//! contains a secret in the query string — window titles can be read by
//! OS accessibility APIs. See the security section in the crate-level
//! README.

use std::sync::Arc;

use super::IpcError;
use crate::connector::Connector;
use crate::path_utils;
use crate::windows;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::{DialogExt, FilePath};

#[tauri::command]
pub fn get_home_directory() -> Result<String, IpcError> {
    path_utils::home_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .ok_or_else(|| IpcError::Internal("home directory not available".into()))
}

#[tauri::command]
pub fn get_settings_directory() -> Result<String, IpcError> {
    path_utils::settings_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .ok_or_else(|| IpcError::Internal("settings directory not available".into()))
}

#[tauri::command]
pub fn check_directory_exists(path: String) -> bool {
    std::path::Path::new(&path).is_dir()
}

#[tauri::command]
pub fn check_file_exists(path: String) -> bool {
    std::path::Path::new(&path).is_file()
}

#[tauri::command]
pub async fn select_directory(app: AppHandle, prompt: Option<String>) -> Result<String, IpcError> {
    let (tx, rx) = tokio::sync::oneshot::channel::<Option<FilePath>>();
    let mut dlg = app.dialog().file();
    if let Some(p) = prompt {
        dlg = dlg.set_title(p);
    }
    dlg.pick_folder(move |chosen| {
        let _ = tx.send(chosen);
    });
    let chosen = rx.await.map_err(|e| IpcError::Internal(e.to_string()))?;
    match chosen {
        Some(FilePath::Path(p)) => Ok(p.to_string_lossy().into_owned()),
        Some(FilePath::Url(u)) => Ok(u.to_string()),
        None => Err(IpcError::InvalidArgument("user cancelled".into())),
    }
}

#[tauri::command]
pub async fn select_file(app: AppHandle, _filter: Option<String>) -> Result<String, IpcError> {
    let (tx, rx) = tokio::sync::oneshot::channel::<Option<FilePath>>();
    app.dialog().file().pick_file(move |chosen| {
        let _ = tx.send(chosen);
    });
    let chosen = rx.await.map_err(|e| IpcError::Internal(e.to_string()))?;
    match chosen {
        Some(FilePath::Path(p)) => Ok(p.to_string_lossy().into_owned()),
        Some(FilePath::Url(u)) => Ok(u.to_string()),
        None => Err(IpcError::InvalidArgument("user cancelled".into())),
    }
}

#[tauri::command]
pub fn open_url_in_window(
    app: AppHandle,
    url: String,
    title: Option<String>,
) -> Result<(), IpcError> {
    let title = title.unwrap_or_else(|| "External".into());
    windows::open_external_url_window(&app, &url, &title)
        .map(|_| ())
        .map_err(|e| IpcError::Internal(e.to_string()))
}

#[tauri::command]
pub fn open_workspace_in_browser(url: Option<String>) -> Result<(), IpcError> {
    let url = url.unwrap_or_else(|| "https://example.com".into());
    open::that(&url).map_err(|e| IpcError::Internal(e.to_string()))?;
    Ok(())
}

#[tauri::command]
pub fn open_logs_window(
    app: AppHandle,
    label_prefix: String,
    id: String,
    route: String,
    id_key: Option<String>,
    title: Option<String>,
) -> Result<(), IpcError> {
    let id_key = id_key.unwrap_or_else(|| "id".into());
    let title = title.unwrap_or_else(|| "Logs".into());
    windows::open_logs_window(&app, &label_prefix, &id, &route, &id_key, &title)
        .map(|_| ())
        .map_err(|e| IpcError::Internal(e.to_string()))
}

// --- Connector-backed below: depend on your preferences store --------------

/// Returns the persisted working directory via [`Connector::get_working_directory`].
#[tauri::command]
pub async fn get_working_directory(
    default_dir: Option<String>,
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<String, IpcError> {
    connector
        .get_working_directory(default_dir)
        .await
        .map_err(IpcError::from)
}

/// Persists the working directory via [`Connector::save_working_directory`].
#[tauri::command]
pub async fn save_working_directory(
    path: String,
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<bool, IpcError> {
    connector
        .save_working_directory(path)
        .await
        .map_err(IpcError::from)
}
