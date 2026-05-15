//! Public face of the LogStorage: idempotent register/unregister, history
//! replay, and clear. These are wired as Tauri commands in
//! `ipc::infrastructure`.

use crate::state::{self, LogStorage};

/// Returns true if a new buffer was created; false if one already existed.
pub fn register(storage: &LogStorage, process_id: &str) -> bool {
    state::register_process(storage, process_id)
}

pub fn unregister(storage: &LogStorage, process_id: &str) -> bool {
    state::unregister_process(storage, process_id)
}

pub fn clear(storage: &LogStorage, process_id: &str) -> bool {
    state::clear_process_logs(storage, process_id)
}

pub fn history(
    storage: &LogStorage,
    process_id: &str,
    count: Option<usize>,
) -> Vec<state::LogEntry> {
    state::get_logs(storage, process_id, count)
}
