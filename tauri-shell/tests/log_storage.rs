//! Integration tests for the in-memory ring buffer (`LogBuffer`) and the
//! map-of-buffers (`LogStorage`).
//!
//! These tests focus on the production-quality invariants:
//! - The ring evicts at *exactly* `capacity + 1` entries.
//! - `tail(None)` returns everything.
//! - `tail(Some(n))` returns the last `n` even when `n` exceeds the size.
//! - Per-process-id buffers are independent.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tauri_shell::state::{
    append_entry, clear_process_logs, get_logs, register_process, unregister_process, LogBuffer,
    LogEntry, LogStorage, DEFAULT_RING_CAPACITY,
};

fn fresh_storage() -> LogStorage {
    Arc::new(Mutex::new(HashMap::new()))
}

fn entry(pid: &str, n: usize) -> LogEntry {
    LogEntry {
        timestamp: n as i64,
        content: format!("line-{n}"),
        process_id: pid.to_string(),
    }
}

#[test]
fn push_evicts_at_exact_capacity_plus_one() {
    let cap = DEFAULT_RING_CAPACITY; // 10_000
    let mut buf = LogBuffer::new(cap);
    for i in 0..cap {
        buf.push(entry("p", i));
    }
    assert_eq!(buf.len(), cap, "should fill exactly to capacity");
    let first_before = buf.tail(Some(1))[0].clone();
    assert_eq!(first_before.content, format!("line-{}", cap - 1));

    // The (cap + 1)th entry pushes line-0 out.
    buf.push(entry("p", cap));
    assert_eq!(buf.len(), cap, "must not grow past capacity");
    let all = buf.tail(None);
    assert_eq!(all.first().unwrap().content, "line-1");
    assert_eq!(all.last().unwrap().content, format!("line-{cap}"));
}

#[test]
fn tail_none_returns_all_entries() {
    let mut buf = LogBuffer::new(50);
    for i in 0..7 {
        buf.push(entry("p", i));
    }
    let got = buf.tail(None);
    assert_eq!(got.len(), 7);
    for (i, e) in got.iter().enumerate() {
        assert_eq!(e.content, format!("line-{i}"));
    }
}

#[test]
fn tail_some_n_returns_last_n() {
    let mut buf = LogBuffer::new(50);
    for i in 0..20 {
        buf.push(entry("p", i));
    }
    let got = buf.tail(Some(5));
    assert_eq!(got.len(), 5);
    assert_eq!(got.first().unwrap().content, "line-15");
    assert_eq!(got.last().unwrap().content, "line-19");
}

#[test]
fn tail_some_larger_than_size_returns_all() {
    let mut buf = LogBuffer::new(50);
    for i in 0..3 {
        buf.push(entry("p", i));
    }
    let got = buf.tail(Some(1000));
    assert_eq!(got.len(), 3);
}

#[test]
fn isolation_between_process_ids() {
    let storage = fresh_storage();
    register_process(&storage, "alpha");
    register_process(&storage, "beta");
    for i in 0..10 {
        append_entry(&storage, entry("alpha", i));
    }
    for i in 0..5 {
        append_entry(&storage, entry("beta", i));
    }
    assert_eq!(get_logs(&storage, "alpha", None).len(), 10);
    assert_eq!(get_logs(&storage, "beta", None).len(), 5);

    // Clear beta — alpha unaffected.
    clear_process_logs(&storage, "beta");
    assert_eq!(get_logs(&storage, "alpha", None).len(), 10);
    assert_eq!(get_logs(&storage, "beta", None).len(), 0);

    // Unregister alpha — get_logs returns empty without panicking.
    unregister_process(&storage, "alpha");
    assert!(get_logs(&storage, "alpha", None).is_empty());
}

#[test]
fn empty_buffer_state_is_consistent() {
    let buf = LogBuffer::new(100);
    assert!(buf.is_empty());
    assert_eq!(buf.len(), 0);
    assert!(buf.tail(None).is_empty());
    assert!(buf.tail(Some(5)).is_empty());
}

#[test]
fn append_to_unregistered_id_creates_buffer_on_demand() {
    let storage = fresh_storage();
    // No register call — the helper should still create a buffer.
    append_entry(&storage, entry("autocreated", 0));
    let logs = get_logs(&storage, "autocreated", None);
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].content, "line-0");
}

#[test]
fn unregister_returns_false_for_missing_id() {
    let storage = fresh_storage();
    assert!(!unregister_process(&storage, "nonexistent"));
    register_process(&storage, "real");
    assert!(unregister_process(&storage, "real"));
    assert!(!unregister_process(&storage, "real"));
}
