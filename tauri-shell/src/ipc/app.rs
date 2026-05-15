//! Top-level app commands — most are real.

use super::IpcError;
use crate::cleanup;
use crate::events::{NavigateEvent, NAVIGATE};
use crate::state::{InstallationSnapshot, InstallationState};
use tauri::{AppHandle, Emitter, Manager, State};

#[tauri::command]
pub fn get_installation_state(state: State<'_, InstallationState>) -> InstallationSnapshot {
    state.read()
}

#[tauri::command]
pub fn navigate_to_page(app: AppHandle, path: String) -> Result<(), IpcError> {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
    app.emit(NAVIGATE, NavigateEvent { path })
        .map_err(|e| IpcError::Internal(e.to_string()))
}

#[tauri::command]
pub async fn quit_application(app: AppHandle) {
    cleanup::cleanup_all_processes(app.clone()).await;
    app.exit(0);
}

#[tauri::command]
pub fn get_app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

/// Stub: theme persistence is connector-specific (depends on where you
/// store user preferences).
#[tauri::command]
pub fn toggle_theme(_theme: String) -> Result<bool, IpcError> {
    Err(IpcError::not_implemented(
        "wire to your preferences store; see docs/typescript-port/20-features/feature-api-keys.md",
    ))
}
