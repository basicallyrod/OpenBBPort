//! System tray menu.
//!
//! Items are configurable via [`TrayConfig`]. Navigation items send a
//! `navigate` event to the renderer rather than calling `window.eval`,
//! so the renderer's router stays in charge.

use crate::cleanup;
use crate::events::{NavigateEvent, NAVIGATE};
use crate::updater;
use tauri::{
    menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, WebviewWindow,
};

#[derive(Debug, Clone)]
pub struct TrayNavItem {
    pub id: String,
    pub label: String,
    pub route: String,
}

#[derive(Debug, Clone)]
pub struct TrayConfig {
    pub tooltip: String,
    /// Navigation items shown between the open-window and "Check for
    /// Updates" entries.
    pub nav_items: Vec<TrayNavItem>,
    /// Show the "Start at Login" check item.
    pub show_autostart_toggle: bool,
    /// Show "Check for Updates".
    pub show_updater: bool,
    /// Show "Uninstall" — routes to `/uninstall`.
    pub show_uninstall: bool,
}

impl Default for TrayConfig {
    fn default() -> Self {
        Self {
            tooltip: "Tauri Shell".into(),
            nav_items: Vec::new(),
            show_autostart_toggle: true,
            show_updater: true,
            show_uninstall: false,
        }
    }
}

pub fn build_tray(app: &AppHandle, config: TrayConfig) -> tauri::Result<()> {
    let mut menu = MenuBuilder::new(app);

    let open_item = MenuItemBuilder::with_id("tray:open", "Open Window").build(app)?;
    menu = menu.item(&open_item);

    if !config.nav_items.is_empty() {
        menu = menu.item(&PredefinedMenuItem::separator(app)?);
        for nav in &config.nav_items {
            let item = MenuItemBuilder::with_id(format!("tray:nav:{}", nav.id), &nav.label)
                .build(app)?;
            menu = menu.item(&item);
        }
    }

    if config.show_autostart_toggle {
        let initial = crate::autostart::is_enabled(app).unwrap_or(false);
        let item = CheckMenuItemBuilder::with_id("tray:autostart", "Start at Login")
            .checked(initial)
            .build(app)?;
        menu = menu.item(&PredefinedMenuItem::separator(app)?);
        menu = menu.item(&item);
    }

    if config.show_updater {
        menu = menu.item(&PredefinedMenuItem::separator(app)?);
        let item = MenuItemBuilder::with_id("tray:updater", "Check for Updates").build(app)?;
        menu = menu.item(&item);
    }

    if config.show_uninstall {
        let item = MenuItemBuilder::with_id("tray:uninstall", "Uninstall").build(app)?;
        menu = menu.item(&item);
    }

    menu = menu.item(&PredefinedMenuItem::separator(app)?);
    let quit_item = MenuItemBuilder::with_id("tray:quit", "Quit").build(app)?;
    menu = menu.item(&quit_item);

    let menu = menu.build()?;
    let tooltip_for_builder: String = config.tooltip.clone();
    let app_for_build = app.clone();
    let config_for_closure = config.clone();

    TrayIconBuilder::new()
        .tooltip(tooltip_for_builder.as_str())
        .menu(&menu)
        .on_menu_event(move |app, event| {
            let id = event.id.as_ref();
            match id {
                "tray:open" => {
                    if let Some(w) = main_window(app) {
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                }
                "tray:quit" => {
                    let app2 = app.clone();
                    tauri::async_runtime::spawn(async move {
                        cleanup::cleanup_all_processes(app2.clone()).await;
                        app2.exit(0);
                    });
                }
                "tray:updater" => {
                    let app2 = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = updater::check_and_apply(app2, true).await;
                    });
                }
                "tray:autostart" => {
                    let enabled = crate::autostart::is_enabled(app).unwrap_or(false);
                    let target = !enabled;
                    let result = if target {
                        crate::autostart::enable(app)
                    } else {
                        crate::autostart::disable(app)
                    };
                    if let Err(e) = result {
                        log::error!("autostart toggle failed: {e}");
                    }
                    // Note: the check-item state will drift from reality
                    // if the OS rejects the toggle. Re-query on next menu
                    // open is left to the connector if needed.
                }
                "tray:uninstall" => emit_navigate(app, "/uninstall"),
                other if other.starts_with("tray:nav:") => {
                    let key = other.trim_start_matches("tray:nav:").to_string();
                    if let Some(nav) = config_for_closure.nav_items.iter().find(|n| n.id == key) {
                        emit_navigate(app, &nav.route);
                    }
                }
                _ => {}
            }
        })
        .build(&app_for_build)?;
    Ok(())
}

fn main_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("main")
}

fn emit_navigate(app: &AppHandle, path: &str) {
    if let Some(w) = main_window(app) {
        let _ = w.show();
        let _ = w.set_focus();
    }
    let _ = app.emit(NAVIGATE, NavigateEvent { path: path.into() });
}
