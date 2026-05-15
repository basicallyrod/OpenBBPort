# tauri-shell — Spec Sheet for Parallel Build

This document is the **master spec** for building out the remaining work on
`tauri-shell/`. It is designed so multiple agents can pick up independent
work slices in parallel without conflict.

> **If you are an agent picking up work here:** read §1 first, find your
> assigned slice in §2, then follow the conventions in §3. Each slice is
> independent — agents can run concurrently without touching each other's
> files.

---

## 1. Project state (read this first)

### What's already done

- **Foundation** (39 files, 5343 lines, `cargo check` + `cargo build --release` pass):
  - `Cargo.toml`, `tauri.conf.json`, `build.rs`, `capabilities/`, `icons/` (placeholder)
  - All 8 Tauri 2.x plugins wired (updater, opener, single-instance, fs, shell, persisted-scope, log, dialog)
  - `state.rs`: `LogStorage` (10k ring), `RunningProcesses`, `InstallationState`, `CancellationRegistry`, `ShutdownHook` trait
  - `process_monitor.rs` + `process_spawn.rs` + `process_kill.rs`: full subprocess lifecycle
  - `cleanup.rs`: bounded cascade (10s outer / 3s per-subsystem)
  - `tray.rs`: 9-item system tray with configurable nav
  - `autostart/`: per-OS impls (macOS osascript / Windows COM `.lnk` / Linux `.desktop`)
  - `updater.rs`: Tauri updater wrapper
  - `windows.rs`: close-to-tray, log-window helper, external-URL window
  - `events.rs`: 8 typed event payloads
  - `path_utils.rs`: settings-dir resolution
  - `settings.rs`: atomic JSON writes (`*.tmp` + flock + `chmod 0600` + rename)
  - `proxy.rs`: HTTP client for the Python REST server

- **162 IPC commands** registered in `tauri::generate_handler!`:

| Module                | Count | Status                              |
|-----------------------|-------|-------------------------------------|
| `ipc::infrastructure` | 4     | ✅ Real (LogStorage register/get/clear) |
| `ipc::app`            | 5     | ✅ Real (nav, quit, version, theme stub) |
| `ipc::helpers`        | 11    | ✅ Mostly real (dir/file pickers, opens) |
| `ipc::credentials`    | 3     | ✅ Real (atomic write, strict allow-list) |
| `ipc::installation`   | 9     | 🪝 Stubs (connector wires the installer) |
| `ipc::environments`   | 11    | 🪝 Stubs (connector wires conda/uv) |
| `ipc::backends`       | 7     | 🪝 Stubs (1 real: open_logs_window) |
| `ipc::jupyter`        | 6     | 🪝 Stubs (2 real: window opener + status emit) |
| `ipc::uninstall`      | 1     | 🪝 Stub |
| `ipc::certs`          | 1     | 🪝 Stub |
| `ipc::obb`            | 16    | ✅ Real (generic REST proxy) |
| `ipc::obb_routes`     | 60    | ✅ Real (typed wrappers over proxy) |
| `ipc::openbb_meta`    | 3     | ✅ Real (route introspection from /openapi.json) |
| `ipc::provider`       | 4     | ✅ Real (provider catalog + validation) |
| `ipc::settings_files` | 5     | ✅ Real (5-file allow-list, atomic writes) |
| `ipc::server`         | 6     | 🪝 Stubs (server_attach + server_health real) |
| `ipc::mcp`            | 5     | 🪝 Stubs |
| `ipc::routines`       | 5     | ✅ Real (.openbb file CRUD) |

✅ = production-ready implementation in the shell
🪝 = stub returning `Err(NotImplemented)` with a `// TODO: connect to your backend` comment

### What's still missing

The 8 work slices below cover all remaining work. Each is independent.

---

## 2. Work slices

### Slice A — TS type bindings (highest priority)

**Goal:** Auto-generate TypeScript types for every IPC command's args + return
type. Eliminates the stringly-typed `invoke<T>("name", args)` problem.

**Approach:** Add the [`ts-rs`](https://crates.io/crates/ts-rs) crate (v9+) as
a dev-dependency. Decorate every public struct in `src/state.rs`,
`src/events.rs`, and every `ipc/*.rs` module with `#[derive(TS)]` and
`#[ts(export, export_to = "../bindings/")]`. Add a `cargo test --features
ts-rs-export` target that runs the ts-rs export tests.

**Files to create/modify:**
- `Cargo.toml`: add `ts-rs = { version = "9", features = ["serde-json-impl", "chrono-impl"] }` under `[dev-dependencies]`, behind a feature flag `bindings`
- `bindings/`: new directory at repo root (gitignored except for `README.md`)
- All public structs in: `state.rs`, `events.rs`, every `ipc/*.rs` module
- `tests/bindings.rs`: a test file that wraps every exportable type

**Deliverable:** Running `cargo test --features bindings` produces a `bindings/` dir with one `.ts` file per exported type, plus an `index.ts` that re-exports all. The TS frontend can then `import { ProcessOutputEvent } from "tauri-shell/bindings"`.

**Verification:** `bindings/index.ts` exists, contains at least 30 type exports, and `tsc --noEmit bindings/index.ts` succeeds.

---

### Slice B — Connector trait abstraction

**Goal:** Replace the scattered `// TODO: connect to your backend` comments
with a single `Connector` trait object. Domain handlers delegate to the trait,
the binary picks an impl at startup.

**Approach:** Define `pub trait Connector: Send + Sync + 'static` in
`src/connector.rs` with one method per domain command family:

```rust
#[async_trait::async_trait]
pub trait Connector: Send + Sync + 'static {
    // Installation
    async fn install_to_directory(&self, args: InstallArgs) -> Result<bool, ConnectorError>;
    async fn install_runtime(&self, args: InstallRuntimeArgs, app: AppHandle) -> Result<bool, ConnectorError>;
    async fn setup_environment(&self, args: SetupEnvArgs, app: AppHandle) -> Result<bool, ConnectorError>;
    // ... ~35 methods total

    // Defaults: every method returns Err(ConnectorError::NotImplemented)
}
```

Provide:
- `NoopConnector` — every method `Err(NotImplemented)` (current behavior)
- `HttpProxyConnector` — delegates over HTTP to a connector service the user
  runs separately. Useful for the "Python backend over HTTP" pattern from
  the README.

Register via `.manage::<Arc<dyn Connector>>(...)` in `main.rs`, then update
every stub handler in `ipc/installation.rs`, `ipc/environments.rs`,
`ipc/backends.rs`, `ipc/jupyter.rs`, `ipc/uninstall.rs`, `ipc/certs.rs`,
`ipc/server.rs`, `ipc/mcp.rs` to delegate.

**Files to create/modify:**
- `src/connector.rs` (new)
- `src/lib.rs`: add `pub mod connector`
- `src/main.rs`: register `NoopConnector` (or user-overridden)
- All stub `ipc/*.rs` modules: replace `Err(NotImplemented)` body with
  `state.connector().method(args).await.map_err(IpcError::from)`

**Deliverable:** `cargo check` passes. The shape of a connector swap is
documented in the README. `tests/connector.rs` shows a custom impl with
3-4 methods overridden.

---

### Slice C — Extended typed route wrappers (~120 more)

**Goal:** Cover the remaining ~124 of 184 OpenBB routes as typed Tauri commands.

**Approach:** Add `src/ipc/obb_routes_extended.rs`. Use the same shape as
`obb_routes.rs`:

```rust
#[tauri::command]
pub async fn equity_compare_groups(params: Params, proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/compare/groups", params).await
}
```

Source: scan `/home/user/OpenBBPort/openbb_platform/extensions/**/*_router.py`
for every `@router.command(model="...")` decorator, derive the route from the
function name and the nesting (the file path tells you the prefix).

Already-covered routes (do NOT duplicate):
- All 60 in `obb_routes.rs` (search file for the full list)

Still to cover (~124):
- Equity: `compare/*`, `darkpool/*`, `shorts/*`, `estimates/*` (remaining), `fundamental/*` (remaining), `ownership/*` (remaining), `currency/snapshots`, plus the dozen single-method routers
- Crypto: any beyond search/historical
- Currency: any beyond pairs/snapshots/reference_rates/historical
- ETF: `equity_exposure`, `price_performance`
- Economy: all the FRED/labor/trade/treasury routes — at least 30 routes
- Fixed Income: all `corporate/*`, `government/*`, `rate/*` not yet covered
- Index: any beyond search/historical/constituents/snapshots/available
- News: source-specific endpoints if any
- Regulators: `sec/*` deep, `cftc/*` deep
- Technical: every indicator (~25 methods) — POST endpoints
- Quantitative: every method (`stats/*`, `rolling/*`, `performance/*`)
- Econometrics: every regression/test

**Files to create/modify:**
- `src/ipc/obb_routes_extended.rs` (new)
- `src/ipc/mod.rs`: add `pub mod obb_routes_extended`
- `src/main.rs`: append all new commands to `generate_handler!`

**Deliverable:** Total wrapper count ≥ 180 (close to the 184 cap).
`cargo check` passes.

---

### Slice D — Integration tests for infrastructure

**Goal:** Verify the production-quality infrastructure modules with real tests.

**Approach:** Add `tests/` directory with one file per subsystem. Each test
uses real OS resources (temp dirs, real spawned subprocesses) but should be
hermetic enough to run in CI.

**Files to create:**
- `tests/process_monitor.rs` — register/get/clear with 100-line load, capacity overflow eviction, concurrent reads/writes
- `tests/settings.rs` — atomic write under simulated crash (panic mid-write), flock contention, chmod 0600 verification (Unix only), corrupt-file recovery
- `tests/proxy.rs` — mock HTTP server (using `httpmock` crate) verifying GET/POST, basic auth, bearer auth, timeout, 204 No Content, error envelope
- `tests/log_storage.rs` — ring buffer wrap behavior, tail-N queries, multi-process-id isolation
- `tests/cleanup.rs` — cascade with a shutdown hook that takes too long (verify outer 10s timeout)
- `tests/process_kill.rs` — spawn a sleep subprocess, port-kill (with a TCP listener), kill_pid

**Files to modify:**
- `Cargo.toml`: add `httpmock = "0.7"`, `tempfile = "3"` to `[dev-dependencies]`

**Deliverable:** `cargo test` runs and all tests pass. At least 30 test
functions across 6 files.

---

### Slice E — Documentation: command catalog + recipes

**Goal:** Expand `README.md` from a project overview into a usable reference,
plus add per-module Rustdoc.

**Approach:**

1. Expand `README.md` with:
   - The **full 162-command catalog** as a table grouped by module, each row
     citing the source file:line, args type, return type, and a one-line
     description. Pull command names from `src/main.rs`'s `generate_handler!` block.
   - **Cookbook** section: 8-10 worked examples showing how a TS frontend
     calls representative commands. Cover at least:
     - `invoke<{ baseUrl: string }>("obb_set_base_url", { url: "..." })`
     - Streaming logs (`register_process_monitoring` + `listen("process-output")`)
     - Reading user_settings.json
     - Opening a log window
     - Tray autostart toggle
   - **Connector patterns** section: HTTP-proxy / TS sidecar / pure Rust,
     with a code snippet for each.
   - **Security** section pulling forward the bullets from
     `docs/typescript-port/raw-deep-dives/api-keys.md`.

2. Per-module Rustdoc: every public function in `ipc/*.rs` should have at
   least a one-paragraph doc-comment.

**Files to create/modify:**
- `README.md`: expand to ~600 lines (currently ~150)
- Every `src/ipc/*.rs` module: add `//!` module-level docs (some have, some don't)

**Deliverable:** `cargo doc --open` works and looks complete. `README.md`
covers all 162 commands with examples.

---

### Slice F — Example TS frontend

**Goal:** A minimal "smoke test" TS frontend that exercises every command
family. Useful as both documentation and an integration test.

**Approach:** Add `examples/typescript-frontend/` with:
- `index.html` — single page with a button per command family
- `main.ts` — `invoke(...)` calls for ~30 representative commands
- `package.json` — Vite config
- `vite.config.ts` — Tauri dev URL bridge

Build instructions in the example's README so the user can `cd
examples/typescript-frontend && npm install && npm run tauri dev` and have
a working desktop app that talks to the shell.

**Files to create:**
- `examples/typescript-frontend/index.html`
- `examples/typescript-frontend/src/main.ts`
- `examples/typescript-frontend/src/style.css`
- `examples/typescript-frontend/package.json`
- `examples/typescript-frontend/tsconfig.json`
- `examples/typescript-frontend/vite.config.ts`
- `examples/typescript-frontend/README.md`

**Deliverable:** Repo has a runnable example end-to-end.

---

### Slice G — `tauri-shell-cli` companion binary

**Goal:** A command-line tool that invokes Tauri commands without booting
the full Tauri runtime, for testing the connector quickly.

**Approach:** Add a second `[[bin]]` target `tauri-shell-cli` in `Cargo.toml`.
The binary instantiates the same `Proxy`, `LogStorage`, etc. state but
exposes them via a simple CLI (using `clap`):

```
tauri-shell-cli obb call /equity/price/historical --param symbol=AAPL
tauri-shell-cli settings read user_settings.json
tauri-shell-cli routines list
tauri-shell-cli logs tail backend-abc-123
```

**Files to create/modify:**
- `Cargo.toml`: add `clap = { version = "4", features = ["derive"] }`, add `[[bin]] name = "tauri-shell-cli"`
- `src/bin/cli.rs` (new): subcommand structure mirroring the IPC module layout

**Deliverable:** `cargo run --bin tauri-shell-cli -- --help` shows the
subcommand tree. `cargo run --bin tauri-shell-cli -- obb call /equity/price/historical --param symbol=AAPL --param provider=yfinance` works against a running Python server.

---

### Slice H — Connector reference implementations

**Goal:** Ship two reference `Connector` impls (depends on Slice B
completion) so users have something to clone.

**Approach:** Add `connectors/` directory at the repo root with:
- `connectors/http-proxy/` — an HTTP proxy connector that forwards every
  Tauri command to a localhost endpoint
- `connectors/openbb-platform/` — a reference implementation that wraps the
  real `openbb-api` + `openbb-mcp` Python servers (matches the OpenBB
  reference architecture)

Each connector is its own Cargo workspace member; the user picks one and
points `main.rs` at it.

**Files to create:**
- `connectors/http-proxy/Cargo.toml`
- `connectors/http-proxy/src/lib.rs`
- `connectors/openbb-platform/Cargo.toml`
- `connectors/openbb-platform/src/lib.rs`
- `connectors/openbb-platform/src/install.rs` — Miniforge download + bash installer
- `connectors/openbb-platform/src/environments.rs` — conda CLI wrappers
- `connectors/openbb-platform/src/backends.rs` — openbb-api spawn logic
- `connectors/openbb-platform/src/jupyter.rs` — `conda run jupyter lab` spawn
- Workspace `Cargo.toml` at repo root to include connectors as members

**Deliverable:** `cargo build -p openbb-platform-connector` succeeds. The
README documents how to swap connectors.

---

## 3. Conventions agents must follow

### File-level conventions

1. **Module docstring at top.** Every file starts with `//! ...` explaining
   purpose + relation to other modules. ~3-8 lines.
2. **`#[tauri::command]` placement.** Public functions exposed as Tauri
   commands live in `src/ipc/*.rs`. Internal helpers go in non-`ipc`
   modules at the crate root.
3. **Error type.** Tauri commands return `Result<T, IpcError>`. The
   `IpcError` enum is defined in `src/ipc/mod.rs`. Add new variants if
   needed but keep it tagged-union serializable.
4. **camelCase wire format.** Structs that cross the IPC boundary use
   `#[serde(rename_all = "camelCase")]`. Snake_case for Rust-internal
   types only.
5. **Doc comments.** Every public function has a one-paragraph `///` doc
   comment. Cite the relevant `feature-*.md` spec where applicable.

### Verification before committing

Every PR must:
- `cd tauri-shell && cargo check` (passes)
- `cd tauri-shell && cargo build --release` (passes)
- `cd tauri-shell && cargo test` (passes — only after Slice D lands)
- `cd tauri-shell && cargo clippy -- -D warnings` (passes — currently not enforced; agents should still aim for clippy-clean code)

### Updating `main.rs::generate_handler!`

When adding new commands:
1. Add the function to its `src/ipc/<module>.rs` file
2. Re-export from `src/ipc/mod.rs` if the module is new
3. **Append** to `tauri::generate_handler!` in `src/main.rs` — never
   reorder existing entries
4. Run `cargo check` before committing

### Conflicts between agents

If two slices want to modify the same file:
- Slice A (TS bindings) modifies struct annotations — coordinate with
  whichever slice introduced the struct
- Slice B (Connector trait) modifies every stub handler — must run after
  no other slice is mid-flight in `src/ipc/`
- Slice E (Docs) modifies `README.md` and module-level `//!` comments —
  conflicts with no one
- Slice F (TS frontend) is in `examples/` — conflicts with no one
- Slice G (CLI) is in `src/bin/` — conflicts with no one

Suggested order if running sequentially:
1. Slice D (tests) — establishes baseline
2. Slice C (more wrappers) — same shape as existing
3. Slice A (bindings) — needs all structs in place
4. Slice B (Connector trait) — refactors stubs
5. Slice F (TS frontend) — depends on bindings (A)
6. Slice G (CLI binary) — independent
7. Slice H (connector reference impls) — depends on B
8. Slice E (docs) — last, captures the final surface

---

## 4. Command catalog (current state)

| Module | Command | Args | Returns | Status |
|---|---|---|---|---|
| app | get_installation_state | — | InstallationSnapshot | Real |
| app | navigate_to_page | path: String | () | Real |
| app | quit_application | — | () | Real |
| app | get_app_version | — | String | Real |
| app | toggle_theme | theme: String | bool | Stub |
| infrastructure | register_process_monitoring | process_id: String | bool | Real |
| infrastructure | unregister_process_monitoring | process_id: String | bool | Real |
| infrastructure | get_process_logs_history | process_id, count? | Vec<LogEntry> | Real |
| infrastructure | clear_process_logs_history | process_id: String | bool | Real |
| helpers | get_home_directory | — | String | Real |
| helpers | get_settings_directory | — | String | Real |
| helpers | select_directory | prompt? | String | Real |
| helpers | select_file | filter? | String | Real |
| helpers | check_directory_exists | path: String | bool | Real |
| helpers | check_file_exists | path: String | bool | Real |
| helpers | open_url_in_window | url, title? | () | Real |
| helpers | open_workspace_in_browser | url? | () | Real |
| helpers | open_logs_window | label_prefix, id, route, id_key?, title? | () | Real |
| helpers | get_working_directory | default_dir? | String | Stub |
| helpers | save_working_directory | path | bool | Stub |
| installation | install_to_directory | directory, user_data_directory | bool | Stub |
| installation | install_conda | directory | bool | Stub |
| installation | setup_python_environment | directory, python_version | bool | Stub |
| installation | abort_installation | directory | () | Stub |
| installation | get_installation_status | — | InstallationProgress | Real (reads global) |
| installation | create_default_backend_services | — | () | Stub |
| installation | update_openbb_settings | conda_dir, environment | () | Stub |
| installation | get_installation_directory | — | String | Stub |
| installation | get_userdata_directory | — | String | Stub |
| environments | list_conda_environments | directory? | Vec<CondaEnvironment> | Stub |
| environments | create_environment | name, py_version, exts, processId | bool | Stub |
| environments | create_environment_from_requirements | name, filePath, dir, processId | bool | Stub |
| environments | select_requirements_file | — | String | Stub |
| environments | get_environment_extensions | name | Value | Stub |
| environments | install_extensions | extensions, environment | bool | Stub |
| environments | update_extension | package, environment, directory | bool | Stub |
| environments | update_environment | environment, directory | bool | Stub |
| environments | remove_extension | package, environment, directory | bool | Stub |
| environments | remove_environment | name | bool | Stub |
| environments | execute_in_environment | command, environment, directory | ExecResult | Stub |
| backends | list_backend_services | — | Vec<BackendService> | Stub |
| backends | create_backend_service | backend | BackendService | Stub |
| backends | update_backend_service | backend | BackendService | Stub |
| backends | delete_backend_service | id | () | Stub |
| backends | start_backend_service | id | BackendService | Stub |
| backends | stop_backend_service | id | () | Stub |
| backends | open_backend_logs_window | id | () | Real |
| jupyter | start_jupyter_server | environment, directory, working | JupyterStatus | Stub |
| jupyter | stop_jupyter_server | environment | bool | Stub |
| jupyter | check_jupyter_server | environment | JupyterStatus | Stub |
| jupyter | list_jupyter_servers | — | Value | Stub |
| jupyter | open_jupyter_logs_window | environment | () | Real |
| jupyter | update_jupyter_status | environment_name, status | () | Real (event emit) |
| credentials | get_user_credentials | — | Value | Real |
| credentials | update_user_credentials | credentials | bool | Real |
| credentials | open_credentials_file | file_name | bool | Real |
| certs | generate_self_signed_cert | args struct | Value | Stub |
| uninstall | uninstall_application | remove_user_data, remove_settings | Option<String> | Stub |
| obb | obb_call | route, params?, method? | Value | Real |
| obb | obb_set_base_url | url | () | Real |
| obb | obb_get_base_url | — | String | Real |
| obb | obb_set_basic_auth | username, password | () | Real |
| obb | obb_set_bearer | token | () | Real |
| obb | obb_clear_auth | — | () | Real |
| obb | obb_health | — | Value | Real |
| obb | obb_openapi | — | Value | Real |
| obb | obb_widgets | — | Value | Real |
| obb | obb_apps | — | Value | Real |
| obb | obb_agents | — | Value | Real |
| obb | obb_coverage_commands | — | Value | Real |
| obb | obb_coverage_providers | — | Value | Real |
| obb | obb_coverage_command_model | — | Value | Real |
| obb | obb_user_me | — | Value | Real |
| obb | obb_system | — | Value | Real |
| obb_routes | (60 wrappers — see src/ipc/obb_routes.rs) | params? | Value | Real |
| openbb_meta | list_all_routes | — | Vec<RouteInfo> | Real |
| openbb_meta | search_routes | query | Vec<RouteInfo> | Real |
| openbb_meta | route_parameters | path, method? | Value | Real |
| provider | provider_list | — | Value | Real |
| provider | provider_routes | provider | Vec<String> | Real |
| provider | provider_credentials | provider | Vec<String> | Real |
| provider | provider_validate | provider, probe_route, probe_params? | bool | Real |
| settings_files | read_settings_json | file_name | Option<Value> | Real |
| settings_files | write_settings_json | file_name, content | bool | Real |
| settings_files | read_settings_text | file_name | Option<String> | Real |
| settings_files | write_settings_text | file_name, content | bool | Real |
| settings_files | list_settings_files | — | Vec<String> | Real |
| server | server_spawn | spec | ServerStatus | Stub |
| server | server_stop | id | () | Stub |
| server | server_status | id | ServerStatus | Stub |
| server | server_list | — | Vec<ServerStatus> | Stub |
| server | server_attach | url | () | Real |
| server | server_health | — | Value | Real |
| mcp | mcp_spawn | spec | McpStatus | Stub |
| mcp | mcp_stop | id | () | Stub |
| mcp | mcp_status | id | McpStatus | Stub |
| mcp | mcp_list | — | Vec<McpStatus> | Stub |
| mcp | mcp_list_tools | id | Value | Stub |
| routines | routines_list | — | Vec<RoutineMetadata> | Real |
| routines | routines_read | name | Option<String> | Real |
| routines | routines_save | name, content | bool | Real |
| routines | routines_delete | name | bool | Real |
| routines | routines_rename | old_name, new_name | bool | Real |

**Total: 162 commands.**

## 5. Event catalog

Defined in `src/events.rs`. Every event has a typed payload struct.

| Event | Payload | Emitted by |
|---|---|---|
| `process-output` | `ProcessOutputEvent { processId, output, timestamp, type }` | `process_spawn::spawn_with_streaming` reader threads |
| `install-progress` | `InstallProgressEvent { step, progress, message }` | Installation pipeline (connector) |
| `uninstall-progress` | `String` | Uninstall pipeline (connector) |
| `backend-url-discovered` | `BackendUrlEvent { id, url }` | Server spawn helper after URL extraction |
| `jupyter-status-update` | `JupyterStatusEvent { environmentName, status }` | `ipc::jupyter::update_jupyter_status` |
| `navigate` | `NavigateEvent { path }` | Tray menu navigation handlers |

Renderer subscribes with `listen("event-name", cb)`.

## 6. State catalog

| State | Type | Purpose | Accessed via |
|---|---|---|---|
| `ProcessLogState` | wrapper around `LOG_STORAGE` singleton | Per-process ring buffer | `State<'_, ProcessLogState>` |
| `RunningProcesses` | `Arc<Mutex<HashMap<String, Child>>>` | Tracked subprocess handles | `State<'_, RunningProcesses>` |
| `InstallationState` | `Mutex<InstallationSnapshot>` | Boot-time install marker | `State<'_, InstallationState>` |
| `CancellationRegistry` | `Mutex<HashMap<String, AbortHandle>>` | Cancellable long-running ops | `State<'_, CancellationRegistry>` |
| `Arc<dyn ShutdownHook>` | trait object | Connector's graceful-stop callback | `try_state::<Arc<dyn ShutdownHook>>` |
| `Proxy` | `proxy::Proxy` | HTTP client for Python REST server | `State<'_, Proxy>` |

Globals (NOT `tauri::State`, accessed directly):
- `state::LOG_STORAGE` — singleton backing `ProcessLogState`
- `state::INSTALLATION_PROGRESS` — live install phase mirror
- `state::INSTALLATION_IN_PROGRESS` — re-entrancy guard

## 7. Files & directories (current)

```
tauri-shell/
├── Cargo.toml                       deps + bin target
├── Cargo.lock                       (gitignored)
├── README.md                        project intro (Slice E expands this)
├── SPEC.md                          this file
├── build.rs                         tauri-build invocation
├── tauri.conf.json                  Tauri 2.x config
├── .gitignore
├── capabilities/
│   ├── default.json                 main window perms
│   └── logs-window.json             log-window perms
├── icons/                           placeholder PNGs + .ico
├── dist/                            placeholder frontend
│   └── index.html
└── src/
    ├── main.rs                      builder, plugins, generate_handler!, RunEvent
    ├── lib.rs                       module re-exports
    ├── state.rs                     LogStorage, RunningProcesses, InstallationState
    ├── events.rs                    8 event payloads
    ├── path_utils.rs                home/settings dir resolution
    ├── settings.rs                  atomic JSON writes
    ├── process_monitor.rs           ring buffer API
    ├── process_spawn.rs             spawn + reader threads + emit
    ├── process_kill.rs              port/pid/pattern kill (3 OSes)
    ├── cleanup.rs                   bounded cascade
    ├── tray.rs                      tray menu
    ├── updater.rs                   updater wrapper
    ├── windows.rs                   close-to-tray, log windows
    ├── proxy.rs                     reqwest client for Python REST
    ├── autostart/
    │   ├── mod.rs                   per-OS dispatch
    │   ├── macos.rs                 osascript
    │   ├── windows.rs               COM IShellLink
    │   └── linux.rs                 XDG .desktop
    └── ipc/
        ├── mod.rs                   IpcError enum, module exports
        ├── infrastructure.rs        4 commands (real)
        ├── app.rs                   5 commands (real)
        ├── helpers.rs               11 commands (mostly real)
        ├── installation.rs          9 commands (stubs)
        ├── environments.rs          11 commands (stubs)
        ├── backends.rs              7 commands (stubs)
        ├── jupyter.rs               6 commands (mostly stubs)
        ├── credentials.rs           3 commands (real)
        ├── uninstall.rs             1 command (stub)
        ├── certs.rs                 1 command (stub)
        ├── obb.rs                   16 commands (real proxy)
        ├── obb_routes.rs            60 commands (real wrappers)
        ├── openbb_meta.rs           3 commands (real)
        ├── provider.rs              4 commands (real)
        ├── settings_files.rs        5 commands (real)
        ├── server.rs                6 commands (stubs)
        ├── mcp.rs                   5 commands (stubs)
        └── routines.rs              5 commands (real)
```

## 8. Status

| Slice | Owner | Status | Output |
|---|---|---|---|
| A — TS bindings | ⬜ | ⬜ | `bindings/*.ts` |
| B — Connector trait | ⬜ | ⬜ | `src/connector.rs` + refactored stubs |
| C — Extended typed wrappers | ⬜ | ⬜ | `src/ipc/obb_routes_extended.rs` |
| D — Integration tests | ⬜ | ⬜ | `tests/*.rs` |
| E — Docs + cookbook | ⬜ | ⬜ | expanded `README.md` |
| F — TS frontend example | ⬜ | ⬜ | `examples/typescript-frontend/` |
| G — CLI binary | ⬜ | ⬜ | `src/bin/cli.rs` |
| H — Connector reference impls | ⬜ | ⬜ | `connectors/{http-proxy,openbb-platform}/` |

Update this table when starting and finishing a slice. Legend: ⬜ not started · 🟡 in progress · ✅ done.
