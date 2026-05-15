//! Atomic JSON file read/write with file-lock and `chmod 0600` (Unix).
//!
//! Use this for any sensitive on-disk file the shell writes (credentials,
//! tokens, anything you don't want world-readable). Crashes mid-write
//! cannot truncate the destination because writes go to a sibling `*.tmp`
//! and are renamed into place.
//!
//! Locking uses `fs2::FileExt::try_lock_exclusive` (non-blocking). Failure
//! to acquire the lock returns an error rather than waiting, so callers
//! can decide whether to retry.

use fs2::FileExt;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("could not acquire file lock on {0}")]
    LockFailed(PathBuf),
    #[error("settings dir not found")]
    NoSettingsDir,
}

/// Read a JSON file. Returns `Ok(None)` if the file does not exist; an
/// error for any other failure.
pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, SettingsError> {
    if !path.exists() {
        return Ok(None);
    }
    let mut f = File::open(path)?;
    let mut buf = String::new();
    f.read_to_string(&mut buf)?;
    if buf.trim().is_empty() {
        return Ok(None);
    }
    let v = serde_json::from_str(&buf)?;
    Ok(Some(v))
}

/// Atomically write a JSON file. The parent directory must exist.
///
/// Pipeline:
/// 1. Open `<path>.tmp` with O_CREAT|O_RDWR.
/// 2. Acquire exclusive flock on it (non-blocking).
/// 3. Truncate, write pretty JSON, flush.
/// 4. `chmod 0600` on Unix.
/// 5. Rename `<path>.tmp` -> `<path>`.
/// 6. Release lock (implicit on file close).
pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), SettingsError> {
    let parent = path
        .parent()
        .ok_or(SettingsError::NoSettingsDir)?;
    if !parent.exists() {
        std::fs::create_dir_all(parent)?;
    }

    let tmp = path.with_extension(
        path.extension()
            .map(|e| format!("{}.tmp", e.to_string_lossy()))
            .unwrap_or_else(|| "tmp".to_string()),
    );

    let mut f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&tmp)?;

    f.try_lock_exclusive()
        .map_err(|_| SettingsError::LockFailed(tmp.clone()))?;

    let payload = serde_json::to_string_pretty(value)?;
    f.write_all(payload.as_bytes())?;
    f.flush()?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = f.metadata()?.permissions();
        perms.set_mode(0o600);
        f.set_permissions(perms)?;
    }

    // Drop closes the file and releases the lock before rename.
    drop(f);
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Read-modify-write helper that holds an exclusive lock on the target
/// file across the closure. Use for "merge into existing tree" patterns.
///
/// If the file does not exist, `modify` is called with `Value::Object(...)`
/// containing the default tree.
pub fn modify_json<F>(path: &Path, default: serde_json::Value, modify: F) -> Result<(), SettingsError>
where
    F: FnOnce(&mut serde_json::Value) -> Result<(), SettingsError>,
{
    let parent = path
        .parent()
        .ok_or(SettingsError::NoSettingsDir)?;
    if !parent.exists() {
        std::fs::create_dir_all(parent)?;
    }

    // Open or create a sibling lock file so we can hold an exclusive lock
    // across read+write without using the destination file itself (which
    // we're about to replace via rename).
    let lock_path = path.with_extension("lock");
    let lock = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)?;
    lock.try_lock_exclusive()
        .map_err(|_| SettingsError::LockFailed(lock_path.clone()))?;

    let mut current: serde_json::Value = if path.exists() {
        let mut f = File::open(path)?;
        let mut buf = String::new();
        f.read_to_string(&mut buf)?;
        if buf.trim().is_empty() {
            default
        } else {
            serde_json::from_str(&buf)?
        }
    } else {
        default
    };

    modify(&mut current)?;
    write_json_atomic(path, &current)?;
    // lock dropped here
    Ok(())
}
