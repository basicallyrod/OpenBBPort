# Feature: Logs Streaming (Process Monitor Infrastructure)

## Purpose
Provide the **shared subprocess-output plumbing** that every spawning feature
(Jupyter, backend services, environment installs) uses to surface real-time
stdout/stderr to the user. This doc covers the cross-feature core: the
`process-output` event channel, the `LogStorage` ring buffer, the dedicated logs
windows, and the `register_process_monitoring` / `get_process_logs_history` /
`clear_process_logs_history` invokes. Producer-specific lifecycle lives in
`feature-jupyter.md`, `feature-backend-services.md`, and
`feature-environments.md`; the **Miniforge installer** is **not** part of this
infrastructure (see Interfaces).

## User flows
1. **Live tail of a known process.** User opens an environment / backend, clicks
   "Logs". A dedicated Tauri window opens at `/jupyter-logs?env=<env>` or
   `/backend-logs?id=<uuid>`, calls `register_process_monitoring`,
   pulls backlog via `get_process_logs_history`, then `listen('process-output')`
   for live appends.
2. **Reopening a closed logs window.** Window labels are reused
   (`jupyter-logs-<env>`, `backend-logs-<uuid>`); a previously-hidden window is
   reshown with the full ring buffer still in memory.
3. **Search inside logs.** Cmd/Ctrl+F opens an in-page search bar with
   regex-escaped substring match, prev/next navigation, current-match
   highlight. Auto-scroll is suppressed while a search term is set.
4. **Clear logs.** Hover-only toolbar exposes a Clear button that calls
   `clear_process_logs_history` and resets local React state.
5. **Cross-window status (Jupyter only).** When the logs window observes
   `Shutting down on /api/shutdown request`, it writes
   `localStorage[jupyter-shutdown-<env>]` so the main environments window can
   flip status to "stopped".

## UI surface
- Routes: `desktop/src/routes/jupyter-logs.tsx:22-29` (validates `env`),
  `desktop/src/routes/backend-logs.tsx:18-25` (validates `id`).
- Components: `desktop/src/components/JupyterLogsPage.tsx:153-348` and
  `desktop/src/components/BackendLogsPage.tsx:155-260` (near-identical;
  Backend is the trimmed variant — no cross-window shutdown notifier).
- Toolbar: `logs-toolbar-container opacity-0 hover:opacity-100`
  (`JupyterLogsPage.tsx:293-308`) — Clear button is invisible until hover.
- Search highlight CSS: `desktop/src/styles/jupyter-logs.css:76-84`
  (`#ffd90080` non-current, `#ff8c00` current).
- No log-level coloring (monochrome `font-mono text-xs whitespace-pre-wrap`).
- No virtualization — one `<div data-log-index={i}>` per line
  (`JupyterLogsPage.tsx:340-348`).

## Data flow

```mermaid
sequenceDiagram
    participant Child as subprocess<br/>(jupyter / uvicorn / conda)
    participant Reader as OS thread<br/>BufReader::lines()
    participant Store as LogStorage<br/>(Arc&lt;Mutex&lt;HashMap&gt;&gt;)
    participant Tauri as app_handle.emit
    participant FE as Logs window<br/>(React)
    participant UI as &lt;div data-log-index&gt;

    Child->>Reader: stdout/stderr pipe (Stdio::piped)
    loop per line (\n-delimited)
        Reader->>Store: LogStorage.add(LogEntry{ts,content,pid})
        Reader->>Tauri: emit("process-output", {processId, output, ...})
        Tauri-->>FE: broadcast to ALL webviews
        FE->>FE: filter by payload.processId === myId
        FE->>UI: setLogs(prev => [...prev, cleaned])
    end

    Note over FE,Store: On mount:<br/>1. register_process_monitoring(processId)<br/>2. get_process_logs_history(processId) → backlog<br/>3. listen('process-output') → live tail<br/>RACE: 1-line dup possible at boundary
```

Pipeline summary per producer:
- `jupyter.rs:119-195` — two threads; writes `LogEntry` and emits
  `{processId, output, timestamp}`. Plumbs lines to an mpsc channel for URL
  detection in parallel.
- `backends.rs:977-1161` — same shape, emits `{processId, output, timestamp,
  type}`; in-line ANSI strip for PID/URL regex.
- `environments.rs:36-105` (`run_command_with_logging`) — pre-cleans
  ANSI/backspace/CR-overwrite server-side (`clean_output_line`,
  `:14-34`), emits `{processId, output}` only. **Does NOT call
  `LogStorage.add()`** — see Known Bugs (1).

## `process-output` payload variations
Single event name, three slightly different schemas:

| Producer | Source | Payload shape |
|---|---|---|
| Jupyter | `jupyter.rs:149-154`, stop at `:476-482` | `{processId, output, timestamp}` (+`type:"system"` on stop) |
| Backend | `backends.rs:997-1002` (+ `:338-355, :444-462, :483-500`) | `{processId, output, timestamp, type:"stdout"\|"stderr"\|"system"}` |
| Environments / conda | `environments.rs:60-66, :82-89` | `{processId, output}` — **no timestamp, no type** |

> ⚠️ BUG: every renderer (`JupyterLogsPage.tsx:246`, `BackendLogsPage.tsx:175`,
> `backends.tsx:665`) destructures `{processId, output, timestamp}` and discards
> `type`. The discriminator is dead bytes on the wire — system stop banners are
> distinguished only by emoji prefix (`🎯 🛑 💀 ✅`).

## Process-ID naming conventions

| Producer | LogStorage key | RunningProcesses key | Window label | Built at |
|---|---|---|---|---|
| Jupyter | `jupyter-<env>` | (none — port-kill) | `jupyter-logs-<env>` | `jupyter.rs:108`, `JupyterLogsPage.tsx:162` |
| Backend | `backend-<uuidV4>` | `<uuidV4>` (bare) | `backend-logs-<uuidV4>` | `backends.rs:966`, `BackendLogsPage.tsx:157` |
| Env create from form | `create-env-<env>-<ms>` | (none) | (no window) | `environments.tsx:1518` |
| Env requirements install | `requirements-<env>-<ms>` | (none) | (no window) | `environments.tsx:595` |
| Miniforge install | n/a — uses `install-progress` event | n/a | n/a | `startup.rs:477,517` |
| Extension install | n/a — buffered `.output()`, no streaming | n/a | n/a | `environments.rs:2431-2443` |

Prefix-namespacing rules out collisions across producers. Asymmetry to watch:
`RunningProcesses` uses the **bare** UUID, `LogStorage` uses `backend-<uuid>`
— wrap in `processIdForLogs(uuid)` / `processIdForKill(uuid)` helpers in the
port.

## IPC contract

| Direction | Name | Payload | Returns | Used by |
|---|---|---|---|---|
| invoke (FE→BE) | `register_process_monitoring` | `{processId: string}` | `bool` (idempotent) | every logs page on mount |
| invoke (FE→BE) | `unregister_process_monitoring` | `{processId: string}` | `bool` | **nobody — dead code** (see bugs) |
| invoke (FE→BE) | `get_process_logs_history` | `{processId: string, count?: number}` | `LogEntry[]` | every logs page on mount |
| invoke (FE→BE) | `clear_process_logs_history` | `{processId: string}` | `bool` | Clear button |
| invoke (FE→BE) | `open_jupyter_logs_window` | `{environment: string}` | `()` | `routes/environments.tsx:1984` |
| invoke (FE→BE) | `open_backend_logs_window` | `{id: string}` | `()` | `routes/backends.tsx:2414` |
| event (BE→FE) | `process-output` | see payload variations above | n/a | Logs windows + main window + per-row monitor (3+ listeners per emit) |

Command registrations: `main.rs:75-98, :471-490`. Storage singleton:
`process_monitor.rs:7-9` (`LOG_STORAGE: Lazy<Arc<Mutex<HashMap<String,
LogBuffer>>>>`).

## State surfaces
- **React state** (per logs window): `logs: LogEntry[]`, `searchTerm`,
  `searchMatches`, `currentMatchIndex`, `logsCleared` (empty-state suppression
  flag, transient — any incoming event resets it).
- **Rust state**:
  - `LOG_STORAGE` (global `Lazy`, `process_monitor.rs:9`) — wrapped in
    `ProcessLogState` managed state (`main.rs:489`).
  - `RunningProcesses` (`process_monitor.rs:106-209`) — independent of
    LogStorage; only Jupyter does NOT register here (port-kill instead).
- **Disk files**: none. **In-memory only**; buffers vanish on app restart.
  Tauri's `tauri_plugin_log` (`main.rs:485-487`) writes to stdout/stderr, not
  LogStorage.
- **localStorage** (Jupyter cross-window only): `jupyter-shutdown-<env>` keys
  with a `Date.now()` value; read by `routes/environments.tsx:2107-2153` with a
  60s freshness window.

## Persistence
None. The 10,000-line ring buffer per process is the only retention layer:
- Default cap `process_monitor.rs:68` — hardcoded `LogBuffer::new(10000)`.
- Ring policy: `pop_front` when `len >= max_size`, then `push_back`
  (`process_monitor.rs:36-41`). Silent drop-oldest — no UI indicator.
- `clear_process_logs` drains entries but keeps the buffer
  (`process_monitor.rs:80-88`).
- `unregister_process` removes the buffer entirely (`process_monitor.rs:75-78`)
  — but nothing calls this from the frontend.

## Error handling
- `app_handle.emit(...)` is fire-and-forget; failures only log
  (`backends.rs:1003-1005`). No flow control.
- The OS thread reading the pipe applies natural backpressure to the child via
  the kernel pipe buffer (~64KB on Linux) if React falls behind.
- 10k cap silently drops oldest. **No "history truncated" banner**
  (`JupyterLogsPage.tsx:336-340`, `BackendLogsPage.tsx:244-248`).
- Backlog/live race: `get_process_logs_history` resolves, then `listen` attaches.
  Lines emitted in the gap *can* be re-listed (1-line dup at boundary). The code
  doesn't de-dup. For producers that don't write to the buffer (env-create,
  requirements — see bugs item 1), the backlog is always `[]` and any lines
  emitted before `listen` attaches are **lost**.
- `clear_process_logs_history` race: events landing between `await invoke(...)`
  and `setLogs([])` are benign (they append to the cleared state). The
  `logsCleared` flag toggles back to `false` on any new event
  (`JupyterLogsPage.tsx:250`).

## ▸ Interfaces with
- **depended-on-by** `feature-backend-services.md` — uses `backend-<uuid>`
  namespace; `start_backend_service` registers and emits via this channel.
  Stop-banner messages (`🎯 🛑 💀`) flow through `process-output` with
  `type:"system"`.
- **depended-on-by** `feature-jupyter.md` — uses `jupyter-<env>` namespace;
  `start_jupyter_server` registers and emits. The logs window doubles as the
  shutdown-detector via the `Shutting down on /api/shutdown request` regex →
  `localStorage` write.
- **depended-on-by** `feature-environments.md` — uses
  `create-env-<env>-<ts>` and `requirements-<env>-<ts>` IDs. **Emits only**
  (no LogStorage writes — see bugs item 1). No dedicated logs window; output
  surfaces inline in the environments page.
- **NOT used by** `feature-installation.md` — the Miniforge installer uses a
  **separate** `install-progress` event (`startup.rs:477,517`) with a
  `{step, progress, message}` payload and zero LogStorage interaction. This is
  the dividing line between "subprocess log infra" and "installer progress
  UX": don't conflate the two in the port.
- **shares-state-with** every spawning feature via the global `LOG_STORAGE`
  singleton (`Arc<Mutex<HashMap>>`) and the `RunningProcesses` managed state.

## TS port mapping

| Tauri concept | TS equivalent | Notes |
|---|---|---|
| `LogStorage = Arc<Mutex<HashMap<String, LogBuffer>>>` | `const logStorage = new Map<string, LogBuffer>()` | Single-threaded JS — no mutex. Module-private singleton. |
| `LogBuffer { entries: VecDeque, max_size }` | `class LogBuffer { entries: LogEntry[] = []; add(e){ if(this.entries.length >= this.maxSize) this.entries.shift(); this.entries.push(e); } }` | `Array.shift()` is O(n); for 10k cap it's fine. Use a circular array if profiling demands. |
| `LogEntry { timestamp:i64, content:String, process_id:String }` | `interface LogEntry { timestamp:number; content:string; processId:string }` | camelCase to match wire payload. |
| `app_handle.emit("process-output", payload)` | **Multiplexed WebSocket** `/ws/process-output` OR Electron `webContents.send("process-output", ...)` broadcast to all `BrowserWindow`s | Mirrors Tauri's all-windows broadcast. Per-process WS (`/ws/logs/:id`) is cleaner but more sockets — see open questions. |
| `listen('process-output', cb)` filtered by `processId` | `ws.onmessage = e => { const p = JSON.parse(e.data); if (p.processId === myId) ... }` OR `ipcRenderer.on('process-output', (_,p)=>...)` | Filter is mandatory if multiplexed. |
| `register_process_monitoring` / `get_process_logs_history` / `clear_process_logs_history` | `POST /processes/register` / `GET /processes/:id/logs?since=<ts>` / `DELETE /processes/:id/logs` (or RPC methods) | Add `?since=<ts>` to fix the backlog/live race deterministically. |
| `open_*_logs_window` | Electron `new BrowserWindow({...})` keyed by label map; `if(map.has(label)) map.get(label).show()` | Browser-only: `window.open(url, label, ...)` (second arg reuses window). |
| `WindowEvent::CloseRequested` → hide-instead-of-close | Electron: `win.on('close', e => { e.preventDefault(); win.hide(); })` | Browser: cannot prevent tab close; rely on parent-side buffer survival. |
| 10k DOM nodes per window | **`react-window` or `@tanstack/react-virtual`** — not optional | Hidden windows with 10k `<div>`s × N processes resident is the worst trap. |
| ANSI strip (Rust: `\x1B\[[0-9;]*[a-zA-Z]`) | [`strip-ansi`](https://www.npmjs.com/package/strip-ansi) npm package | Use **one** regex everywhere — see bugs item 4. |
| Backspace + CR-overwrite collapse | Port `clean_output_line` verbatim | `s.split('\r').filter(x=>x.trim()).pop()` for progress bars. |
| Frontend cleaner `[\x00-\x1F\x7F-\x9F]` | Replace with HTML-escape + safe JSX render | Fixes the XSS surface (bugs item 2). |

## Known bugs and port-time fixes
1. **Conda streams never touch LogStorage.** `run_command_with_logging`
   (`environments.rs:53-95`) emits but does not call `LogStorage.add()`.
   `register_process` at `environments.rs:131, 1017, 1192` creates an empty
   buffer that stays empty. Consequences: `get_process_logs_history` returns
   `[]` for `create-env-*` / `requirements-*`; any line emitted before a
   listener attaches is **lost**. Port fix: write to the buffer in the same
   thread that emits.
2. **XSS via `dangerouslySetInnerHTML`.** Log content is injected into
   innerHTML without HTML escaping (`JupyterLogsPage.tsx:345-347`,
   `BackendLogsPage.tsx:253-255`). A subprocess printing
   `<img src=x onerror=...>` runs in the logs window's DOM context.
   Port fix: render via JSX (auto-escapes) with `<span class="...">` for
   highlights instead of `dangerouslySetInnerHTML`.
3. **`unregister_process_monitoring` has zero callers.** Buffers accumulate
   monotonically — stale `jupyter-<env>` and `backend-<uuid>` entries persist
   after env/backend deletion. ~24MB worst case for 30 long-lived processes.
   Port fix: hook unregister into `delete_backend_service`, `remove_environment`,
   and app shutdown.
4. **Three different ANSI strippers with different regexes.**
   - Rust `\x1B\[[0-9;]*[a-zA-Z]` (full CSI) — `backends.rs:574-577`.
   - Frontend per-row `cleanAnsiCodes`: `\[[0-9;]*m` (SGR only) —
     `backends.tsx:619`.
   - Frontend `cleanLogContent`: `\[[0-9;]*m` + `[\x00-\x1F\x7F-\x9F]` (SGR +
     C0/C1 controls, which **includes `\n` 0x0A and `\r` 0x0D**) —
     `JupyterLogsPage.tsx:287`, `BackendLogsPage.tsx:195`.
   Cursor-control escapes (e.g. `\x1B[2K` from pip progress) survive the
   per-row monitor and could break the `Started server process` substring
   detection. Port fix: one canonical stripper used everywhere.
5. **`window.opener` is always null in Tauri.** The `postMessage` branch at
   `JupyterLogsPage.tsx:217-224` is dead code — webviews built via
   `WebviewWindowBuilder` have no JS opener relationship. Only the
   `localStorage` write (line 229) fires in production. Port fix: replace with
   a typed Tauri event (`app_handle.emit("jupyter-stopped", env)`) or
   `BroadcastChannel` in a browser port.
6. **`type` discriminator is destructured away.** Every consumer ignores it
   (`JupyterLogsPage.tsx:246`, `BackendLogsPage.tsx:175`, `backends.tsx:665`).
   System messages render identically to stdout/stderr; emoji prefix is the
   only visual cue. Port fix: either drop the field, or actually render system
   lines with a divider / dim color.
7. **Unconditional auto-scroll.** `JupyterLogsPage.tsx:278-283` snaps to
   bottom on every `logs` mutation when there's no search term — no "user
   scrolled up" detection. Port fix: classic `tail -f` pattern: suppress
   auto-scroll when `scrollTop + clientHeight < scrollHeight - 50`; show a
   floating "Jump to latest (N new)" button when not at bottom.
8. **No history-truncated indicator.** When the 10k ring wraps, the UI is
   silent. Port fix: emit a one-shot `process-output-truncated` event the
   first time `pop_front` fires per process, render a banner at the top of
   the buffer.
9. **Per-window memory** can balloon: 10k DOM nodes × N hidden logs windows
   stay resident (hide-on-close). Port fix: virtualization is required, not
   optional.
10. **Clock-change brittleness** in the Jupyter shutdown notifier
    (`environments.tsx:2137-2153` — 60s freshness check on
    `localStorage[jupyter-shutdown-<env>]`). System clock jumps break the
    check. Port fix: replace localStorage-IPC with `BroadcastChannel` or a
    backend-provided monotonic timestamp.

## Open questions
- **Per-process WebSocket vs multiplexed?** Current Tauri impl is multiplexed
  (one `process-output` event, client-side filter). A per-process WS
  (`/ws/logs/:processId`) is cleaner — no filter, simpler typing — at the cost
  of more sockets and reconnection complexity. Decide before locking the
  client API.
- **Persist logs to disk?** Today everything is RAM-only; a crash loses
  history. Consider an append-only NDJSON per process under
  `~/.openbb-tslogs/<processId>.ndjson` with a rotation policy. Trade-off:
  disk IO on hot loops (uvicorn) and a cleanup story on env/backend delete.
- **Make `MAX_BUFFER_LINES` configurable?** Today hard-coded `10000`
  (`process_monitor.rs:68`). Surface as a constant + env override, especially
  if disk persistence isn't added.
- **Batch emits for burst loads?** The current path emits one event per line.
  Coalescing N lines or a 50ms window before sending over WS would smooth out
  React re-render storms — but adds latency.
- **Should env-creation flows have dedicated logs windows?** Today they don't
  (output is inline). If retained, fix the LogStorage gap (bug 1); if not,
  drop the unused `register_process_monitoring` calls.

## Cross-feature dependencies
- **depended-on-by:** `feature-backend-services.md` (`backend-<uuid>`),
  `feature-jupyter.md` (`jupyter-<env>`), `feature-environments.md`
  (`create-env-*`, `requirements-*` — emit only).
- **independent-of:** `feature-installation.md` (uses `install-progress`
  event — **dividing line**), `feature-api-keys.md`, `feature-platform-rest-api.md`.
- **shares-state-with:** every spawning feature via `LOG_STORAGE` singleton
  and `RunningProcesses` (`process_monitor.rs:9,:106`).

### Files referenced
- `desktop/src-tauri/src/utils/process_monitor.rs`, `main.rs`
- `desktop/src-tauri/src/tauri_handlers/{jupyter,backends,environments,startup}.rs`
- `desktop/src/routes/{jupyter-logs,backend-logs,environments,backends}.tsx`
- `desktop/src/components/{JupyterLogsPage,BackendLogsPage}.tsx`
- `desktop/src/styles/jupyter-logs.css`
