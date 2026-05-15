//! Auto-updater wrapper.
//!
//! Thin abstraction over `tauri-plugin-updater` that emits user-facing
//! dialogs only when `always_prompt` is true (i.e. user clicked "Check
//! for Updates"). Background checks are silent unless an update is found.
//!
//! Configure the endpoint and minisign pubkey in `tauri.conf.json` →
//! `plugins.updater`.

use tauri::AppHandle;

#[cfg(desktop)]
use tauri_plugin_updater::UpdaterExt;

#[cfg(desktop)]
pub async fn check_and_apply(app: AppHandle, always_prompt: bool) -> Result<(), String> {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

    let updater = app.updater().map_err(|e| e.to_string())?;
    match updater.check().await.map_err(|e| e.to_string())? {
        Some(update) => {
            let version = update.version.clone();
            let app_for_dialog = app.clone();
            let (tx, rx) = tokio::sync::oneshot::channel::<bool>();
            app_for_dialog
                .dialog()
                .message(format!(
                    "A new version ({version}) is available. Install now?",
                ))
                .kind(MessageDialogKind::Info)
                .buttons(MessageDialogButtons::OkCancel)
                .show(move |answer| {
                    let _ = tx.send(answer);
                });
            let accepted = rx.await.unwrap_or(false);
            if !accepted {
                return Ok(());
            }

            update
                .download_and_install(|_, _| {}, || {})
                .await
                .map_err(|e| e.to_string())?;

            // Write the post-restart flag so the next boot knows to show
            // the window (rather than starting hidden).
            if let Ok(dir) = crate::path_utils::ensure_settings_dir() {
                let _ = std::fs::write(dir.join(".show_on_restart"), b"1");
            }

            app.request_restart();
        }
        None => {
            if always_prompt {
                app.dialog()
                    .message("You are running the latest version.")
                    .kind(MessageDialogKind::Info)
                    .show(|_| {});
            }
        }
    }
    Ok(())
}

#[cfg(not(desktop))]
pub async fn check_and_apply(_app: AppHandle, _always_prompt: bool) -> Result<(), String> {
    Ok(())
}
