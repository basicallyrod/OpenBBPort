//! Subprocess spawn helper.
//!
//! Wraps `std::process::Command` with the line-streaming pattern used
//! throughout the shell:
//!
//! 1. `stdout`/`stderr` are `Stdio::piped()`.
//! 2. One thread per stream reads `BufRead::lines()`.
//! 3. Each line is appended to `LogStorage[process_id]` AND emitted as a
//!    `process-output` Tauri event.
//! 4. Optional ANSI/CR cleanup runs server-side so the renderer doesn't
//!    have to know about terminal control codes.
//!
//! Returns immediately with the `Child` handle; callers can register it
//! with `RunningProcesses` for cleanup tracking.

use crate::events::{ProcessOutputEvent, PROCESS_OUTPUT};
use crate::state::{self, LogEntry, LogStorage};
use regex::Regex;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use tauri::{AppHandle, Emitter};

/// Strip CSI escape sequences (color codes, cursor movement, etc.).
fn strip_ansi(s: &str) -> String {
    static ANSI: once_cell::sync::Lazy<Regex> =
        once_cell::sync::Lazy::new(|| Regex::new(r"\x1B\[[0-9;]*[a-zA-Z]").unwrap());
    ANSI.replace_all(s, "").into_owned()
}

/// Handle backspace + carriage-return rewrites (progress bars).
fn collapse_overwrites(s: &str) -> String {
    // Drop the character preceding any backspace.
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch == '\x08' {
            out.pop();
        } else {
            out.push(ch);
        }
    }
    // For lines containing `\r`, keep only the last non-empty segment.
    if out.contains('\r') {
        let last = out
            .rsplit('\r')
            .find(|seg| !seg.trim().is_empty())
            .unwrap_or("");
        last.to_string()
    } else {
        out
    }
}

#[derive(Debug, Clone, Default)]
pub struct SpawnOptions {
    /// Strip ANSI escapes server-side before storage + event emit.
    pub strip_ansi: bool,
    /// Collapse `\b` and `\r`-overwritten progress lines.
    pub collapse_overwrites: bool,
    /// Drop emitted lines that are empty after cleaning.
    pub drop_empty: bool,
    /// Optional override for the LogStorage cap (defaults to 10000).
    pub ring_capacity: Option<usize>,
}

impl SpawnOptions {
    /// Recommended defaults for shell-spawned subprocesses.
    pub fn defaults() -> Self {
        Self {
            strip_ansi: true,
            collapse_overwrites: true,
            drop_empty: true,
            ring_capacity: None,
        }
    }
}

/// Spawn a configured `Command`, attach the two reader threads, return the `Child`.
///
/// The caller is responsible for registering the child with `RunningProcesses`
/// if cleanup-on-quit tracking is needed.
///
/// `process_id` is the LogStorage / event-channel namespace (e.g.
/// `"backend-<uuid>"`, `"jupyter-<env>"`).
pub fn spawn_with_streaming(
    mut cmd: Command,
    process_id: impl Into<String>,
    storage: LogStorage,
    app: AppHandle,
    opts: SpawnOptions,
) -> std::io::Result<Child> {
    let process_id = process_id.into();
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn()?;

    state::register_process(&storage, &process_id);

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    if let Some(stdout) = stdout {
        let pid = process_id.clone();
        let storage = storage.clone();
        let app = app.clone();
        let opts = opts.clone();
        std::thread::spawn(move || {
            stream_reader(stdout, pid, storage, app, opts, "stdout");
        });
    }

    if let Some(stderr) = stderr {
        let pid = process_id.clone();
        let storage = storage.clone();
        let app = app.clone();
        let opts = opts.clone();
        std::thread::spawn(move || {
            stream_reader(stderr, pid, storage, app, opts, "stderr");
        });
    }

    Ok(child)
}

fn stream_reader<R: std::io::Read + Send + 'static>(
    reader: R,
    process_id: String,
    storage: LogStorage,
    app: AppHandle,
    opts: SpawnOptions,
    kind: &'static str,
) {
    let buf = BufReader::new(reader);
    for line in buf.lines().map_while(Result::ok) {
        let mut content = line;
        if opts.strip_ansi {
            content = strip_ansi(&content);
        }
        if opts.collapse_overwrites {
            content = collapse_overwrites(&content);
        }
        if opts.drop_empty && content.trim().is_empty() {
            continue;
        }

        let timestamp = chrono::Utc::now().timestamp_millis();
        state::append_entry(
            &storage,
            LogEntry {
                timestamp,
                content: content.clone(),
                process_id: process_id.clone(),
            },
        );

        let _ = app.emit(
            PROCESS_OUTPUT,
            ProcessOutputEvent {
                process_id: process_id.clone(),
                output: content,
                timestamp,
                kind: kind.to_string(),
            },
        );
    }
}

/// Convenience: emit a synthesized `system`-kind line into the buffer
/// and event stream. Use for "Stopping X..." banners.
pub fn emit_system_line(
    app: &AppHandle,
    storage: &LogStorage,
    process_id: impl Into<String>,
    line: impl Into<String>,
) {
    let process_id = process_id.into();
    let line = line.into();
    let timestamp = chrono::Utc::now().timestamp_millis();
    state::append_entry(
        storage,
        LogEntry {
            timestamp,
            content: line.clone(),
            process_id: process_id.clone(),
        },
    );
    let _ = app.emit(
        PROCESS_OUTPUT,
        ProcessOutputEvent {
            process_id,
            output: line,
            timestamp,
            kind: "system".into(),
        },
    );
}
