//! Environment / extension CRUD — delegates to the registered [`Connector`].
//!
//! Contract: see `docs/typescript-port/20-features/feature-environments.md`
//! and `feature-extensions.md`. Long-running operations stream output
//! by emitting `process-output` events with a caller-supplied `processId`
//! from inside the connector implementation.

use std::sync::Arc;

use super::IpcError;
use crate::connector::{
    Connector, CreateEnvironmentArgs, CreateEnvironmentFromRequirementsArgs,
    ExecuteInEnvironmentArgs, InstallExtensionsArgs, RemoveExtensionArgs, UpdateEnvironmentArgs,
    UpdateExtensionArgs,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "../bindings/", rename_all = "camelCase"))]
pub struct CondaEnvironment {
    pub name: String,
    pub python_version: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "../bindings/"))]
pub struct Extension {
    pub package: String,
    pub version: String,
    pub install_method: String,
    pub channel: String,
}

#[tauri::command]
pub async fn list_conda_environments(
    directory: Option<String>,
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<Vec<CondaEnvironment>, IpcError> {
    connector
        .list_conda_environments(directory)
        .await
        .map_err(IpcError::from)
}

#[tauri::command]
pub async fn create_environment(
    app: AppHandle,
    name: String,
    python_version: String,
    extensions: Vec<String>,
    process_id: String,
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<bool, IpcError> {
    connector
        .create_environment(
            CreateEnvironmentArgs {
                name,
                python_version,
                extensions,
                process_id,
            },
            app,
        )
        .await
        .map_err(IpcError::from)
}

#[tauri::command]
pub async fn create_environment_from_requirements(
    app: AppHandle,
    name: String,
    file_path: String,
    directory: String,
    process_id: String,
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<bool, IpcError> {
    connector
        .create_environment_from_requirements(
            CreateEnvironmentFromRequirementsArgs {
                name,
                file_path,
                directory,
                process_id,
            },
            app,
        )
        .await
        .map_err(IpcError::from)
}

#[tauri::command]
pub async fn select_requirements_file(
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<String, IpcError> {
    connector
        .select_requirements_file()
        .await
        .map_err(IpcError::from)
}

#[tauri::command]
pub async fn get_environment_extensions(
    name: String,
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<serde_json::Value, IpcError> {
    connector
        .get_environment_extensions(name)
        .await
        .map_err(IpcError::from)
}

#[tauri::command]
pub async fn install_extensions(
    extensions: Vec<String>,
    environment: String,
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<bool, IpcError> {
    connector
        .install_extensions(InstallExtensionsArgs {
            extensions,
            environment,
        })
        .await
        .map_err(IpcError::from)
}

#[tauri::command]
pub async fn update_extension(
    package: String,
    environment: String,
    directory: String,
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<bool, IpcError> {
    connector
        .update_extension(UpdateExtensionArgs {
            package,
            environment,
            directory,
        })
        .await
        .map_err(IpcError::from)
}

#[tauri::command]
pub async fn update_environment(
    environment: String,
    directory: String,
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<bool, IpcError> {
    connector
        .update_environment(UpdateEnvironmentArgs {
            environment,
            directory,
        })
        .await
        .map_err(IpcError::from)
}

#[tauri::command]
pub async fn remove_extension(
    package: String,
    environment: String,
    directory: String,
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<bool, IpcError> {
    connector
        .remove_extension(RemoveExtensionArgs {
            package,
            environment,
            directory,
        })
        .await
        .map_err(IpcError::from)
}

#[tauri::command]
pub async fn remove_environment(
    name: String,
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<bool, IpcError> {
    connector.remove_environment(name).await.map_err(IpcError::from)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "../bindings/"))]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[tauri::command]
pub async fn execute_in_environment(
    command: String,
    environment: String,
    directory: String,
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<ExecResult, IpcError> {
    connector
        .execute_in_environment(ExecuteInEnvironmentArgs {
            command,
            environment,
            directory,
        })
        .await
        .map_err(IpcError::from)
}
