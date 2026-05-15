//! Lifecycle commands for the Python `openbb-api` REST server.
//!
//! These are stubs in the sense that they require a connector to actually
//! spawn the Python process — but the shapes match the catalog in
//! `feature-platform-rest-api.md` and `feature-backend-services.md`.
//!
//! Wire to your backend by overriding the bodies, or by registering a
//! `ServerConnector` impl as managed state. The shell provides:
//! - A typed `ServerSpec` describing where/how the server runs
//! - A typed `ServerStatus` for query results
//! - The `Proxy::set_base_url` call so HTTP requests land on the right server

use std::sync::Arc;

use super::IpcError;
use crate::connector::Connector;
use crate::proxy::Proxy;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "../bindings/", rename_all = "camelCase"))]
pub struct ServerSpec {
    pub id: String,
    pub host: String,
    pub port: u16,
    /// Conda env name or Python venv path. Connector decides which.
    pub environment: String,
    /// Optional path to a `.env` file the server should source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env_file: Option<String>,
    /// Extra env vars. Merged after `env_file`.
    #[serde(default)]
    pub env_vars: std::collections::HashMap<String, String>,
    /// CLI args to append to `openbb-api`. E.g. `["--ssl-keyfile", "key.pem"]`.
    #[serde(default)]
    pub extra_args: Vec<String>,
    /// Working directory for the spawned process.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "../bindings/", rename_all = "camelCase"))]
pub struct ServerStatus {
    pub id: String,
    pub running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Spawn the Python `openbb-api` REST server via the registered
/// [`Connector`]. The connector typically:
///   1. Validates the command for dangerous patterns
///   2. Generates a shell wrapper that activates the env + exports vars
///   3. Calls `spawn_with_streaming(...)` to register a `backend-<id>` log
///      channel
///   4. Inserts the child into [`crate::state::RunningProcesses`]
///   5. Parses `Started server process [N]` from logs to discover the PID
///   6. Parses the listen URL from uvicorn's banner (with a debounce)
///   7. Emits `backend-url-discovered` once the URL is confirmed
///   8. Calls [`Proxy::set_base_url`] with the discovered URL
#[tauri::command]
pub async fn server_spawn(
    app: AppHandle,
    spec: ServerSpec,
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<ServerStatus, IpcError> {
    connector.server_spawn(spec, app).await.map_err(IpcError::from)
}

/// Stop the server by id. The connector runs the full stop dance:
///   port-based kill → tracked-child kill → PID fallback → state reset.
#[tauri::command]
pub async fn server_stop(
    app: AppHandle,
    id: String,
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<(), IpcError> {
    connector.server_stop(id, app).await.map_err(IpcError::from)
}

#[tauri::command]
pub async fn server_status(
    id: String,
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<ServerStatus, IpcError> {
    connector.server_status(id).await.map_err(IpcError::from)
}

#[tauri::command]
pub async fn server_list(
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<Vec<ServerStatus>, IpcError> {
    connector.server_list().await.map_err(IpcError::from)
}

/// Convenience: point the HTTP proxy at a server URL without spawning.
/// Useful when the user runs the Python server manually.
#[tauri::command]
pub fn server_attach(url: String, proxy: State<'_, Proxy>) -> Result<(), IpcError> {
    proxy.set_base_url(url);
    Ok(())
}

#[tauri::command]
pub async fn server_health(proxy: State<'_, Proxy>) -> Result<serde_json::Value, IpcError> {
    use crate::proxy::ProxyError;
    match proxy.get_raw::<serde_json::Value>("/").await {
        Ok(v) => Ok(v),
        Err(ProxyError::Http { status, .. }) if status > 0 => {
            // Server responded — that's healthy enough for a health probe.
            Ok(serde_json::json!({"status": "ok", "http_status": status}))
        }
        Err(e) => Err(IpcError::Internal(e.to_string())),
    }
}
