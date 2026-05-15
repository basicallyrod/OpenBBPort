//! Infrastructure commands — fully implemented.
//!
//! These are the commands that operate on the in-memory `LogStorage`
//! ring buffer. Connector code does NOT need to reimplement them; just
//! call `tauri_shell::process_spawn::spawn_with_streaming(...)` to push
//! into the buffer and the renderer can call these to read it back out.

use crate::process_monitor;
use crate::state::{LogEntry, ProcessLogState};
use tauri::State;

#[tauri::command]
pub fn register_process_monitoring(
    process_id: String,
    state: State<'_, ProcessLogState>,
) -> bool {
    process_monitor::register(&state.0, &process_id)
}

#[tauri::command]
pub fn unregister_process_monitoring(
    process_id: String,
    state: State<'_, ProcessLogState>,
) -> bool {
    process_monitor::unregister(&state.0, &process_id)
}

#[tauri::command]
pub fn get_process_logs_history(
    process_id: String,
    count: Option<usize>,
    state: State<'_, ProcessLogState>,
) -> Vec<LogEntry> {
    process_monitor::history(&state.0, &process_id, count)
}

#[tauri::command]
pub fn clear_process_logs_history(
    process_id: String,
    state: State<'_, ProcessLogState>,
) -> bool {
    process_monitor::clear(&state.0, &process_id)
}
