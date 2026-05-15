//! Cross-platform process kill helpers.
//!
//! Three strategies, ordered from least to most invasive:
//!
//! - [`kill_pid`] — direct kill of a known PID.
//! - [`kill_listeners_on_port`] — find and kill whichever process is
//!   listening on a TCP port. Required because shell-wrapper-spawned
//!   subprocesses often have a server child that the parent PID doesn't
//!   identify.
//! - [`kill_by_pattern`] — `pkill -f <pattern>` / `taskkill /F /FI`.
//!   Brute-force; use only for install-abort scenarios.
//!
//! All functions are best-effort. They log on failure rather than
//! propagating, so caller can chain strategies as fallbacks.

use std::process::Command;

#[cfg(unix)]
pub fn kill_pid(pid: u32, graceful: bool) -> bool {
    if graceful {
        let _ = Command::new("kill").arg("-15").arg(pid.to_string()).status();
        std::thread::sleep(std::time::Duration::from_secs(2));
        // check liveness
        if Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .status()
            .map(|s| !s.success())
            .unwrap_or(true)
        {
            return true;
        }
    }
    Command::new("kill")
        .arg("-9")
        .arg(pid.to_string())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(windows)]
pub fn kill_pid(pid: u32, _graceful: bool) -> bool {
    // Windows has no SIGTERM equivalent for arbitrary processes; the
    // "graceful" flag is ignored. `taskkill /F` is the only reliable path.
    Command::new("taskkill")
        .arg("/F")
        .arg("/PID")
        .arg(pid.to_string())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Find every PID currently listening on `port` and kill it.
///
/// On macOS/Linux this uses `lsof -ti tcp:<port> -sTCP:LISTEN` (which is
/// stricter than the original Tauri reference's `lsof -ti tcp:<port>`
/// because we want to ignore established outbound connections that happen
/// to share the port number).
///
/// Returns the number of PIDs killed.
#[cfg(unix)]
pub fn kill_listeners_on_port(port: u16, graceful: bool) -> usize {
    let out = Command::new("lsof")
        .args(["-ti", &format!("tcp:{port}"), "-sTCP:LISTEN"])
        .output();
    let pids = match out {
        Ok(o) if o.status.success() => parse_pids(&String::from_utf8_lossy(&o.stdout)),
        _ => Vec::new(),
    };

    if pids.is_empty() {
        // Linux fallback: fuser
        let _ = Command::new("fuser")
            .arg("-k")
            .arg(format!("{port}/tcp"))
            .status();
        return 0;
    }

    let mut killed = 0;
    for pid in pids {
        if kill_pid(pid, graceful) {
            killed += 1;
        }
    }
    killed
}

#[cfg(windows)]
pub fn kill_listeners_on_port(port: u16, _graceful: bool) -> usize {
    let out = Command::new("netstat").args(["-ano"]).output();
    let pids = match out {
        Ok(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            stdout
                .lines()
                .filter(|line| line.contains(&format!(":{port}")) && line.contains("LISTENING"))
                .filter_map(|line| {
                    line.split_whitespace()
                        .last()
                        .and_then(|s| s.parse::<u32>().ok())
                })
                .collect::<Vec<_>>()
        }
        _ => Vec::new(),
    };

    let mut killed = 0;
    for pid in pids {
        if kill_pid(pid, false) {
            killed += 1;
        }
    }
    killed
}

#[cfg(unix)]
fn parse_pids(s: &str) -> Vec<u32> {
    s.lines()
        .filter_map(|line| line.trim().parse::<u32>().ok())
        .collect()
}

/// Brute-force kill any process whose argv matches `pattern`.
///
/// On Unix: `pkill -f <pattern>`. On Windows: `taskkill /F /FI "WINDOWTITLE eq <pattern>*"`.
///
/// Returns true if the command exited successfully (no guarantee that
/// any process was actually killed).
#[cfg(unix)]
pub fn kill_by_pattern(pattern: &str) -> bool {
    Command::new("pkill")
        .arg("-f")
        .arg(pattern)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(windows)]
pub fn kill_by_pattern(pattern: &str) -> bool {
    Command::new("taskkill")
        .arg("/F")
        .arg("/FI")
        .arg(format!("WINDOWTITLE eq *{pattern}*"))
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Liveness probe. `unix` uses `kill -0`; Windows uses `tasklist /FI`.
#[cfg(unix)]
pub fn is_alive(pid: u32) -> bool {
    Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(windows)]
pub fn is_alive(pid: u32) -> bool {
    let out = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout);
            s.lines().any(|l| l.contains(&pid.to_string()))
        }
        _ => false,
    }
}

/// Extract a TCP port from a URL like `http://127.0.0.1:8888/lab?token=...`.
pub fn port_from_url(url: &str) -> Option<u16> {
    if let Ok(u) = url::Url::parse(url) {
        if let Some(p) = u.port_or_known_default() {
            return Some(p);
        }
    }
    // Fallback: regex out the first `:NNNN` from anywhere in the string.
    let re = regex::Regex::new(r":(\d{2,5})\b").ok()?;
    re.captures(url).and_then(|c| c.get(1)).and_then(|m| m.as_str().parse().ok())
}
