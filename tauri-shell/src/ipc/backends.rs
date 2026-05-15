//! Backend service CRUD + lifecycle — all stubs.
//!
//! Contract: `docs/typescript-port/20-features/feature-backend-services.md`.
//!
//! Start/stop must use `crate::process_spawn::spawn_with_streaming` for log
//! streaming and `crate::state::RunningProcesses` for tracked-kill support.
//! The shell ships one real command here — `open_backend_logs_window` —
//! which builds a per-backend log window using `windows::open_logs_window`.
//! Wire the rest of these to your connector (HTTP proxy, sidecar, or pure
//! Rust). The list of services is connector-owned; the shell does not
//! persist anything between calls.

use std::sync::Arc;

use super::IpcError;
use crate::connector::Connector;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::{AppHandle, State};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "../bindings/", rename_all = "camelCase"))]
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
pub async fn list_backend_services(
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<Vec<BackendService>, IpcError> {
    connector.list_backend_services().await.map_err(IpcError::from)
}

#[tauri::command]
pub async fn create_backend_service(
    backend: BackendService,
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<BackendService, IpcError> {
    connector
        .create_backend_service(backend)
        .await
        .map_err(IpcError::from)
}

#[tauri::command]
pub async fn update_backend_service(
    backend: BackendService,
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<BackendService, IpcError> {
    connector
        .update_backend_service(backend)
        .await
        .map_err(IpcError::from)
}

#[tauri::command]
pub async fn delete_backend_service(
    app: AppHandle,
    id: String,
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<(), IpcError> {
    connector
        .delete_backend_service(id, app)
        .await
        .map_err(IpcError::from)
}

#[tauri::command]
pub async fn start_backend_service(
    app: AppHandle,
    id: String,
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<BackendService, IpcError> {
    connector
        .start_backend_service(id, app)
        .await
        .map_err(IpcError::from)
}

#[tauri::command]
pub async fn stop_backend_service(
    app: AppHandle,
    id: String,
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<(), IpcError> {
    connector
        .stop_backend_service(id, app)
        .await
        .map_err(IpcError::from)
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
