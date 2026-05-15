//! Path resolution helpers.
//!
//! These are intentionally generic — the shell does not assume any
//! particular settings layout. Your connector decides what lives where
//! by setting the relevant getters.
//!
//! The defaults below place app data under `<home>/.tauri-shell/`. To
//! change the directory name, set the `TAURI_SHELL_DATA_DIR_NAME` env
//! var at startup, or override `data_dir_name` via [`set_data_dir_name`].

use once_cell::sync::OnceCell;
use std::path::PathBuf;

static DATA_DIR_NAME: OnceCell<String> = OnceCell::new();

/// Override the on-disk directory name. Must be called before any
/// other path helper. If unset, defaults to the `TAURI_SHELL_DATA_DIR_NAME`
/// env var, then to `.tauri-shell`.
pub fn set_data_dir_name(name: impl Into<String>) {
    let _ = DATA_DIR_NAME.set(name.into());
}

pub fn data_dir_name() -> String {
    DATA_DIR_NAME
        .get()
        .cloned()
        .or_else(|| std::env::var("TAURI_SHELL_DATA_DIR_NAME").ok())
        .unwrap_or_else(|| ".tauri-shell".to_string())
}

/// Cross-platform home directory.
pub fn home_dir() -> Option<PathBuf> {
    dirs::home_dir()
}

/// The settings directory the shell writes to. By default
/// `<home>/<data_dir_name>/`. Connectors can ignore this and use their
/// own paths.
pub fn settings_dir() -> Option<PathBuf> {
    home_dir().map(|p| p.join(data_dir_name()))
}

pub fn ensure_settings_dir() -> std::io::Result<PathBuf> {
    let dir = settings_dir()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no home directory"))?;
    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
    }
    Ok(dir)
}

/// Convenience: a typed JSON file inside the settings dir.
pub fn settings_file(name: &str) -> Option<PathBuf> {
    settings_dir().map(|p| p.join(name))
}

/// Temp dir for shell-internal scratch files (install scripts, etc.).
pub fn temp_subdir(name: &str) -> std::io::Result<PathBuf> {
    let dir = std::env::temp_dir().join(name);
    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
    }
    Ok(dir)
}
