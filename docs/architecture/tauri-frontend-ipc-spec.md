# Tauri ↔ TS IPC — Spec Sheet

One-page reference. For full prose, signatures, and diagrams see
[`tauri-frontend-ipc.md`](./tauri-frontend-ipc.md).

---

## At a Glance

| Metric | Value |
|---|---|
| Tauri version | **2.10.3** |
| Rust edition / toolchain | 2024 / 1.90.0 |
| Frontend | React 18.3.1 + TS, Vite 7, TanStack Router 1.131 |
| Rust commands exposed | **56** (`tauri::generate_handler![…]` in `main.rs`) |
| Rust handler modules | **6** + `main.rs` |
| Distinct Rust→TS event names | **5** (+1 dormant listener) |
| Tauri plugins (Rust) | **8** |
| Tauri plugins (JS) | **10** |
| Type-codegen layer | **None** (manual TS `interface` per route) |
| Test DI strategy | trait-based + `mockall = 0.14` |

---

## Stack

| Layer | Tech |
|---|---|
| Shell | Tauri 2.10.3 (`tray-icon`, `devtools` features) |
| IPC | `@tauri-apps/api/core` `invoke` + `@tauri-apps/api/event` `listen` |
| Frontend | React 18.3.1, Vite 7, TanStack Router |
| Forms | `react-hook-form` 7.62 + Zod |
| UI kit | `@openbb/ui-pro` |
| Backend | Rust 1.90, Tokio, serde, mockall |

---

## Commands by Module

| Module | Path | Cmds |
|---|---|---:|
| Startup | `tauri_handlers/startup.rs` | 6 |
| Environments | `tauri_handlers/environments.rs` | 12 |
| Jupyter | `tauri_handlers/jupyter.rs` | 7 |
| Backends | `tauri_handlers/backends.rs` | 7 |
| Credentials | `tauri_handlers/credentials.rs` | 3 |
| Helpers | `tauri_handlers/helpers.rs` | 13 |
| Lifecycle | `main.rs` | 9 (incl. uninstall, quit, process monitoring) |
| **Total** | | **57**¹ |

¹ Registry shows 56 unique invocations; one helper (`create_default_backend_services`) is called from Rust setup and not from JS.

---

## IPC Mechanisms

| Pattern | Direction | API |
|---|---|---|
| Request / response | JS → Rust → JS | `invoke<T>("cmd", {…}) : Promise<T>` |
| Event stream | Rust → JS | `app_handle.emit("name", payload)` ↔ `listen<P>("name", cb)` |
| Hybrid | both | `invoke` kicks off, `listen` receives progress (install, jupyter start, backend start) |

Error convention: every command returns `Result<T, String>`; the
`String` becomes the promise rejection.

---

## Events

| Name | Payload | Emitter file | Frontend listener |
|---|---|---|---|
| `install-progress` | `{ step, message, percent? }` | `startup.rs` | `installation-progress.tsx` |
| `installation-directory` | `string` | `startup.rs` | `index.tsx` |
| `process-output` | `{ processId, output, timestamp? }` | `jupyter.rs`, `backends.rs`, `environments.rs` | `JupyterLogsPage`, `BackendLogsPage`, env/backend routes |
| `jupyter-status-update` | `{ environment, status, url? }` | `jupyter.rs` | `environments.tsx`, `backends.tsx` |
| `uninstall_progress` | `string` | `uninstall.rs` | `uninstall.tsx` |
| `installation-status` ⚠ | `boolean` | **none (dormant)** | `index.tsx`, test mocks |

---

## State Containers (Rust)

| Container | Mechanism | Owner |
|---|---|---|
| `INSTALLATION_STATE` | `Lazy<Mutex<…>>` | `main.rs` |
| `ACTIVE_JUPYTER_SERVERS` | `Lazy<Mutex<HashMap<…>>>` | `jupyter.rs` |
| `RunningProcesses` | Tauri `.manage(…)` | `main.rs` |
| `ProcessLogState` | Tauri `.manage(…)` (ring buffer) | `utils/process_monitor.rs` |

Frontend: per-route `useState`, TanStack search params, `localStorage`
caches (`env-extensions-cache`, `environments-first-load-done`).

---

## Plugins

**Rust (`Cargo.toml`):** `log`, `shell`, `dialog`, `fs`,
`persisted-scope`, `opener`, `single-instance`†, `updater`†.
† non-mobile only.

**JS (`package.json`):** `@tauri-apps/plugin-{app, dialog, fs, http,
log, opener, process, updater, shell, window}`.

**Vestigial:** `taurpc` listed in JS deps but **not imported anywhere**
in `desktop/src`.

---

## Capabilities

`tauri.conf.json` has `app.security.capabilities: []` — permissions
are loaded from `desktop/src-tauri/capabilities/` instead.

| File | Scope | Key permissions |
|---|---|---|
| `default.json` | all windows | core, dialog, opener, shell (open), fs (read/write), log |
| `desktop.json` | macOS / win / linux | adds `shell:allow-execute`, `shell:allow-spawn`, `opener:allow-open-path` with `path: "**"`, updater |

CSP: `null` (eval enabled for `navigate_to_page`-style routing).

---

## Type Bridge

| Side | How types are declared |
|---|---|
| Rust | `#[derive(Serialize, Deserialize)]` on structs (`BackendService`, `CondaEnvironment`, `LogEntry`, `InstallationState`) |
| TS | Hand-written `interface` declared inline in the route that owns the screen |
| Validation | Zod, **only** on user input before `invoke()` — not on responses |

**Drift risk:** no compile-time enforcement; reviewers must grep both
sides when changing a shared struct. No `ts-rs` / `specta` /
`taurpc`-wired codegen.

---

## Window / Build Config

| Key | Value |
|---|---|
| Bundle ID | `co.openbb.platform` |
| Product | "Open Data Platform by OpenBB" |
| Window starts | `visible: false` (Rust setup shows after install check) |
| Title bar | Transparent + Mica/vibrancy |
| Dev URL | `http://localhost:1470` |
| Frontend dist | `../dist` |
| Auto-updater | enabled, endpoint = GitHub `latest.json` |
| Platform overrides | `tauri.{linux,macos,windows}.conf.json` |

---

## Testability

| Side | Mechanism |
|---|---|
| Rust | Generic `*_impl<F, E, …>` functions over `FileSystem` / `EnvSystem` / `FileExtTrait`; `#[tauri::command]` wrapper injects `Real*` impls. Tests inject `mockall::automock` mocks. |
| TS | `vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))` |

---

## Quick Reference — Key Paths

```
desktop/
├── src-tauri/
│   ├── src/main.rs                        # 56-command registry, state, lifecycle
│   ├── src/tauri_handlers/{startup,environments,jupyter,backends,credentials,helpers}.rs
│   ├── src/uninstall.rs                   # emits uninstall_progress
│   ├── src/utils/process_monitor.rs       # log buffer + process-output helper
│   ├── tauri.conf.json (+ .{linux,macos,windows} variants)
│   ├── capabilities/{default,desktop}.json
│   └── Cargo.toml
└── src/
    ├── routes/setup.tsx                   # invoke + Zod entry flow
    ├── routes/installation-progress.tsx   # install-progress listener
    ├── routes/environments.tsx            # hybrid invoke + stream
    ├── routes/backends.tsx                # backend CRUD + stream
    ├── routes/api-keys.tsx                # credentials
    ├── routes/uninstall.tsx               # uninstall_progress listener
    ├── routes/index.tsx                   # installation-{status,directory}
    ├── components/JupyterLogsPage.tsx     # process-output viewer
    └── components/BackendLogsPage.tsx     # process-output viewer
```
