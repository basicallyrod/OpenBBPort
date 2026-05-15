//! Lifecycle commands for the Python `openbb-mcp` Model Context Protocol server.
//!
//! Parallel architecture to `openbb-api`. Three transports:
//! - `streamable-http` (default) — long-poll over HTTP
//! - `sse` — Server-Sent Events
//! - `stdio` — for embedding in agent frameworks
//!
//! Reference: `feature-platform-rest-api.md` (v2 addendum, MCP section).

use std::sync::Arc;

use super::IpcError;
use crate::connector::Connector;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "../bindings/", rename_all = "camelCase"))]
pub struct McpSpec {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub environment: String,
    /// `"streamable-http"` (default) | `"sse"` | `"stdio"`.
    pub transport: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_password: Option<String>,
    /// Optional allowlist of tools to expose. None = all.
    #[serde(default)]
    pub tool_allowlist: Vec<String>,
    /// Optional denylist.
    #[serde(default)]
    pub tool_denylist: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "../bindings/", rename_all = "camelCase"))]
pub struct McpStatus {
    pub id: String,
    pub running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[tauri::command]
pub async fn mcp_spawn(
    app: AppHandle,
    spec: McpSpec,
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<McpStatus, IpcError> {
    connector.mcp_spawn(spec, app).await.map_err(IpcError::from)
}

#[tauri::command]
pub async fn mcp_stop(
    app: AppHandle,
    id: String,
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<(), IpcError> {
    connector.mcp_stop(id, app).await.map_err(IpcError::from)
}

#[tauri::command]
pub async fn mcp_status(
    id: String,
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<McpStatus, IpcError> {
    connector.mcp_status(id).await.map_err(IpcError::from)
}

#[tauri::command]
pub async fn mcp_list(
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<Vec<McpStatus>, IpcError> {
    connector.mcp_list().await.map_err(IpcError::from)
}

/// Once spawned, an MCP server exposes its tool catalog over its protocol.
/// The connector queries it (e.g. JSON-RPC `tools/list`) and returns the
/// raw response.
#[tauri::command]
pub async fn mcp_list_tools(
    id: String,
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<serde_json::Value, IpcError> {
    connector.mcp_list_tools(id).await.map_err(IpcError::from)
}
