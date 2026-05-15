//! Integration tests for `process_kill.rs`.
//!
//! These spawn real subprocesses, so the tests are slower and OS-specific.
//! Both supported platforms are exercised through separate `#[cfg]` paths.

use std::net::TcpListener;
use std::process::Command;
use std::time::Duration;

use tauri_shell::process_kill;

#[cfg(unix)]
fn spawn_sleeper(seconds: u64) -> std::process::Child {
    Command::new("sleep")
        .arg(seconds.to_string())
        .spawn()
        .expect("spawn sleep")
}

#[cfg(windows)]
fn spawn_sleeper(seconds: u64) -> std::process::Child {
    // `timeout /T <n> /NOBREAK` blocks for the given seconds.
    Command::new("cmd")
        .args(["/C", "timeout", "/T", &seconds.to_string(), "/NOBREAK"])
        .spawn()
        .expect("spawn timeout")
}

#[test]
fn kill_pid_kills_a_live_subprocess() {
    let mut child = spawn_sleeper(60);
    let pid = child.id();
    assert!(process_kill::is_alive(pid), "child should be alive after spawn");

    let killed = process_kill::kill_pid(pid, false);
    assert!(killed, "kill_pid should report success");

    // Give the OS a moment to reap.
    std::thread::sleep(Duration::from_millis(300));
    // is_alive may briefly report true as a zombie on Unix; reap then check.
    let _ = child.wait();
    assert!(!process_kill::is_alive(pid));
}

#[test]
fn kill_pid_graceful_path() {
    let mut child = spawn_sleeper(60);
    let pid = child.id();
    let killed = process_kill::kill_pid(pid, true);
    assert!(killed);
    let _ = child.wait();
    assert!(!process_kill::is_alive(pid));
}

#[test]
fn kill_pid_returns_false_for_unknown_pid() {
    // Pick a PID extremely unlikely to exist (u32::MAX is fine on both Unix
    // and Windows because PID space is far smaller).
    let bogus = u32::MAX;
    let killed = process_kill::kill_pid(bogus, false);
    assert!(!killed, "kill of nonexistent PID should not report success");
}

#[test]
fn is_alive_flips_after_child_exits() {
    let mut child = spawn_sleeper(0);
    let pid = child.id();
    let _ = child.wait();
    // Give the kernel a moment to drop the zombie.
    std::thread::sleep(Duration::from_millis(150));
    assert!(!process_kill::is_alive(pid));
}

#[test]
fn port_from_url_extracts_known_ports() {
    assert_eq!(
        process_kill::port_from_url("http://127.0.0.1:6900/health"),
        Some(6900)
    );
    assert_eq!(
        process_kill::port_from_url("http://localhost:8888/lab?token=abc"),
        Some(8888)
    );
    // Defaults from the scheme (HTTP = 80).
    assert_eq!(
        process_kill::port_from_url("http://example.com/api"),
        Some(80)
    );
    // Regex fallback for non-URL strings.
    assert_eq!(
        process_kill::port_from_url("the server is at :9000 right now"),
        Some(9000)
    );
}

#[cfg(unix)]
#[test]
fn kill_listeners_on_port_terminates_listener() {
    // Bind a TCP listener so we have *something* on a known port. We then
    // call `kill_listeners_on_port` and observe that the listener no longer
    // has a process holding it. Because `lsof` would target the *test* binary
    // itself, we instead spawn a tiny helper subprocess that binds the port
    // and sleeps.

    // Pick an OS-assigned free port, then close that listener and immediately
    // re-bind it in the helper. There's a TOCTOU window but it's small enough
    // not to flake in practice.
    let port = {
        let l = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
        l.local_addr().unwrap().port()
    };

    // Helper: a netcat-style listener using `python3 -c '...'` if available,
    // else `nc -l`. We fall back to sleeping if neither is found.
    let helper = if which::which("python3").is_ok() {
        Some(
            Command::new("python3")
                .arg("-c")
                .arg(format!(
                    "import socket, time; \
                     s = socket.socket(); s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1); \
                     s.bind(('127.0.0.1', {port})); s.listen(1); time.sleep(60)"
                ))
                .spawn()
                .expect("spawn python listener"),
        )
    } else {
        None
    };

    if helper.is_none() {
        // No suitable helper — skip the assertion path, but at least exercise
        // the no-op branch.
        let n = process_kill::kill_listeners_on_port(port, false);
        assert_eq!(n, 0);
        return;
    }

    let mut helper = helper.unwrap();

    // Give the listener a moment to bind.
    std::thread::sleep(Duration::from_millis(400));

    let killed = process_kill::kill_listeners_on_port(port, false);
    assert!(killed >= 1, "expected at least 1 listener killed, got {killed}");

    // Reap the helper.
    std::thread::sleep(Duration::from_millis(200));
    let _ = helper.wait();

    // The port should be free again — we can re-bind it.
    let rebind = TcpListener::bind(format!("127.0.0.1:{port}"));
    assert!(rebind.is_ok(), "port {port} not free after kill: {rebind:?}");
}

#[test]
fn port_from_url_returns_none_for_garbage() {
    assert_eq!(process_kill::port_from_url("not a url at all"), None);
}
