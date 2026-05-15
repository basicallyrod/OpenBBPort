# Deep-Dive: Backends Page (HTTP Service Management)

> Raw findings from Wave 1 agent. Source of truth for `20-features/feature-backend-services.md`.
> Heavily related to `feature-logs-streaming.md` (shares process-output infra).
> Generated 2026-05-15.

## 0. File Map

| Layer | File |
|---|---|
| Frontend page | `/home/user/OpenBBPort/desktop/src/routes/backends.tsx` (2741 lines) |
| Logs window page | `/home/user/OpenBBPort/desktop/src/components/BackendLogsPage.tsx` (266 lines) |
| Logs window route | `/home/user/OpenBBPort/desktop/src/routes/backend-logs.tsx` (26 lines) |
| Rust handlers | `/home/user/OpenBBPort/desktop/src-tauri/src/tauri_handlers/backends.rs` (2000 lines) |
| Process monitor | `/home/user/OpenBBPort/desktop/src-tauri/src/utils/process_monitor.rs` (740 lines) |
| Cert generation | `/home/user/OpenBBPort/desktop/src-tauri/src/utils/certs.rs` (562 lines) |
| Wiring + state | `/home/user/OpenBBPort/desktop/src-tauri/src/main.rs` (858 lines) |
| Helpers (paths) | `/home/user/OpenBBPort/desktop/src-tauri/src/tauri_handlers/helpers.rs` |
| Command sanitiser | `/home/user/OpenBBPort/desktop/src-tauri/src/utils/command_sanitizer.rs` |
| Default seed | `/home/user/OpenBBPort/desktop/src-tauri/src/tauri_handlers/startup.rs:1432` (`create_default_backend_services`) |

---

## 1. Reproduced Domain Types

### Frontend (TS — `backends.tsx:23-64`)

```ts
interface BackendService {
  id: string;
  name: string;
  command: string;
  host?: string;
  port?: number;
  envFile?: string;          // camelCase form, used by UI
  env_file?: string;         // snake_case form, exists because Rust serialises with #[serde(rename = "envFile")] BUT older entries / list responses sometimes carry both
  envVars?: Record<string, string>;
  environment: string;       // conda env name
  autoStart: boolean;        // UI-side
  auto_start: boolean;       // Rust-side; both kept around because list_backend_services returns auto_start
  status: "running" | "stopped" | "starting" | "stopping" | "error";
  pid?: number;
  startedAt?: string;        // ISO-8601 — purely client-side mirror; Rust calls it started_at
  error?: string;
  apiUrl?: string;           // alias for url
  url?: string;
  working_directory?: string;
}

interface BackendFormData {
  id: string;
  name: string;
  command: string;
  envFile?: string;
  envVars?: Record<string, string>;
  host?: string;
  port?: number;
  environment: string;
  autoStart: boolean;
  status: string;
  working_directory?: string;
  apiUrl?: string;
  pid?: number;
}

interface Environment { name: string; path: string; }
```

### Rust (`backends.rs:32-75`)

`BackendService` is `Serialize + Deserialize + Default + Clone`. Field renames are critical:
- `env_file` ↔ `envFile` (alias + rename), `env_vars` ↔ `envVars`. All other snake_case fields keep snake_case on the wire (`auto_start`, `working_directory`, `started_at`).
- All optional fields are `#[serde(skip_serializing_if = "Option::is_none")]`, so JSON on disk is sparse.
- `status` is plain `String`, not an enum (the `BackendStatus` enum at `:114-133` only renders to/from the canonical strings `running|stopped|starting|stopping|error`).

The Rust→JS payload for `list_backend_services` therefore has fields: `id, name, command, environment, auto_start, status` (always present) plus optional `envFile, envVars, working_directory, host, port, url, pid, started_at, error`. The frontend normalises both casings on receipt (`backends.tsx:2322-2330`, `2365-2372`).

---

## 2. Persistence

- **Location:** `<install_dir>/backends/backends.json`
- **`<install_dir>`:** read from `~/.openbb_platform/system_settings.json → install_settings.installation_directory` (`helpers.rs:627-650`, `get_installation_directory_impl`).
- **Backends dir is auto-created** if missing (`backends.rs:197-207`, `get_backends_dir`).
- **Format:** pretty JSON array of `BackendService` (`backends.rs:251`, `serde_json::to_string_pretty`).
- **Concurrent-safe writes** (`backends.rs:243-286`, `save_backends_config`):
  1. `open_rw_create`
  2. `try_lock_exclusive` (advisory file lock via `RealFileExtTrait`)
  3. `seek 0 → set_len 0 → write_all → flush`
  4. `unlock`
- **Reads** (`backends.rs:215-240`, `load_backends_config`): missing file → `Vec::new()`; empty file → `Vec::new()`; otherwise `serde_json::from_str`. No locking on read (last-writer-wins).

---

## 3. Wiring & Global State (`main.rs`)

- **Plugins:** `tauri_plugin_updater`, `_opener`, `_single_instance`, `_fs`, `_shell`, `_persisted_scope`, `_log`, `_dialog`.
- **Managed state:**
  - `ProcessLogState(LogStorage)` — `main.rs:73-78, 489`. `LogStorage = Arc<Mutex<HashMap<String, LogBuffer>>>` (`process_monitor.rs:7,9`). Single global `LOG_STORAGE` Lazy singleton at `process_monitor.rs:9`. `ProcessLogState` is just a wrapper for State injection while `crate::get_log_storage()` is the same singleton accessed directly in `backends.rs`.
  - `RunningProcesses` — `main.rs:490`. `Arc<Mutex<HashMap<String, std::process::Child>>>` (`process_monitor.rs:106`). Keys are **backend ids** (NOT `backend-<id>` strings; that prefix is for log streams).
  - `InstallationState` — `main.rs:491`. Read once at boot.
- **Invoke handlers** registered (`main.rs:492-550`): for backends, the relevant ones are
  - `register_process_monitoring`, `unregister_process_monitoring`, `get_process_logs_history`, `clear_process_logs_history` (`main.rs:75-98`)
  - `open_backend_logs_window`
  - `start_backend_service`, `stop_backend_service`
  - `update_backend_service`, `create_backend_service`, `delete_backend_service`
  - `list_backend_services`
  - `generate_self_signed_cert`
  - `create_default_backend_services`
- **Auto-start at app launch** (`main.rs:569-579`): if installation is valid, after a 100 ms delay, spawn `initialize_backends(...)`.
- **Cleanup on quit / SIGINT / Tauri ExitRequested** (`main.rs:413-467, 762-772, 820-840`): bounded by a 10 s wall-clock; calls `stop_all_backend_services` (3 s budget) then `stop_all_jupyter_servers`. Hooked to: `quit_application` invoke, tray "Quit", ctrlc handler, RunEvent::ExitRequested (with restart fork via `RESTART_EXIT_CODE`).

---

## 4. CRUD Flow (per command)

### 4.1 List — page mount

- **Trigger:** initial mount in `BackendsPage` (`backends.tsx:2351-2402`, `useEffect`).
  1. `loadEnvironmentsFromCache()` — reads `localStorage["env-extensions-cache"]` (`backends.tsx:2156-2168`). Pure client.
  2. `invoke("list_backend_services")` → `BackendService[]`.
  3. Normalises both casings: `auto_start ?? autoStart`, `env_file ?? envFile`, `url ?? apiUrl` (`:2365-2372`).
  4. If env cache empty, `invoke("list_conda_environments")` → `Environment[]`, filters out `base`.
- **Rust:** `list_backend_services` (`backends.rs:1224-1227`) → `list_backend_services_impl` (`:1215-1222`) → `load_backends_config().unwrap_or_default()`.
- **Refresh hook:** `fetchBackends()` (`backends.tsx:2309-2348`) — early-aborts if `isEditing` is true to avoid stomping the open form. Called after every start/stop/delete.

### 4.2 Create

- **Trigger:** "New Backend" button → `setIsCreating(true)` (`backends.tsx:2684-2690`). Form pre-fills `command: "openbb-api"`, `host: "127.0.0.1"`, `environment: environments[0]?.name`. Submit handler in `backends.tsx:2604-2637`:
  ```ts
  invoke("create_backend_service", { backend: {
    id: "", name, command, host, port, envFile, envVars,
    environment, auto_start, status, working_directory, pid
  }})
  ```
  Note: `id` sent is empty string (`formData.id`), because **Rust always overwrites with a fresh UUID** (`backends.rs:1259`).
- **Rust:** `create_backend_service` (`backends.rs:1272-1275`) → `create_backend_service_impl` (`:1230-1270`):
  1. Load existing config.
  2. Validate `name`, `command`, `environment` non-empty.
  3. Reject duplicate name (`:1249-1251`).
  4. `validate_command_input(command, fs, env_sys)` — server-side dangerous-pattern check in `command_sanitizer.rs:369-420`. Catches things the client also bans (sudo, eval, `$(`, `\``, `||`, more than 2 `|`, control chars), plus a list of restricted Unix utilities at `:340-353`.
  5. Force `id = Uuid::new_v4()`, `status = "stopped"`, clear `pid/url/started_at`.
  6. Append + `save_backends_config`.
- **Frontend after success:** `window.location.reload()` (`backends.tsx:2626`) — this is the dirt-simple way the page picks up the new entry.

### 4.3 Update

- **Two entry points:**
  1. From the form (full edit) — `backends.tsx:2604-2637`, action `"update_backend_service"`.
  2. Inline from the per-row config panel — `handleFormSubmit` at `backends.tsx:815-855`. Same payload shape.
  3. Auto-emitted from log-listener side effects (status→error, status→stopped on dismiss, pid set) — see §6.
- **Payload:** the same `BackendService` shape. Frontend maps `formData.autoStart → auto_start` before sending.
- **Rust:** `update_backend_service` (`backends.rs:1345-1348`) → `update_backend_service_impl` (`:1278-1343`):
  - Find by id (errors if missing).
  - Re-validate command if non-empty.
  - **Always overwrites:** `name, command, environment, auto_start, error`.
  - **Conditionally overwrites (only if Some)**: `working_directory, env_file, env_vars, host, port, url`. This means a partial PATCH is safe — sending `null/None` does NOT clear them. To clear, Rust uses an explicit empty string.
  - `pid`, `status`, `started_at` are NEVER touched here — those are runtime-managed by start/stop/log-reader.
  - **Server is NOT restarted** even if command/env changes (`:1340-1342`). New settings apply on next manual start.

### 4.4 Delete

- **Trigger:** trash icon (visible only when `status !== "running" && !isProcessing`, `backends.tsx:577,884`). Sets `backendToDelete`, opens `DeleteConfirmationModal` (`:217-280`). On confirm → `handleDeleteBackend` (`:2423-2449`) → `invoke("delete_backend_service", { id })`.
- **Rust:** `delete_backend_service` (`:1380-1393`) → `delete_backend_service_impl` (`:1351-1378`):
  1. Find index.
  2. **If `status == "running"` → call `stop_backend_service(...)` first** (cascades into the full stop dance below).
  3. Remove from vec; save.
- **Frontend:** clears `backendToDelete`, deselects, `fetchBackends()`.

---

## 5. Process Lifecycle

### 5.1 Start

**Trigger:** `BackendServiceItem` start button → `handleStartStop(id, "start")` (`backends.tsx:2452-2525`):
1. Local client-side `validateCommandInput` (UI regex blacklist at `:1302-1377`). On fail, set local `status: "error"` and **also push the error state via `update_backend_service`** so the persisted config reflects it.
2. Optimistically set `status: "starting"` in local state.
3. `invoke("start_backend_service", { id })`.
4. Always `fetchBackends()` afterwards.

**Rust** — `start_backend_service` (`backends.rs:639-652`) → `start_backend_service_impl` (`:655-1212`). This is the heavy lifter:

1. **Reload config**, find backend (`:667-672`).
2. **Re-validate command** server-side (`:674-691`). On dangerous pattern → flip backend to `error` in config and bail.
3. **Idempotent guard:** if currently `running` and `pid` alive → return existing backend (`:694-699`). Note `is_process_running` (`:291-309`) uses `kill -0 <pid>` on Unix, `tasklist /FI "PID eq N" /NH` on Windows.
4. **Locate conda exe:** `<install_dir>/conda/bin/conda` (Unix) or `<install_dir>/conda/Scripts/conda.exe` (Windows) (`:702-717`).
5. **Build env-export prelude** (`:719-762`):
   - Read `.env` file via `load_env_file` (`:136-176`): line-by-line, skips `#` comments / blanks, parses first `=`, strips matching surrounding `'`/`"`.
   - For each KV: emit `export K='v\'\\v'\''...'` on Unix or `set "K=V"` on Windows. The single-quote escape is `value.replace('\'', "'\\''")`.
   - Then merge `backend.env_vars` the same way (env-vars run AFTER env-file → can override).
6. **Special-case `openbb-api`** (`:774-807`): if command contains `openbb-api`,
   - append ` --env_file "<path>"` if env_file is set,
   - for any KV starting with `UVICORN_`, transform to `--<lowercase-of-rest>` flag (e.g. `UVICORN_HOST=0.0.0.0` → ` --host "0.0.0.0"`), unless that flag is already in the command. Done for both env-file vars and direct `env_vars`.
7. **Generate activation script** in `temp_dir()/backend_start_<id>.sh|.bat` (`:765-903`):
   - Sets `CONDA_ROOT`, `CONDA_ENVS_PATH`, `CONDA_PKGS_DIRS`, `CONDARC`; **unsets** `CONDA_DEFAULT_ENV`/`CONDA_PREFIX`/`CONDA_SHLVL` to avoid leakage; prepends `<conda>/bin:<conda>/condabin` to PATH.
   - Sources `<conda>/etc/profile.d/conda.sh` (Unix) or calls `<conda>\condabin\conda.bat` (Windows).
   - `conda activate <environment>`.
   - Echoes `Environment <name> activated successfully`.
   - Inlines the env exports.
   - Inlines the (modified) `command_to_run` last.
8. **`chmod 0755`** on Unix (`:910-920`).
9. **Wrap in shell:** `bash <script>` on Unix, `cmd /c <script>` on Windows (`:923-931`). Stdout/stderr both `Stdio::piped()`.
10. **CWD:** `backend.working_directory` if set, else `<install_dir>/backends/` (`:937-941`).
11. **`cmd.spawn()`** → `Child`. On error, delete the temp script and return.
12. **Schedule script deletion** 5 s later via detached thread (`:954-958`).
13. **Capture `child.id()`** as `process_pid` — this is the PID of the **shell wrapper**, NOT the eventual server. The real server PID is later harvested from logs.
14. **Register log buffer:** `register_process(&LOG_STORAGE, "backend-<id>")` (`:962-967`). Process-id namespace is `backend-<id>` for logs.
15. **Spawn two reader threads** — one for stdout, one for stderr (`:1122-1161`). Each line goes through the closure `log_processor` (`:977-1119`):
   - Strip `<script_path>:` prefix from sourcemap-style errors.
   - Push `LogEntry { timestamp, content, process_id }` into the in-memory `LogBuffer` (capped at 10000 entries, FIFO eviction — `process_monitor.rs:36-41`).
   - `app_handle.emit("process-output", { processId, output, timestamp, type: "stdout"|"stderr" })`. **Schema is camelCase `processId`** (the JSON serializer for `serde_json::json!` keeps the literal keys).
   - **Command-not-found detection** (`:1008-1026`): if line ends with `: command not found`, run `clean_error_message` (`:178-194`) to extract the bit after the second-to-last `: `, persist `status=error, error=clean, pid=None`, and kill the tracked process.
   - **PID extraction** (`:1030-1048`): regex `Started server process \[(\d+)\]` (uvicorn's standard log line). When matched, persist the **real** server PID (overwriting the wrapper PID).
   - **URL extraction** (`:1031-1118`): regex `(https?://(?:localhost|\d{1,3}(?:\.\d{1,3}){3})(?::\d+)?(?:[^\s]*)?)`. Collected URLs are debounced for **1500 ms**; whenever a new line with URLs comes, the previous debounce thread is `unpark`ed (effectively a "restart" by replacing the join-handle in a Mutex). When debounce fires, `select_best_url` (`:580-637`) picks:
     - Priority 1: URL ending in `/mcp` or `/sse`.
     - Priority 2: URL containing `/mcp` or `/sse`.
     - Priority 3: URL containing `docs|openapi|redoc`.
     - Fallback: last URL found.
     - Then if `original_log_line.contains("MCP server")` and the chosen URL lacks `/mcp|/sse`, append `/mcp` (or `/sse` if line says `sse`, or `/mcp` if `streamable-http`).
   - The chosen URL is parsed with `url::Url::parse` to derive `host_str()` and `port()`; all three (`url`, `host`, `port`) are persisted in `backends.json` and broadcast as `app_handle.emit("backend-url-discovered", { id, url })` (camelCase struct literal at `:22-26`).
16. **Track child:** `RunningProcesses::add_process(id, child)` (`:1164-1174` ↔ `process_monitor.rs:121-128`). Errors out if duplicate id (so duplicate starts are guarded both in step 3 and here).
17. **Reload config** (race-window mitigation explicitly noted in comment at `:1176-1178`), then mutate `status="running"`, `pid=Some(wrapper_pid)`, `started_at=Utc::now().to_rfc3339()`, `error=None`. Save. Note `host/port/url` are intentionally NOT touched here — those come from the async log reader.
18. Emit `boolean-message { message: "true" }` (legacy event).
19. Return the in-memory `BackendService` snapshot.

### 5.2 Stop

**Trigger:** `handleStartStop(id, "stop")` → optimistic `status: "stopping"` → `invoke("stop_backend_service", { id })`.

**Rust** — `stop_backend_service` (`backends.rs:561-571`) → `stop_backend_service_impl` (`:314-559`):

1. Load config, find backend.
2. **Port-based kill (if `backend.port` known)** (`:331-435`):
   - Emit a `process-output` system message `🎯 Killing all processes on port {port}` to the log buffer + event stream.
   - **macOS:** `lsof -ti tcp:<port>` → for each PID, `kill -9 <pid>`.
   - **Linux:** `fuser -k <port>/tcp` then `lsof -ti tcp:<port>` + `kill -9 <pid>` as backup.
   - **Windows:** `netstat -ano`, parse lines containing `:<port>` AND `LISTENING`, take last whitespace token as PID, `taskkill /F /PID <pid>`.
   - Sleep 2 s.
3. Emit `🛑 Stopping backend service '<name>'` to logs + event.
4. **Tracked-child kill:** `RunningProcesses::kill_process(id)` (`:465-473` ↔ `process_monitor.rs:157-172`) — `child.kill()` then `child.wait()` to reap. This kills the **shell wrapper**, not necessarily the real server child.
5. **PID-based fallback kill** (`:476-516`): `kill -9 <backend.pid>` on Unix, `taskkill /F /PID <pid>` on Windows. Plus log message `💀 Terminating process PID <pid>`.
6. Sleep 1 s.
7. **Reset config:** `status="stopped"`, `pid=None`, `url=None`, `started_at=None`, `host=None`, `port=None`. Save.
8. Emit `🟢 Backend service '<name>' stopped successfully`.

> **Signal:** SIGKILL (`-9`) only; no graceful SIGTERM/SIGINT first. No timeout — relies on the 2 s and 1 s sleeps.

### 5.3 Crash detection / status truth

The Rust side does NOT poll dead processes for the Backends page. Truth comes from three places:

1. **At startup** (`:1455-1475` in `initialize_backends`): for each `status==running`, call `is_process_running(pid)` (`kill -0` / tasklist). If dead, downgrade to `stopped` and save.
2. **Frontend log-listener heuristics** (`backends.tsx:656-801`): subscribed to `process-output` filtered by `eventProcessId === "backend-<id>"`. After ANSI strip:
   - If line contains `"ERROR:"` or `"address already in use"` → mark error, `stop_backend_service`, `update_backend_service` with `status=error`, propagate via `onStatusUpdate`.
   - If line contains `"Traceback"` → start buffering. End-of-traceback detected by blank line OR line starting with `>`/`$`. 2 s timeout flushes early. Then mark error + stop + persist.
   - PID extraction same regex `\[(\d+)\]` after `"Started server process"` → calls `update_backend_service` to persist PID immediately.
3. **45-second failsafe** in `BackendServiceItem` (`backends.tsx:804-813`): if `running && !urlConfirmed` after 45 s, just hide the spinner (does NOT change status).
4. **`backend-url-discovered` event** (`backends.tsx:2218-2293`): page-level listener that replaces `apiUrl/url` on the matching service AND triggers a one-time toast (`platform-api-run-once`, `platform-mcp-run-once` localStorage flags) for the OpenBB API / OpenBB MCP defaults.

### 5.4 Status state machine (truth table)

| Trigger | Status set by | Set to |
|---|---|---|
| User click Start | Frontend optimistic | `starting` |
| Rust spawn success + reload | Rust `start_backend_service_impl:1184` | `running` |
| Rust validation rejects command | Rust `:684` | `error` |
| Log reader sees `command not found` | Rust `:1014` | `error` |
| Frontend log heuristic (ERROR/Traceback) | Frontend `:677, 707, 727, 757` (via `update_backend_service`) | `error` |
| User click Stop | Frontend optimistic | `stopping` |
| `stop_backend_service` complete | Rust `:524` | `stopped` |
| App startup, was running but PID dead | Rust `:1467` | `stopped` |
| Dismiss error from row | Frontend `:986` (via `update_backend_service`) | `stopped` |

> ⚠️ Real gap: there is no proactive "running → error" transition triggered by detecting the wrapper child has exited; covered only by the log-text heuristics.

---

## 6. Logs Subsystem

See `logs-streaming.md` for the full process-monitor architecture. Backend-specific notes:

### 6.1 Process-id naming

- Logs key = `"backend-<backend.id>"`.
- `RunningProcesses` key = `<backend.id>` (no prefix).
- Both are derived from the same `id` UUID (so a TS port can reuse one map keyed by id, with a derived "log channel id" for emission).

### 6.2 Event schema

`app_handle.emit("process-output", payload)` payload object is built ad-hoc with `serde_json::json!` so the wire schema is:
```jsonc
{
  "processId": "backend-<id>",   // camelCase
  "output":    "<line of text>",
  "timestamp": 1715792000123,    // i64 ms
  "type":      "stdout" | "stderr" | "system"
}
```
`"system"` is used by stop messages (port-kill, terminate, complete).

`app_handle.emit("backend-url-discovered", { id, url })` — both string. Listener at `backends.tsx:2219`.

### 6.3 Subscriber: per-row monitor (`backends.tsx:656-801`)

- Mounts a `listen<...>("process-output", ...)` only when `backend.status === "running" && !urlConfirmed`.
- Only acts on events whose `processId === "backend-<this row's id>"`.
- Side effects: PID extraction; error/traceback detection; for traceback collection, holds a `tracebackBuffer.current` ref + `tracebackTimeout.current` 2 s flush timer.
- Cleanup on unmount or status change.

### 6.4 Open-logs-window

- **Trigger:** Logs button per row → `viewBackendLogs(id)` (`backends.tsx:2405-2420`):
  1. `invoke("register_process_monitoring", { processId: "backend-"+id })` — pre-registers buffer in case the backend is stopped (so cleared logs re-appear blank cleanly).
  2. `invoke("open_backend_logs_window", { id })`.
- **Rust** — `open_backend_logs_window` (`backends.rs:1537-1605`):
  1. Build `window_label = "backend-logs-<id>"`.
  2. If a window with that label already exists → `show()` + `set_focus()`, return.
  3. Look up backend name (for window title).
  4. `WebviewWindowBuilder::new(label, WebviewUrl::App("/backend-logs?id=<id>"))`. Title `"Open Data Platform: <name> Logs"`. 1000×600, resizable, min 600×200, centered, visible.
  5. macOS: transparent title bar + black `NSWindow` background.
  6. **Close intercept:** the window's `CloseRequested` is intercepted → `hide()` + `api.prevent_close()`. The window is never destroyed during the app session, just hidden.

So: **one Tauri webview window per backend**, scoped via the URL param. Logs are streamed by the global Rust event bus; the JS just filters by `processId`.

---

## 7. Certificate Generation

- **Trigger:** "Generate Certificate" button → `setIsGeneratingCert(true)` opens `CertificateGenerationModal` (`backends.tsx:284-559`). Form fields: `commonName, orgName, altNames (CSV), outputDir, daysValid (default 365), password (optional), addToTrustStore (checkbox)`.
- Output dir is selected via `invoke("select_directory", { prompt: "Select Output Directory" })` callback (`:2722-2733`).
- Submit → `invoke("generate_self_signed_cert", { commonName, orgName, altNames: [...], outputDir, daysValid, password: password||null, installInTrustStore: addToTrustStore })` (`:325-336`).
- **Rust** — `generate_self_signed_cert` (`certs.rs:165-189`) constructs `CertService` with `RealFileSystem`, `SystemTrustStore`, `RealCommandExecutor`. Then `generate_and_save_cert` (`:99-160`):
  1. `create_dir_all(output_dir)`.
  2. RSA-2048 keypair via `openssl::rsa::Rsa::generate(2048)`.
  3. `generate_cert` (`:193-249`): X509 v3, random 159-bit serial, CN + O subject, validity `now → now+days_valid`, BasicConstraints critical CA, KeyUsage(DigitalSignature, KeyEncipherment), SAN entries (each parsed: IPv4/IPv6 → `.ip()`, else `.dns()`), SHA-256 self-sign.
  4. **Files written to `output_dir`:**
     - `private.key` (PKCS#8 PEM, no password on the key itself)
     - `certificate.pem`
     - `certificate.p12` (PKCS#12 DER, optionally password-protected)
  5. **Optional trust store install** (`install_in_trust_store: true`):
     - **Windows:** `certutil -user -addstore Root <pem>` (current user store, no elevation).
     - **macOS:** `security add-trusted-cert -d -r trustRoot -k ~/Library/Keychains/login.keychain-db <pem>`.
     - **Linux:** uses `which certutil`; creates `~/.pki/nssdb` if missing (`certutil -N -d sql:<dir> --empty-password`); then `certutil -A -n "OpenBB Platform - <filename>" -t TC,, -i <pem> -d sql:<nssdb>`. Installs into the user's NSS DB only (works for Firefox/Chrome). Errors with hint to install `libnss3-tools` if `certutil` missing.
  6. Returns JSON `{ key_path, cert_path, pkcs12_path, expires: days_valid }`.
- **Important:** the cert-gen flow is INDEPENDENT of any backend service; it does not modify `backends.json` or wire the cert into any process. The user must manually point their backend command at the generated files (e.g. via `--ssl-keyfile` `--ssl-certfile` flags).

---

## 8. Auto-Start (`autoStart` flag)

- **Persistence:** `auto_start: bool` field on each `BackendService` in `backends.json`.
- **At app startup** (`main.rs:569-579`): if installation is valid, after 100 ms delay, spawn `initialize_backends(...)` in async runtime. Sequence (`backends.rs:1439-1533`):
  1. Load config.
  2. **Stale-state cleanup:** for each `status=="running"`, check `is_process_running(pid)`. If dead → downgrade to `stopped`, clear pid/url/host/port. Save if any changed.
  3. **Auto-start loop:** for each `auto_start && status=="stopped"`:
     - Run `validate_command_input`. On danger → set `status=error, error="Backend could not be started -> ..."` and skip.
     - Else `start_backend_service_impl(...)` (full spawn dance).
     - Sleep 500 ms between starts (`:1527`).
- **From the UI** (`backends.tsx:874`): the row shows an "Auto-Start" pill if `backend.autoStart` truthy. The toggle in the per-row config panel writes back via `update_backend_service` with `auto_start: true|false`.
- **Tray menu** (`main.rs:609-613, 680-735`): "Start at Login in Background" is OS-level autostart of the **app itself**, separate from backend autostart. Both are needed for "start backend on system boot".

---

## 9. URL composition

- **Composition is one-way and log-driven.** The frontend never sends `host/port/scheme` to the spawn step; instead, the Rust log reader extracts the URL from the server's startup banner.
- `select_best_url` (`backends.rs:580-637`) picks the most useful URL. Then `url::Url::parse` derives `host` + `port`. All three are saved to `backends.json` and emitted as `backend-url-discovered`.
- The frontend `BackendServiceItem` displays either `apiUrl` (when `running && urlConfirmed`) or the original `command` text. The URL becomes "confirmed" when (a) the page-level `backend-url-discovered` listener fires, or (b) the row mounts with `backend.apiUrl` already set, or (c) the failsafe 45 s timer flips `urlConfirmed` to true (so the spinner stops even if URL extraction fails).
- **Stop wipes URL/host/port** in config (`backends.rs:524-529`), so the next start has to re-discover.

---

## 10. Error handling specifics

- **Port collision:** detected only via the log heuristic (`address already in use` substring) → frontend triggers stop + persist `status=error`. The Rust side does NOT pre-bind/check the port before starting.
- **Missing executable:** the shell wrapper emits `: command not found`; Rust log reader catches this (`backends.rs:1008`), persists error, kills the wrapper. `clean_error_message` extracts `"<bin>: command not found"` from the bash-formatted prefix.
- **Bad env_file path:** `load_env_file` returns Err; Rust **logs a warning and continues** (`backends.rs:741-744`) — the start does NOT fail. The UI marks the env-file input red via `check_file_exists` (`backends.tsx:1595, 1620`).
- **Bad working_directory:** UI debounces `check_directory_exists` validation (`backends.tsx:1440-1461`); the actual `cmd.current_dir` call (`backends.rs:937-941`) does not pre-check, so spawn would fail.
- **Conda missing:** explicit error `"Conda executable not found at: ..."` (`:712-717`).
- **Dangerous command:** double-validated client (regex blacklist `:1302-1377`) and server (`command_sanitizer.rs:369-420`).
- **File-locking on backends.json:** `try_lock_exclusive` is non-blocking; if another writer holds it, the entire save returns Err and the operation surfaces as a Tauri command failure.

---

## 11. Tauri Invoke Surface (this page only)

| Command | Args (JS, camelCase as sent) | Returns | Where |
|---|---|---|---|
| `list_backend_services` | – | `BackendService[]` | `backends.rs:1224` |
| `create_backend_service` | `{ backend: BackendService }` | `BackendService` | `:1272` |
| `update_backend_service` | `{ backend: BackendService }` | `BackendService` | `:1345` |
| `delete_backend_service` | `{ id }` | `()` | `:1381` |
| `start_backend_service` | `{ id }` | `BackendService` | `:639` |
| `stop_backend_service` | `{ id }` | `()` | `:561` |
| `open_backend_logs_window` | `{ id }` | `()` | `:1537` |
| `register_process_monitoring` | `{ processId }` | `bool` | `main.rs:75` |
| `unregister_process_monitoring` | `{ processId }` | `bool` | `main.rs:80` |
| `get_process_logs_history` | `{ processId, count? }` | `LogEntry[]` | `main.rs:86` |
| `clear_process_logs_history` | `{ processId }` | `bool` | `main.rs:95` |
| `generate_self_signed_cert` | `{ commonName, orgName, altNames, outputDir, daysValid, password, installInTrustStore }` | `{ key_path, cert_path, pkcs12_path, expires }` | `certs.rs:165` |
| `select_directory` | `{ prompt? }` | `string` | `helpers.rs:1594` |
| `select_file` | `{ filter? }` | `string` | `helpers.rs:1355` |
| `check_directory_exists` | `{ path }` | `bool` | `helpers.rs:1365` |
| `check_file_exists` | `{ path }` | `bool` | `helpers.rs:172` |
| `list_conda_environments` | – | `Environment[]` | env handler |
| `open_url_in_window` | `{ url, title }` | `()` | helper (used for docs) |
| `create_default_backend_services` | – | `()` | `startup.rs:1432` (post-install seed) |

Events (Rust → JS):
- `process-output` `{ processId, output, timestamp, type }` — every captured stdout/stderr line + system messages.
- `backend-url-discovered` `{ id, url }` — debounced result of URL detection.
- `boolean-message` `{ message: "true" }` — vestigial signal that a backend was started.

---

## 12. TS-port translation table

| Concern | Tauri/Rust today | TS port equivalent |
|---|---|---|
| Process spawn | `std::process::Command` via `EnvSystem::new_command` | `child_process.spawn` (Node) / `Deno.Command` / `Bun.spawn` |
| Shell wrapper | `bash <script>` / `cmd /c <script>` | Same — write to `os.tmpdir()` and exec; or skip the script and use `{ shell: true, env, cwd }` plus `source conda.sh && conda activate <env> && <cmd>` as a single `-c` string |
| Stdin/out capture | `Stdio::piped()` + two `BufReader`/`lines()` threads | Pipe stdio + `readline.createInterface({ input: child.stdout })` per stream |
| Log buffer | `Arc<Mutex<HashMap<String, VecDeque<LogEntry>>>>` cap 10k | A `Map<string, RingBuffer>` in main process (a deque trimmed on push) |
| Event bus to renderer | `app_handle.emit("process-output", ...)` (Tauri) | Electron: `webContents.send` / `BrowserWindow.webContents.send`; Tauri-JS: keep as-is |
| Per-window scope | `WebviewUrl::App("/backend-logs?id=<id>")` + JS filter on `processId` | Same approach (URL param) — works in any multi-window framework |
| File lock on JSON | `fs2::FileExt::try_lock_exclusive` via `RealFileExtTrait` | `proper-lockfile` npm package, or lockfile sentinel; without it, expect data corruption under concurrent write |
| PID liveness check | `kill -0 <pid>` / `tasklist /FI "PID eq N" /NH` | Node: `process.kill(pid, 0)` throws if dead; Windows fallback to `tasklist` exec |
| Port-based kill (mac) | `lsof -ti tcp:<port>` + `kill -9` | Same shell out |
| Port-based kill (linux) | `fuser -k <port>/tcp` then `lsof -ti` | Same |
| Port-based kill (win) | `netstat -ano` parse, `taskkill /F /PID` | Same |
| Single-shot tray cleanup | `ctrlc::set_handler` + Tauri `RunEvent::ExitRequested` | Node: `process.on('SIGINT'/'SIGTERM'/'beforeExit')`; Electron: `app.on('before-quit')` |
| Conda exe lookup | `<install_dir>/conda/bin/conda` (Unix) / `Scripts/conda.exe` (Win) | Same, plus existence check |
| `.env` file parsing | hand-rolled in `load_env_file` (handles quotes + comments) | `dotenv` package (verify it strips matching surrounding quotes the same way) |
| Env merge order | env_file first, then `env_vars` (override) | Replicate exactly: build object, spread file → spread overrides |
| `UVICORN_*` → CLI flag | regex transform applied only when command contains `openbb-api` | Same condition; regex `/^UVICORN_(.+)$/`, build ` --<lower(rest)> "<value>"`, skip if flag already present |
| URL debounce | `std::thread::spawn` + `Mutex<Option<JoinHandle>>`; cancels by `unpark` (effectively replace) | `setTimeout` + clear; far simpler in JS |
| URL parsing | `url::Url::parse` → `host_str()` + `port()` | `new URL(s)` (note: `Url.port` is `""` if default 80/443) |
| ANSI strip | `regex::Regex::new(r"\x1B\[[0-9;]*[a-zA-Z]")` | `strip-ansi` npm or same regex |
| Self-signed cert | `openssl` crate, RSA-2048, X509 v3, PKCS#8 PEM key, PKCS#12 bundle | `node-forge` (pure JS, ships PKCS#12) or `node:crypto` X509 helpers; or shell out to `openssl` |
| Trust-store install | `certutil` (win/linux), `security add-trusted-cert` (mac) | Same shell-outs; no built-in API |
| Single-instance app | `tauri-plugin-single-instance` | `app.requestSingleInstanceLock()` (Electron) |
| Auto-start at login | per-OS modules under `utils/autostart/` | Electron `app.setLoginItemSettings` covers mac/win; Linux needs `~/.config/autostart/*.desktop` |

---

## 13. OS-level concerns the port MUST replicate

1. **Spawning bash with a generated activation script** — you can't just `spawn('conda', ['activate', ...])`; conda activation is a function injected by sourcing `conda.sh`. Either keep the temp-script pattern, or pass `{ shell: '/bin/bash' }` with `-c '. <conda>/etc/profile.d/conda.sh && conda activate <env> && <cmd>'`.
2. **Wrapper-PID vs server-PID divergence** — the spawn returns the bash/cmd PID. The actual server PID must be parsed from stdout (`Started server process [N]`) and persisted separately. Killing only the wrapper may leave the child alive, hence the port-kill fallback.
3. **Signal semantics** — Rust uses SIGKILL (`-9`) directly. On Node, `child.kill('SIGTERM')` first then SIGKILL after a timeout would be friendlier, but the current behavior is SIGKILL only.
4. **Port reclamation on stop** — must shell out to `lsof`/`fuser`/`netstat`+`taskkill` because the wrapper-only kill leaves uvicorn workers behind. The 2-second sleep after port-kill is load-bearing.
5. **Env-file value escaping** — `value.replace('\'', "'\\''")` is the canonical bash single-quote-escape; on Windows it becomes `set "K=V"`. A naive `${k}=${v}` will break on values containing quotes.
6. **File locking on `backends.json`** — required because multiple invoke handlers can race (e.g. log reader updates pid concurrently with user editing config). Without an exclusive lock, the JSON gets truncated/garbled.
7. **In-memory ring buffer + global event bus** — logs must be both broadcast (live tail) and replayed (window opened later). Need a process-wide singleton and a per-window backfill query.
8. **Multi-window with hide-on-close semantics** — log windows are hidden, not destroyed, so a re-open is instant. Listeners survive between hide/show.
9. **Conda env is location of `python`/`openbb-api`** — PATH must be `<conda>/bin:<conda>/condabin:$PATH` BEFORE the activate, and stale `CONDA_DEFAULT_ENV/CONDA_PREFIX/CONDA_SHLVL` MUST be unset to avoid leakage from the parent process.
10. **Dangerous-command sanitiser must run server-side** — client regex can be bypassed; the Rust side enforces it again (`command_sanitizer.rs:369-420`) and ALSO refuses to auto-start such backends.
11. **Auto-start delay (100 ms after Tauri setup, 500 ms between backends)** — empirically necessary to let state-management settle and avoid port hammering. Replicate or risk flakiness.
12. **URL-discovery debounce (1500 ms)** — multiple URL-bearing log lines (uvicorn prints the listen URL twice + docs URLs etc.); need to wait and pick the best, not just the first.
13. **Default seed backends** (`startup.rs:1432`): post-install, an "OpenBB API" and "OpenBB MCP" service are created with hardcoded commands and `auto_start: false`, environment `"openbb"`. Re-implement at the same lifecycle hook (post-conda-env-install).
14. **App-quit cleanup must be bounded** — 10 s wall-clock total, 3 s per subsystem. Without this, the app hangs on shutdown if a backend won't die.
15. **macOS-specific UI** — transparent title bar + black `NSWindow` background applied via objc2 in both `main.rs` and `open_backend_logs_window`. A TS port that uses Electron gets this with `vibrancy`/`titleBarStyle: 'hiddenInset'`.

---

## Cross-feature dependencies

- **depends-on** `feature-logs-streaming.md` — uses `LOG_STORAGE` singleton, `process-output` event channel, `register_process_monitoring`. The shell-wrapper PID divergence story is shared.
- **depends-on** `feature-installation.md` — `create_default_backend_services` is called at install completion. `<install_dir>` resolution comes from `system_settings.json` written during install.
- **depends-on** `feature-environments.md` — backends spawn inside a conda env; the conda exe path resolution comes from the install dir; `list_conda_environments` is called to populate the env dropdown.
- **depends-on** `feature-platform-rest-api.md` — the primary thing a user starts here is `openbb-api`. The `UVICORN_*` env-var translation only makes sense in the context of the Python server.
- **depended-on-by** `feature-tray-and-autostart.md` — tray's app-level autostart is the precondition for backend `auto_start: true` to be useful on system boot.
- **depended-on-by** `feature-uninstall.md` — uninstall calls `stop_all_backend_services` before removing files.
- **shares-state-with** `feature-api-keys.md` — backends inherit credentials from `user_settings.json` when they're `openbb-api` instances.
- **shares-state-with** `feature-logs-streaming.md` via the `process-output` event and `LOG_STORAGE` ring buffer.
