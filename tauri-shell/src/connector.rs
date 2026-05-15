//! Backend connector trait.
//!
//! Every IPC command that previously returned `Err(NotImplemented)` now
//! delegates to a [`Connector`] trait object registered as Tauri-managed
//! state. The shell ships [`NoopConnector`] by default (every method
//! returns [`ConnectorError::NotImplemented`]), which preserves the legacy
//! "wire to your backend" behavior. Real applications override by
//! registering their own impl in `main.rs`:
//!
//! ```ignore
//! .manage::<std::sync::Arc<dyn tauri_shell::connector::Connector>>(
//!     std::sync::Arc::new(MyConnector::new())
//! )
//! ```
//!
//! ## Trait surface
//!
//! Each trait method maps 1:1 to a stub IPC command. Where the IPC command
//! historically took multiple primitive arguments, a small `*Args` struct
//! is defined in this module so the trait surface stays cohesive.
//!
//! Methods that need to emit lifecycle events (install progress, process
//! output, etc.) receive a [`tauri::AppHandle`] so they can call
//! [`tauri::Emitter::emit`] without smuggling state through globals.
//!
//! ## Error model
//!
//! Connector impls return [`ConnectorError`]. The IPC layer converts it to
//! [`crate::ipc::IpcError`] via `From`. Most variants mirror the IPC
//! error tags so connectors can express their failure mode directly.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::ipc::backends::BackendService;
use crate::ipc::certs::GenerateCertArgs;
use crate::ipc::environments::{CondaEnvironment, ExecResult};
use crate::ipc::jupyter::JupyterStatus;
use crate::ipc::mcp::{McpSpec, McpStatus};
use crate::ipc::server::{ServerSpec, ServerStatus};

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

/// Errors a connector can surface. The IPC layer maps these into
/// [`crate::ipc::IpcError`] variants of the same shape.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ConnectorError {
    /// Method is unimplemented in this connector. The `&'static str` is the
    /// method name (matches the IPC command name).
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),
    /// I/O failure (filesystem, subprocess, network).
    #[error("io error: {0}")]
    Io(String),
    /// Caller passed an invalid argument.
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    /// Caller is not authorized for this action.
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    /// Resource is in a conflicting state (e.g. already running, already
    /// installed, name collision).
    #[error("conflict: {0}")]
    Conflict(String),
    /// Generic / unclassified internal error.
    #[error("internal: {0}")]
    Internal(String),
}

impl ConnectorError {
    pub fn io(detail: impl Into<String>) -> Self {
        Self::Io(detail.into())
    }
    pub fn invalid(detail: impl Into<String>) -> Self {
        Self::InvalidArgument(detail.into())
    }
    pub fn unauthorized(detail: impl Into<String>) -> Self {
        Self::Unauthorized(detail.into())
    }
    pub fn conflict(detail: impl Into<String>) -> Self {
        Self::Conflict(detail.into())
    }
    pub fn internal(detail: impl Into<String>) -> Self {
        Self::Internal(detail.into())
    }
}

// ---------------------------------------------------------------------------
// Argument wrapper structs
// ---------------------------------------------------------------------------

/// Args for [`Connector::install_to_directory`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallToDirectoryArgs {
    pub directory: String,
    pub user_data_directory: String,
}

/// Args for [`Connector::setup_python_environment`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupPythonEnvironmentArgs {
    pub directory: String,
    pub python_version: String,
}

/// Args for [`Connector::update_openbb_settings`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateOpenbbSettingsArgs {
    pub conda_dir: String,
    pub environment: String,
}

/// Args for [`Connector::create_environment`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateEnvironmentArgs {
    pub name: String,
    pub python_version: String,
    pub extensions: Vec<String>,
    pub process_id: String,
}

/// Args for [`Connector::create_environment_from_requirements`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateEnvironmentFromRequirementsArgs {
    pub name: String,
    pub file_path: String,
    pub directory: String,
    pub process_id: String,
}

/// Args for [`Connector::install_extensions`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallExtensionsArgs {
    pub extensions: Vec<String>,
    pub environment: String,
}

/// Args for [`Connector::update_extension`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateExtensionArgs {
    pub package: String,
    pub environment: String,
    pub directory: String,
}

/// Args for [`Connector::update_environment`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateEnvironmentArgs {
    pub environment: String,
    pub directory: String,
}

/// Args for [`Connector::remove_extension`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveExtensionArgs {
    pub package: String,
    pub environment: String,
    pub directory: String,
}

/// Args for [`Connector::execute_in_environment`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteInEnvironmentArgs {
    pub command: String,
    pub environment: String,
    pub directory: String,
}

/// Args for [`Connector::start_jupyter_server`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartJupyterArgs {
    pub environment: String,
    pub directory: String,
    pub working: String,
}

/// Args for [`Connector::uninstall_application`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallArgs {
    pub remove_user_data: bool,
    pub remove_settings: bool,
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// The backend connector. Methods correspond 1:1 to the stub IPC commands.
///
/// All methods have a default implementation that returns
/// [`ConnectorError::NotImplemented`] — that way a connector author can
/// override only the methods they care about (e.g. just installation or
/// just servers) without writing out boilerplate for the rest.
#[async_trait]
pub trait Connector: Send + Sync + 'static {
    // ----- app ---------------------------------------------------------------

    /// Persist the chosen theme (light/dark/system).
    async fn toggle_theme(&self, _theme: String) -> Result<bool, ConnectorError> {
        Err(ConnectorError::NotImplemented("toggle_theme"))
    }

    // ----- helpers (preference-backed) --------------------------------------

    /// Read the persisted working directory, or fall back to `default_dir`.
    async fn get_working_directory(
        &self,
        _default_dir: Option<String>,
    ) -> Result<String, ConnectorError> {
        Err(ConnectorError::NotImplemented("get_working_directory"))
    }

    /// Persist the working directory.
    async fn save_working_directory(&self, _path: String) -> Result<bool, ConnectorError> {
        Err(ConnectorError::NotImplemented("save_working_directory"))
    }

    // ----- installation -----------------------------------------------------

    async fn install_to_directory(
        &self,
        _args: InstallToDirectoryArgs,
    ) -> Result<bool, ConnectorError> {
        Err(ConnectorError::NotImplemented("install_to_directory"))
    }

    async fn install_conda(
        &self,
        _directory: String,
        _app: AppHandle,
    ) -> Result<bool, ConnectorError> {
        Err(ConnectorError::NotImplemented("install_conda"))
    }

    async fn setup_python_environment(
        &self,
        _args: SetupPythonEnvironmentArgs,
        _app: AppHandle,
    ) -> Result<bool, ConnectorError> {
        Err(ConnectorError::NotImplemented("setup_python_environment"))
    }

    async fn abort_installation(&self, _directory: String) -> Result<(), ConnectorError> {
        Err(ConnectorError::NotImplemented("abort_installation"))
    }

    async fn create_default_backend_services(&self) -> Result<(), ConnectorError> {
        Err(ConnectorError::NotImplemented(
            "create_default_backend_services",
        ))
    }

    async fn update_openbb_settings(
        &self,
        _args: UpdateOpenbbSettingsArgs,
    ) -> Result<(), ConnectorError> {
        Err(ConnectorError::NotImplemented("update_openbb_settings"))
    }

    async fn get_installation_directory(&self) -> Result<String, ConnectorError> {
        Err(ConnectorError::NotImplemented("get_installation_directory"))
    }

    async fn get_userdata_directory(&self) -> Result<String, ConnectorError> {
        Err(ConnectorError::NotImplemented("get_userdata_directory"))
    }

    // ----- environments -----------------------------------------------------

    async fn list_conda_environments(
        &self,
        _directory: Option<String>,
    ) -> Result<Vec<CondaEnvironment>, ConnectorError> {
        Err(ConnectorError::NotImplemented("list_conda_environments"))
    }

    async fn create_environment(
        &self,
        _args: CreateEnvironmentArgs,
        _app: AppHandle,
    ) -> Result<bool, ConnectorError> {
        Err(ConnectorError::NotImplemented("create_environment"))
    }

    async fn create_environment_from_requirements(
        &self,
        _args: CreateEnvironmentFromRequirementsArgs,
        _app: AppHandle,
    ) -> Result<bool, ConnectorError> {
        Err(ConnectorError::NotImplemented(
            "create_environment_from_requirements",
        ))
    }

    async fn select_requirements_file(&self) -> Result<String, ConnectorError> {
        Err(ConnectorError::NotImplemented("select_requirements_file"))
    }

    async fn get_environment_extensions(
        &self,
        _name: String,
    ) -> Result<serde_json::Value, ConnectorError> {
        Err(ConnectorError::NotImplemented("get_environment_extensions"))
    }

    async fn install_extensions(
        &self,
        _args: InstallExtensionsArgs,
    ) -> Result<bool, ConnectorError> {
        Err(ConnectorError::NotImplemented("install_extensions"))
    }

    async fn update_extension(
        &self,
        _args: UpdateExtensionArgs,
    ) -> Result<bool, ConnectorError> {
        Err(ConnectorError::NotImplemented("update_extension"))
    }

    async fn update_environment(
        &self,
        _args: UpdateEnvironmentArgs,
    ) -> Result<bool, ConnectorError> {
        Err(ConnectorError::NotImplemented("update_environment"))
    }

    async fn remove_extension(
        &self,
        _args: RemoveExtensionArgs,
    ) -> Result<bool, ConnectorError> {
        Err(ConnectorError::NotImplemented("remove_extension"))
    }

    async fn remove_environment(&self, _name: String) -> Result<bool, ConnectorError> {
        Err(ConnectorError::NotImplemented("remove_environment"))
    }

    async fn execute_in_environment(
        &self,
        _args: ExecuteInEnvironmentArgs,
    ) -> Result<ExecResult, ConnectorError> {
        Err(ConnectorError::NotImplemented("execute_in_environment"))
    }

    // ----- backends ---------------------------------------------------------

    async fn list_backend_services(&self) -> Result<Vec<BackendService>, ConnectorError> {
        Err(ConnectorError::NotImplemented("list_backend_services"))
    }

    async fn create_backend_service(
        &self,
        _backend: BackendService,
    ) -> Result<BackendService, ConnectorError> {
        Err(ConnectorError::NotImplemented("create_backend_service"))
    }

    async fn update_backend_service(
        &self,
        _backend: BackendService,
    ) -> Result<BackendService, ConnectorError> {
        Err(ConnectorError::NotImplemented("update_backend_service"))
    }

    async fn delete_backend_service(
        &self,
        _id: String,
        _app: AppHandle,
    ) -> Result<(), ConnectorError> {
        Err(ConnectorError::NotImplemented("delete_backend_service"))
    }

    async fn start_backend_service(
        &self,
        _id: String,
        _app: AppHandle,
    ) -> Result<BackendService, ConnectorError> {
        Err(ConnectorError::NotImplemented("start_backend_service"))
    }

    async fn stop_backend_service(
        &self,
        _id: String,
        _app: AppHandle,
    ) -> Result<(), ConnectorError> {
        Err(ConnectorError::NotImplemented("stop_backend_service"))
    }

    // ----- jupyter ----------------------------------------------------------

    async fn start_jupyter_server(
        &self,
        _args: StartJupyterArgs,
        _app: AppHandle,
    ) -> Result<JupyterStatus, ConnectorError> {
        Err(ConnectorError::NotImplemented("start_jupyter_server"))
    }

    async fn stop_jupyter_server(
        &self,
        _environment: String,
        _app: AppHandle,
    ) -> Result<bool, ConnectorError> {
        Err(ConnectorError::NotImplemented("stop_jupyter_server"))
    }

    async fn check_jupyter_server(
        &self,
        _environment: String,
    ) -> Result<JupyterStatus, ConnectorError> {
        Err(ConnectorError::NotImplemented("check_jupyter_server"))
    }

    async fn list_jupyter_servers(&self) -> Result<serde_json::Value, ConnectorError> {
        Err(ConnectorError::NotImplemented("list_jupyter_servers"))
    }

    // ----- certs ------------------------------------------------------------

    async fn generate_self_signed_cert(
        &self,
        _args: GenerateCertArgs,
    ) -> Result<serde_json::Value, ConnectorError> {
        Err(ConnectorError::NotImplemented("generate_self_signed_cert"))
    }

    // ----- uninstall --------------------------------------------------------

    async fn uninstall_application(
        &self,
        _args: UninstallArgs,
        _app: AppHandle,
    ) -> Result<Option<String>, ConnectorError> {
        Err(ConnectorError::NotImplemented("uninstall_application"))
    }

    // ----- server (openbb-api) ----------------------------------------------

    async fn server_spawn(
        &self,
        _spec: ServerSpec,
        _app: AppHandle,
    ) -> Result<ServerStatus, ConnectorError> {
        Err(ConnectorError::NotImplemented("server_spawn"))
    }

    async fn server_stop(&self, _id: String, _app: AppHandle) -> Result<(), ConnectorError> {
        Err(ConnectorError::NotImplemented("server_stop"))
    }

    async fn server_status(&self, _id: String) -> Result<ServerStatus, ConnectorError> {
        Err(ConnectorError::NotImplemented("server_status"))
    }

    async fn server_list(&self) -> Result<Vec<ServerStatus>, ConnectorError> {
        Err(ConnectorError::NotImplemented("server_list"))
    }

    // ----- mcp --------------------------------------------------------------

    async fn mcp_spawn(
        &self,
        _spec: McpSpec,
        _app: AppHandle,
    ) -> Result<McpStatus, ConnectorError> {
        Err(ConnectorError::NotImplemented("mcp_spawn"))
    }

    async fn mcp_stop(&self, _id: String, _app: AppHandle) -> Result<(), ConnectorError> {
        Err(ConnectorError::NotImplemented("mcp_stop"))
    }

    async fn mcp_status(&self, _id: String) -> Result<McpStatus, ConnectorError> {
        Err(ConnectorError::NotImplemented("mcp_status"))
    }

    async fn mcp_list(&self) -> Result<Vec<McpStatus>, ConnectorError> {
        Err(ConnectorError::NotImplemented("mcp_list"))
    }

    async fn mcp_list_tools(&self, _id: String) -> Result<serde_json::Value, ConnectorError> {
        Err(ConnectorError::NotImplemented("mcp_list_tools"))
    }
}

// ---------------------------------------------------------------------------
// Default impl: NoopConnector
// ---------------------------------------------------------------------------

/// Default connector that returns [`ConnectorError::NotImplemented`] from
/// every method. Registered by `main.rs` so the shell remains functional
/// in the absence of a user-supplied connector.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopConnector;

#[async_trait]
impl Connector for NoopConnector {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn noop_returns_not_implemented() {
        let c: Arc<dyn Connector> = Arc::new(NoopConnector);
        match c.list_conda_environments(None).await {
            Err(ConnectorError::NotImplemented("list_conda_environments")) => {}
            other => panic!("unexpected: {other:?}"),
        }
    }
}
