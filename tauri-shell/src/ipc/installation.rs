//! Installation pipeline — all stubs.
//!
//! Each command's signature matches the documented contract in
//! `docs/typescript-port/20-features/feature-installation.md`. Wire to
//! your installer of choice (curl + bash, uv, conda, Python venv,
//! anything). For long-running steps, emit `INSTALL_PROGRESS` events
//! along the way.

use super::IpcError;
use crate::events::{InstallProgressEvent, INSTALL_PROGRESS};
use tauri::{AppHandle, Emitter};

/// Helper for stub implementations to broadcast a progress beat.
#[allow(dead_code)]
fn report(app: &AppHandle, step: &str, progress: f32, message: &str) {
    let _ = app.emit(
        INSTALL_PROGRESS,
        InstallProgressEvent {
            step: step.into(),
            progress,
            message: message.into(),
        },
    );
}

/// Validate paths and pre-write settings files. No process spawning.
#[tauri::command]
pub fn install_to_directory(
    _directory: String,
    _user_data_directory: String,
) -> Result<bool, IpcError> {
    // TODO: connect to your backend.
    // Reference: feature-installation.md §2.4 "Submit — Begin Installation".
    Err(IpcError::not_implemented("install_to_directory"))
}

/// Download + run the runtime installer (Miniforge in the OpenBB reference).
/// Emits `INSTALL_PROGRESS` events throughout.
#[tauri::command]
pub async fn install_conda(
    _app: AppHandle,
    _directory: String,
) -> Result<bool, IpcError> {
    // TODO: connect to your backend.
    Err(IpcError::not_implemented("install_conda"))
}

#[tauri::command]
pub async fn setup_python_environment(
    _app: AppHandle,
    _directory: String,
    _python_version: String,
) -> Result<bool, IpcError> {
    Err(IpcError::not_implemented("setup_python_environment"))
}

#[tauri::command]
pub fn abort_installation(_directory: String) -> Result<(), IpcError> {
    Err(IpcError::not_implemented("abort_installation"))
}

#[tauri::command]
pub fn get_installation_status() -> Result<crate::state::InstallationProgress, IpcError> {
    let g = crate::state::INSTALLATION_PROGRESS
        .lock()
        .map_err(|e| IpcError::Internal(e.to_string()))?;
    Ok(g.clone())
}

#[tauri::command]
pub fn create_default_backend_services() -> Result<(), IpcError> {
    Err(IpcError::not_implemented("create_default_backend_services"))
}

#[tauri::command]
pub fn update_openbb_settings(
    _conda_dir: String,
    _environment: String,
) -> Result<(), IpcError> {
    Err(IpcError::not_implemented("update_openbb_settings"))
}

#[tauri::command]
pub fn get_installation_directory() -> Result<String, IpcError> {
    Err(IpcError::not_implemented(
        "get_installation_directory — read from your settings file",
    ))
}

#[tauri::command]
pub fn get_userdata_directory() -> Result<String, IpcError> {
    Err(IpcError::not_implemented(
        "get_userdata_directory — read from your settings file",
    ))
}
