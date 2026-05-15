//! Read/write commands for the five settings files used by the OpenBB Python
//! backend. All writes go through `settings::write_json_atomic` (or the raw
//! file write for non-JSON), so they're atomic + flocked + chmod 0600.
//!
//! Strict file-name allow-list prevents path-traversal — see the security
//! review in `feature-api-keys.md` §11.
//!
//! Schema is connector-defined; this module is shape-agnostic.

use super::IpcError;
use crate::path_utils;
use crate::settings;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

const FILES: &[&str] = &[
    "user_settings.json",
    "system_settings.json",
    "mcp_settings.json",
    "widget_settings.json",
    ".env",
    ".condarc",
];

fn resolve(file_name: &str) -> Result<PathBuf, IpcError> {
    if !FILES.iter().any(|f| *f == file_name) {
        return Err(IpcError::InvalidArgument(format!(
            "file_name not in allow-list: {file_name}"
        )));
    }
    path_utils::settings_file(file_name)
        .ok_or_else(|| IpcError::Internal("no settings directory".into()))
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "../bindings/"))]
pub struct ReadJsonArgs {
    pub file_name: String,
}

#[tauri::command]
pub fn read_settings_json(args: ReadJsonArgs) -> Result<Option<Value>, IpcError> {
    let path = resolve(&args.file_name)?;
    Ok(settings::read_json(&path)?)
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "../bindings/"))]
pub struct WriteJsonArgs {
    pub file_name: String,
    pub content: Value,
}

#[tauri::command]
pub fn write_settings_json(args: WriteJsonArgs) -> Result<bool, IpcError> {
    let path = resolve(&args.file_name)?;
    path_utils::ensure_settings_dir().map_err(IpcError::from)?;
    settings::write_json_atomic(&path, &args.content)?;
    Ok(true)
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "../bindings/"))]
pub struct ReadTextArgs {
    pub file_name: String,
}

#[tauri::command]
pub fn read_settings_text(args: ReadTextArgs) -> Result<Option<String>, IpcError> {
    let path = resolve(&args.file_name)?;
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path).map_err(IpcError::from)?;
    Ok(Some(content))
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "../bindings/"))]
pub struct WriteTextArgs {
    pub file_name: String,
    pub content: String,
}

#[tauri::command]
pub fn write_settings_text(args: WriteTextArgs) -> Result<bool, IpcError> {
    let path = resolve(&args.file_name)?;
    path_utils::ensure_settings_dir().map_err(IpcError::from)?;
    // Use atomic write semantics manually for non-JSON files.
    let tmp = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_default()
    ));
    std::fs::write(&tmp, args.content.as_bytes()).map_err(IpcError::from)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&tmp).map_err(IpcError::from)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(&tmp, perms).map_err(IpcError::from)?;
    }

    std::fs::rename(&tmp, &path).map_err(IpcError::from)?;
    Ok(true)
}

#[tauri::command]
pub fn list_settings_files() -> Vec<String> {
    FILES.iter().map(|s| s.to_string()).collect()
}
