//! Integration tests for `cleanup.rs` — the bounded shutdown cascade.
//!
//! Uses `tauri::test::mock_app` to obtain an `AppHandle` without booting a
//! real window. The mock runtime is enough because the cleanup cascade only
//! reads `Manager::try_state::<T>()`.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serial_test::serial;
use tauri::test::{mock_app, MockRuntime};
use tauri::{AppHandle, Manager};
use tauri_shell::cleanup::{
    cleanup_all_processes, NoopShutdownHook, ShutdownHook, HOOK_TIMEOUT, KILL_TIMEOUT,
    OUTER_TIMEOUT,
};
use tauri_shell::state::RunningProcesses;

type HookObj = Arc<dyn ShutdownHook<MockRuntime>>;

// -----------------------------------------------------------------------------
// Test hooks
// -----------------------------------------------------------------------------

struct SlowHook {
    sleep: Duration,
    ran: Arc<AtomicBool>,
    finished: Arc<AtomicBool>,
}

#[async_trait]
impl ShutdownHook<MockRuntime> for SlowHook {
    async fn shutdown(&self, _app: AppHandle<MockRuntime>) {
        self.ran.store(true, Ordering::SeqCst);
        tokio::time::sleep(self.sleep).await;
        self.finished.store(true, Ordering::SeqCst);
    }
}

struct CountingHook {
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl ShutdownHook<MockRuntime> for CountingHook {
    async fn shutdown(&self, _app: AppHandle<MockRuntime>) {
        self.calls.fetch_add(1, Ordering::SeqCst);
    }
}

// -----------------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------------

#[cfg(unix)]
fn spawn_sleeper() -> std::process::Child {
    std::process::Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("spawn sleep")
}

#[cfg(windows)]
fn spawn_sleeper() -> std::process::Child {
    std::process::Command::new("cmd")
        .args(["/C", "timeout", "/T", "60", "/NOBREAK"])
        .spawn()
        .expect("spawn timeout")
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[test]
#[serial]
fn timeouts_match_spec() {
    // Sanity check: the public constants haven't drifted from the spec.
    assert_eq!(OUTER_TIMEOUT, Duration::from_secs(10));
    assert_eq!(HOOK_TIMEOUT, Duration::from_secs(3));
    assert_eq!(KILL_TIMEOUT, Duration::from_secs(3));
}

#[tokio::test(flavor = "current_thread")]
#[serial]
async fn slow_hook_bounded_by_hook_timeout() {
    let app = mock_app();
    let handle = app.handle().clone();

    let ran = Arc::new(AtomicBool::new(false));
    let finished = Arc::new(AtomicBool::new(false));
    let hook: HookObj = Arc::new(SlowHook {
        sleep: Duration::from_secs(5),
        ran: Arc::clone(&ran),
        finished: Arc::clone(&finished),
    });
    handle.manage(hook);
    handle.manage(RunningProcesses::new());

    let start = Instant::now();
    cleanup_all_processes(handle.clone()).await;
    let elapsed = start.elapsed();

    assert!(ran.load(Ordering::SeqCst), "hook must have started");
    assert!(
        !finished.load(Ordering::SeqCst),
        "hook should not have finished within budget"
    );
    assert!(
        elapsed < OUTER_TIMEOUT,
        "cleanup should not block the outer 10s timeout (got {elapsed:?})"
    );
    assert!(
        elapsed >= HOOK_TIMEOUT,
        "cleanup should respect the 3s hook budget (got {elapsed:?})"
    );
    // 3s hook + 3s kill ceiling = 6s upper bound.
    assert!(
        elapsed < HOOK_TIMEOUT + KILL_TIMEOUT + Duration::from_secs(2),
        "cleanup should not exceed hook+kill+slack (got {elapsed:?})"
    );
}

#[tokio::test(flavor = "current_thread")]
#[serial]
async fn cleanup_drains_running_processes() {
    let app = mock_app();
    let handle = app.handle().clone();

    let hook: HookObj = Arc::new(NoopShutdownHook);
    handle.manage(hook);

    let procs = RunningProcesses::new();
    let child1 = spawn_sleeper();
    let pid1 = child1.id();
    procs.add("proc-1".into(), child1).expect("register child");
    let child2 = spawn_sleeper();
    let pid2 = child2.id();
    procs.add("proc-2".into(), child2).expect("register child");
    handle.manage(procs);

    cleanup_all_processes(handle.clone()).await;

    // RunningProcesses must be empty after cleanup.
    let procs = handle.state::<RunningProcesses>();
    assert!(procs.ids().is_empty(), "tracked processes must be drained");

    // Subprocesses must actually have died.
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        !tauri_shell::process_kill::is_alive(pid1),
        "proc-1 (pid {pid1}) should be dead"
    );
    assert!(
        !tauri_shell::process_kill::is_alive(pid2),
        "proc-2 (pid {pid2}) should be dead"
    );
}

#[tokio::test(flavor = "current_thread")]
#[serial]
async fn cleanup_with_no_hook_still_drains_processes() {
    let app = mock_app();
    let handle = app.handle().clone();
    // Deliberately do NOT call manage::<Arc<dyn ShutdownHook>>.

    let procs = RunningProcesses::new();
    let child = spawn_sleeper();
    let pid = child.id();
    procs.add("solo".into(), child).expect("register");
    handle.manage(procs);

    cleanup_all_processes(handle.clone()).await;

    let procs = handle.state::<RunningProcesses>();
    assert!(procs.ids().is_empty());
    std::thread::sleep(Duration::from_millis(200));
    assert!(!tauri_shell::process_kill::is_alive(pid));
}

#[tokio::test(flavor = "current_thread")]
#[serial]
async fn noop_hook_completes_immediately() {
    let app = mock_app();
    let handle = app.handle().clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let hook: HookObj = Arc::new(CountingHook {
        calls: Arc::clone(&calls),
    });
    handle.manage(hook);
    handle.manage(RunningProcesses::new());

    let start = Instant::now();
    cleanup_all_processes(handle.clone()).await;
    let elapsed = start.elapsed();

    assert_eq!(calls.load(Ordering::SeqCst), 1, "hook called exactly once");
    assert!(elapsed < HOOK_TIMEOUT, "fast-hook path should be quick");
}

#[tokio::test(flavor = "current_thread")]
#[serial]
async fn cleanup_is_idempotent() {
    let app = mock_app();
    let handle = app.handle().clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let hook: HookObj = Arc::new(CountingHook {
        calls: Arc::clone(&calls),
    });
    handle.manage(hook);
    handle.manage(RunningProcesses::new());

    cleanup_all_processes(handle.clone()).await;
    cleanup_all_processes(handle.clone()).await;
    assert_eq!(calls.load(Ordering::SeqCst), 2, "second call should re-run hook");
    let procs = handle.state::<RunningProcesses>();
    assert!(procs.ids().is_empty());
}
