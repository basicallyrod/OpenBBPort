//! Integration tests for `settings.rs` — atomic write, flock, chmod, and
//! `modify_json` recovery semantics.
//!
//! All tests run against `tempfile::TempDir` so they are hermetic and leave
//! no residue on the developer's filesystem.

use std::fs::{self, OpenOptions};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use serde_json::json;
use tauri_shell::settings::{modify_json, read_json, write_json_atomic, SettingsError};
use tempfile::TempDir;

fn fixture() -> (TempDir, PathBuf) {
    let dir = TempDir::new().expect("create tempdir");
    let path = dir.path().join("user_settings.json");
    (dir, path)
}

#[test]
fn write_then_read_roundtrips() {
    let (_dir, path) = fixture();
    let payload = json!({ "theme": "dark", "n": 42 });
    write_json_atomic(&path, &payload).expect("write");
    let got: Option<serde_json::Value> = read_json(&path).expect("read");
    assert_eq!(got, Some(payload));
}

#[test]
fn read_returns_none_for_missing_file() {
    let (_dir, path) = fixture();
    let got: Option<serde_json::Value> = read_json(&path).expect("read");
    assert!(got.is_none());
}

#[test]
fn write_atomic_leaves_no_tmp_artefact() {
    let (_dir, path) = fixture();
    write_json_atomic(&path, &json!({ "x": 1 })).expect("write");
    assert!(path.exists(), "destination file must exist");
    // After a successful write, the sibling .tmp must be gone (renamed away).
    let tmp_candidates: Vec<_> = fs::read_dir(path.parent().unwrap())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.ends_with(".tmp")
        })
        .collect();
    assert!(
        tmp_candidates.is_empty(),
        "expected no *.tmp left behind, found: {tmp_candidates:?}"
    );
}

#[test]
fn write_survives_simulated_crash_mid_write() {
    // Simulate a crash by directly creating a partial *.tmp that *would* be
    // visible to a subsequent run, then verify a fresh atomic write still
    // succeeds and the destination is well-formed.
    let (_dir, path) = fixture();
    let tmp_path = path.with_extension("json.tmp");

    // Pre-write a corrupt tmp file as if the previous process died mid-write.
    {
        let mut f = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&tmp_path)
            .unwrap();
        f.write_all(b"{ \"truncated\": ").unwrap();
        // No flush + drop without rename: this is exactly the crash signature.
    }

    // The real write should overwrite the stale tmp and rename atomically.
    let payload = json!({ "after_crash": true });
    write_json_atomic(&path, &payload).expect("write after simulated crash");

    let got: Option<serde_json::Value> = read_json(&path).expect("read");
    assert_eq!(got, Some(payload));
    assert!(!tmp_path.exists(), "stale tmp must be cleaned up");
}

#[test]
fn flock_contention_returns_lock_failed() {
    // Open the tmp file ourselves and hold an exclusive flock; write_json_atomic
    // should fail with LockFailed rather than blocking.
    let (_dir, path) = fixture();
    let tmp_path = path.with_extension("json.tmp");

    let blocker = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(true)
        .open(&tmp_path)
        .unwrap();
    fs2::FileExt::try_lock_exclusive(&blocker).expect("hold flock");

    let result = write_json_atomic(&path, &json!({ "n": 1 }));
    match result {
        Err(SettingsError::LockFailed(p)) => assert_eq!(p, tmp_path),
        other => panic!("expected LockFailed, got {other:?}"),
    }

    // Release for cleanup.
    let _ = fs2::FileExt::unlock(&blocker);
}

#[cfg(unix)]
#[test]
fn chmod_0600_applied_on_unix() {
    let (_dir, path) = fixture();
    write_json_atomic(&path, &json!({ "secret": "xxx" })).expect("write");
    let meta = fs::metadata(&path).expect("stat");
    let mode = meta.permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "expected mode 0600, got {mode:o}");
}

#[test]
fn modify_json_recovers_corrupt_file_via_default() {
    let (_dir, path) = fixture();
    // Plant a corrupt JSON file directly.
    fs::write(&path, "{ not valid json").unwrap();

    // modify_json must surface the parse error rather than silently nuking.
    let result = modify_json(
        &path,
        json!({}),
        |_v| Ok(()),
    );
    assert!(matches!(result, Err(SettingsError::Json(_))));

    // Recovery path: overwrite the file with a fresh write_json_atomic.
    write_json_atomic(&path, &json!({ "recovered": true })).expect("overwrite");
    let got: Option<serde_json::Value> = read_json(&path).expect("read");
    assert_eq!(got, Some(json!({ "recovered": true })));
}

#[test]
fn modify_json_merges_into_existing_tree() {
    let (_dir, path) = fixture();
    write_json_atomic(&path, &json!({ "a": 1 })).expect("seed");

    modify_json(
        &path,
        json!({}),
        |v| {
            let obj = v.as_object_mut().expect("object");
            obj.insert("b".to_string(), json!(2));
            Ok(())
        },
    )
    .expect("modify");

    let got: Option<serde_json::Value> = read_json(&path).expect("read");
    assert_eq!(got, Some(json!({ "a": 1, "b": 2 })));
}

#[test]
fn modify_json_seeds_default_when_file_missing() {
    let (_dir, path) = fixture();
    assert!(!path.exists());

    modify_json(
        &path,
        json!({ "first": "default" }),
        |v| {
            let obj = v.as_object_mut().expect("object");
            obj.insert("added_in_closure".to_string(), json!(true));
            Ok(())
        },
    )
    .expect("modify");

    let got: Option<serde_json::Value> = read_json(&path).expect("read");
    assert_eq!(
        got,
        Some(json!({ "first": "default", "added_in_closure": true }))
    );
}
