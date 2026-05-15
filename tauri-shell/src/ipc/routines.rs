//! `.openbb` routine file management.
//!
//! The CLI stores user-defined command sequences as `.openbb` files
//! under `<settings_dir>/routines/`. Each file is a YAML/markdown hybrid
//! with frontmatter metadata (title, tags, description) and a body of
//! CLI commands (one per line). The desktop app can offer the same as
//! a "saved query" feature.

use super::IpcError;
use crate::path_utils;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

fn routines_dir() -> Result<PathBuf, IpcError> {
    let base = path_utils::settings_dir()
        .ok_or_else(|| IpcError::Internal("no settings directory".into()))?;
    let dir = base.join("routines");
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(IpcError::from)?;
    }
    Ok(dir)
}

fn validate_name(name: &str) -> Result<(), IpcError> {
    if name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || name.starts_with('.')
    {
        return Err(IpcError::InvalidArgument(format!(
            "invalid routine name: {name}"
        )));
    }
    Ok(())
}

fn resolve(name: &str) -> Result<PathBuf, IpcError> {
    validate_name(name)?;
    let n = if name.ends_with(".openbb") {
        name.to_string()
    } else {
        format!("{name}.openbb")
    };
    Ok(routines_dir()?.join(n))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineMetadata {
    pub name: String,
    pub modified_unix: u64,
    pub size_bytes: u64,
}

#[tauri::command]
pub fn routines_list() -> Result<Vec<RoutineMetadata>, IpcError> {
    let dir = routines_dir()?;
    let mut out = Vec::new();
    if !dir.exists() {
        return Ok(out);
    }
    for entry in std::fs::read_dir(&dir).map_err(IpcError::from)? {
        let entry = entry.map_err(IpcError::from)?;
        let path = entry.path();
        if path.extension().map(|e| e == "openbb").unwrap_or(false) {
            let meta = entry.metadata().map_err(IpcError::from)?;
            out.push(RoutineMetadata {
                name: path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                modified_unix: meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
                size_bytes: meta.len(),
            });
        }
    }
    out.sort_by(|a, b| b.modified_unix.cmp(&a.modified_unix));
    Ok(out)
}

#[derive(Deserialize)]
pub struct RoutinesReadArgs {
    pub name: String,
}

#[tauri::command]
pub fn routines_read(args: RoutinesReadArgs) -> Result<Option<String>, IpcError> {
    let path = resolve(&args.name)?;
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(std::fs::read_to_string(&path).map_err(IpcError::from)?))
}

#[derive(Deserialize)]
pub struct RoutinesSaveArgs {
    pub name: String,
    pub content: String,
}

#[tauri::command]
pub fn routines_save(args: RoutinesSaveArgs) -> Result<bool, IpcError> {
    let path = resolve(&args.name)?;
    // Atomic write via temp + rename.
    let tmp = path.with_extension("openbb.tmp");
    std::fs::write(&tmp, args.content.as_bytes()).map_err(IpcError::from)?;
    std::fs::rename(&tmp, &path).map_err(IpcError::from)?;
    Ok(true)
}

#[derive(Deserialize)]
pub struct RoutinesDeleteArgs {
    pub name: String,
}

#[tauri::command]
pub fn routines_delete(args: RoutinesDeleteArgs) -> Result<bool, IpcError> {
    let path = resolve(&args.name)?;
    if path.exists() {
        std::fs::remove_file(&path).map_err(IpcError::from)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[derive(Deserialize)]
pub struct RoutinesRenameArgs {
    pub old_name: String,
    pub new_name: String,
}

#[tauri::command]
pub fn routines_rename(args: RoutinesRenameArgs) -> Result<bool, IpcError> {
    let old = resolve(&args.old_name)?;
    let new_path = resolve(&args.new_name)?;
    if !old.exists() {
        return Ok(false);
    }
    if new_path.exists() {
        return Err(IpcError::Conflict(format!(
            "target exists: {}",
            args.new_name
        )));
    }
    std::fs::rename(&old, &new_path).map_err(IpcError::from)?;
    Ok(true)
}
