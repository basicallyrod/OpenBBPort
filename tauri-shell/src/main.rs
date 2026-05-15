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
        .manage(tauri_shell::proxy::Proxy::new())
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
            // ============ Python REST proxy ============
            tauri_shell::ipc::obb::obb_call,
            tauri_shell::ipc::obb::obb_set_base_url,
            tauri_shell::ipc::obb::obb_get_base_url,
            tauri_shell::ipc::obb::obb_set_basic_auth,
            tauri_shell::ipc::obb::obb_set_bearer,
            tauri_shell::ipc::obb::obb_clear_auth,
            tauri_shell::ipc::obb::obb_health,
            tauri_shell::ipc::obb::obb_openapi,
            tauri_shell::ipc::obb::obb_widgets,
            tauri_shell::ipc::obb::obb_apps,
            tauri_shell::ipc::obb::obb_agents,
            tauri_shell::ipc::obb::obb_coverage_commands,
            tauri_shell::ipc::obb::obb_coverage_providers,
            tauri_shell::ipc::obb::obb_coverage_command_model,
            tauri_shell::ipc::obb::obb_user_me,
            tauri_shell::ipc::obb::obb_system,
            // ============ Typed route wrappers (60) ============
            tauri_shell::ipc::obb_routes::equity_search,
            tauri_shell::ipc::obb_routes::equity_screener,
            tauri_shell::ipc::obb_routes::equity_profile,
            tauri_shell::ipc::obb_routes::equity_market_snapshots,
            tauri_shell::ipc::obb_routes::equity_historical_market_cap,
            tauri_shell::ipc::obb_routes::equity_price_historical,
            tauri_shell::ipc::obb_routes::equity_price_quote,
            tauri_shell::ipc::obb_routes::equity_price_nbbo,
            tauri_shell::ipc::obb_routes::equity_price_performance,
            tauri_shell::ipc::obb_routes::equity_fundamental_balance,
            tauri_shell::ipc::obb_routes::equity_fundamental_income,
            tauri_shell::ipc::obb_routes::equity_fundamental_cash,
            tauri_shell::ipc::obb_routes::equity_fundamental_ratios,
            tauri_shell::ipc::obb_routes::equity_fundamental_metrics,
            tauri_shell::ipc::obb_routes::equity_fundamental_dividends,
            tauri_shell::ipc::obb_routes::equity_fundamental_filings,
            tauri_shell::ipc::obb_routes::equity_calendar_earnings,
            tauri_shell::ipc::obb_routes::equity_calendar_dividends,
            tauri_shell::ipc::obb_routes::equity_calendar_splits,
            tauri_shell::ipc::obb_routes::equity_calendar_ipo,
            tauri_shell::ipc::obb_routes::equity_calendar_events,
            tauri_shell::ipc::obb_routes::equity_discovery_gainers,
            tauri_shell::ipc::obb_routes::equity_discovery_losers,
            tauri_shell::ipc::obb_routes::equity_discovery_active,
            tauri_shell::ipc::obb_routes::equity_ownership_insider_trading,
            tauri_shell::ipc::obb_routes::equity_ownership_institutional,
            tauri_shell::ipc::obb_routes::equity_estimates_price_target,
            tauri_shell::ipc::obb_routes::equity_estimates_consensus,
            tauri_shell::ipc::obb_routes::crypto_search,
            tauri_shell::ipc::obb_routes::crypto_price_historical,
            tauri_shell::ipc::obb_routes::currency_search,
            tauri_shell::ipc::obb_routes::currency_pairs,
            tauri_shell::ipc::obb_routes::currency_snapshots,
            tauri_shell::ipc::obb_routes::currency_reference_rates,
            tauri_shell::ipc::obb_routes::currency_price_historical,
            tauri_shell::ipc::obb_routes::derivatives_options_chains,
            tauri_shell::ipc::obb_routes::derivatives_options_unusual,
            tauri_shell::ipc::obb_routes::derivatives_options_snapshots,
            tauri_shell::ipc::obb_routes::derivatives_futures_historical,
            tauri_shell::ipc::obb_routes::derivatives_futures_curve,
            tauri_shell::ipc::obb_routes::derivatives_futures_info,
            tauri_shell::ipc::obb_routes::derivatives_futures_instruments,
            tauri_shell::ipc::obb_routes::etf_search,
            tauri_shell::ipc::obb_routes::etf_info,
            tauri_shell::ipc::obb_routes::etf_historical,
            tauri_shell::ipc::obb_routes::etf_holdings,
            tauri_shell::ipc::obb_routes::etf_sectors,
            tauri_shell::ipc::obb_routes::etf_countries,
            tauri_shell::ipc::obb_routes::index_search,
            tauri_shell::ipc::obb_routes::index_historical,
            tauri_shell::ipc::obb_routes::index_constituents,
            tauri_shell::ipc::obb_routes::index_snapshots,
            tauri_shell::ipc::obb_routes::index_available,
            tauri_shell::ipc::obb_routes::economy_cpi,
            tauri_shell::ipc::obb_routes::economy_calendar,
            tauri_shell::ipc::obb_routes::economy_indicators,
            tauri_shell::ipc::obb_routes::economy_gdp_real,
            tauri_shell::ipc::obb_routes::economy_gdp_nominal,
            tauri_shell::ipc::obb_routes::economy_gdp_forecast,
            tauri_shell::ipc::obb_routes::economy_unemployment,
            tauri_shell::ipc::obb_routes::economy_fred_search,
            tauri_shell::ipc::obb_routes::economy_fred_series,
            tauri_shell::ipc::obb_routes::fixedincome_government_yield_curve,
            tauri_shell::ipc::obb_routes::fixedincome_government_treasury_rates,
            tauri_shell::ipc::obb_routes::fixedincome_corporate_bond_indices,
            tauri_shell::ipc::obb_routes::fixedincome_rate_sofr,
            tauri_shell::ipc::obb_routes::fixedincome_rate_fed_funds,
            tauri_shell::ipc::obb_routes::news_world,
            tauri_shell::ipc::obb_routes::news_company,
            tauri_shell::ipc::obb_routes::regulators_sec_filings,
            tauri_shell::ipc::obb_routes::regulators_sec_company_filings,
            tauri_shell::ipc::obb_routes::regulators_cftc_cot,
            tauri_shell::ipc::obb_routes::commodity_price_spot,
            tauri_shell::ipc::obb_routes::commodity_petroleum_status,
            tauri_shell::ipc::obb_routes::commodity_weather,
            tauri_shell::ipc::obb_routes::technical_sma,
            tauri_shell::ipc::obb_routes::technical_ema,
            tauri_shell::ipc::obb_routes::technical_rsi,
            tauri_shell::ipc::obb_routes::technical_macd,
            tauri_shell::ipc::obb_routes::technical_bbands,
            tauri_shell::ipc::obb_routes::quantitative_summary,
            tauri_shell::ipc::obb_routes::quantitative_normality,
            tauri_shell::ipc::obb_routes::quantitative_unit_root,
            tauri_shell::ipc::obb_routes::quantitative_performance_omega,
            tauri_shell::ipc::obb_routes::quantitative_performance_sharpe,
            tauri_shell::ipc::obb_routes::econometrics_correlation,
            tauri_shell::ipc::obb_routes::econometrics_ols,
            tauri_shell::ipc::obb_routes::econometrics_granger,
            // ============ Discovery / introspection ============
            tauri_shell::ipc::openbb_meta::list_all_routes,
            tauri_shell::ipc::openbb_meta::search_routes,
            tauri_shell::ipc::openbb_meta::route_parameters,
            // ============ Provider catalog ============
            tauri_shell::ipc::provider::provider_list,
            tauri_shell::ipc::provider::provider_routes,
            tauri_shell::ipc::provider::provider_credentials,
            tauri_shell::ipc::provider::provider_validate,
            // ============ Settings files ============
            tauri_shell::ipc::settings_files::read_settings_json,
            tauri_shell::ipc::settings_files::write_settings_json,
            tauri_shell::ipc::settings_files::read_settings_text,
            tauri_shell::ipc::settings_files::write_settings_text,
            tauri_shell::ipc::settings_files::list_settings_files,
            // ============ Server lifecycle ============
            tauri_shell::ipc::server::server_spawn,
            tauri_shell::ipc::server::server_stop,
            tauri_shell::ipc::server::server_status,
            tauri_shell::ipc::server::server_list,
            tauri_shell::ipc::server::server_attach,
            tauri_shell::ipc::server::server_health,
            // ============ MCP server ============
            tauri_shell::ipc::mcp::mcp_spawn,
            tauri_shell::ipc::mcp::mcp_stop,
            tauri_shell::ipc::mcp::mcp_status,
            tauri_shell::ipc::mcp::mcp_list,
            tauri_shell::ipc::mcp::mcp_list_tools,
            // ============ Routines (.openbb files) ============
            tauri_shell::ipc::routines::routines_list,
            tauri_shell::ipc::routines::routines_read,
            tauri_shell::ipc::routines::routines_save,
            tauri_shell::ipc::routines::routines_delete,
            tauri_shell::ipc::routines::routines_rename,
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
