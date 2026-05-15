//! Jupyter lifecycle — all stubs except the logs window opener.
//!
//! Contract: `docs/typescript-port/20-features/feature-jupyter.md`.
//! Process-id namespace: `jupyter-<env>`.
//!
//! Typical wiring: the connector spawns `conda run -n <env> jupyter lab
//! --no-browser --port=auto` via `process_spawn::spawn_with_streaming`,
//! captures the printed URL with token, and emits
//! `BACKEND_URL_DISCOVERED`. The renderer then displays it as an
//! "Open in browser" affordance.
//!
//! Two commands are real:
//! - `open_jupyter_logs_window` opens a per-environment log window using
//!   the shared `windows::open_logs_window` helper.
//! - `update_jupyter_status` is a pure event emitter — handy from
//!   another connector subprocess to broadcast lifecycle changes
//!   without re-entering the IPC layer.
//!
//! Related: `crate::events::JUPYTER_STATUS_UPDATE`, `BACKEND_URL_DISCOVERED`.

use std::sync::Arc;

use super::IpcError;
use crate::connector::{Connector, StartJupyterArgs};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "../bindings/", rename_all = "camelCase"))]
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
    app: AppHandle,
    environment: String,
    directory: String,
    working: String,
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<JupyterStatus, IpcError> {
    connector
        .start_jupyter_server(
            StartJupyterArgs {
                environment,
                directory,
                working,
            },
            app,
        )
        .await
        .map_err(IpcError::from)
}

#[tauri::command]
pub async fn stop_jupyter_server(
    app: AppHandle,
    environment: String,
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<bool, IpcError> {
    connector
        .stop_jupyter_server(environment, app)
        .await
        .map_err(IpcError::from)
}

#[tauri::command]
pub async fn check_jupyter_server(
    environment: String,
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<JupyterStatus, IpcError> {
    connector
        .check_jupyter_server(environment)
        .await
        .map_err(IpcError::from)
}

#[tauri::command]
pub async fn list_jupyter_servers(
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<serde_json::Value, IpcError> {
    connector.list_jupyter_servers().await.map_err(IpcError::from)
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
