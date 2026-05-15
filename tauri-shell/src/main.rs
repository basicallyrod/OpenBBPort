//! Tauri shell entry point.
//!
//! Boot order:
//! 1. `fix-path-env` to repair `$PATH` for macOS GUI launches (not used
//!    here by default; uncomment the import to enable if you ship a
//!    bundled CLI).
//! 2. Build the Tauri app with all 8 plugins.
//! 3. Manage state singletons (`ProcessLogState`, `RunningProcesses`,
//!    `InstallationState`, `CancellationRegistry`, `ShutdownHook`).
//! 4. Register IPC handlers via `tauri::generate_handler!`.
//! 5. In the `.setup` hook:
//!    - Read the boot installation snapshot
//!    - Install the system tray
//!    - Apply close-to-tray on the main window
//!    - Install Ctrl-C handler
//!    - Show the main window
//!    - Kick off a background update check
//! 6. Drive the run loop, intercepting `ExitRequested` to run cleanup.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;

use tauri::{Manager, RunEvent};
use tauri_shell::{
    cleanup::{self, NoopShutdownHook, ShutdownHook},
    events::NAVIGATE,
    state::{
        CancellationRegistry, InstallationSnapshot, InstallationState, ProcessLogState,
        RunningProcesses,
    },
    tray,
    updater,
    windows,
};

fn main() {
    #[cfg(desktop)]
    let single_instance_plugin = tauri_plugin_single_instance::init(|app, _argv, _cwd| {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.set_focus();
        }
    });

    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        builder = builder.plugin(single_instance_plugin);
        builder = builder.plugin(tauri_plugin_updater::Builder::default().build());
    }

    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_persisted_scope::init())
        .plugin(tauri_plugin_log::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        // State
        .manage(ProcessLogState::default())
        .manage(RunningProcesses::new())
        .manage(InstallationState::new(boot_snapshot()))
        .manage(CancellationRegistry::new())
        .manage::<Arc<dyn ShutdownHook>>(Arc::new(NoopShutdownHook))
        // IPC
        .invoke_handler(tauri::generate_handler![
            // app
            tauri_shell::ipc::app::get_installation_state,
            tauri_shell::ipc::app::navigate_to_page,
            tauri_shell::ipc::app::quit_application,
            tauri_shell::ipc::app::get_app_version,
            tauri_shell::ipc::app::toggle_theme,
            // infrastructure
            tauri_shell::ipc::infrastructure::register_process_monitoring,
            tauri_shell::ipc::infrastructure::unregister_process_monitoring,
            tauri_shell::ipc::infrastructure::get_process_logs_history,
            tauri_shell::ipc::infrastructure::clear_process_logs_history,
            // helpers
            tauri_shell::ipc::helpers::get_home_directory,
            tauri_shell::ipc::helpers::get_settings_directory,
            tauri_shell::ipc::helpers::select_directory,
            tauri_shell::ipc::helpers::select_file,
            tauri_shell::ipc::helpers::check_directory_exists,
            tauri_shell::ipc::helpers::check_file_exists,
            tauri_shell::ipc::helpers::open_url_in_window,
            tauri_shell::ipc::helpers::open_workspace_in_browser,
            tauri_shell::ipc::helpers::open_logs_window,
            tauri_shell::ipc::helpers::get_working_directory,
            tauri_shell::ipc::helpers::save_working_directory,
            // installation (stubs)
            tauri_shell::ipc::installation::install_to_directory,
            tauri_shell::ipc::installation::install_conda,
            tauri_shell::ipc::installation::setup_python_environment,
            tauri_shell::ipc::installation::abort_installation,
            tauri_shell::ipc::installation::get_installation_status,
            tauri_shell::ipc::installation::create_default_backend_services,
            tauri_shell::ipc::installation::update_openbb_settings,
            tauri_shell::ipc::installation::get_installation_directory,
            tauri_shell::ipc::installation::get_userdata_directory,
            // environments (stubs)
            tauri_shell::ipc::environments::list_conda_environments,
            tauri_shell::ipc::environments::create_environment,
            tauri_shell::ipc::environments::create_environment_from_requirements,
            tauri_shell::ipc::environments::select_requirements_file,
            tauri_shell::ipc::environments::get_environment_extensions,
            tauri_shell::ipc::environments::install_extensions,
            tauri_shell::ipc::environments::update_extension,
            tauri_shell::ipc::environments::update_environment,
            tauri_shell::ipc::environments::remove_extension,
            tauri_shell::ipc::environments::remove_environment,
            tauri_shell::ipc::environments::execute_in_environment,
            // backends (stubs)
            tauri_shell::ipc::backends::list_backend_services,
            tauri_shell::ipc::backends::create_backend_service,
            tauri_shell::ipc::backends::update_backend_service,
            tauri_shell::ipc::backends::delete_backend_service,
            tauri_shell::ipc::backends::start_backend_service,
            tauri_shell::ipc::backends::stop_backend_service,
            tauri_shell::ipc::backends::open_backend_logs_window,
            // jupyter (stubs)
            tauri_shell::ipc::jupyter::start_jupyter_server,
            tauri_shell::ipc::jupyter::stop_jupyter_server,
            tauri_shell::ipc::jupyter::check_jupyter_server,
            tauri_shell::ipc::jupyter::list_jupyter_servers,
            tauri_shell::ipc::jupyter::open_jupyter_logs_window,
            tauri_shell::ipc::jupyter::update_jupyter_status,
            // credentials
            tauri_shell::ipc::credentials::get_user_credentials,
            tauri_shell::ipc::credentials::update_user_credentials,
            tauri_shell::ipc::credentials::open_credentials_file,
            // certs (stub)
            tauri_shell::ipc::certs::generate_self_signed_cert,
            // uninstall (stub)
            tauri_shell::ipc::uninstall::uninstall_application,
        ])
        .setup(|app| {
            let app_handle = app.handle().clone();

            // Tray
            tray::build_tray(
                &app_handle,
                tray::TrayConfig {
                    tooltip: "Tauri Shell".into(),
                    nav_items: vec![],
                    show_autostart_toggle: true,
                    show_updater: true,
                    show_uninstall: false,
                },
            )?;

            // Close-to-tray on main window
            if let Some(main_window) = app.get_webview_window("main") {
                windows::apply_close_to_tray(main_window.clone());
                // Show the window — visible:false was set in tauri.conf.json
                let _ = main_window.show();
                let _ = main_window.set_focus();
            }

            // SIGINT handler — runs cleanup then exits
            let app_for_sigint = app_handle.clone();
            ctrlc::set_handler(move || {
                log::info!("SIGINT — running cleanup cascade");
                let app2 = app_for_sigint.clone();
                cleanup::cleanup_blocking(app2);
                std::process::exit(0);
            })
            .map_err(|e| Box::<dyn std::error::Error>::from(format!("ctrlc handler: {e}")))?;

            // Background update check (3s delay so the UI loads first)
            let app_for_updater = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                let _ = updater::check_and_apply(app_for_updater, false).await;
            });

            // Forward a one-time `bootReady` event to the renderer so it
            // can resolve initial routing without polling.
            use tauri::Emitter;
            let _ = app_handle.emit(NAVIGATE, tauri_shell::events::NavigateEvent { path: "/".into() });

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build Tauri app")
        .run(|app_handle, event| match event {
            RunEvent::ExitRequested { api, .. } => {
                api.prevent_exit();
                let app2 = app_handle.clone();
                tauri::async_runtime::spawn(async move {
                    cleanup::cleanup_all_processes(app2.clone()).await;
                    std::process::exit(0);
                });
            }
            #[cfg(target_os = "macos")]
            RunEvent::Reopen { .. } => {
                if let Some(w) = app_handle.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
            _ => {}
        });
}

/// Snapshot the installation state at boot. Connectors override this by
/// re-populating `tauri_shell::state::InstallationState` after their own
/// settings load completes (see `feature-installation.md`).
fn boot_snapshot() -> InstallationSnapshot {
    InstallationSnapshot {
        is_installed: false,
        installation_directory: None,
    }
}
