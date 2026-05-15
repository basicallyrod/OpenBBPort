//! Jupyter lifecycle — all stubs except the logs window opener.
//!
//! Contract: `docs/typescript-port/20-features/feature-jupyter.md`.
//! Process-id namespace: `jupyter-<env>`.

use super::IpcError;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JupyterStatus {
    pub running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_id: Option<u32>,
}

#[tauri::command]
pub async fn start_jupyter_server(
    _app: AppHandle,
    _environment: String,
    _directory: String,
    _working: String,
) -> Result<JupyterStatus, IpcError> {
    Err(IpcError::not_implemented("start_jupyter_server"))
}

#[tauri::command]
pub async fn stop_jupyter_server(_app: AppHandle, _environment: String) -> Result<bool, IpcError> {
    Err(IpcError::not_implemented("stop_jupyter_server"))
}

#[tauri::command]
pub fn check_jupyter_server(_environment: String) -> Result<JupyterStatus, IpcError> {
    Err(IpcError::not_implemented("check_jupyter_server"))
}

#[tauri::command]
pub fn list_jupyter_servers() -> Result<serde_json::Value, IpcError> {
    Err(IpcError::not_implemented("list_jupyter_servers"))
}

#[tauri::command]
pub fn open_jupyter_logs_window(app: AppHandle, environment: String) -> Result<(), IpcError> {
    crate::windows::open_logs_window(
        &app,
        "jupyter-logs",
        &environment,
        "/jupyter-logs",
        "env",
        "Jupyter Logs",
    )
    .map(|_| ())
    .map_err(|e| IpcError::Internal(e.to_string()))
}

#[tauri::command]
pub fn update_jupyter_status(
    app: AppHandle,
    environment_name: String,
    status: String,
) -> Result<(), IpcError> {
    use tauri::Emitter;
    app.emit(
        crate::events::JUPYTER_STATUS_UPDATE,
        crate::events::JupyterStatusEvent {
            environment_name,
            status,
        },
    )
    .map_err(|e| IpcError::Internal(e.to_string()))
}
