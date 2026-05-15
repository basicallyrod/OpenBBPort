//! Uninstall flow — stub.
//!
//! Contract: `docs/typescript-port/20-features/feature-uninstall.md`.
//! Emits `UNINSTALL_PROGRESS` events as it works.

use super::IpcError;
use tauri::AppHandle;

#[tauri::command]
pub async fn uninstall_application(
    _app: AppHandle,
    _remove_user_data: bool,
    _remove_settings: bool,
) -> Result<Option<String>, IpcError> {
    // TODO: connect to your backend.
    // Typical cascade:
    //   1. graceful stop of all background services
    //   2. disable autostart (call crate::autostart::disable)
    //   3. remove conda envs / Python runtime
    //   4. remove install dir
    //   5. optionally wipe user data + settings
    //   6. spawn an OS-level uninstaller for the .app/.exe binary
    Err(IpcError::not_implemented("uninstall_application"))
}
