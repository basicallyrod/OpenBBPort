//! Managed state singletons.
//!
//! All shared state used by the shell lives here. Three of these are
//! `Lazy<Mutex<_>>` globals (the conventional Tauri pattern when state must
//! be reached from background threads where extracting `tauri::State<T>` is
//! awkward); the rest are wrapped in newtypes for `.manage(...)`.

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// LogStorage — per-process in-memory ring buffer
// ---------------------------------------------------------------------------

pub const DEFAULT_RING_CAPACITY: usize = 10_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "../bindings/"))]
pub struct LogEntry {
    /// Milliseconds since epoch.
    pub timestamp: i64,
    pub content: String,
    pub process_id: String,
}

#[derive(Debug)]
pub struct LogBuffer {
    entries: VecDeque<LogEntry>,
    capacity: usize,
}

impl LogBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(capacity.min(1024)),
            capacity,
        }
    }

    /// Append one entry. Evicts the oldest if at capacity. O(1) amortized.
    pub fn push(&mut self, entry: LogEntry) {
        if self.entries.len() >= self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the last `count` entries, or all if `count` is None or exceeds the buffer length.
    pub fn tail(&self, count: Option<usize>) -> Vec<LogEntry> {
        match count {
            Some(n) if n < self.entries.len() => self
                .entries
                .iter()
                .skip(self.entries.len() - n)
                .cloned()
                .collect(),
            _ => self.entries.iter().cloned().collect(),
        }
    }
}

pub type LogStorage = Arc<Mutex<HashMap<String, LogBuffer>>>;

pub static LOG_STORAGE: Lazy<LogStorage> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

/// Convenience accessor — returns a clone of the global `Arc`.
pub fn log_storage() -> LogStorage {
    Arc::clone(&LOG_STORAGE)
}

/// Tauri-managed wrapper so it can be injected as `State<'_, ProcessLogState>`.
#[derive(Clone)]
pub struct ProcessLogState(pub LogStorage);

impl Default for ProcessLogState {
    fn default() -> Self {
        Self(log_storage())
    }
}

/// Idempotent: returns false if a buffer already exists for that id.
pub fn register_process(storage: &LogStorage, process_id: &str) -> bool {
    let mut g = match storage.lock() {
        Ok(g) => g,
        Err(_) => return false,
    };
    if g.contains_key(process_id) {
        return false;
    }
    g.insert(process_id.to_string(), LogBuffer::new(DEFAULT_RING_CAPACITY));
    true
}

pub fn unregister_process(storage: &LogStorage, process_id: &str) -> bool {
    let mut g = match storage.lock() {
        Ok(g) => g,
        Err(_) => return false,
    };
    g.remove(process_id).is_some()
}

pub fn clear_process_logs(storage: &LogStorage, process_id: &str) -> bool {
    let mut g = match storage.lock() {
        Ok(g) => g,
        Err(_) => return false,
    };
    match g.get_mut(process_id) {
        Some(buf) => {
            buf.clear();
            true
        }
        None => false,
    }
}

pub fn append_entry(storage: &LogStorage, entry: LogEntry) {
    let pid = entry.process_id.clone();
    if let Ok(mut g) = storage.lock() {
        g.entry(pid)
            .or_insert_with(|| LogBuffer::new(DEFAULT_RING_CAPACITY))
            .push(entry);
    }
}

pub fn get_logs(storage: &LogStorage, process_id: &str, count: Option<usize>) -> Vec<LogEntry> {
    let g = match storage.lock() {
        Ok(g) => g,
        Err(_) => return Vec::new(),
    };
    g.get(process_id)
        .map(|b| b.tail(count))
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// RunningProcesses — map of caller-supplied id → spawned Child
// ---------------------------------------------------------------------------

#[derive(Default)]
pub struct RunningProcesses(pub Arc<Mutex<HashMap<String, std::process::Child>>>);

impl RunningProcesses {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a child. Errors if `id` is already present (caller is
    /// responsible for ensuring uniqueness).
    pub fn add(&self, id: String, child: std::process::Child) -> Result<(), String> {
        let mut g = self.0.lock().map_err(|e| e.to_string())?;
        if g.contains_key(&id) {
            return Err(format!("process id already registered: {id}"));
        }
        g.insert(id, child);
        Ok(())
    }

    /// Returns true if a tracked child with this id is still running.
    /// Self-cleans the map if the child has exited.
    pub fn is_running(&self, id: &str) -> bool {
        let mut g = match self.0.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };
        match g.get_mut(id) {
            None => false,
            Some(child) => match child.try_wait() {
                Ok(Some(_)) => {
                    g.remove(id);
                    false
                }
                Ok(None) => true,
                Err(_) => false,
            },
        }
    }

    /// Kills (`SIGKILL`/`TerminateProcess`) and reaps the tracked child.
    /// Returns true if a child was killed.
    pub fn kill(&self, id: &str) -> bool {
        let mut g = match self.0.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };
        match g.remove(id) {
            Some(mut child) => {
                let _ = child.kill();
                let _ = child.wait();
                true
            }
            None => false,
        }
    }

    /// Removes any tracked children that have exited.
    pub fn cleanup_dead(&self) {
        let mut g = match self.0.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        let dead: Vec<String> = g
            .iter_mut()
            .filter_map(|(k, child)| match child.try_wait() {
                Ok(Some(_)) => Some(k.clone()),
                _ => None,
            })
            .collect();
        for k in dead {
            g.remove(&k);
        }
    }

    pub fn ids(&self) -> Vec<String> {
        self.0
            .lock()
            .map(|g| g.keys().cloned().collect())
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// InstallationState — boot-time snapshot, immutable after setup
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "../bindings/", rename_all = "camelCase"))]
pub struct InstallationSnapshot {
    pub is_installed: bool,
    pub installation_directory: Option<String>,
}

/// Tauri-managed wrapper.
pub struct InstallationState(pub Mutex<InstallationSnapshot>);

impl InstallationState {
    pub fn new(snapshot: InstallationSnapshot) -> Self {
        Self(Mutex::new(snapshot))
    }

    pub fn read(&self) -> InstallationSnapshot {
        self.0.lock().map(|g| g.clone()).unwrap_or_default()
    }

    pub fn replace(&self, snapshot: InstallationSnapshot) {
        if let Ok(mut g) = self.0.lock() {
            *g = snapshot;
        }
    }
}

// ---------------------------------------------------------------------------
// InstallationProgress — live phase mirror for a running install
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "../bindings/", rename_all = "camelCase"))]
pub struct InstallationProgress {
    pub is_downloading: bool,
    pub is_installing: bool,
    pub is_configuring: bool,
    pub is_complete: bool,
    pub message: String,
}

pub static INSTALLATION_PROGRESS: Lazy<Mutex<InstallationProgress>> =
    Lazy::new(|| Mutex::new(InstallationProgress::default()));

/// Re-entrancy guard for the install pipeline.
pub static INSTALLATION_IN_PROGRESS: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));

// ---------------------------------------------------------------------------
// CancellationRegistry — names map to AbortHandles for long-running ops
// ---------------------------------------------------------------------------

use tokio::task::AbortHandle;

#[derive(Default)]
pub struct CancellationRegistry(pub Mutex<HashMap<String, AbortHandle>>);

impl CancellationRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&self, token: String, handle: AbortHandle) {
        if let Ok(mut g) = self.0.lock() {
            g.insert(token, handle);
        }
    }

    pub fn cancel(&self, token: &str) -> bool {
        let mut g = match self.0.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };
        match g.remove(token) {
            Some(h) => {
                h.abort();
                true
            }
            None => false,
        }
    }

    pub fn remove(&self, token: &str) {
        if let Ok(mut g) = self.0.lock() {
            g.remove(token);
        }
    }
}
