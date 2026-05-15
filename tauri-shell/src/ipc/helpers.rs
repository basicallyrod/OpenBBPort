//! Misc helper commands. Mostly real; a couple of working-directory
//! helpers are stubs because they assume your connector's settings shape.

use super::IpcError;
use crate::path_utils;
use crate::windows;
use tauri::AppHandle;
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

// --- Stubs below depend on the connector's settings schema -----------------

#[tauri::command]
pub fn get_working_directory(_default_dir: Option<String>) -> Result<String, IpcError> {
    Err(IpcError::not_implemented(
        "wire to your preferences store",
    ))
}

#[tauri::command]
pub fn save_working_directory(_path: String) -> Result<bool, IpcError> {
    Err(IpcError::not_implemented(
        "wire to your preferences store",
    ))
}
