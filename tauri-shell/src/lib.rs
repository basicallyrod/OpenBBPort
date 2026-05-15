//! tauri-shell — desktop wrapper with open-ended connectors.
//!
//! This crate exposes its modules so integration tests under `tests/` can
//! exercise the infrastructure without booting a Tauri runtime.

pub mod autostart;
pub mod cleanup;
pub mod connector;
pub mod events;
pub mod ipc;
pub mod path_utils;
pub mod process_kill;
pub mod process_monitor;
pub mod process_spawn;
pub mod proxy;
pub mod settings;
pub mod state;
pub mod tray;
pub mod updater;
pub mod windows;
