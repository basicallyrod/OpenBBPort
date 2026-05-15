//! Bounded cleanup cascade.
//!
//! This is the function wired to:
//! - Tray "Quit" menu item
//! - Ctrl-C / SIGINT
//! - Tauri `RunEvent::ExitRequested`
//! - macOS `NSApplicationWillTerminateNotification`
//! - The `quit_application` IPC command
//!
//! The cascade is bounded:
//!
//! ```text
//! 10s outer (whole cleanup)
//!   ├── 3s shutdown_hook (user-provided, runs first)
//!   └── 3s tracked-process kill (RunningProcesses + ACTIVE_*)
//! ```
//!
//! The `shutdown_hook` is where the connector stops its own subsystems
//! (Jupyter servers, backend services, etc.). It runs first so it can
//! issue graceful stops before we resort to SIGKILL.

use crate::state::RunningProcesses;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Manager};

pub const OUTER_TIMEOUT: Duration = Duration::from_secs(10);
pub const HOOK_TIMEOUT: Duration = Duration::from_secs(3);
pub const KILL_TIMEOUT: Duration = Duration::from_secs(3);

/// Trait implemented by the connector so it can register graceful-stop
/// behaviour for whatever subsystems it owns (Jupyter, backends, MCP, etc.).
#[async_trait::async_trait]
pub trait ShutdownHook: Send + Sync {
    /// Called inside the cleanup cascade with a bounded 3-second budget.
    /// Must not block beyond that. Failures should be logged, not returned —
    /// the cascade never blocks on hook errors.
    async fn shutdown(&self, app: AppHandle);
}

/// No-op default. The shell registers this if the connector doesn't override.
pub struct NoopShutdownHook;

#[async_trait::async_trait]
impl ShutdownHook for NoopShutdownHook {
    async fn shutdown(&self, _app: AppHandle) {}
}

/// Run the full cascade. Returns when complete or when the 10s outer
/// timeout fires, whichever is sooner.
pub async fn cleanup_all_processes(app: AppHandle) {
    let result = tokio::time::timeout(OUTER_TIMEOUT, async {
        // Step 1: connector-provided graceful shutdown.
        if let Some(hook) = app.try_state::<Arc<dyn ShutdownHook>>() {
            let hook = Arc::clone(&hook);
            let app2 = app.clone();
            let _ = tokio::time::timeout(HOOK_TIMEOUT, hook.shutdown(app2)).await;
        } else {
            log::warn!("no ShutdownHook registered; skipping graceful step");
        }

        // Step 2: kill any tracked processes the connector failed to stop.
        let _ = tokio::time::timeout(KILL_TIMEOUT, async {
            if let Some(procs) = app.try_state::<RunningProcesses>() {
                for id in procs.ids() {
                    log::info!("force-killing tracked process: {id}");
                    procs.kill(&id);
                }
            }
        })
        .await;
    })
    .await;

    if result.is_err() {
        log::error!(
            "cleanup_all_processes hit outer 10s timeout; exiting anyway"
        );
    }

    // Windows-only: give GDI/UI handles a moment to settle before exit.
    #[cfg(windows)]
    tokio::time::sleep(Duration::from_millis(500)).await;
}

/// Variant for sync contexts (SIGINT handler, Obj-C observer): builds a
/// fresh runtime to drive the async cascade. Use only where you can't
/// reach the existing runtime.
pub fn cleanup_blocking(app: AppHandle) {
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            log::error!("could not build runtime for blocking cleanup: {e}");
            return;
        }
    };
    rt.block_on(cleanup_all_processes(app));
}
