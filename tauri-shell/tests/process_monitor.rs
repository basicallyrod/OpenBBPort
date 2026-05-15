//! Integration tests for `process_monitor` + the underlying `LogStorage`
//! singleton in `state.rs`.
//!
//! Every test uses a freshly-constructed `LogStorage` instance (via
//! `Arc::new(Mutex::new(...))`) so the tests are hermetic w.r.t. the global
//! `LOG_STORAGE`. Tests that *do* mutate the global are marked `#[serial]`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;

use serial_test::serial;
use tauri_shell::process_monitor;
use tauri_shell::state::{
    self, append_entry, LogBuffer, LogEntry, LogStorage, DEFAULT_RING_CAPACITY, LOG_STORAGE,
};

fn fresh_storage() -> LogStorage {
    Arc::new(Mutex::new(HashMap::new()))
}

fn entry(pid: &str, content: &str) -> LogEntry {
    LogEntry {
        timestamp: 0,
        content: content.to_string(),
        process_id: pid.to_string(),
    }
}

#[test]
fn register_creates_buffer_and_is_idempotent() {
    let storage = fresh_storage();
    assert!(process_monitor::register(&storage, "proc-1"));
    // Second register on the same id is a no-op (returns false).
    assert!(!process_monitor::register(&storage, "proc-1"));
    // History is empty for a freshly-registered process.
    assert!(process_monitor::history(&storage, "proc-1", None).is_empty());
}

#[test]
fn get_returns_appended_entries_in_order() {
    let storage = fresh_storage();
    process_monitor::register(&storage, "proc-x");
    for i in 0..100 {
        append_entry(&storage, entry("proc-x", &format!("line-{i}")));
    }
    let logs = process_monitor::history(&storage, "proc-x", None);
    assert_eq!(logs.len(), 100);
    assert_eq!(logs.first().unwrap().content, "line-0");
    assert_eq!(logs.last().unwrap().content, "line-99");
}

#[test]
fn clear_empties_buffer_but_keeps_registration() {
    let storage = fresh_storage();
    process_monitor::register(&storage, "proc-c");
    for i in 0..10 {
        append_entry(&storage, entry("proc-c", &format!("l{i}")));
    }
    assert_eq!(process_monitor::history(&storage, "proc-c", None).len(), 10);
    assert!(process_monitor::clear(&storage, "proc-c"));
    assert!(process_monitor::history(&storage, "proc-c", None).is_empty());
    // Re-register should still report "already registered".
    assert!(!process_monitor::register(&storage, "proc-c"));
}

#[test]
fn unregister_drops_buffer_and_history() {
    let storage = fresh_storage();
    process_monitor::register(&storage, "proc-u");
    append_entry(&storage, entry("proc-u", "hello"));
    assert!(process_monitor::unregister(&storage, "proc-u"));
    // After unregister, history is empty.
    assert!(process_monitor::history(&storage, "proc-u", None).is_empty());
    // Second unregister returns false.
    assert!(!process_monitor::unregister(&storage, "proc-u"));
}

#[test]
fn capacity_overflow_evicts_oldest() {
    // Use the LogBuffer directly with a small capacity for a fast test.
    let mut buf = LogBuffer::new(5);
    for i in 0..8 {
        buf.push(entry("proc", &format!("line-{i}")));
    }
    assert_eq!(buf.len(), 5);
    let all = buf.tail(None);
    // Oldest 3 should have been evicted.
    assert_eq!(all.first().unwrap().content, "line-3");
    assert_eq!(all.last().unwrap().content, "line-7");
}

#[test]
fn multi_id_isolation() {
    let storage = fresh_storage();
    for pid in ["a", "b", "c"] {
        process_monitor::register(&storage, pid);
        for i in 0..5 {
            append_entry(&storage, entry(pid, &format!("{pid}-{i}")));
        }
    }
    // Clearing one id must not affect the others.
    process_monitor::clear(&storage, "b");
    assert_eq!(process_monitor::history(&storage, "a", None).len(), 5);
    assert_eq!(process_monitor::history(&storage, "b", None).len(), 0);
    assert_eq!(process_monitor::history(&storage, "c", None).len(), 5);
    // Sanity-check that the entries did not leak across keys.
    for e in process_monitor::history(&storage, "a", None) {
        assert!(e.content.starts_with("a-"));
    }
    for e in process_monitor::history(&storage, "c", None) {
        assert!(e.content.starts_with("c-"));
    }
}

#[test]
fn concurrent_writes_then_reads_are_safe() {
    let storage = fresh_storage();
    process_monitor::register(&storage, "proc-mt");

    let mut handles = Vec::new();
    for t in 0..8 {
        let storage = Arc::clone(&storage);
        handles.push(thread::spawn(move || {
            for i in 0..125 {
                append_entry(
                    &storage,
                    entry("proc-mt", &format!("t{t}-i{i}")),
                );
            }
        }));
    }
    // Reader thread runs concurrently with writers.
    let storage_r = Arc::clone(&storage);
    let reader = thread::spawn(move || {
        let mut last = 0usize;
        for _ in 0..50 {
            let n = process_monitor::history(&storage_r, "proc-mt", None).len();
            assert!(n >= last, "log length must be monotonic");
            last = n;
        }
    });
    for h in handles {
        h.join().expect("writer thread panicked");
    }
    reader.join().expect("reader thread panicked");

    // 8 writers * 125 entries = 1000 entries (well under ring capacity).
    let final_len = process_monitor::history(&storage, "proc-mt", None).len();
    assert_eq!(final_len, 1000);
}

#[test]
#[serial]
fn global_log_storage_handle_round_trips() {
    // Touches the global singleton — keep serial to avoid contention with
    // other tests that mutate LOG_STORAGE.
    let storage: LogStorage = state::log_storage();
    let pid = "global-roundtrip-test";
    // Cleanup any prior state from earlier serial tests.
    process_monitor::unregister(&storage, pid);

    assert!(process_monitor::register(&storage, pid));
    append_entry(&storage, entry(pid, "global-hello"));
    let logs = process_monitor::history(&storage, pid, None);
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].content, "global-hello");
    assert!(process_monitor::unregister(&storage, pid));

    // Sanity: both handle clones point at the same map.
    let s2 = state::log_storage();
    assert!(Arc::ptr_eq(&storage, &s2));
    // The original LOG_STORAGE lazy is the same Arc.
    assert!(Arc::ptr_eq(&LOG_STORAGE, &storage));
    // Default ring capacity hasn't been silently changed.
    assert_eq!(DEFAULT_RING_CAPACITY, 10_000);
}
