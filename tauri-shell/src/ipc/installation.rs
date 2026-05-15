//! Installation pipeline — delegates to the registered [`Connector`].
//!
//! Each command's signature matches the documented contract in
//! `docs/typescript-port/20-features/feature-installation.md`. The shell
//! does not embed an installer; it forwards the call to whichever
//! `Connector` impl the binary registered. The default `NoopConnector`
//! returns `NotImplemented` for every method, preserving the legacy
//! "wire to your backend" behavior.
//!
//! Long-running steps may emit `INSTALL_PROGRESS` events via the
//! [`AppHandle`] passed to the trait method.

use std::sync::Arc;

use super::IpcError;
use crate::connector::{
    Connector, InstallToDirectoryArgs, SetupPythonEnvironmentArgs, UpdateOpenbbSettingsArgs,
};
use crate::events::{InstallProgressEvent, INSTALL_PROGRESS};
use tauri::{AppHandle, Emitter, State};

/// Helper for connector implementations to broadcast a progress beat.
/// Re-exposed here so a connector that wants to reuse the canonical event
/// shape can do so without re-declaring the payload struct.
#[allow(dead_code)]
pub fn report_install_progress(app: &AppHandle, step: &str, progress: f32, message: &str) {
    let _ = app.emit(
        INSTALL_PROGRESS,
        InstallProgressEvent {
            step: step.into(),
            progress,
            message: message.into(),
        },
    );
}

/// Validate paths and pre-write settings files. Connector decides what
/// "pre-write" means in its settings schema.
#[tauri::command]
pub async fn install_to_directory(
    directory: String,
    user_data_directory: String,
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<bool, IpcError> {
    connector
        .install_to_directory(InstallToDirectoryArgs {
            directory,
            user_data_directory,
        })
        .await
        .map_err(IpcError::from)
}

/// Download + run the runtime installer (Miniforge in the OpenBB reference).
/// Connector emits `INSTALL_PROGRESS` events throughout.
#[tauri::command]
pub async fn install_conda(
    app: AppHandle,
    directory: String,
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<bool, IpcError> {
    connector
        .install_conda(directory, app)
        .await
        .map_err(IpcError::from)
}

#[tauri::command]
pub async fn setup_python_environment(
    app: AppHandle,
    directory: String,
    python_version: String,
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<bool, IpcError> {
    connector
        .setup_python_environment(
            SetupPythonEnvironmentArgs {
                directory,
                python_version,
            },
            app,
        )
        .await
        .map_err(IpcError::from)
}

#[tauri::command]
pub async fn abort_installation(
    directory: String,
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<(), IpcError> {
    connector
        .abort_installation(directory)
        .await
        .map_err(IpcError::from)
}

#[tauri::command]
pub fn get_installation_status() -> Result<crate::state::InstallationProgress, IpcError> {
    let g = crate::state::INSTALLATION_PROGRESS
        .lock()
        .map_err(|e| IpcError::Internal(e.to_string()))?;
    Ok(g.clone())
}

#[tauri::command]
pub async fn create_default_backend_services(
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<(), IpcError> {
    connector
        .create_default_backend_services()
        .await
        .map_err(IpcError::from)
}

#[tauri::command]
pub async fn update_openbb_settings(
    conda_dir: String,
    environment: String,
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<(), IpcError> {
    connector
        .update_openbb_settings(UpdateOpenbbSettingsArgs {
            conda_dir,
            environment,
        })
        .await
        .map_err(IpcError::from)
}

#[tauri::command]
pub async fn get_installation_directory(
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<String, IpcError> {
    connector
        .get_installation_directory()
        .await
        .map_err(IpcError::from)
}

#[tauri::command]
pub async fn get_userdata_directory(
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<String, IpcError> {
    connector
        .get_userdata_directory()
        .await
        .map_err(IpcError::from)
}
