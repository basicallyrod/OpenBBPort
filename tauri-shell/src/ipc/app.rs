//! Top-level app commands — most are real.
//!
//! These are the shell-wide commands that don't fit into any of the
//! domain modules. They cover the boot snapshot (`get_installation_state`),
//! the in-app navigation event hop (`navigate_to_page`), a graceful
//! shutdown trigger (`quit_application`), the runtime version of the
//! binary (`get_app_version`), and a stubbed theme toggle the connector
//! can fill in.
//!
//! Related modules:
//! - `crate::state::InstallationState` — boot-time snapshot consumed by
//!   `get_installation_state` to tell the renderer whether the connector
//!   has finished its install pipeline.
//! - `crate::events::NAVIGATE` — the event emitted by `navigate_to_page`.
//!   The renderer subscribes once at boot and routes accordingly.
//! - `crate::cleanup::cleanup_all_processes` — invoked from
//!   `quit_application` to run the bounded shutdown cascade.
//!
//! See `docs/typescript-port/20-features/feature-app-shell.md` for the
//! full contract.

use std::sync::Arc;

use super::IpcError;
use crate::cleanup;
use crate::connector::Connector;
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

/// Persist the chosen theme via the registered [`Connector`]. The default
/// `NoopConnector` returns `NotImplemented`; user-supplied connectors
/// wire this to their preferences store.
#[tauri::command]
pub async fn toggle_theme(
    theme: String,
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<bool, IpcError> {
    connector.toggle_theme(theme).await.map_err(IpcError::from)
}
