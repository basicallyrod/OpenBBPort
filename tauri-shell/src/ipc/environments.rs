//! Environment / extension CRUD — all stubs.
//!
//! Contract: see `docs/typescript-port/20-features/feature-environments.md`
//! and `feature-extensions.md`. Long-running operations stream output
//! by emitting `process-output` events with a caller-supplied `processId`.

use super::IpcError;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CondaEnvironment {
    pub name: String,
    pub python_version: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Extension {
    pub package: String,
    pub version: String,
    pub install_method: String,
    pub channel: String,
}

#[tauri::command]
pub fn list_conda_environments(_directory: Option<String>) -> Result<Vec<CondaEnvironment>, IpcError> {
    // TODO: connect to your backend (conda env list, uv toolchain list, etc.).
    Err(IpcError::not_implemented("list_conda_environments"))
}

#[tauri::command]
pub async fn create_environment(
    _app: AppHandle,
    _name: String,
    _python_version: String,
    _extensions: Vec<String>,
    _process_id: String,
) -> Result<bool, IpcError> {
    Err(IpcError::not_implemented("create_environment"))
}

#[tauri::command]
pub async fn create_environment_from_requirements(
    _app: AppHandle,
    _name: String,
    _file_path: String,
    _directory: String,
    _process_id: String,
) -> Result<bool, IpcError> {
    Err(IpcError::not_implemented("create_environment_from_requirements"))
}

#[tauri::command]
pub async fn select_requirements_file() -> Result<String, IpcError> {
    Err(IpcError::not_implemented(
        "use ipc::helpers::select_file with an appropriate filter",
    ))
}

#[tauri::command]
pub fn get_environment_extensions(_name: String) -> Result<serde_json::Value, IpcError> {
    Err(IpcError::not_implemented("get_environment_extensions"))
}

#[tauri::command]
pub async fn install_extensions(
    _extensions: Vec<String>,
    _environment: String,
) -> Result<bool, IpcError> {
    Err(IpcError::not_implemented("install_extensions"))
}

#[tauri::command]
pub async fn update_extension(
    _package: String,
    _environment: String,
    _directory: String,
) -> Result<bool, IpcError> {
    Err(IpcError::not_implemented("update_extension"))
}

#[tauri::command]
pub async fn update_environment(
    _environment: String,
    _directory: String,
) -> Result<bool, IpcError> {
    Err(IpcError::not_implemented("update_environment"))
}

#[tauri::command]
pub async fn remove_extension(
    _package: String,
    _environment: String,
    _directory: String,
) -> Result<bool, IpcError> {
    Err(IpcError::not_implemented("remove_extension"))
}

#[tauri::command]
pub async fn remove_environment(_name: String) -> Result<bool, IpcError> {
    Err(IpcError::not_implemented("remove_environment"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[tauri::command]
pub async fn execute_in_environment(
    _command: String,
    _environment: String,
    _directory: String,
) -> Result<ExecResult, IpcError> {
    Err(IpcError::not_implemented("execute_in_environment"))
}
