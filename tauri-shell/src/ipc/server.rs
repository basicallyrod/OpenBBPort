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

use super::IpcError;
use crate::proxy::Proxy;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

#[tauri::command]
pub async fn server_spawn(
    _app: AppHandle,
    _spec: ServerSpec,
) -> Result<ServerStatus, IpcError> {
    // TODO: connect to your backend. Reference flow:
    //   1. Validate command for dangerous patterns (command_sanitizer)
    //   2. Generate a shell wrapper that activates the env + exports vars
    //   3. spawn_with_streaming(...) — registers a "backend-<id>" log channel
    //   4. Insert into RunningProcesses
    //   5. Parse `Started server process [N]` from logs → real PID
    //   6. Parse the listen URL from uvicorn's banner (1500ms debounce)
    //   7. Emit `backend-url-discovered` when URL is confirmed
    //   8. Call Proxy::set_base_url with the discovered URL
    Err(IpcError::not_implemented("server_spawn"))
}

#[tauri::command]
pub async fn server_stop(_app: AppHandle, _id: String) -> Result<(), IpcError> {
    // TODO: see feature-backend-services.md §5.2 for the full stop dance:
    //   port-based kill → tracked-child kill → PID fallback → state reset.
    Err(IpcError::not_implemented("server_stop"))
}

#[tauri::command]
pub fn server_status(_id: String) -> Result<ServerStatus, IpcError> {
    Err(IpcError::not_implemented("server_status"))
}

#[tauri::command]
pub fn server_list() -> Result<Vec<ServerStatus>, IpcError> {
    Err(IpcError::not_implemented("server_list"))
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
