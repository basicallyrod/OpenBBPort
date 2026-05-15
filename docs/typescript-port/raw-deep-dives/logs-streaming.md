# Deep-Dive: Logs Pages + Process Monitor Infrastructure

> Raw findings from Wave 1 agent. Source of truth for `20-features/feature-logs-streaming.md` and
> shared with `feature-backend-services.md`, `feature-jupyter.md`, `feature-environments.md`.
> Generated 2026-05-15.

A complete data-flow trace of how stdout/stderr from spawned subprocesses (Jupyter, backend services, conda installs) reaches the separate Logs windows in real-time, plus a TS-port translation table.

---

## 1. Process-monitor infrastructure (the shared core)

**File: `/home/user/OpenBBPort/desktop/src-tauri/src/utils/process_monitor.rs`**

### 1.1 Core types (lines 7-26)
```
pub type LogStorage = Arc<Mutex<HashMap<String, LogBuffer>>>;
pub static LOG_STORAGE: Lazy<LogStorage> = Lazy::new(create_log_storage);  // line 9 - global singleton

pub struct LogEntry { timestamp: i64, content: String, process_id: String }   // line 16-20
pub struct LogBuffer { entries: VecDeque<LogEntry>, max_size: usize }         // line 23-26
```
- Storage is `HashMap<process_id_string, LogBuffer>` — keyed by a string, NOT a numeric PID. Keys are conventional: `jupyter-<envName>`, `backend-<uuid>`, plus ad-hoc IDs for conda installs.
- `LogBuffer.add()` (lines 36-41) is a strict ring buffer: `pop_front` once `len >= max_size`, then `push_back`. Default cap is **10,000 lines** per process (`process_monitor.rs:68`).

### 1.2 Storage API (lines 11-103)
- `get_log_storage()` → returns `Arc::clone` of the `LOG_STORAGE` singleton (line 11-13).
- `register_process(logs, process_id)` — inserts a new 10k LogBuffer if absent (line 65-73). Returns `false` if already there (idempotent, doesn't re-init).
- `unregister_process(logs, process_id)` — removes the buffer entirely (line 75-78).
- `clear_process_logs(logs, process_id)` — drains entries, keeps the buffer (line 80-88).
- `get_process_logs(logs, request)` — fetches all or last `count` entries (line 96-103). Tail-slicing logic at line 45-52 (`skip(len - n)`).

### 1.3 RunningProcesses (lines 106-209)
- Independent of `LogStorage`. `RunningProcesses(Arc<Mutex<HashMap<String, std::process::Child>>>)`.
- Methods: `add_process` (line 121-128, fails on duplicate name), `is_process_running` (line 131-154, calls `try_wait` and self-cleans dead entries), `kill_process` (line 157-172, removes-then-kills then waits), `cleanup_dead_processes` (line 181-208).
- Note: keys here are the **bare** name (e.g. backend UUID `id`, not `backend-<id>`). See `backends.rs:1165` (`processes.add_process(backend.id.clone(), child)`).

### 1.4 Wiring into Tauri (`main.rs`)
- `init_process_monitoring()` called early in `main()` (line 471).
- Two managed states (lines 489-490):
  ```
  .manage(ProcessLogState(get_log_storage()))   // wraps LogStorage
  .manage(RunningProcesses::new())              // child handles
  ```
- `ProcessLogState` is just `struct ProcessLogState(LogStorage)` (line 73).

### 1.5 Frontend-facing commands (`main.rs` lines 75-98)
| Command | Args | Returns | Purpose |
|---|---|---|---|
| `register_process_monitoring` | `process_id` | `bool` | Idempotent registration of a buffer |
| `unregister_process_monitoring` | `process_id` | `bool` | Drop buffer |
| `get_process_logs_history` | `process_id`, `count?` | `Vec<LogEntry>` | Backlog fetch |
| `clear_process_logs_history` | `process_id` | `bool` | Empty buffer |

---

## 2. Capture pipeline (line readers)

Each producer follows the same pattern: `Stdio::piped()` on stdout/stderr, then **two OS threads** each running `BufReader::lines()` in a `for line in reader.lines().map_while(Result::ok)` loop. Line buffering is OS-line-granular; binary chunks are split at `\n`.

### 2.1 Jupyter readers (`jupyter.rs:119-195`)
Per stream (stdout & stderr) inside `start_jupyter_server_impl`:
1. Spawn an `std::thread`.
2. For each line:
   - **Send to URL-detection mpsc channel** (`tx_sender.blocking_send(line.clone())`, line 132 / 171). The receiver runs in the spawning task and breaks once it finds the Jupyter URL.
   - **Write to LogStorage**: build `LogEntry { timestamp = chrono::Utc::now().timestamp_millis(), content = line, process_id = "jupyter-<env>" }` (lines 135-146).
   - **Emit `process-output` event** (lines 149-154).
- No ANSI cleaning is done in Rust for Jupyter; raw line is stored. (Cleaning happens in the React component.)

### 2.2 Backend readers (`backends.rs:977-1161`)
Closures + threads. The shared `log_processor` closure (line 977) does the work for both stdout and stderr.
- Strips a script-prefix noise: `line.replace("<script_path>: ", "")` (line 982).
- Builds the `LogEntry` and writes to LogStorage (lines 985-996).
- Emits `process-output` with an extra `"type": "stdout"|"stderr"|"system"` discriminator (lines 997-1005).
- Detects `: command not found`, cleans via `clean_error_message` (`backends.rs:178-194`), marks backend errored, kills it (lines 1008-1026).
- ANSI strip via `remove_ansi_escape_sequences` (line 1029, helper at `backends.rs:574-577`, regex `\x1B\[[0-9;]*[a-zA-Z]`).
- PID extraction from `Started server process [<n>]` (line 1030).
- URL extraction via `(https?:\/\/(?:localhost|\d{1,3}(?:\.\d{1,3}){3})(?::\d+)?(?:[^\s]*)?)` (line 1031).
- Debounced "best URL" selection (lines 1066-1117) — coalesces URLs over 1500ms, picks via `select_best_url` (line 580-637) prioritizing `/mcp`, `/sse`, `/docs`, `/openapi`, `/redoc`. Final URL is emitted on a separate event `backend-url-discovered` (line 1104).

### 2.3 Conda / environment readers (`environments.rs:36-105`)
`run_command_with_logging` mirrors the same pattern but uses a richer cleaner.
- `clean_output_line` (lines 14-34):
  - Strip ANSI: `\x1B\[[0-9;]*[a-zA-Z]`.
  - Backspace handling: `\x08` pops the previous char.
  - Carriage-return handling: `processed.rsplit('\r').find(|s| !s.trim().is_empty())` — keeps only the **last non-empty segment** of CR-overwritten lines (great for pip/conda progress bars).
  - Trims and drops fully-empty lines (only emitted if `!clean_line.is_empty()`).
- Emits `process-output` with `{ processId, output }` (no timestamp here, lines 60-66 / 84-89). The frontend uses event arrival time implicitly.
- Note: in this path the cleaner runs **server-side** before emit; in the Jupyter/backend paths cleaning is mostly client-side.

### 2.4 Backpressure
- `app_handle.emit(...)` is fire-and-forget; failures only log (`backends.rs:1003-1005`). There is no flow control.
- The OS thread reading the pipe will block on the pipe if the child outpaces the consumer, applying natural backpressure to the child (kernel pipe buffer, ~64KB on Linux).
- The 10,000-line ring buffer caps memory but **drops oldest first** silently.
- The frontend re-renders the entire `<div>` per state update; under burst load it will lag (no virtualization, see §5.4).

---

## 3. Event emission pipeline

### 3.1 Single shared event name
**`process-output`** is the one global Tauri event for ALL processes (Jupyter, backends, conda installs). Frontends discriminate by checking `payload.processId`.

### 3.2 Payload shapes (varies slightly by emitter)
- Jupyter (`jupyter.rs:149-154`): `{ processId: string, output: string, timestamp: number }`.
- Jupyter-stop completion (`jupyter.rs:476-482`): adds `"type": "system"`.
- Backend (`backends.rs:997-1002`): `{ processId, output, timestamp, type: "stdout"|"stderr"|"system" }`.
- Backend stop messages (e.g. `backends.rs:349-355`, 456-462, 494-500): `type: "system"` with emoji-prefixed strings (`"🎯 Killing all processes on port {port}"`, `"🛑 Stopping backend service '<name>'"`, `"💀 Terminating process PID {pid}"`).
- Environments / conda (`environments.rs:60-66`): `{ processId, output }` only — no timestamp.

### 3.3 Other related events
- `backend-url-discovered` (`backends.rs:1104-1110`) — payload `BackendUrlPayload { id, url }`.
- `jupyter-status-update` (`jupyter.rs:651-658`) — `{ environmentName, status }`. Manually emitted via the `update_jupyter_status` command (line 645).
- `boolean-message` (`backends.rs:1202-1209`) — generic `{ message: "true" }` after start succeeds.

### 3.4 Tauri event broadcast model
`app_handle.emit(name, payload)` broadcasts to **all** windows. Every Logs window listens for `process-output` and filters by `processId === expected`. There is NO per-process channel/event-name; one multiplexed stream + filter.

---

## 4. Storage details

- **In-memory only.** No disk persistence anywhere in `process_monitor.rs` or the handlers. If the app restarts, all log buffers are gone. (Tauri's own `tauri_plugin_log` writes to `Stdout`/`Stderr` only — `main.rs:485-487` — not to LogStorage.)
- **Per-process.** Each `process_id` gets its own `LogBuffer`.
- **Cap = 10,000 lines** (`process_monitor.rs:68`).
- **Ring policy:** drop-oldest on overflow.
- **Clearing** does not unregister (entries cleared, buffer kept) — `process_monitor.rs:80-88`.
- **Lookup:** `get_process_logs(storage, GetProcessLogsRequest { process_id, count: Option<usize> })`. `count = None` → all; `count = Some(n)` → last `n` (or all if n exceeds buffer).

---

## 5. Frontend subscription

### 5.1 Routing & deep-link
- Routes use TanStack Router file routes:
  - `/jupyter-logs?env=<envName>` — `routes/jupyter-logs.tsx:22-29` validates `env` into `environment`.
  - `/backend-logs?id=<backendId>` — `routes/backend-logs.tsx:18-25` validates `id`.
- Each route's wrapper toggles `document.body.classList` for `'jupyter-logs-view'` (used by `styles/jupyter-logs.css` to neuter horizontal scroll and force full-width). `routes/jupyter-logs.tsx:7-16` keeps localStorage on unmount so the main window can still detect shutdown signals.

### 5.2 Logs page lifecycle (`components/JupyterLogsPage.tsx` / `BackendLogsPage.tsx`)
The two components are near-identical (Backend is the trimmed version). For Jupyter (`JupyterLogsPage.tsx:153-275`):
1. Read URL param: `const search = useSearch({ from: '/jupyter-logs' }); const environmentName = search.environment` (lines 17-18).
2. Compute `processId = \`jupyter-${environmentName}\`` (line 162). Backend version: `\`backend-${backendId}\`` (`BackendLogsPage.tsx:157`).
3. Call `invoke("register_process_monitoring", { processId })` (line 166) — guarantees a buffer exists even if logs window opens before the producer.
4. Call `invoke<LogEntry[]>("get_process_logs_history", { processId })` for the backlog (line 176-178). Map and **clean each line** (`cleanLogContent`, line 285-288: strips `\[[0-9;]*m` and control chars `[\x00-\x1F\x7F-\x9F]`).
5. `listen<{processId, output, timestamp}>('process-output', event => ...)` (line 246). On each event:
   - Filter `if (eventProcessId === processId)` (line 249) — this is how the single channel is demultiplexed per window.
   - Append cleaned line to `logs` state.
   - For Jupyter only: scan for `"Shutting down on /api/shutdown request"` and call `notifyShutdown()` which posts to `window.opener` (`postMessage`) AND sets `localStorage[\`jupyter-shutdown-<env>\`]` (lines 213-240).
6. Cleanup: `unsubscribe.then(fn => fn())` (line 273).

### 5.3 Backlog vs live tail
There's a small race window: `get_process_logs_history` returns whatever's in the buffer at call-time, then the `listen` hook starts. Lines emitted between those two calls can either be missed or duplicated. The code does not de-dup; in practice the producer has been writing to the buffer all along, so the backlog already contains them, and any further events appended in transit will get re-listed.

> ⚠️ Known limitation: TS port needs idempotent merging by timestamp+content if you want strict no-dup.

### 5.4 UI rendering & search
- Logs render as one `<div>` per line in a flex column (`JupyterLogsPage.tsx:340-348`). No virtualization — long histories will be slow.
- Auto-scroll to bottom on each `logs` change unless `searchTerm` is set (`useEffect` lines 278-283).
- Search:
  - In-memory regex over `logs[].content`. Escapes special chars then `new RegExp(escaped, caseSensitive ? 'g' : 'gi')` (lines 36-39).
  - Builds `searchMatches: { logIndex, startIndex, endIndex }[]` (memoized lines 32-53).
  - `highlightSearchTerm` injects `<span class="search-highlight">` / `search-highlight-current` and renders via `dangerouslySetInnerHTML` (lines 56-78, 345-347).
  - Keyboard: Cmd/Ctrl+F opens, Esc closes, Enter→next, Shift+Enter→prev (lines 117-139).
  - `scrollToMatch` uses `querySelectorAll('[data-log-index]')` then `scrollIntoView({behavior:'smooth', block:'center'})` (lines 81-98).
- Color highlight CSS in `styles/jupyter-logs.css:76-84`: `#ffd90080` (yellow translucent) for non-current, `#ff8c00` (orange) for current. **No log-level coloring** (no INFO/WARN/ERROR styling — content is monochrome `font-mono text-xs whitespace-pre-wrap`).
- Clear button calls `clear_process_logs_history` and resets state, plus a `logsCleared` flag to suppress the "no logs" empty state (lines 296-308).

### 5.5 Filtering logic recap
**The frontend does not use a different event per process.** Single `process-output` listener, filter by `processId`. Any window can subscribe to any process's logs by knowing the ID.

---

## 6. Window management

### 6.1 Open
- Jupyter: `open_jupyter_logs_window(environment)` (`jupyter.rs:583-641`) — label `"jupyter-logs-<env>"`, URL `"/jupyter-logs?env=<env>"`, title `"Open Data Platform: Jupyter Logs - <env>"`, 1000×600, min 600×200, centered, resizable.
- Backend: `open_backend_logs_window(id)` (`backends.rs:1537-1605`) — label `"backend-logs-<id>"`, URL `"/backend-logs?id=<id>"`, title `"Open Data Platform: <backendName> Logs"` (looks up name via `load_backends_config`).
- Both: if `app_handle.get_webview_window(&label)` returns `Some`, just `show()` + `set_focus()` (no rebuild) — windows are reusable.
- macOS specifics: `TitleBarStyle::Transparent`, then sets NSWindow background color to black via `objc2_app_kit` (lines 632-638 / 1597-1602).

### 6.2 Close
- Both register `on_window_event(CloseRequested)` to **prevent close** and hide instead (`jupyter.rs:622-628`, `backends.rs:1587-1593`). Windows persist for the app session, so a "closed" log window reopens with full backlog (because the buffer is still in memory).

### 6.3 Lifecycle vs the underlying process
The window is **not** automatically destroyed when the underlying process dies. The buffer in `LOG_STORAGE` keeps the last 10k lines indefinitely (until app exit or explicit `unregister_process_monitoring`). On stop, the handlers append a final emoji-prefixed `"system"` entry (e.g. `jupyter.rs:459-482`) so the user sees the stop event in the logs window.

### 6.4 Frontend invokers
- `routes/environments.tsx:1984` — Jupyter logs button: `invoke("open_jupyter_logs_window", { environment: envName })`.
- `routes/backends.tsx:2414` — Backend logs button: `invoke("open_backend_logs_window", { id })`.

---

## 7. Jupyter specifics

### 7.1 Port allocation & token
The Rust side does NOT pick the port. It runs `jupyter lab --no-browser --notebook-dir <working>` (`jupyter.rs:75-85`) and lets Jupyter pick (default 8888 or next free). Same for the token — Jupyter generates it.

### 7.2 URL extraction
- A bounded `tokio::sync::mpsc::channel::<String>(100)` is plumbed through both reader threads (`jupyter.rs:116`).
- The async task awaits lines for up to **30 seconds** (line 198-230) and matches them against three regexes (`jupyter.rs:13-34`):
  1. `(https?://[^\s]+token=[^\s]+)` — preferred (URL with token).
  2. `(https?://(?:localhost|127\.0\.0\.1):[0-9]+[^\s]*)`.
  3. `(http://[^:\s]+:[0-9]+[^\s]*)`.
- Trailing punctuation `. , ) ] }` is stripped (line 28).
- Fallback heuristic at lines 213-226 looks for `http://...localhost...lab|8888`.
- On timeout without URL: kills the child and returns an error (line 247-249).

### 7.3 Active server tracking (`jupyter.rs:9-10`)
```
static ACTIVE_JUPYTER_SERVERS: Lazy<Mutex<HashMap<String, (String, u32)>>>;
                                                       // env -> (url, pid)
```
- Keyed by environment name. Stores `(jupyter_url, process_id_u32)`.
- `start_jupyter_server` short-circuits if env already present (returns existing URL with `already_running: true`, lines 47-60).

### 7.4 Stop semantics (`stop_jupyter_server_impl`, lines 291-485)
Critically, this does NOT use `RunningProcesses` (Jupyter is never put there — the `add_process` call doesn't happen for Jupyter). Instead:
1. Pop `(url, pid)` from `ACTIVE_JUPYTER_SERVERS`.
2. Extract port via `extract_port_from_url` (lines 496-531) — three regexes plus a colon-suffix fallback.
3. **Find the listening process by port and kill it**, NOT by the stored PID (because `conda run` spawns the actual Jupyter as a grandchild — the stored PID is the conda wrapper, not the server):
   - macOS/Linux: `lsof -ti tcp:<port> -sTCP:LISTEN` → SIGTERM, sleep 2s, check `kill -0`, then SIGKILL (lines 379-426). Fallback: `fuser -k <port>/tcp` (lines 432-451).
   - Windows: `netstat -ano | findstr :<port> | findstr LISTENING` → `taskkill /F /PID <pid>` (lines 327-374).
4. Append a `"✅ Jupyter server '<env>' stopped successfully"` entry to the buffer AND emit it as `process-output` with `type: "system"` (lines 459-482).

### 7.5 Cross-window shutdown notification
The logs window itself is the watcher: when it sees `"Shutting down on /api/shutdown request"` in any line (live or backlog), it notifies the main window via `window.opener.postMessage({type: 'jupyter-status-update', environmentName, status: 'stopped'}, '*')` plus a localStorage key `jupyter-shutdown-<env>` (`JupyterLogsPage.tsx:201-240`). The main window listens for both (`routes/environments.tsx:2089-2099`).

### 7.6 Process-ID conventions table
| Producer | LogStorage key | RunningProcesses key | Window label |
|---|---|---|---|
| Jupyter | `jupyter-<env>` | (none — port-kill instead) | `jupyter-logs-<env>` |
| Backend | `backend-<uuid>` | `<uuid>` | `backend-logs-<uuid>` |
| Env create / install | caller-supplied `process_id` | (none) | (no dedicated window) |

---

## 8. TS-port translation table

Target: re-implement the same UX in pure TypeScript (Node + Electron, or a Bun-server + browser, etc.).

| Rust / Tauri concept | File:line | TS equivalent |
|---|---|---|
| `LogStorage = Arc<Mutex<HashMap<String, LogBuffer>>>` | `process_monitor.rs:7-9` | `const logStorage = new Map<string, LogBuffer>()` (single-threaded JS — no mutex). Wrap in module-private singleton; export `getLogStorage()`. |
| `LogBuffer { entries: VecDeque<LogEntry>, max_size }` | `process_monitor.rs:23-58` | `class LogBuffer { entries: LogEntry[] = []; constructor(public maxSize = 10_000){}; add(e){ if(this.entries.length>=this.maxSize) this.entries.shift(); this.entries.push(e); } getLogs(n?){ return n && n<this.entries.length ? this.entries.slice(-n) : this.entries.slice(); } }` |
| `LogEntry { timestamp: i64, content: String, process_id: String }` | `process_monitor.rs:16-20` | `interface LogEntry { timestamp: number; content: string; processId: string }` (note camelCase to match wire payload). |
| `register_process` / `unregister_process` / `clear_process_logs` | `process_monitor.rs:65-88` | Same names, same idempotency. Return booleans. |
| `RunningProcesses(Arc<Mutex<HashMap<String, Child>>>)` | `process_monitor.rs:106-209` | `class RunningProcesses { private map = new Map<string, ChildProcess>(); add(name, child){...}; isRunning(name){ return this.map.get(name)?.exitCode == null }; kill(name){ const c=this.map.get(name); c?.kill('SIGKILL'); this.map.delete(name); return !!c }; cleanupDead(){...} }` Use `child_process.spawn` from Node. |
| `tauri::AppHandle.emit("process-output", payload)` | `jupyter.rs:154`, `backends.rs:1003` | A central `EventBus` (Node `EventEmitter`) PLUS a per-window WebSocket / `BroadcastChannel`. Two viable models: (a) **single multiplexed stream**: one WebSocket on `/ws/process-output`, server pushes `{processId, output, timestamp, type?}`, client filters by `processId` (matches current behaviour). (b) **per-process**: WebSocket `/ws/logs/:processId` — cleaner but more sockets. The current Tauri impl is (a). |
| Frontend `listen('process-output', cb)` | `JupyterLogsPage.tsx:246` | `ws.addEventListener('message', e => { const p = JSON.parse(e.data); if (p.event==='process-output' && p.processId===myId) ... })`. |
| `invoke("register_process_monitoring", {processId})` | `JupyterLogsPage.tsx:166` | `await fetch('/api/processes/register', {method:'POST', body: JSON.stringify({processId})})` or RPC method `processes.register({processId})`. |
| `invoke("get_process_logs_history", {processId})` | `JupyterLogsPage.tsx:176` | `await fetch(\`/api/processes/\${id}/logs\`).then(r=>r.json())` — returns `LogEntry[]`. **Important:** to fix the backlog/live race, also accept a `?since=<timestamp>` and have the WS reply include `since`-bookmark before live stream starts. |
| `invoke("clear_process_logs_history", {processId})` | `JupyterLogsPage.tsx:299-303` | `DELETE /api/processes/:id/logs`. |
| `invoke("open_jupyter_logs_window", {environment})` | `routes/environments.tsx:1984` | Electron: `new BrowserWindow({...}); win.loadURL('app://./jupyter-logs?env=...')` keyed by a label map, `if(map.has(label)) map.get(label).show()`. Browser-only port: `window.open('/jupyter-logs?env=...', \`jupyter-logs-\${env}\`, 'width=1000,height=600')` — the second arg's name reuses an existing window with the same name. |
| Window labels | `jupyter.rs:587`, `backends.rs:1542` | Same string convention; use as Electron window IDs / `window.open` name. |
| `WindowEvent::CloseRequested` → hide + prevent_close | `jupyter.rs:623-628` | Electron: `win.on('close', e => { e.preventDefault(); win.hide(); })`. Browser: tougher — closing a tab actually closes; you can prompt with `beforeunload` but cannot prevent. Workaround: just let it close and have the buffer survive in the parent so reopening re-fetches. |
| ANSI strip `\x1B\[[0-9;]*[a-zA-Z]` | `backends.rs:574-577`, `environments.rs:14-15` | Use [`strip-ansi`](https://www.npmjs.com/package/strip-ansi) or inline `s.replace(/\x1B\[[0-9;]*[a-zA-Z]/g,'')`. |
| Frontend cleaner `\[[0-9;]*m` + `[\x00-\x1F\x7F-\x9F]` | `JupyterLogsPage.tsx:285-288` | Same regex literal works in TS. Note this only catches SGR escapes (`m`-terminator), unlike the Rust one which catches all CSI. |
| Backspace + CR collapse | `environments.rs:18-33` | Same algorithm: iterate, drop char before `\x08`; `s.split('\r').filter(x=>x.trim()).pop()` to keep last segment of CR-overwritten line. |
| `tokio::mpsc::channel(100)` for URL detection | `jupyter.rs:116` | TS async generator + `AbortController` with timeout. Or a `Promise.race([urlPromise, sleep(30_000)])` that rejects on timeout. |
| Jupyter URL regex set | `jupyter.rs:14-19` | Port verbatim. |
| Port extraction regexes | `jupyter.rs:498-502` | Port verbatim plus the colon-rfind fallback (lines 514-528). |
| Port-based kill: `lsof -ti tcp:<port> -sTCP:LISTEN` → SIGTERM/SIGKILL or `netstat -ano \| findstr` → `taskkill /F` | `jupyter.rs:324-454` | Use Node `child_process.execFile` for `lsof`/`netstat`/`taskkill`. Or use a cross-platform package like [`fkill`](https://www.npmjs.com/package/fkill) that already does port→PID→kill. |
| `select_best_url` priority chain (`/mcp` > `/sse` > `/docs|openapi|redoc` > last) | `backends.rs:580-637` | Direct port — pure function. |
| Debounced URL coalescing (1500ms) | `backends.rs:1066-1117` | `debounce` from lodash, or a manual `setTimeout(..., 1500)` reset on each new URL line. |
| Global ring cap = 10_000 lines | `process_monitor.rs:68` | Same constant. Consider making it env-configurable. |
| Disk persistence | (none) | Optional addition: append-only log file per process under `~/.openbb-tslogs/<processId>.ndjson`. Not in current behaviour. |
| Log-level coloring | (not present) | Already absent — can add later by parsing `WARN`/`ERROR` etc. in the React component. |
| Single `process-output` event multiplexing | everywhere | If you choose per-process WS, the client gets a clean stream and you can drop the `if (eventProcessId === processId)` filter. If you keep multiplexed, mirror the filter. |
| In-memory only | by omission | Same — buffers vanish on restart. Document this. |
| Backpressure | (relies on OS pipe + dropped-oldest) | Same. For high-volume ports, consider batching emits (coalesce N lines or 50ms windows) before sending over WS to avoid React re-render storms. The current implementation has NO virtualization and NO batching, so a TS port can already match it; for a *better* port, use `react-window` or `@tanstack/react-virtual` over `logs[]`. |

### Key invariants to preserve in any port
1. **Process-ID convention strings** must remain stable: `jupyter-<env>` and `backend-<uuid>` — the frontend builds them client-side from URL params (`JupyterLogsPage.tsx:162`, `BackendLogsPage.tsx:157`). Change either side and you break the filter.
2. **Idempotent registration** — frontend calls `register_process_monitoring` even when the producer hasn't been started yet, so the buffer must exist before `get_process_logs_history` returns.
3. **System messages** are emitted as regular `process-output` events with `type:"system"` so the same renderer displays them — don't put them on a separate channel or you'll lose the stop banner in the logs view.
4. **Backlog-then-listen race** — if you fix nothing else, keep the same forgive-the-race behaviour; users won't notice a 1-line dup at the boundary, but they will notice missing lines if you reverse the order.
5. **Ring cap dropping silently** — UI never warns. Match that or add a "history truncated" indicator at top of buffer.

---

## Cross-feature dependencies

- **depended-on-by** `feature-backend-services.md` (uses `backend-<uuid>` namespace, `start_backend_service` registers and emits)
- **depended-on-by** `feature-jupyter.md` (uses `jupyter-<env>` namespace, `start_jupyter_server` registers and emits)
- **depended-on-by** `feature-environments.md` (conda installs use caller-supplied processIds and stream via the same `process-output` event)
- **depended-on-by** `feature-installation.md` (the Miniforge install pipeline emits via the same path)
- **shares-state-with** all spawning features via `LOG_STORAGE` singleton and `RunningProcesses` managed state
- **independent-of** `feature-api-keys.md` and `feature-platform-rest-api.md`

### Files referenced
- `/home/user/OpenBBPort/desktop/src-tauri/src/utils/process_monitor.rs`
- `/home/user/OpenBBPort/desktop/src-tauri/src/tauri_handlers/jupyter.rs`
- `/home/user/OpenBBPort/desktop/src-tauri/src/tauri_handlers/backends.rs`
- `/home/user/OpenBBPort/desktop/src-tauri/src/tauri_handlers/environments.rs`
- `/home/user/OpenBBPort/desktop/src-tauri/src/main.rs`
- `/home/user/OpenBBPort/desktop/src/routes/jupyter-logs.tsx`
- `/home/user/OpenBBPort/desktop/src/routes/backend-logs.tsx`
- `/home/user/OpenBBPort/desktop/src/components/JupyterLogsPage.tsx`
- `/home/user/OpenBBPort/desktop/src/components/BackendLogsPage.tsx`
- `/home/user/OpenBBPort/desktop/src/routes/environments.tsx` (window invokers + cross-window status)
- `/home/user/OpenBBPort/desktop/src/routes/backends.tsx` (window invoker + url-discovered listener)
- `/home/user/OpenBBPort/desktop/src/styles/jupyter-logs.css` (search highlight colors)
