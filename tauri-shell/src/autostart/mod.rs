//! Cross-platform "start at login" support.
//!
//! Each OS uses a different mechanism. We pick the one that doesn't
//! require admin/elevation and survives app upgrades:
//!
//! | OS      | Mechanism                                          |
//! |---------|----------------------------------------------------|
//! | macOS   | `osascript` → System Events login items            |
//! | Windows | `.lnk` in `%APPDATA%\Microsoft\...\Startup\`        |
//! | Linux   | `~/.config/autostart/<package>.desktop`            |

use tauri::AppHandle;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{disable as disable_impl, enable as enable_impl, is_enabled as is_enabled_impl};

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::{disable as disable_impl, enable as enable_impl, is_enabled as is_enabled_impl};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{disable as disable_impl, enable as enable_impl, is_enabled as is_enabled_impl};

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
mod fallback {
    use super::*;
    pub fn enable(_app: &AppHandle) -> Result<(), String> { Err("autostart unsupported on this platform".into()) }
    pub fn disable(_app: &AppHandle) -> Result<(), String> { Err("autostart unsupported on this platform".into()) }
    pub fn is_enabled(_app: &AppHandle) -> Result<bool, String> { Ok(false) }
}
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub use fallback::{disable as disable_impl, enable as enable_impl, is_enabled as is_enabled_impl};

/// Enable launch at login for the current binary.
pub fn enable(app: &AppHandle) -> Result<(), String> {
    enable_impl(app)
}

/// Disable launch at login.
pub fn disable(app: &AppHandle) -> Result<(), String> {
    disable_impl(app)
}

/// Read current autostart state.
pub fn is_enabled(app: &AppHandle) -> Result<bool, String> {
    is_enabled_impl(app)
}
