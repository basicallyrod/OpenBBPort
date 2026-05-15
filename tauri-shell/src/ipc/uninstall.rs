//! Uninstall flow — delegates to the registered [`Connector`].
//!
//! Contract: `docs/typescript-port/20-features/feature-uninstall.md`.
//! Emits `UNINSTALL_PROGRESS` events from inside the connector impl.
//!
//! The connector decides the cascade order; the typical OpenBB one is:
//!
//! 1. Stop every backend service (`crate::ipc::backends::stop_backend_service`)
//!    and every Jupyter server (`crate::ipc::jupyter::stop_jupyter_server`).
//! 2. Disable autostart via `crate::autostart::disable`.
//! 3. Remove conda envs / Python runtime under the install dir.
//! 4. Remove the install dir itself.
//! 5. Optionally remove the user-data dir (gated by `remove_user_data`)
//!    and the settings dir (gated by `remove_settings`).
//!
//! Returns `Ok(Some(reason))` on user-visible failure, `Ok(None)` on
//! success. The renderer is expected to relaunch / quit afterwards.

use std::sync::Arc;

use super::IpcError;
use crate::connector::{Connector, UninstallArgs};
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn uninstall_application(
    app: AppHandle,
    remove_user_data: bool,
    remove_settings: bool,
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<Option<String>, IpcError> {
    connector
        .uninstall_application(
            UninstallArgs {
                remove_user_data,
                remove_settings,
            },
            app,
        )
        .await
        .map_err(IpcError::from)
}
