# Architecture: Tauri ↔ Frontend IPC Bridge

Port-team-facing digest of the IPC layer. The exhaustive catalog with file:line
citations lives in [`raw-deep-dives/ipc-bridge.md`](../raw-deep-dives/ipc-bridge.md)
and [`raw-deep-dives/ipc-bridge.v2.md`](../raw-deep-dives/ipc-bridge.v2.md);
this doc is the consolidated map.

## Two channels, three patterns

The app uses **vanilla Tauri 2.x IPC and nothing else** — no WebSocket, no HTTP,
no tRPC, no socket.io. `taurpc` is in `package.json` but is not imported anywhere
(vestigial, drop in the port).

| Channel | Frontend → Rust | Rust → Frontend |
|---|---|---|
| **Request/response** | `invoke<T>("command_name", args)` from `@tauri-apps/api/core` → Rust `#[tauri::command]` handler returning `Result<T, String>` | (response is the resolved Promise) |
| **Push events** | (no inbound events) | `app_handle.emit("event-name", payload)` (broadcast to ALL windows) or `window.emit(...)` (one window); frontend subscribes via `listen<T>("event-name", cb)` |

Three usage patterns are present in the codebase (raw v2 §M):

- **Pattern A — Await + listen.** Frontend attaches `listen('process-output')`, then
  `await invoke('start_something')` which blocks until the subprocess loop ends.
  Used for every streaming long-running call (env create, jupyter start, backend start).
- **Pattern B — Fire + poll status.** JS calls `invoke('install_conda')` once, then
  polls `invoke('get_installation_status')` every ~2 s while a parallel
  `install-progress` event stream fills in real-time progress.
- **Pattern C — Fire-and-forget.** No `await`, errors discarded via `.catch(() => {})`.
  Used for `register_process_monitoring`, `save_working_directory`, and several
  `update_backend_service` writes from event handlers.

## Command inventory: 57 commands across 8 modules

Counted directly from `generate_handler![...]` at `main.rs:493-549` (raw v2 §A
corrects v1's "53"). 51 of 57 are called by the frontend; 6 are registered but
unreachable from JS (raw v2 §F).

| Module | Count | One-line summary |
|---|---|---|
| `main.rs` | 7 | Window/process meta: `register_process_monitoring`, `unregister_process_monitoring`, `get_process_logs_history`, `clear_process_logs_history`, `get_installation_state`, `navigate_to_page` (tray-internal), `quit_application` |
| `tauri_handlers/startup.rs` | 6 | Installation pipeline: `install_to_directory`, `install_conda`, `abort_installation`, `setup_python_environment`, `get_installation_status`, `create_default_backend_services` |
| `tauri_handlers/environments.rs` | 13 | Conda env CRUD + extensions: `create_environment`, `create_environment_from_requirements`, `list_conda_environments`, `get_environment_extensions`, `install_extensions`, `update_extension`, `update_environment`, `remove_extension`, `remove_environment`, `execute_in_environment`, `select_requirements_file`, `update_installation_error`, `update_jupyter_status` |
| `tauri_handlers/jupyter.rs` | 7 | Jupyter lifecycle: `start_jupyter_server`, `stop_jupyter_server`, `stop_all_jupyter_servers` (Rust-internal), `check_jupyter_server`, `list_jupyter_servers`, `open_jupyter_logs_window` |
| `tauri_handlers/credentials.rs` | 3 | API-keys file ops: `get_user_credentials`, `update_user_credentials`, `open_credentials_file` |
| `tauri_handlers/backends.rs` | 7 | Long-lived HTTP servers: `start_backend_service`, `stop_backend_service`, `create_backend_service`, `update_backend_service`, `delete_backend_service`, `list_backend_services`, `open_backend_logs_window` |
| `tauri_handlers/helpers.rs` | 13 | Filesystem / dialogs / theme / cross-cutting: `check_file_exists`, `check_directory_exists`, `select_file`, `select_directory`, `get_home_directory`, `get_settings_directory`, `get_installation_directory`, `get_userdata_directory`, `save_working_directory`, `get_working_directory`, `toggle_theme`, `update_openbb_settings`, `open_url_in_window` |
| `utils/certs.rs` + `uninstall.rs` | 1 + 1 | TLS cert generation and full uninstall |

> ⚠️ BUG: `check_installer_file_exists` (`startup.rs:962`) and
> `list_conda_environments_impl` (`environments.rs:1616`) carry `#[tauri::command]`
> but are not (or cannot be) registered. Drop the attribute in the port.

## Event catalog: 8 events (6 live, 2 dead)

| Event | Payload | Emitters | Listeners | Notes |
|---|---|---|---|---|
| **`process-output`** | `{processId, output, timestamp?, type?}` | `jupyter.rs:154,193,482`; `backends.rs:355,462,500,555,1003`; `environments.rs:60,83` | `JupyterLogsPage.tsx:246`; `BackendLogsPage.tsx:175`; `environments.tsx:599,1522,2047`; `backends.tsx:665` | **Load-bearing multiplexed channel.** Every line of stdout/stderr from every subprocess (jupyter, backends, conda) broadcasts on this one event; clients filter by `processId`. 3+ listeners per emit (logs window + main window + per-row monitor). |
| `install-progress` | `{step, progress, message}` | `startup.rs:477,517,1264` | `installation-progress.tsx:822` | Miniforge installer + env setup. **Separate channel from `process-output`** — installer does NOT use the log infra. |
| `installation-directory` | `string` | `startup.rs:1307` | `index.tsx:27` | Fires once on successful install. |
| `backend-url-discovered` | `{id, url}` | `backends.rs:1103` | `backends.tsx:2219` | URL parser found a serving URL in subprocess output. |
| `jupyter-status-update` | `{environmentName, status}` | `jupyter.rs:657` | (no `listen`; consumed via `localStorage` in practice) | Cross-window status; postMessage path is dead (see §Multi-window). |
| `uninstall_progress` | `string` | `uninstall.rs:26,450,470,535` | `uninstall.tsx:43` | Free-form status during uninstall. |
| `installation-status` | `boolean` | **NEVER EMITTED** | `index.tsx:15` | Dead listener. The `setTimeout` fallback at `index.tsx:34` fires `invoke('get_installation_state')` instead. |
| `boolean-message` | `{message: "true"}` | `backends.rs:1202` | (none) | Dead emit. |

`process-output` payload shape **varies across emitters** — jupyter sends
`{processId, output, timestamp}`, backends adds `type: "stdout"|"stderr"|"system"`,
environments sends only `{processId, output}`. Every consumer destructures
`{processId, output, timestamp}` and ignores `type`, so the discriminator is dead
bytes on the wire. Normalize the schema in the port.

## Wire conventions

- **`snake_case` ↔ `camelCase` auto-conversion.** Tauri's `#[tauri::command]` macro
  generates `#[serde(rename_all = "camelCase")]` on the args struct, so JS
  `{processId, filePath}` maps to Rust `process_id, file_path` automatically. The
  codebase uses both conventions inconsistently (raw v1 §4.7).
- **Errors are `Result<T, String>` everywhere.** Every fallible command stringifies
  with `format!("...: {e}")`; the JS side gets a string in `.catch(err => ...)`.
  There is **no typed-error system, no error codes, no discriminated unions**. A
  TS port that adopts zod or tRPC has a free upgrade here.
- **Unknown JS args are silently dropped.** `#[serde(deny_unknown_fields)]` is not
  applied. `install_conda` accepts a `userDataDir` arg the Rust signature ignores;
  `install_extensions` and `remove_environment` ignore `directory`. (raw v2 §H)
  Strict Zod validation in the port would surface these.

## State surfaces accessible from commands

Two mechanisms, used for different reasons:

### `.manage(T)` Tauri-managed state (4 registrations)

Passed into commands as `State<T>` parameter. Builder-phase except the tray
(raw v2 §C):

| Type | Where | Used by |
|---|---|---|
| `ProcessLogState(LogStorage)` = `Arc<Mutex<HashMap<String, LogBuffer>>>` | `main.rs:489` (builder) | `register_process_monitoring`, `get_process_logs_history` |
| `RunningProcesses(Arc<Mutex<HashMap<String, Child>>>)` | `main.rs:490` | `start_backend_service`, `stop_backend_service`, cleanup cascade |
| `InstallationState` (boot snapshot) | `main.rs:491` | `get_installation_state` |
| `tray: TrayIcon` | `main.rs:742` **(inside setup hook)** | Tray menu event handlers |

### `Lazy<Mutex<T>>` globals (4 of them)

Used where Tauri state extraction inside spawned threads is awkward:

| Global | File:line | Purpose |
|---|---|---|
| `INSTALLATION_STATE` | `startup.rs:15` — `Mutex<InstallationState>` (different struct from the managed one) | Install-phase flags for `get_installation_status` polling |
| `INSTALLATION_IN_PROGRESS` | `startup.rs:434` — `Mutex<bool>` | Re-entrancy guard for `install_conda` |
| `ACTIVE_JUPYTER_SERVERS` | `jupyter.rs:9` — `Mutex<HashMap<env, (url, pid)>>` | Jupyter lifecycle bookkeeping |
| `LOG_STORAGE` | `process_monitor.rs:9` — `Lazy<Arc<Mutex<HashMap<String, LogBuffer>>>>` | Same `Arc` as `ProcessLogState` (single instance) |

> ⚠️ BUG: there are **two distinct `InstallationState` structs** — one in
> `main.rs:67` (boot snapshot), one in `startup.rs:18` (install phase tracker) —
> and two near-identically-named commands (`get_installation_state` vs
> `get_installation_status`). Rename in the port (raw v2 §I).

In a TS port, single-threaded JS removes the `Mutex`/`Arc` plumbing entirely:
module-scoped variables work for both shapes.

## The cancellation gap

There is **no proper cancellation primitive** — no `AbortController`, no cancel
tokens, no graceful-shutdown bubbling. Three ad-hoc mechanisms cover the cases:

1. **Dedicated kill commands** — `stop_jupyter_server`, `stop_backend_service`
   are first-class IPC calls that kill the tracked PID and clear the relevant
   state map. Used during normal user-initiated stop.
2. **Brute-force pattern killers** — `abort_installation` (`startup.rs:1097`)
   uses `pkill -f <pattern>` (Unix) / `taskkill /F /FI "WINDOWTITLE eq ..."`
   (Windows) to nuke install processes by directory match. Last resort.
3. **Frontend `isCancelling` ref flags** — UI-only flag (e.g.
   `installation-progress.tsx:828`) checked inside `listen` handlers to discard
   late-arriving events after the user clicked Cancel. Does not stop the
   underlying work — just hides its output.

A TS port can adopt `AbortController` end-to-end: pass an `AbortSignal` into
`fetch`/`spawn`/etc., and the cancellation becomes uniform. This is a major
clean-up opportunity.

## Multi-window patterns

Four window types beyond the main window (`raw-deep-dives/ipc-bridge.md` §6):

| Window | Label format | URL | Close behavior |
|---|---|---|---|
| Main | `main` | `/` (TanStack Router) | Hide on close (kept for tray restore) |
| Jupyter logs | `jupyter-logs-<env>` | `/jupyter-logs?env=<env>` | Hide on close (re-show on next open) |
| Backend logs | `backend-logs-<uuid>` | `/backend-logs?id=<uuid>` | Hide on close |
| External URL popup | `url_<timestamp>` | `WebviewUrl::External(url)` | Destroy on close (one-shot) |

Logs window reuse: `app_handle.get_webview_window(&label)` lookup at
`jupyter.rs:585` / `backends.rs:1539` — existing windows are `show()` + `set_focus()`
rather than rebuilt. The label is the only identity. Port equivalent in Electron:
a `Map<string, BrowserWindow>` keyed by the same label strings.

**Cross-window communication.** The codebase has three nominal channels for
cross-window status:

1. **Tauri broadcast events** — `app_handle.emit(...)` reaches every window. **Works.**
2. **`window.postMessage` to `window.opener`** — `JupyterLogsPage.tsx:217-224` writes
   to `window.opener` if non-null.
3. **`localStorage` + `storage` events** — logs window writes
   `jupyter-shutdown-<env>` key; main window's `storage` listener consumes it
   with a 60-s freshness check (`environments.tsx:2107-2153`). **Works.**

> ⚠️ BUG: `window.opener` is **always null** in Tauri webviews. Windows built by
> `WebviewWindowBuilder` (`jupyter.rs:596-606`) do not establish a JS opener
> relationship — they are independent webviews launched by Rust. The postMessage
> branch is dead code (raw logs-streaming.v2 §J). Only Tauri events and
> localStorage actually fire. In the port, drop the postMessage branch and use
> a single typed event bus.

## Known security issues

> ⚠️ BUG: `open_url_in_window` (`helpers.rs:957`) opens arbitrary external URLs
> inside a `WebviewWindow` that **inherits the main window's full IPC capability
> set** — `fs:read-all`, `fs:write-all`, `shell:allow-execute`, plus every
> custom `#[tauri::command]` (raw v2 §J). No URL allow-list, no sandbox.

> ⚠️ BUG: **CSP is `null`** in `tauri.conf.json:38`. The popped external-URL
> window can load arbitrary scripts and call `invoke()` for any of the 57
> registered commands. Combined with the wildcard capability scope
> (`"windows": ["*"]`), this is a credential-exfiltration vector.

> ⚠️ BUG: **5 `window.eval()` injection points** in `main.rs` (lines 408, 676,
> 784, 785, 799) — tray nav, install gate, post-install nav. All currently use
> Rust-controlled static strings, but the pattern is a code-execution surface
> if any parameter ever becomes user-influenced. In Electron, replace with
> `webContents.send('navigate', '/path')` + router-side handler.

> ⚠️ BUG: Tauri **capabilities apply to `windows: ["*"]`** (raw v2 §K-L) —
> every webview, including popped external URLs, gets the same IPC permissions
> as the main window. There is no per-window or per-command ACL.

> ⚠️ BUG: **Custom `#[tauri::command]` handlers are not subject to
> capability gating.** Anything in `generate_handler!` is callable by any
> window with `core:default`. A TS port should adopt Electron's explicit
> per-channel whitelist model — never auto-expose all `ipcMain.handle`
> channels to all renderers.

> ⚠️ BUG: **Log content is rendered with `dangerouslySetInnerHTML` and not
> HTML-escaped** (`JupyterLogsPage.tsx:345`, `BackendLogsPage.tsx:253`). A
> subprocess printing `<img src=x onerror=...>` runs in the logs window's
> DOM context. Low severity (requires malicious local subprocess) but
> trivially fixable via JSX text rendering.

## TS port options, ranked

| # | Option | Risk | Effort | Recommendation |
|---|---|---|---|---|
| 1 | **Tauri 2 + TypeScript frontend rewrite** | Lowest | Smallest scope | Keep all 57 commands and 6 live events as-is; rewrite only `src/` in TS. The `invoke()`/`listen()` API is framework-agnostic and stable. Best if goal is "modernise the React UI" rather than "remove Rust". |
| 2 | **Electron + preload bridge** | Medium | 6-month tax | `ipcRenderer.invoke` + `ipcMain.handle` maps 1:1 to current `invoke`/`#[tauri::command]`. `webContents.send` + `ipcRenderer.on` maps 1:1 to `app_handle.emit` + `listen`. Per-window `webContents` makes broadcast-to-all-windows trivial. **Requires reimplementing subprocess management, port-kill, autostart, updater, tray, native dialogs.** |
| 3 | **Wails 2 (Go backend, TS frontend)** | Medium | Medium | Go's `os/exec` and `syscall` stdlib mirror the existing Rust subprocess patterns closely. Small static binary, mature multi-platform packaging. Needs Go expertise. |
| 4 | **Pure web app + Node agent** | High | Highest | Static SPA + a local Node HTTP/WS agent owns the OS work. Cleanest separation but loses native window chrome, tray, and autostart without a separate launcher process. |

A TS port that adopts Option 2 should plan for:

- A **`camelCase ↔ snake_case` middleware** wrapper around `ipcMain.handle` (mirrors
  Tauri's auto-conversion so existing JS callers using either convention work).
- A **`process-output` multiplexed broadcast helper** that iterates
  `BrowserWindow.getAllWindows()` and calls `webContents.send('process-output', ...)`
  — mirroring `app_handle.emit`'s all-windows fan-out.
- A **window-label `Map<string, BrowserWindow>`** keyed by the existing
  `jupyter-logs-{env}` / `backend-logs-{id}` strings to preserve identity.
- Adoption of `AbortController` as the cancellation primitive instead of the
  three ad-hoc mechanisms.
- Dropping the 8 vestigial frontend deps (`taurpc`, `@tauri-apps/plugin-app`,
  `-http`, `-process`, `-window`, `-shell`, `-log`, `-updater` — none are
  imported, raw v2 §O).
