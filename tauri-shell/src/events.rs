//! Canonical event names and payload types.
//!
//! Every event emitted from the Rust side is declared here so the renderer
//! has a single source of truth. The TS port's IPC catalog (in
//! `docs/typescript-port/30-port/port-invoke-mapping.md`) maps to these.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Event names
// ---------------------------------------------------------------------------

/// Multiplexed subprocess output. All log producers (any spawn via
/// `process_spawn::spawn_with_streaming`) emit on this channel; consumers
/// filter by `processId`.
pub const PROCESS_OUTPUT: &str = "process-output";

/// Long-running installation pipeline progress events. Distinct from
/// `process-output` because installation has structured phase semantics
/// (download / install / config / complete / error / abort).
pub const INSTALL_PROGRESS: &str = "install-progress";

/// Uninstall pipeline progress. Free-form string payload.
pub const UNINSTALL_PROGRESS: &str = "uninstall-progress";

/// Emitted when the spawn helper extracts a URL from a backend service's
/// startup banner. Payload includes the backend id and the discovered URL.
pub const BACKEND_URL_DISCOVERED: &str = "backend-url-discovered";

/// Cross-window Jupyter status change. Emitted by the connector
/// implementation; the renderer subscribes to update the Environments page.
pub const JUPYTER_STATUS_UPDATE: &str = "jupyter-status-update";

/// Renderer should navigate to a route. Used by tray-menu clicks to drive
/// in-app routing without `window.eval`.
pub const NAVIGATE: &str = "navigate";

// ---------------------------------------------------------------------------
// Payload types
// ---------------------------------------------------------------------------

/// Payload for [`PROCESS_OUTPUT`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessOutputEvent {
    pub process_id: String,
    pub output: String,
    /// Milliseconds since epoch.
    pub timestamp: i64,
    /// `"stdout"`, `"stderr"`, or `"system"` (for synthesized banners).
    #[serde(rename = "type")]
    pub kind: String,
}

impl ProcessOutputEvent {
    pub fn stdout(process_id: impl Into<String>, output: impl Into<String>) -> Self {
        Self {
            process_id: process_id.into(),
            output: output.into(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            kind: "stdout".into(),
        }
    }

    pub fn stderr(process_id: impl Into<String>, output: impl Into<String>) -> Self {
        Self {
            process_id: process_id.into(),
            output: output.into(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            kind: "stderr".into(),
        }
    }

    pub fn system(process_id: impl Into<String>, output: impl Into<String>) -> Self {
        Self {
            process_id: process_id.into(),
            output: output.into(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            kind: "system".into(),
        }
    }
}

/// Payload for [`INSTALL_PROGRESS`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallProgressEvent {
    /// Phase identifier — `"download"`, `"install"`, `"config"`,
    /// `"complete"`, `"error"`, `"abort"`. Connector decides the vocabulary.
    pub step: String,
    /// 0.0–1.0.
    pub progress: f32,
    /// Human-readable status line.
    pub message: String,
}

/// Payload for [`BACKEND_URL_DISCOVERED`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendUrlEvent {
    pub id: String,
    pub url: String,
}

/// Payload for [`JUPYTER_STATUS_UPDATE`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JupyterStatusEvent {
    pub environment_name: String,
    pub status: String,
}

/// Payload for [`NAVIGATE`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavigateEvent {
    pub path: String,
}
