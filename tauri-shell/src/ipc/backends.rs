//! Backend service CRUD + lifecycle — all stubs.
//!
//! Contract: `docs/typescript-port/20-features/feature-backend-services.md`.
//!
//! Start/stop must use [`process_spawn::spawn_with_streaming`] for log
//! streaming and [`state::RunningProcesses`] for tracked-kill support.

use super::IpcError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::AppHandle;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendService {
    pub id: String,
    pub name: String,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env_vars: Option<HashMap<String, String>>,
    pub environment: String,
    pub auto_start: bool,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
}

#[tauri::command]
pub fn list_backend_services() -> Result<Vec<BackendService>, IpcError> {
    Err(IpcError::not_implemented(
        "list_backend_services — read your backends.json",
    ))
}

#[tauri::command]
pub fn create_backend_service(_backend: BackendService) -> Result<BackendService, IpcError> {
    Err(IpcError::not_implemented("create_backend_service"))
}

#[tauri::command]
pub fn update_backend_service(_backend: BackendService) -> Result<BackendService, IpcError> {
    Err(IpcError::not_implemented("update_backend_service"))
}

#[tauri::command]
pub async fn delete_backend_service(_app: AppHandle, _id: String) -> Result<(), IpcError> {
    Err(IpcError::not_implemented("delete_backend_service"))
}

#[tauri::command]
pub async fn start_backend_service(
    _app: AppHandle,
    _id: String,
) -> Result<BackendService, IpcError> {
    Err(IpcError::not_implemented("start_backend_service"))
}

#[tauri::command]
pub async fn stop_backend_service(_app: AppHandle, _id: String) -> Result<(), IpcError> {
    Err(IpcError::not_implemented("stop_backend_service"))
}

#[tauri::command]
pub fn open_backend_logs_window(app: AppHandle, id: String) -> Result<(), IpcError> {
    crate::windows::open_logs_window(
        &app,
        "backend-logs",
        &id,
        "/backend-logs",
        "id",
        "Backend Logs",
    )
    .map(|_| ())
    .map_err(|e| IpcError::Internal(e.to_string()))
}
