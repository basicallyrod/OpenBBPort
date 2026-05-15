# Deep-Dive: Tauri ↔ Frontend IPC Bridge (Cross-Cutting Reference)

> Raw findings from Wave 1 agent. Source of truth for `10-architecture/ipc-bridge.md`
> and `30-port/port-invoke-mapping.md`. This is the architectural ground truth — every
> other feature doc references this catalog.
> Generated 2026-05-15.

This is the architectural ground truth for the Tauri 2.x desktop app. All file:line citations are absolute paths.

## 0. Top-Level Architecture Summary

The communication layer is **vanilla Tauri 2.x IPC only**. There are exactly two channels:

1. **Request/response** — `invoke<T>(commandName, args)` from `@tauri-apps/api/core` calls Rust `#[tauri::command]` handlers registered in `tauri::generate_handler![...]` at `/home/user/OpenBBPort/desktop/src-tauri/src/main.rs:492-550`.
2. **Push events** — Rust calls `app_handle.emit("event-name", payload)` (or `window.emit(...)`) using `tauri::Emitter`; frontend subscribes via `listen<T>("event-name", cb)` from `@tauri-apps/api/event`.

There is **no WebSocket, no HTTP server, no tRPC, no socket.io**. `taurpc` is in `package.json` (`/home/user/OpenBBPort/desktop/package.json:43`) but is not imported anywhere in `src/` or used in `src-tauri/src/`. It's a vestigial dependency.

Plugins used (all `@tauri-apps/plugin-*`): `dialog`, `fs`, `opener`, `app`, plus the global Tauri runtime (`@tauri-apps/api/{core,event,app}`). Many other plugins are listed in `package.json` and `Cargo.toml` (shell, http, log, process, updater, window, single-instance, persisted-scope, fix-path-env) but most are loaded server-side and not directly called from JS.

---

## 1. Full Invoke Command Catalog

The `.invoke_handler(tauri::generate_handler![...])` block at `/home/user/OpenBBPort/desktop/src-tauri/src/main.rs:492-550` exposes 53 commands. They are organized by source module below. Tauri auto-converts Rust `snake_case` parameter names to JS `camelCase` on the wire (e.g. Rust `process_id` becomes JS `processId`). The convention `Result<T, String>` is universal — every error becomes a string in the rejected promise.

### 1a. Module: `main.rs` (window/process meta — 6 commands)

| Command | File:Line | Params | Returns | Frontend caller(s) | Description |
|---|---|---|---|---|---|
| `register_process_monitoring` | main.rs:75-78 | `process_id: String` (state: `ProcessLogState`) | `bool` | environments.tsx:633,1919,2035; backends.tsx:2411; BackendLogsPage.tsx:158; JupyterLogsPage.tsx:166 | Pre-registers a `LogBuffer` (10000-line ring) for a process id so emitted `process-output` events get persisted. |
| `unregister_process_monitoring` | main.rs:80-83 | `process_id: String` | `bool` | (unused in src/, defined for completeness) | Drops the LogBuffer for a process id. |
| `get_process_logs_history` | main.rs:85-93 | `process_id: String, count: Option<usize>` | `Vec<LogEntry>` | BackendLogsPage.tsx:162; JupyterLogsPage.tsx:176 | Replays the persisted ring buffer when a logs window opens. |
| `clear_process_logs_history` | main.rs:95-98 | `process_id: String` | `bool` | BackendLogsPage.tsx:208; JupyterLogsPage.tsx:300 | Empties the buffer (user clicked "Clear logs"). |
| `get_installation_state` | main.rs:388-391 | (state: `InstallationState`) | `InstallationState { is_installed, installation_directory }` | index.tsx:37; environments.tsx:704 | Cached at startup by `check_installation_on_startup()` (main.rs:272-386). Cheap synchronous read. |
| `navigate_to_page` | main.rs:393-410 | `page: &str` (app_handle) | `()` | (Rust-internal, called by tray menu) | Calls `window.eval("window.location.href = ...")` — server-side navigation. |
| `quit_application` | main.rs:412-417 | (app_handle) | `()` | setup.tsx:302 | Runs `cleanup_all_processes` then `app_handle.exit(0)`. |

### 1b. Module: `tauri_handlers/startup.rs` (8 commands)

| Command | File:Line | Params | Returns | Frontend caller(s) | Description |
|---|---|---|---|---|---|
| `get_installation_status` | startup.rs:34-54 | — | `serde_json::Value` (`{phase, isDownloading, isInstalling, isConfiguring, isComplete, message}`) | installation-progress.tsx:1059 | Polls global `INSTALLATION_STATE` mutex (Lazy at startup.rs:15-16). |
| `install_to_directory` | startup.rs:418-431 | `directory: String, user_data_directory: String` | `bool` | setup.tsx:121 | Creates dirs, writes `system_settings.json` + `user_settings.json`. |
| `install_conda` | startup.rs:436-905 | `directory: String, window: Window` | `bool` | installation-progress.tsx:938 | Long-running. Downloads & runs Miniforge installer; emits `install-progress` events with InstallProgress payload throughout. Guarded by static `INSTALLATION_IN_PROGRESS` mutex (startup.rs:434). |
| `abort_installation` | startup.rs:1097-1101 | `directory: String` | `()` | installation-progress.tsx:1269 | Resets `INSTALLATION_STATE`, kills install processes via `pkill`/`taskkill`, removes temp files. |
| `setup_python_environment` | startup.rs:1227-1242 | `directory: String, python_version: String, window: Window` | `bool` | installation-progress.tsx:1118 | Long-running. Generates `openbb.yaml`, runs `conda env create`. Emits `install-progress` and `installation-directory`. |
| `check_installer_file_exists` | startup.rs:962-966 | — | `bool` | (registered? **Not in `generate_handler!` block** — defined but unreachable from frontend) | Defined but never registered in main.rs. |
| `create_default_backend_services` | startup.rs:1431-1435 | — | `()` | installation-progress.tsx:1218 | Inserts "OpenBB API" + "OpenBB MCP" into `backends.json` with hardcoded commands. |

### 1c. Module: `tauri_handlers/environments.rs` (12 commands)

| Command | File:Line | Params | Returns | Frontend caller(s) | Description |
|---|---|---|---|---|---|
| `create_environment` | environments.rs:418-437 | `name, python_version, extensions: Vec<String>, process_id, app_handle` | `bool` | environments.tsx:1557 | Long-running conda create. Streams stdout/stderr as `process-output` events tagged with `processId`. |
| `create_environment_from_requirements` | environments.rs:1345-1364 | `name, file_path, directory, process_id, app_handle` | `bool` | environments.tsx:635 | Same pattern; reads requirements.txt / pyproject.toml. |
| `select_requirements_file` | environments.rs:1611-1614 | — | `String` | environments.tsx:562 | Opens AppleScript/etc native picker (not plugin-dialog). |
| `list_conda_environments` | environments.rs:1798-1803 | `directory: Option<String>` | `Vec<CondaEnvironment>` | backends.tsx:2377; environments.tsx:416,494,545,2379 | Reads conda envs dir. |
| `get_environment_extensions` | environments.rs:2046-2049 | `name: String` | `serde_json::Value` (`{extensions: Extension[]}`) | environments.tsx:1048,1137,1397,1480,1595,2636 | Runs `pip list --format=json` in the env. |
| `install_extensions` | environments.rs:2681-2687 | `environment, extensions: Vec<String>` | `bool` | environments.tsx:1152,1375,1574 | `pip install ...`. Long-running but **does not** stream output (no app_handle param). |
| `update_extension` | environments.rs:2328-2336 | `package, environment, directory` | `bool` | environments.tsx:1471 | `pip install -U`. |
| `update_environment` | environments.rs:3039-3042 | `environment, directory` | `bool` | environments.tsx:763 | `conda update --all`. |
| `update_installation_error` | environments.rs:2758-2773 | `error: String` | `()` | (defined but no frontend caller found) | Manually pushes an error message into `INSTALLATION_STATE`. |
| `remove_extension` | environments.rs:2236-2249 | `package, environment, directory` | `bool` | environments.tsx:1445 | `pip uninstall`. |
| `remove_environment` | environments.rs:2753-2755 | `name: String` | `bool` | environments.tsx:672,1316,1679 | `conda env remove`. |
| `execute_in_environment` | environments.rs:3234-3253 | `command, environment, directory` | `serde_json::Value` (`{stdout, stderr, success}`) | installation-progress.tsx:1157; environments.tsx (12 call sites) | Generic shell-out into a conda env. **Uses `command_sanitizer.rs` validation.** |

### 1d. Module: `tauri_handlers/jupyter.rs` (7 commands)

| Command | File:Line | Params | Returns | Frontend caller(s) | Description |
|---|---|---|---|---|---|
| `start_jupyter_server` | jupyter.rs:253-261 | `app_handle, environment, directory, working` | `serde_json::Value` (`{url, already_running, status, process_id}`) | environments.tsx:1921 | Spawns `conda run jupyter lab`, waits up to 30s for URL via mpsc channel. Streams stdout/stderr as `process-output`. |
| `stop_jupyter_server` | jupyter.rs:487-493 | `app_handle, environment` | `bool` | environments.tsx:1950,2064 | Kills the process; emits final completion message via `process-output`. |
| `stop_all_jupyter_servers` | jupyter.rs:263-485 | `app_handle` | `bool` | (Rust-internal: main.rs:428) | Iterates `ACTIVE_JUPYTER_SERVERS` (jupyter.rs:9-10) and stops each. |
| `check_jupyter_server` | jupyter.rs:533-555 | `environment` | `serde_json::Value` (`{running, url, process_id}`) | environments.tsx:1809,1953 | Read-only state lookup. |
| `list_jupyter_servers` | jupyter.rs:558-580 | — | `serde_json::Value` (map of env→{url,pid}) | (no frontend caller) | Defined but unused. |
| `open_jupyter_logs_window` | jupyter.rs:582-642 | `app_handle, environment` | `()` | environments.tsx:1984 | Creates a new WebviewWindow labeled `jupyter-logs-{environment}` pointing at `/jupyter-logs?env=<env>`. |
| `update_jupyter_status` | jupyter.rs:644-661 | `app_handle, environment_name, status` | `()` | (no direct frontend caller — invoked by JupyterLogsPage child window via postMessage instead) | Re-emits as `jupyter-status-update` event. |

### 1e. Module: `tauri_handlers/credentials.rs` (3 commands)

| Command | File:Line | Params | Returns | Frontend caller(s) | Description |
|---|---|---|---|---|---|
| `get_user_credentials` | credentials.rs:36-39 | — | `serde_json::Value` | __root.tsx:132; api-keys.tsx:182 | Reads `~/.openbb_platform/user_settings.json`. |
| `update_user_credentials` | credentials.rs:88-91 | `credentials: serde_json::Value` | `bool` | api-keys.tsx:364 | Merges `credentials` key into the same JSON file. |
| `open_credentials_file` | credentials.rs:183-186 | `file_name: Option<String>` | `bool` | api-keys.tsx:376,386,398,408,418 | Opens `user_settings.json`/`.env`/`.condarc`/`mcp_settings.json` in the OS default text editor (notepad/TextEdit/gedit). |

### 1f. Module: `tauri_handlers/backends.rs` (8 commands)

| Command | File:Line | Params | Returns | Frontend caller(s) | Description |
|---|---|---|---|---|---|
| `start_backend_service` | backends.rs:639-657 | `app_handle, id: String` | `BackendService` | backends.tsx:2507 | Long-running. Writes a shell script wrapper, spawns it, registers PID with `RunningProcesses` state, scrapes URL from output, emits `backend-url-discovered` and `boolean-message`. |
| `stop_backend_service` | backends.rs:561-573 | `app_handle, id: String` | `()` | backends.tsx (multiple sites e.g. 674,702,725,753) | Kills tracked process, emits `process-output` system messages. |
| `update_backend_service` | backends.rs:1345-1348 | `backend: BackendService` | `BackendService` | backends.tsx (12+ sites) | Persists changes to `~/.openbb_platform/backends.json` with file-lock. |
| `create_backend_service` | backends.rs:1272-1275 | `backend: BackendService` | `BackendService` | (uses `update_backend_service` from frontend per `action` var at backends.tsx:2624) | Adds a new entry. |
| `delete_backend_service` | backends.rs:1380-1390 | `app_handle, id: String` | `()` | backends.tsx:2431 | Stops then removes from JSON. |
| `list_backend_services` | backends.rs:1224-1227 | — | `Vec<BackendService>` | backends.tsx:2318,2363 | Reads backends.json. |
| `open_backend_logs_window` | backends.rs:1536-1612 | `app_handle, id: String` | `()` | backends.tsx:2414 | Creates new WebviewWindow labeled `backend-logs-{id}` pointing at `/backend-logs?id=<id>`. |
| `initialize_backends` | (called only from main.rs setup hook, line 575) | — | — | — | Not registered as a `#[tauri::command]`; runs at app boot to auto-start any backends with `auto_start: true`. |

### 1g. Module: `tauri_handlers/helpers.rs` (15 commands)

| Command | File:Line | Params | Returns | Frontend caller(s) | Description |
|---|---|---|---|---|---|
| `check_file_exists` | helpers.rs:171-175 | `path: String` | `bool` | backends.tsx:1595,1620 | Sync `Path::is_file()`. |
| `toggle_theme` | helpers.rs:288-291 | `theme: String` | `bool` | __root.tsx:194 | Persists chosen theme into preferences. |
| `save_working_directory` | helpers.rs:350-353 | `path: &str` | `bool` | environments.tsx:396 | Persists user's chosen workspace dir. |
| `get_working_directory` | helpers.rs:398-401 | `default_dir: &str` | `String` | environments.tsx:545 | Reads back the saved value. |
| `get_settings_directory` | helpers.rs:430-433 | — | `PathBuf` | uninstall.tsx:29 | Returns `~/.openbb_platform`. |
| `get_installation_directory` | helpers.rs:652-655 | — | `String` | uninstall.tsx:25 | Reads from system_settings.json. |
| `get_userdata_directory` | helpers.rs:682-685 | — | `String` | uninstall.tsx:27 | Reads `preferences.data_directory`. |
| `update_openbb_settings` | helpers.rs:949-955 | `conda_dir: &Path, environment: &str` | `()` | installation-progress.tsx:1162,1207 | Mutates system_settings.json with the active environment. |
| `open_url_in_window` | helpers.rs:957-1021 | `url: String, title: Option<String>, window: Window` | `()` | api-keys.tsx:428; environments.tsx:48,1974; backends.tsx:1893 | Opens an EXTERNAL URL inside a new Tauri WebviewWindow (label = `url_{timestamp}`). |
| `select_file` | helpers.rs:1354-1357 | `filter: Option<String>` | `String` | backends.tsx:1616,2647 | Native file picker via AppleScript / shell, not plugin-dialog. |
| `check_directory_exists` | helpers.rs:1364-1367 | `path: String` | `bool` | setup.tsx:91; backends.tsx:1448; environments.tsx:348 | Sync `Path::is_dir()`. |
| `get_home_directory` | helpers.rs:1375-1378 | — | `String` | setup.tsx:56; environments.tsx:719 | `$HOME` or `$USERPROFILE`. |
| `select_directory` | helpers.rs:1593-1596 | `prompt: Option<String>` | `String` | setup.tsx:145; backends.tsx:1125,1860,2652,2725; environments.tsx:438 | Native dir picker. |

### 1h. Module: `utils/certs.rs` & `uninstall.rs` (2 commands)

| Command | File:Line | Params | Returns | Frontend caller(s) | Description |
|---|---|---|---|---|---|
| `generate_self_signed_cert` | certs.rs:165-172 | `common_name, org_name, alt_names: Vec<String>, output_dir, days_valid: u32, password: Option<String>, install_in_trust_store: bool` | `serde_json::Value` | backends.tsx:327 | Generates SSL cert via openssl crate; optionally adds to OS trust store. |
| `uninstall_application` | uninstall.rs:14-20 | `app_handle, window, remove_user_data: bool, remove_settings: bool` | `Option<String>` | uninstall.tsx:73 | Stops services, runs Miniforge uninstaller, deletes dirs. Streams `uninstall_progress` events. |

**Total: 53 commands** in `generate_handler!`. (`check_installer_file_exists` is defined but not registered.)

---

## 2. Full Event Catalog

All emits use `tauri::Emitter` either on `AppHandle` (broadcasts to ALL windows) or on `Window` (the specific window). The frontend uses global `listen()` from `@tauri-apps/api/event` which receives broadcasts regardless of window.

| Event Name | Emit File:Line(s) | Listen File:Line(s) | Payload Schema | Purpose |
|---|---|---|---|---|
| `process-output` | jupyter.rs:154,193,482; backends.rs:355,462,500,555,1003; environments.rs:60,83 | environments.tsx:599,1522,2047; backends.tsx:665; BackendLogsPage.tsx:175; JupyterLogsPage.tsx:246 | `{processId: string, output: string, timestamp?: number, type?: "system" \| "stdout" \| "stderr"}` | The big one — every line of stdout/stderr from every spawned subprocess (conda create, jupyter, backend services) is broadcast. Frontend filters by `processId` match. |
| `install-progress` | startup.rs:477,517,1264 | installation-progress.tsx:822 | `InstallProgress { step: string, progress: f32, message: string }` | Long-running install/setup progress. Phases: `download`, `install`, `config`, `complete`, `error`, `abort`. |
| `installation-directory` | startup.rs:1307 | index.tsx:27 | `string` (the install dir path) | Sent on successful install completion. |
| `installation-status` | **NEVER EMITTED** | index.tsx:15 | `boolean` | Dead listener. The `setTimeout` fallback at index.tsx:34 always fires `invoke("get_installation_state")` instead. |
| `jupyter-status-update` | jupyter.rs:657 | (no `listen` — only consumed via `window.postMessage` from logs window: JupyterLogsPage.tsx:218; environments.tsx:2091) | `{environmentName: string, status: string}` | Cross-window jupyter status. Note: the more reliable path is window-to-window postMessage, not Tauri events. |
| `backend-url-discovered` | backends.rs:1103 | backends.tsx:2219 | `{id: string, url: string}` | Backend URL parser found a serving URL in the subprocess output. |
| `boolean-message` | backends.rs:1202 | (no listener found) | `{message: string}` | Dead emit (legacy). |
| `uninstall_progress` | uninstall.rs:26,450,470,535 | uninstall.tsx:43 | `string` (free-form status) | Streams the uninstall steps. |

---

## 3. Tauri Plugin Usage Map

### Server-side (Cargo.toml) — initialized in main.rs:474-488

| Plugin | Cargo dep | Init line | Purpose | TS port equivalent |
|---|---|---|---|---|
| `tauri-plugin-updater` | Cargo.toml:62 | main.rs:474 | Auto-update from GitHub release JSON. Custom `check_and_apply_update` (main.rs:100-262) bypasses the JS API and uses `app.updater_builder()` directly. | electron-updater (Electron) or Squirrel.Mac/MSI custom updater. |
| `tauri-plugin-opener` | Cargo.toml:48 | main.rs:475 | Used from JS to open URLs / files in OS default app. | Electron `shell.openExternal` / `shell.openPath`. |
| `tauri-plugin-single-instance` | Cargo.toml:61 | main.rs:476-481 | Re-focuses existing window if user launches app twice. | Electron `app.requestSingleInstanceLock()`. |
| `tauri-plugin-fs` | Cargo.toml:45 | main.rs:482 | Capability-gated file system from JS. Used at environments.tsx:5 (`exists` from BaseDirectory). | Node `fs/promises` (Electron preload). |
| `tauri-plugin-shell` | Cargo.toml:32 | main.rs:483 | Shell exec (capability-gated). Not directly imported in src/ JS — Rust spawns subprocesses with `std::process::Command` instead. | Node `child_process.spawn`. |
| `tauri-plugin-persisted-scope` | Cargo.toml:44 | main.rs:484 | Persists fs/shell scopes across restarts. | Custom Electron config. |
| `tauri-plugin-log` | Cargo.toml:25 | main.rs:485-487 | Forwards Rust `log::*` to stdout/stderr. | `electron-log` or `winston`. |
| `tauri-plugin-dialog` | Cargo.toml:43 | main.rs:488 | `confirm()`, `message()`. Used at uninstall.tsx:4, setup.tsx:5, api-keys.tsx:4, plus from Rust at main.rs:102 for update prompts. | Electron `dialog.showMessageBox` / `dialog.showOpenDialog`. |

### Frontend-side (package.json)

| Plugin | package.json | Used in (production code only — tests excluded) | What for |
|---|---|---|---|
| `@tauri-apps/api/core` | (devDep `@tauri-apps/api`:51) | `invoke()` at all 100+ call sites | Core IPC. |
| `@tauri-apps/api/event` | same | `listen()` at 8 sites | Event subscription. |
| `@tauri-apps/api/app` | (built-in) | ShowVersion.tsx:2 (`getVersion`) | App version string. |
| `@tauri-apps/plugin-dialog` | package.json:25 | uninstall.tsx, setup.tsx, api-keys.tsx | `confirm()`, `message()`. |
| `@tauri-apps/plugin-fs` | package.json:26 | environments.tsx:5 only — `exists(path, {baseDir: BaseDirectory.X})` | Single use of plugin-fs, the rest goes through Rust commands. |
| `@tauri-apps/plugin-opener` | package.json:29 | backends.tsx:4 — `openPath()`, `openUrl()` | Opens cert dir, opens user's external URLs in OS browser. |
| `@tauri-apps/plugin-app` | package.json:24 | NOT IMPORTED by app code (only `getVersion` from `@tauri-apps/api/app`) | unused dependency |
| `@tauri-apps/plugin-http` | package.json:27 | NOT IMPORTED | Unused. |
| `@tauri-apps/plugin-log` | package.json:28 | NOT IMPORTED on JS side | Server-only. |
| `@tauri-apps/plugin-process` | package.json:30 | NOT IMPORTED (uninstall.tsx:84 has `invoke('app.exit')` which is misnamed/broken — should be `process.exit()`) | Unused. |
| `@tauri-apps/plugin-updater` | package.json:31 | NOT IMPORTED (handled in Rust) | Unused on JS side. |
| `@tauri-apps/plugin-window` | (devDep:54) | NOT IMPORTED | Unused. |
| `@tauri-apps/plugin-shell` | (devDep:53) | NOT IMPORTED | Unused. |
| `taurpc` | package.json:43 | **NOT IMPORTED ANYWHERE** | Vestigial dependency. No `taurpc::*` macros in Rust either. |

**TS port equivalent for frontend plugins:** All can be replaced with an Electron preload bridge that exposes `window.api.*` functions. `plugin-fs` → Node fs/promises; `plugin-dialog` → `electron.dialog`; `plugin-opener` → `electron.shell.openPath/openExternal`; `getVersion` → `electron.app.getVersion()`.

---

## 4. Patterns and Gotchas

### 4.1 Error returns

**Universal convention: `Result<T, String>`.** Every fallible command stringifies its error with `format!("...: {e}")`. Frontend code uses `await invoke(...).catch(err => ...)` where `err` is just a string. There is no typed error system.

Examples: backends.rs:561, environments.rs:418, helpers.rs throughout.

### 4.2 Long-running calls

Two distinct patterns coexist:

**Pattern A — Await + listen (most common):** Frontend sets up `listen("process-output", ...)` BEFORE invoking, awaits the invoke to completion, AND filters streamed events by `processId`. Example: environments.tsx:598-643 around `create_environment_from_requirements`.

```ts
const processId = `create-env-${name}-${Date.now()}`;
const unlisten = await listen<{processId, output}>("process-output", e => {
  if (e.payload.processId === processId) appendLog(e.payload.output);
});
await invoke("register_process_monitoring", { processId });
await invoke("create_environment", { name, processId, ... });  // awaits to completion
unlisten();
```

The Rust handler is `async` but actually blocks on `child.wait()` in a thread (e.g. environments.rs `run_command_with_logging`).

**Pattern B — Fire-and-forget + poll status:** Used for `install_conda` + `setup_python_environment`. The JS calls `invoke("install_conda", ...)` and then a polling effect (installation-progress.tsx:1059) calls `invoke("get_installation_status")` every N seconds. Events are also emitted as a parallel push channel.

### 4.3 Cancellation

There is **no proper cancellation primitive** — no AbortController, no cancel tokens. The two ad-hoc mechanisms are:

- **`abort_installation`** (startup.rs:1097): a separate command that brute-force kills via `pkill`/`taskkill` matching directory patterns and resets state.
- **`stop_jupyter_server` / `stop_backend_service`**: kills the tracked PID via the `RunningProcesses` state struct.

Frontend protections are flag-based (e.g. `isCancelling` ref at installation-progress.tsx:828) — checked inside event handlers to ignore late-arriving events.

### 4.4 Mutex deadlock risk

Several Rust globals are wrapped in `std::sync::Mutex` (NOT `tokio::Mutex`):

- `INSTALLATION_STATE` — startup.rs:15
- `INSTALLATION_IN_PROGRESS` — startup.rs:434
- `ACTIVE_JUPYTER_SERVERS` — jupyter.rs:9
- `LOG_STORAGE` — process_monitor.rs:9
- `RunningProcesses(Mutex<HashMap<String, Child>>)` — process_monitor.rs:106

**Specific deadlock risks:**
- `install_conda` (startup.rs:436) is `async` but holds `INSTALLATION_STATE` and `INSTALLATION_IN_PROGRESS` locks across awaits. Mitigated by short critical sections and explicit drops (startup.rs:983 `// MutexGuard is dropped here`).
- `LOG_STORAGE` is locked from background threads on every line of subprocess output (jupyter.rs:142, backends.rs:998). Lock contention is high during heavy log streaming, but each critical section is bounded (one buffer add).

The codebase uses `if let Ok(mut storage) = log_storage.lock()` (e.g. jupyter.rs:142) to silently skip on lock failure rather than panic, which is defensive but may drop log entries.

### 4.5 State sharing between handlers

Two mechanisms:

1. **Tauri-managed state** via `.manage(...)` — passed as `State<T>` parameter.
2. **`once_cell::Lazy<Mutex<T>>` globals** — accessed directly from any function via the `LOG_STORAGE` etc. statics. This is heavily used because the global is also needed inside spawned threads where tauri State extraction is awkward.

The Rust code does not use Tauri state for `INSTALLATION_STATE`, `ACTIVE_JUPYTER_SERVERS`, etc. — those are pure globals.

### 4.6 Subprocess output streaming pattern

Repeated identical pattern at jupyter.rs:119-156, backends.rs:870-1010, environments.rs:38-95:

```rust
let stdout = child.stdout.take()...;
std::thread::spawn(move || {
  for line in BufReader::new(stdout).lines().map_while(Result::ok) {
    // 1. Push to LogStorage ring buffer
    // 2. Emit "process-output" event with {processId, output, timestamp}
  }
});
```

Two threads per process (stdout + stderr). Events are best-effort (`let _ = handle.emit(...)` discards errors).

### 4.7 Frontend uses snake_case OR camelCase inconsistently for invoke args

Tauri auto-converts at the boundary, but there's a mix:
- snake_case: `invoke("update_user_credentials", { credentials })` — matches Rust
- camelCase: `invoke("create_environment_from_requirements", { name, filePath, directory, processId })` — Tauri converts `filePath` → `file_path` Rust param

Both work because Tauri 2 normalizes both. A TS port must replicate this case-conversion middleware OR force one convention at the type boundary.

---

## 5. State Surfaces

### 5.1 `.manage(...)` registrations (main.rs:489-491)

| Type | Holds | Lock Granularity | Writers | Readers |
|---|---|---|---|---|
| `ProcessLogState(LogStorage)` = `Arc<Mutex<HashMap<String, LogBuffer>>>` | Map of processId → 10000-line ring buffer | One mutex for whole map | All subprocess output threads (every line: jupyter.rs:142, backends.rs:998, etc.) | `get_process_logs_history` command (main.rs:85), `clear_process_logs_history` (main.rs:95) |
| `RunningProcesses(Arc<Mutex<HashMap<String, std::process::Child>>>)` | Map of backend id → spawned Child | One mutex for whole map | `start_backend_service` (backends.rs:639), `stop_backend_service` | `kill_process`, `is_process_running`, `cleanup_dead_processes` |
| `InstallationState { is_installed, installation_directory }` | Cached install check from boot | Immutable — clone on read | None (set once in setup) | `get_installation_state` command (main.rs:388) |
| `tray` (TrayIcon) | macOS/Win/Linux tray handle | N/A | Setup hook only | Menu event handlers |

### 5.2 `Lazy<Mutex<T>>` globals (NOT Tauri-managed)

| Global | File:Line | Type | Purpose |
|---|---|---|---|
| `INSTALLATION_STATE` | startup.rs:15 | `Mutex<InstallationState>` (different struct than the `.manage`'d one!) | Tracks install phase booleans (is_downloading, etc.) for `get_installation_status` command. **Confusingly there are TWO `InstallationState` types: one in main.rs:67, one in startup.rs:18.** |
| `INSTALLATION_IN_PROGRESS` | startup.rs:434 | `Mutex<bool>` | Mutex flag preventing concurrent `install_conda` calls. |
| `ACTIVE_JUPYTER_SERVERS` | jupyter.rs:9 | `Mutex<HashMap<String, (String, u32)>>` | env name → (URL, PID). |
| `LOG_STORAGE` | process_monitor.rs:9 | `Lazy<LogStorage>` | Same `Arc` is also wrapped in `ProcessLogState` — both refer to the same map. |

---

## 6. Window-Creation Patterns

The app has 4 window types beyond the main window:

### 6.1 Logs window (jupyter)

`open_jupyter_logs_window` (jupyter.rs:582-642):
- **Label:** `jupyter-logs-{environment}` (per-environment uniqueness)
- **URL:** `tauri::WebviewUrl::App("/jupyter-logs?env={environment}")` — internal route
- **Identity flow:** Frontend reads `useSearch({from: '/jupyter-logs'})` (JupyterLogsPage.tsx:18) to extract `env`, computes `processId = jupyter-{env}`, subscribes to `process-output` filtered by that.
- **Re-open:** if window with that label exists, just `show()` + `set_focus()`. No duplicate.
- **Close behavior:** `prevent_close` + `hide` (kept around for re-show).

### 6.2 Logs window (backend)

`open_backend_logs_window` (backends.rs:1536-1612):
- **Label:** `backend-logs-{id}` (UUID per backend)
- **URL:** `/backend-logs?id={id}`
- **Identity flow:** Same as Jupyter — `processId = backend-{id}`, frontend subscribes filtered.

### 6.3 External URL window

`open_url_in_window` (helpers.rs:957-1021):
- **Label:** `url_{timestamp_millis}` (ephemeral; multiple allowed)
- **URL:** `tauri::WebviewUrl::External(url)` — load arbitrary external URL
- **Close behavior:** `destroy()` then `prevent_close` (one-shot).

### 6.4 Cross-window communication

Two channels:
1. **Tauri broadcast events** — `app_handle.emit("jupyter-status-update", ...)` reaches all windows including children.
2. **Browser `window.postMessage` + localStorage** — JupyterLogsPage.tsx:218 uses `window.opener.postMessage(...)` AND a localStorage shim. Environments.tsx:2091 listens via `addEventListener("message", ...)` AND a storage listener.

The dual-channel approach was added defensively because Tauri events were unreliable in some scenarios — a TS port should NOT replicate this and should pick one channel.

---

## 7. TaURPC Investigation

`taurpc` appears at:
- `/home/user/OpenBBPort/desktop/package.json:43` — `"taurpc": "^1.8.1"`
- `/home/user/OpenBBPort/desktop/package-lock.json:37,10879`

It is **not imported anywhere** in `src/` (verified by grep). It does NOT appear in `Cargo.toml`. There are no `#[taurpc::procedures]` or `taurpc::Router::new()` calls in Rust. There are no generated typed bindings.

**Conclusion:** taurpc is a leftover dependency from an experiment / aborted migration. The app uses raw `invoke()` strings everywhere, with the type parameter (`invoke<UserCredentials>(...)`) supplied manually at each call site as a TS-only assertion that the Rust side does not enforce.

This is a maintainability cost: every command string is duplicated between Rust handler names and JS invoke calls, with no compile-time check that they agree.

---

## 8. TS Port Architecture Recommendation

Given the patterns observed, the recommendation is **Electron with a preload bridge using contextBridge + ipcRenderer/ipcMain**. Rationale:

### 8.1 Why Electron IPC and not WebSocket/HTTP/tRPC

| Option | Verdict | Reasoning |
|---|---|---|
| **Electron `ipcRenderer.invoke` + `ipcMain.handle`** | RECOMMENDED | Maps 1:1 onto current `invoke<T>(name, args)` semantics. No transport-layer rework needed. Synchronous-feeling promises. |
| **`webContents.send()` + `ipcRenderer.on()`** | RECOMMENDED for events | Maps 1:1 onto `app_handle.emit("event", payload)` + `listen()`. Per-window vs broadcast control is a perfect match for `window.emit` vs `app_handle.emit`. |
| **tRPC over Electron IPC** | Worth considering | Would solve the type-safety gap (53 invoke names that are stringly-typed). `trpc-electron` exists. Adds build complexity. |
| **Local HTTP server + WebSocket** | NOT RECOMMENDED | The app already uses subprocess servers (Jupyter, OpenBB API on 6900, MCP on 8001). Adding an internal HTTP control-plane port creates port conflicts and a security surface. |
| **socket.io** | NOT RECOMMENDED | Same as above; over-engineered for in-process communication. |
| **gRPC / Cap'n Proto / NeutralinoJS messaging** | NOT RECOMMENDED | Massive overhead vs Electron's built-in V8-serialized IPC. |

### 8.2 Recommended preload bridge sketch

```ts
// preload.ts — runs in renderer with Node access
import { contextBridge, ipcRenderer } from 'electron';

const api = {
  invoke: <T = unknown>(channel: string, args?: unknown) =>
    ipcRenderer.invoke(channel, args) as Promise<T>,

  listen: <T = unknown>(channel: string, cb: (payload: T) => void) => {
    const listener = (_e: unknown, payload: T) => cb(payload);
    ipcRenderer.on(channel, listener);
    return () => ipcRenderer.removeListener(channel, listener);
  },
};

contextBridge.exposeInMainWorld('tauriCompat', api);
```

This lets renderer code keep `invoke("command", args)` and `listen("event", cb)` shape with one-line shims:

```ts
// shims/tauri-core.ts
export const invoke = window.tauriCompat.invoke;
// shims/tauri-event.ts
export const listen = (chan, cb) => Promise.resolve(window.tauriCompat.listen(chan, cb));
```

Most existing call sites (~120) compile unchanged.

### 8.3 Required architectural additions for the port

1. **camelCase ↔ snake_case middleware** — wrap `ipcMain.handle` with a converter so existing JS callers using either convention work (mirrors Tauri's behavior).
2. **`process-output` channel** — port the LogStorage ring buffer (10000-line cap, processId-keyed). Subprocess streaming uses `child_process.spawn(...).stdout.on('data', ...)`. Send via `BrowserWindow.getAllWindows().forEach(w => w.webContents.send('process-output', payload))` to mirror `app_handle.emit` broadcast semantics.
3. **Multi-window URL routing** — Electron `BrowserWindow` with `loadURL("app://./jupyter-logs?env=foo")` or whatever your renderer router expects. Window labels become a Map<string, BrowserWindow> keyed by your existing `jupyter-logs-{env}` / `backend-logs-{id}` strings.
4. **Cross-window events** — `webContents.send` inherently per-window. To broadcast (current Tauri default), wrap with a helper `broadcast(channel, payload)` that iterates all windows. This makes the "all subprocess output goes everywhere" pattern straightforward.
5. **Replace `tauri::State`** — use a singleton module exporting `processLogStore`, `runningProcesses`, `installationState`. Same shape as the current `Lazy<Mutex<T>>` globals; JS doesn't need locks since the main process is single-threaded.
6. **Replace plugins:**
   - `plugin-dialog` → `dialog.showMessageBox`, `dialog.showOpenDialog`
   - `plugin-fs` (single use of `exists` w/ BaseDirectory) → `fs.promises.stat` with path resolution
   - `plugin-opener` → `shell.openPath` / `shell.openExternal`
   - `plugin-updater` → `electron-updater` (matches the GitHub-release flow at main.rs:108)
   - `plugin-single-instance` → `app.requestSingleInstanceLock()` + `second-instance` event
   - `plugin-log` → `electron-log` configured to mirror stdout/stderr
7. **Drop these deps entirely:** `taurpc`, `@tauri-apps/plugin-app`, `@tauri-apps/plugin-http`, `@tauri-apps/plugin-process`, `@tauri-apps/plugin-window`, `@tauri-apps/plugin-shell` — none are actually used.
8. **Type-safety upgrade opportunity:** Either (a) generate a shared types file from a single source-of-truth schema (zod is already a dep at package.json:46) for command args/returns, or (b) adopt tRPC over IPC. Given 53 commands stringly-typed today with manual `invoke<T>` annotations everywhere, this is the highest-ROI improvement to make during the port.

### 8.4 Items that need special attention during port

- **`open_url_in_window`** (helpers.rs:957) opens an EXTERNAL URL inside a BrowserWindow. In Electron this is straightforward (`new BrowserWindow().loadURL(externalUrl)`) but watch out for `nodeIntegration: false` and CSP.
- **`install_conda`** (startup.rs:436) is 470 lines of subprocess orchestration with progress events. Logic ports directly; just substitute `child_process.spawn` for `std::process::Command`.
- **`generate_self_signed_cert`** uses Rust's openssl crate. Port to either Node's `crypto` module or shell out to the system `openssl`. Adding to OS trust store is platform-specific.
- **macOS-specific titlebar coloring** (NSWindow setBackgroundColor at multiple sites) — Electron has `vibrancy` and `titleBarStyle: 'hiddenInset'` options. Behavior won't match exactly but is close.
- **Dead emit `installation-status`** — index.tsx:15 listens for it but Rust never emits. The fallback `setTimeout` + invoke does the actual work. Port should drop the listener entirely.
- **`taurpc` dep** — remove during port.
- **`uninstall.tsx:84` calls `invoke('app.exit')`** — this is a typo/bug in the current app (no such command exists in the handler list). Fix during port to call the correct quit method.

---

## Cross-feature dependencies

This is the cross-cutting reference — every feature doc depends on this one for IPC names and event schemas. Specifically:

- `feature-installation.md` uses `install-progress`, `installation-directory`, `installation-status` events + `install_to_directory`, `install_conda`, `setup_python_environment`, `abort_installation`, `get_installation_status` invokes
- `feature-environments.md` uses `process-output` event + `list_conda_environments`, `create_environment`, `create_environment_from_requirements`, `install_extensions`, `update_extension`, `remove_extension`, `update_environment`, `remove_environment`, `get_environment_extensions`, `select_requirements_file` invokes
- `feature-jupyter.md` uses `process-output`, `jupyter-status-update` events + `start_jupyter_server`, `stop_jupyter_server`, `check_jupyter_server`, `list_jupyter_servers`, `open_jupyter_logs_window`, `update_jupyter_status` invokes
- `feature-backend-services.md` uses `process-output`, `backend-url-discovered`, `boolean-message` events + `start_backend_service`, `stop_backend_service`, `create_backend_service`, `update_backend_service`, `delete_backend_service`, `list_backend_services`, `open_backend_logs_window`, `generate_self_signed_cert` invokes
- `feature-api-keys.md` uses `get_user_credentials`, `update_user_credentials`, `open_credentials_file` invokes
- `feature-logs-streaming.md` uses `process-output` event + `register_process_monitoring`, `unregister_process_monitoring`, `get_process_logs_history`, `clear_process_logs_history` invokes
- `feature-tray-and-autostart.md` uses `navigate_to_page` (Rust-internal), `quit_application` invokes
- `feature-uninstall.md` uses `uninstall_progress` event + `uninstall_application`, `get_installation_directory`, `get_userdata_directory`, `get_settings_directory` invokes
