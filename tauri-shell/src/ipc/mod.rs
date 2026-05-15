//! Aggregator for `tauri::generate_handler!`.
//!
//! Every `#[tauri::command]` exposed by the shell is re-exported here.
//! Add your own commands to the appropriate submodule and include them
//! in `register_handlers!` below.

pub mod infrastructure;
pub mod installation;
pub mod environments;
pub mod backends;
pub mod jupyter;
pub mod credentials;
pub mod helpers;
pub mod uninstall;
pub mod certs;
pub mod app;
// Python REST proxy + workspace + typed wrappers
pub mod obb;
pub mod obb_routes;
pub mod obb_routes_extended;
pub mod openbb_meta;
pub mod provider;
// Settings file API
pub mod settings_files;
// Server lifecycle
pub mod server;
pub mod mcp;
// Routine (.openbb) files
pub mod routines;

/// Typed error returned by every command. Serializes as a tagged JSON
/// object so the renderer can match on `kind` instead of parsing strings.
#[derive(Debug, Clone, serde::Serialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "../bindings/", tag = "kind", rename_all = "kebab-case"))]
pub enum IpcError {
    #[error("not implemented: {0}")]
    NotImplemented(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("internal: {0}")]
    Internal(String),
}

impl IpcError {
    pub fn not_implemented(detail: impl Into<String>) -> Self {
        Self::NotImplemented(detail.into())
    }
    pub fn internal(detail: impl Into<String>) -> Self {
        Self::Internal(detail.into())
    }
}

impl From<std::io::Error> for IpcError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

impl From<serde_json::Error> for IpcError {
    fn from(e: serde_json::Error) -> Self {
        Self::Internal(format!("json: {e}"))
    }
}

impl From<crate::settings::SettingsError> for IpcError {
    fn from(e: crate::settings::SettingsError) -> Self {
        Self::Internal(e.to_string())
    }
}

impl From<crate::connector::ConnectorError> for IpcError {
    fn from(e: crate::connector::ConnectorError) -> Self {
        use crate::connector::ConnectorError;
        match e {
            ConnectorError::NotImplemented(m) => Self::NotImplemented(m.to_string()),
            ConnectorError::Io(m) => Self::Io(m),
            ConnectorError::InvalidArgument(m) => Self::InvalidArgument(m),
            ConnectorError::Unauthorized(m) => Self::Unauthorized(m),
            ConnectorError::Conflict(m) => Self::Conflict(m),
            ConnectorError::Internal(m) => Self::Internal(m),
        }
    }
}

/// One-stop helper: returns the `tauri::generate_handler!` token tree
/// containing every command this shell exposes. Use from `main.rs` like:
///
/// ```ignore
/// .invoke_handler(tauri_shell::ipc::handlers())
/// ```
///
/// We can't use `generate_handler!` directly here because the macro
/// expansion can't cross a function boundary; instead `main.rs` calls
/// it with the same list. This module's purpose is to make sure all
/// command names live in one place.
pub const COMMAND_NAMES: &[&str] = &[
    // app
    "get_installation_state",
    "navigate_to_page",
    "quit_application",
    "toggle_theme",
    "get_app_version",
    // process monitoring (infrastructure)
    "register_process_monitoring",
    "unregister_process_monitoring",
    "get_process_logs_history",
    "clear_process_logs_history",
    // helpers
    "get_home_directory",
    "get_settings_directory",
    "select_directory",
    "select_file",
    "check_directory_exists",
    "check_file_exists",
    "open_url_in_window",
    "open_workspace_in_browser",
    "get_working_directory",
    "save_working_directory",
    "open_logs_window",
    // installation (stub)
    "install_to_directory",
    "install_conda",
    "setup_python_environment",
    "abort_installation",
    "get_installation_status",
    "create_default_backend_services",
    "update_openbb_settings",
    "get_installation_directory",
    "get_userdata_directory",
    // environments (stub)
    "list_conda_environments",
    "create_environment",
    "create_environment_from_requirements",
    "select_requirements_file",
    "get_environment_extensions",
    "install_extensions",
    "update_extension",
    "update_environment",
    "remove_extension",
    "remove_environment",
    "execute_in_environment",
    // backends (stub)
    "list_backend_services",
    "create_backend_service",
    "update_backend_service",
    "delete_backend_service",
    "start_backend_service",
    "stop_backend_service",
    "open_backend_logs_window",
    // jupyter (stub)
    "start_jupyter_server",
    "stop_jupyter_server",
    "check_jupyter_server",
    "list_jupyter_servers",
    "open_jupyter_logs_window",
    "update_jupyter_status",
    // credentials (stub)
    "get_user_credentials",
    "update_user_credentials",
    "open_credentials_file",
    // certs (stub)
    "generate_self_signed_cert",
    // uninstall (stub)
    "uninstall_application",
];
