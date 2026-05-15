//! macOS autostart via `osascript` + System Events login items.
//!
//! Uses path-contains matching so app moves (e.g. /Applications → home)
//! still let us toggle the existing entry.

use std::path::PathBuf;
use std::process::Command;
use tauri::AppHandle;

fn current_app_path() -> PathBuf {
    // Walk up looking for the .app bundle.
    if let Ok(exe) = std::env::current_exe() {
        let mut p = exe.as_path();
        for _ in 0..5 {
            if let Some(parent) = p.parent() {
                if parent
                    .extension()
                    .map(|e| e == "app")
                    .unwrap_or(false)
                {
                    return parent.to_path_buf();
                }
                p = parent;
            }
        }
    }
    std::env::current_exe().unwrap_or_default()
}

fn run_osascript(script: &str) -> Result<String, String> {
    let out = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn is_enabled(_app: &AppHandle) -> Result<bool, String> {
    let path = current_app_path();
    let needle = path.to_string_lossy().to_string();
    let script = format!(
        r#"tell application "System Events" to get path of every login item"#
    );
    let out = run_osascript(&script)?;
    Ok(out.contains(&needle))
}

pub fn enable(_app: &AppHandle) -> Result<(), String> {
    if is_enabled(_app).unwrap_or(false) {
        return Ok(());
    }
    let path = current_app_path();
    let script = format!(
        r#"tell application "System Events" to make new login item at end with properties {{path:"{}", hidden:false, name:"{}"}}"#,
        path.to_string_lossy(),
        path.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "App".into())
    );
    run_osascript(&script).map(|_| ())
}

pub fn disable(_app: &AppHandle) -> Result<(), String> {
    let path = current_app_path();
    let needle = path.to_string_lossy().to_string();
    let script = format!(
        r#"tell application "System Events"
            set theItems to login items
            repeat with i from (count of theItems) to 1 by -1
                set thePath to path of item i of theItems
                if thePath contains "{needle}" then
                    delete item i of theItems
                end if
            end repeat
        end tell"#
    );
    run_osascript(&script).map(|_| ())
}
