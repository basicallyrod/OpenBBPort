//! Lifecycle commands for the Python `openbb-mcp` Model Context Protocol server.
//!
//! Parallel architecture to `openbb-api`. Three transports:
//! - `streamable-http` (default) — long-poll over HTTP
//! - `sse` — Server-Sent Events
//! - `stdio` — for embedding in agent frameworks
//!
//! Reference: `feature-platform-rest-api.md` (v2 addendum, MCP section).

use super::IpcError;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
pub struct McpStatus {
    pub id: String,
    pub running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[tauri::command]
pub async fn mcp_spawn(_app: AppHandle, _spec: McpSpec) -> Result<McpStatus, IpcError> {
    Err(IpcError::not_implemented("mcp_spawn"))
}

#[tauri::command]
pub async fn mcp_stop(_app: AppHandle, _id: String) -> Result<(), IpcError> {
    Err(IpcError::not_implemented("mcp_stop"))
}

#[tauri::command]
pub fn mcp_status(_id: String) -> Result<McpStatus, IpcError> {
    Err(IpcError::not_implemented("mcp_status"))
}

#[tauri::command]
pub fn mcp_list() -> Result<Vec<McpStatus>, IpcError> {
    Err(IpcError::not_implemented("mcp_list"))
}

#[tauri::command]
pub async fn mcp_list_tools(_id: String) -> Result<serde_json::Value, IpcError> {
    // Once spawned, an MCP server exposes its tool catalog over its protocol.
    // Connector should query it (e.g. JSON-RPC `tools/list`) and return.
    Err(IpcError::not_implemented("mcp_list_tools"))
}
