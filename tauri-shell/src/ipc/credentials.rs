//! Credential vault — stubs with a starter implementation.
//!
//! The shell ships a working `get_user_credentials` / `update_user_credentials`
//! that reads/writes JSON at `<settings_dir>/user_settings.json`. If your
//! backend uses a different schema or location, replace these.
//!
//! All writes go through [`settings::write_json_atomic`] which:
//! - writes to `<path>.tmp` then renames into place
//! - acquires `flock` on the temp file
//! - `chmod 0600` on Unix
//!
//! See `docs/typescript-port/20-features/feature-api-keys.md` for the
//! full security checklist.

use super::IpcError;
use crate::path_utils;
use crate::settings;
use serde_json::Value;
use std::path::PathBuf;

const FILE_NAME: &str = "user_settings.json";
const DEFAULT_TREE: &str = r#"{"credentials": {}, "preferences": {}, "defaults": {}}"#;

fn settings_path() -> Result<PathBuf, IpcError> {
    path_utils::settings_file(FILE_NAME)
        .ok_or_else(|| IpcError::Internal("no settings directory".into()))
}

#[tauri::command]
pub fn get_user_credentials() -> Result<Value, IpcError> {
    let path = settings_path()?;
    let tree: Option<Value> = settings::read_json(&path)?;
    match tree {
        Some(v) => Ok(v),
        None => Ok(serde_json::from_str(DEFAULT_TREE)?),
    }
}

#[derive(serde::Deserialize)]
pub struct UpdateCredentialsArgs {
    pub credentials: Value,
}

#[tauri::command]
pub fn update_user_credentials(args: UpdateCredentialsArgs) -> Result<bool, IpcError> {
    let path = settings_path()?;
    path_utils::ensure_settings_dir().map_err(IpcError::from)?;
    let default: Value = serde_json::from_str(DEFAULT_TREE)?;
    settings::modify_json(&path, default, |tree| {
        if let Value::Object(map) = tree {
            map.insert("credentials".into(), args.credentials.clone());
        }
        Ok(())
    })?;
    Ok(true)
}

/// Strict allow-list. Reject any file name not in the set; this is the
/// path-traversal fix flagged in `feature-api-keys.md`.
const ALLOWED_FILES: &[&str] = &[
    "user_settings.json",
    "system_settings.json",
    "mcp_settings.json",
    ".env",
    ".condarc",
];

#[tauri::command]
pub fn open_credentials_file(file_name: String) -> Result<bool, IpcError> {
    if !ALLOWED_FILES.iter().any(|allowed| *allowed == file_name) {
        return Err(IpcError::InvalidArgument(format!(
            "file_name not in allow-list: {file_name}"
        )));
    }
    let path = path_utils::settings_file(&file_name)
        .ok_or_else(|| IpcError::Internal("no settings directory".into()))?;
    if !path.exists() {
        // Create with sensible defaults rather than failing.
        let default = match file_name.as_str() {
            "user_settings.json" => DEFAULT_TREE,
            "system_settings.json" | "mcp_settings.json" => "{}",
            ".env" => "# environment variables\n",
            ".condarc" => "channels:\n  - conda-forge\n  - defaults\n",
            _ => "{}",
        };
        std::fs::write(&path, default).map_err(IpcError::from)?;
    }
    open::that(&path).map_err(|e| IpcError::Internal(e.to_string()))?;
    Ok(true)
}
