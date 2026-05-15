# tauri-shell

A clean Tauri 2.x desktop shell with **fully-implemented infrastructure** and **open-ended connectors** for any TypeScript frontend + Python/TS backend.

## What this is

This crate is a desktop application shell. It does the OS-level work that's painful to do well in TypeScript:

- Window lifecycle (close-to-tray, single-instance, focus restore)
- System tray menu (cross-platform)
- Auto-launch at login (macOS/Windows/Linux)
- Auto-updater (Tauri plugin, GitHub releases + minisign by default)
- Bounded cleanup cascade on quit / Ctrl-C / system shutdown
- Subprocess management with a 10000-line in-memory ring log buffer per process
- Global `process-output` broadcast channel for log streaming to renderer windows
- Per-process log windows (deep-link via URL pattern)
- Atomic JSON file writes with file-lock + `chmod 0600` (settings/credentials helpers)
- Port-based and PID-based process kill on macOS/Linux/Windows
- All the boilerplate around `#[tauri::command]`, `tauri::generate_handler!`, plugin init, RunEvent loop

It does **not** include your domain logic. Install pipelines, environment management, backend service config, credential schemas, Jupyter integration, REST API spawning — all that is left as **typed stubs** with a clear `// TODO: wire to your backend` extension point. Bring your own Python/TS backend; this shell handles the desktop wrapper.

## Repository structure

```
tauri-shell/
├── Cargo.toml
├── tauri.conf.json
├── build.rs
├── capabilities/
│   ├── default.json        main window permissions
│   └── logs-window.json    log-window permissions
├── icons/                  (add your own — see icons/README.md)
└── src/
    ├── main.rs             entry: builder, plugins, setup, RunEvent
    ├── lib.rs              module re-exports for tests
    ├── state.rs            LogStorage, RunningProcesses, InstallationState managed state
    ├── process_monitor.rs  ring buffer + register/unregister/history/clear commands
    ├── process_spawn.rs    spawn helper with two reader threads + process-output emit
    ├── process_kill.rs     port-kill, pid-kill, brute-force kill helpers
    ├── cleanup.rs          cleanup_all_processes with bounded timeouts
    ├── settings.rs         atomic JSON read/write with flock + chmod 0600
    ├── windows.rs          close-to-tray, open_logs_window, open_url_in_window
    ├── tray.rs             tray menu builder + click handlers
    ├── updater.rs          check_and_apply_update wrapper
    ├── events.rs           event name constants + payload types
    ├── path_utils.rs       settings/install/userdata path resolution
    ├── autostart/
    │   ├── mod.rs          per-OS dispatch
    │   ├── macos.rs        osascript login items
    │   ├── windows.rs      .lnk via COM IShellLink
    │   └── linux.rs        ~/.config/autostart/*.desktop
    └── ipc/
        ├── mod.rs          aggregator for tauri::generate_handler!
        ├── infrastructure.rs   register_process_monitoring etc. (REAL)
        ├── installation.rs     install_to_directory, install_conda... (STUBS)
        ├── environments.rs     list_conda_environments... (STUBS)
        ├── backends.rs         list_backend_services... (STUBS)
        ├── jupyter.rs          start_jupyter_server... (STUBS)
        ├── credentials.rs      get_user_credentials... (STUBS)
        ├── helpers.rs          home dir, dir picker, dir-exists (mixed real/stub)
        ├── uninstall.rs        uninstall_application (STUB)
        └── certs.rs            generate_self_signed_cert (STUB)
```

## What's wired vs what's stubbed

| Concern | Status | Notes |
|---|---|---|
| Tauri boot + plugins | ✅ Real | All 8 plugins initialised in `main.rs` |
| Single-instance lock | ✅ Real | Re-focuses existing window on second launch |
| Tray menu + handlers | ✅ Real | 9 items wired; tray nav uses `webContents.send('navigate', ...)` not `window.eval` |
| Cleanup cascade | ✅ Real | 10s outer / 3s per-subsystem timeouts; hooks: Ctrl-C, ExitRequested, macOS `applicationWillTerminate` |
| Autostart (mac/win/linux) | ✅ Real | macOS osascript, Windows .lnk via COM, Linux `.desktop` |
| Updater | ✅ Real | GitHub releases endpoint + minisign signature verify (Tauri plugin) |
| Window mgmt | ✅ Real | Main window close-to-tray; helpers to open child windows |
| Process monitoring | ✅ Real | 10k-line ring buffer, broadcast `process-output`, register/get/clear |
| Subprocess spawn helper | ✅ Real | Two reader threads, line-by-line emit, optional ANSI strip |
| Process kill helpers | ✅ Real | Port-based (`lsof`/`fuser`/`netstat+taskkill`) + PID-based |
| Atomic settings writes | ✅ Real | `*.tmp` + rename + flock + chmod 0600 on Unix |
| Path utilities | ✅ Real | Home, settings dir, install dir from settings file |
| Domain logic | 🪝 Stubs | Every `#[tauri::command]` for install/env/backend/jupyter/credentials returns `Err("not implemented; see TODO")` and has a comment explaining the contract from the original spec |

## Connecting your backend

There are three paths, depending on where your domain code lives:

1. **Python backend over HTTP** (most common): your TS frontend calls Tauri commands; the Rust handler proxies to your localhost Python REST. See the example `proxy_http` helper in `src/ipc/installation.rs`.
2. **TS backend in-process** (Node sidecar): spawn your Node script via Tauri's sidecar feature; commands forward over a stdio channel.
3. **Pure Rust impl**: replace the `// TODO` body with your own logic. The function signatures, types, and events are stable.

Each stub has the original spec'd args/returns annotated as doc comments. See `docs/typescript-port/20-features/feature-*.md` in the parent repo for the full contract per command.

## Setup

```bash
# 1. Add your icons under tauri-shell/icons/ (32x32.png, 128x128.png, 128x128@2x.png, icon.icns, icon.ico)
# 2. Edit tauri.conf.json — set identifier, productName, updater endpoint, pubkey
# 3. Wire your frontend by editing build.frontendDist + build.devUrl
# 4. Build
cargo build --release
# 5. Or via Tauri CLI (recommended for the bundle/sign step)
cargo install tauri-cli --version "^2"
cargo tauri build
```

## IPC contract

The shell exposes the same 57-command + 8-event IPC surface documented in `docs/typescript-port/raw-deep-dives/ipc-bridge.md`, with the following deltas:

- **Drops** the 8 dead commands/events catalogued there (`installation-status`, `boolean-message`, `taurpc`, etc.)
- **Renames** silently-dropped args (e.g. `install_conda(directory, userDataDir)` becomes two real params)
- **Splits** the two `InstallationState` types into `InstallationSnapshot` (boot-time) and `InstallationProgress` (live)
- **Typed errors** — every handler returns `Result<T, IpcError>` (serialized as a tagged union, not bare strings)
- **Cancellation** — every long-running command accepts a `cancel_token: String` that maps to an `AbortHandle` registry

See `src/ipc/mod.rs` for the canonical command list and `src/events.rs` for the event constants.

## Status

This is a **scaffold**. Domain handlers are stubs. Infrastructure is production-quality but lightly tested. Wire your backend, fill in the stubs, ship.
