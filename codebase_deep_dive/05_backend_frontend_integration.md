# 05 — Backend ↔ Frontend Integration Deep Dive

## Purpose

This document explains how the OpenBBPort **Python backend** (the OpenBB Platform FastAPI app and CLI entrypoints) is connected to the **TypeScript frontend** (the React SPA inside the Tauri desktop shell). The integration is not a single HTTP client/server boundary — it is a multi-channel composition: Tauri IPC, Tauri events, child-process management, and shared filesystem state.

## Topology overview

```text
┌──────────────────────────────────────────────────────────────────┐
│  Tauri desktop process                                           │
│  ┌─────────────────────────┐   IPC    ┌────────────────────────┐ │
│  │ TypeScript SPA          │ ───────► │ Rust core (src-tauri)  │ │
│  │ React + TanStack Router │ ◄─────── │ #[tauri::command] +    │ │
│  │ (Vite, @openbb/ui-pro)  │  events  │ AppHandle.emit         │ │
│  └─────────────────────────┘          └──────────┬─────────────┘ │
│                                                  │ spawn         │
│                                                  ▼               │
│                                       ┌──────────────────────┐   │
│                                       │ bash/cmd activation  │   │
│                                       │ script + conda env   │   │
│                                       └──────────┬───────────┘   │
└──────────────────────────────────────────────────┼───────────────┘
                                                   │
                            ┌──────────────────────┴───────────────┐
                            │ Python subprocess (long-lived)       │
                            │  openbb-api  →  uvicorn → FastAPI    │
                            │  openbb-mcp  →  MCP streamable-http  │
                            │  jupyter lab →  Jupyter notebook svr │
                            └──────────────────────┬───────────────┘
                                                   │ HTTP
                                                   ▼
                                     OpenBB Workspace (browser/cloud)
```

Key invariant: the React SPA **does not call the Python REST API directly**. The Python server is exposed at a local URL (e.g. `http://127.0.0.1:6900`) and the user copies that URL into OpenBB Workspace, which is the actual HTTP client. The frontend in this repo only orchestrates the lifecycle of the Python processes.

That distinction is the most important thing to understand about this codebase. Everything below explains how that orchestration works.

## The four integration channels

### Channel 1 — Tauri IPC (TS → Rust)

Path: `desktop/src/**/*.tsx` → `desktop/src-tauri/src/main.rs` → `desktop/src-tauri/src/tauri_handlers/*.rs`.

The frontend calls Rust handlers using `invoke()` from `@tauri-apps/api/core`. Every Rust function exposed to JS is annotated with `#[tauri::command]` and registered in `main.rs:492` (`tauri::generate_handler![...]`). The complete registered surface (see `desktop/src-tauri/src/main.rs:492-550`) covers:

- **Startup / installation**: `install_to_directory`, `install_conda`, `setup_python_environment`, `abort_installation`, `get_installation_status`, `get_installation_state`, `create_default_backend_services`.
- **Conda environments**: `create_environment`, `create_environment_from_requirements`, `list_conda_environments`, `get_environment_extensions`, `install_extensions`, `update_extension`, `remove_extension`, `remove_environment`, `update_environment`, `execute_in_environment`, `select_requirements_file`, `update_installation_error`.
- **Backend services (Python servers as managed processes)**: `list_backend_services`, `create_backend_service`, `update_backend_service`, `delete_backend_service`, `start_backend_service`, `stop_backend_service`, `open_backend_logs_window`.
- **Jupyter**: `start_jupyter_server`, `stop_jupyter_server`, `stop_all_jupyter_servers`, `check_jupyter_server`, `list_jupyter_servers`, `open_jupyter_logs_window`, `update_jupyter_status`.
- **Credentials and OpenBB settings files**: `get_user_credentials`, `update_user_credentials`, `open_credentials_file`, `update_openbb_settings`.
- **Process monitoring**: `register_process_monitoring`, `unregister_process_monitoring`, `get_process_logs_history`, `clear_process_logs_history`.
- **Filesystem / shell helpers**: `select_directory`, `select_file`, `check_directory_exists`, `check_file_exists`, `get_home_directory`, `get_installation_directory`, `get_userdata_directory`, `get_settings_directory`, `get_working_directory`, `save_working_directory`, `open_url_in_window`, `toggle_theme`.
- **Security / lifecycle**: `generate_self_signed_cert`, `uninstall_application`, `quit_application`, `navigate_to_page`.

Representative TS call sites:

- `desktop/src/routes/backends.tsx:2318` — `invoke<BackendService[]>("list_backend_services")`.
- `desktop/src/routes/backends.tsx:2507` — `invoke("start_backend_service" | "stop_backend_service", { id })`.
- `desktop/src/routes/api-keys.tsx:364` — `invoke("update_user_credentials", { credentials })`.
- `desktop/src/routes/setup.tsx:121` — `invoke("install_to_directory", { ... })`.

Each handler has a `*_impl` form that takes `FileSystem` / `EnvSystem` / `FileExtTrait` trait objects, with `Real*` and `Mock*` implementations. This is the principal seam used in `#[cfg(test)] mod tests` blocks throughout `tauri_handlers/`.

### Channel 2 — Tauri events (Rust → TS)

Rust emits events via `AppHandle.emit("<name>", payload)`; TS subscribes via `listen<T>("<name>", handler)` from `@tauri-apps/api/event`. The catalog of event names observed in the source:

| Event | Emitted from | Consumed by |
|---|---|---|
| `process-output` | `backends.rs:1003`, `jupyter.rs:154/193` | `BackendLogsPage.tsx`, `JupyterLogsPage.tsx`, `backends.tsx:665` |
| `backend-url-discovered` | `backends.rs:1103` | `backends.tsx:2219` — shows a "Connect to Workspace" toast |
| `boolean-message` | `backends.rs:1202` | generic UI signal that a backend started |
| `install-progress` | `startup.rs:1264` (`InstallProgress`) | installation progress screen |
| `installation-directory` | `startup.rs:1307` | setup flow |
| `uninstall_progress` | uninstall flow | `routes/uninstall.tsx:43` |

The `process-output` event is the workhorse — every line of stdout/stderr from every spawned Python process flows through it, both for the in-app log viewers and for in-band signal extraction (URL/PID detection from a Python traceback or a uvicorn startup message).

### Channel 3 — Child-process management (Rust ↔ Python)

This is the actual "backend" connection. The Rust core does **not** embed Python; it spawns it as a child process inside an activated conda environment. The lifecycle is:

1. `install_to_directory` (`startup.rs:419`) downloads a Miniforge installer (URL fetched at runtime by `fetch_miniforge_installer_url`, see `startup.rs:595`) into `<install_dir>/conda/`.
2. `setup_python_environment` (`startup.rs:1228`) generates `~/.openbb_platform/environments/openbb.yaml` listing pip deps including `openbb-platform-api` and `openbb-mcp-server` (`startup.rs:1507-1527`), then runs `conda env create -f openbb.yaml`.
3. `update_openbb_settings_impl` (`helpers.rs:687`) runs an **inline Python script** under that env that imports `openbb_core.app.service.user_service.UserService` and `openbb_core.app.service.system_service.SystemService` to materialize `~/.openbb_platform/user_settings.json` and `system_settings.json` with default fields. This is where the Python side first writes its canonical settings shape.
4. `create_default_backend_services_impl` (`startup.rs:1437`) seeds two records in `backends.json`:
   - `OpenBB API`  → `openbb-api --host 127.0.0.1 --port 6900`
   - `OpenBB MCP`  → `openbb-mcp --transport streamable-http --host 127.0.0.1 --port 8001`
5. On every app launch, `main.rs:570-578` calls `initialize_backends` (`backends.rs:1439`) which:
   - reconciles `backends.json` state against live PIDs (stale `"running"` entries with dead PIDs are demoted to `"stopped"`),
   - auto-starts any backend with `auto_start: true`.
6. `start_backend_service_impl` (`backends.rs:655`) builds an activation script per OS (Windows .bat / POSIX .sh) under `temp_dir()`. The script:
   - exports `CONDA_ROOT`, `CONDA_ENVS_PATH`, `CONDA_PKGS_DIRS`, `CONDARC`, prepends `PATH`,
   - sources `etc/profile.d/conda.sh` (POSIX) or calls `condabin\conda.bat` (Windows),
   - `conda activate <env>` against the env stored on the `BackendService`,
   - loads env vars from `backend.env_file` and `backend.env_vars`,
   - **rewrites the command if it is `openbb-api`** to forward `--env_file <path>` and to translate `UVICORN_*` env vars into `--<name>` CLI flags (`backends.rs:776-806`),
   - executes the final `command` string.
7. The script is spawned via `cmd /c` or `bash`, with `Stdio::piped()` on stdout and stderr. Two reader threads parse each line through a closure `log_processor` (`backends.rs:977-1119`) that:
   - mirrors the line into the in-memory log ring buffer (`process_monitor.rs`),
   - emits a `process-output` event,
   - greps for `command not found` (sets status `error` and persists),
   - extracts a real PID via `regex r"Started server process \[(\d+)\]"`,
   - collects URL candidates with `regex r"https?://(?:localhost|\d{1,3}(?:\.\d{1,3}){3})(?::\d+)?[^\s]*"` and debounces for 1500 ms before picking a winner with `select_best_url` (`backends.rs:580`), parsing host/port out via the `url` crate and emitting `backend-url-discovered`.
8. The spawned child is parked in `RunningProcesses` (`utils/process_monitor.rs`) so app-wide cleanup (`cleanup_all_processes` in `main.rs:419`) can kill it on quit, ctrl-c, or macOS `applicationWillTerminate`.

There is **no direct HTTP or stdio JSON protocol** between Rust and Python. The whole signaling channel from Python back to the frontend is log scraping. The Python process is otherwise opaque to the host application once spawned.

`stop_backend_service` (`backends.rs:314-560`) is the inverse path. Besides killing the tracked child, it issues an OS-specific "kill anything on this port" pass (`lsof -ti tcp:<port>` on macOS, `fuser -k <port>/tcp` + a Python fallback on Linux, `netstat` parsing on Windows) before clearing host/port/url/pid in `backends.json`.

### Channel 4 — Shared filesystem state

The two halves of the application read and write a common set of JSON/YAML files under the user's home directory. This is the only place where state survives a restart, and it is the contract surface between Rust and Python.

| File | Owner of canonical schema | Read by Rust | Written by Rust | Read by Python | Written by Python |
|---|---|---|---|---|---|
| `~/.openbb_platform/user_settings.json` | Python (`UserService`) | yes (`get_user_credentials`) | yes (`update_user_credentials`) | yes (`UserService.read_from_file`) | yes (on first launch via inline script) |
| `~/.openbb_platform/system_settings.json` | Python (`SystemService`) | yes (`check_installation_on_startup`, `get_installation_directory`) | yes (boot-script in `helpers.rs:687`) | yes (`SystemService`) | yes (defaults seeded by Rust-invoked Python) |
| `~/.openbb_platform/environments/openbb.yaml` | Rust | – | yes (`startup.rs:1483`) | indirectly (conda input) | – |
| `<install_dir>/backends/backends.json` | Rust | yes (`load_backends_config`) | yes (`save_backends_config`) | no | no |
| `<install_dir>/conda/.condarc` | conda | indirectly | yes (during install) | – | – |
| `~/.openbb_platform/.env` | user | – | – | yes (read by Python via `openbb_core.env.Env`) | – |
| `~/.openbb_platform/mcp_settings.json` | MCP server | – | yes (create-on-open) | yes | yes |

The credentials round-trip is the most user-visible: `api-keys.tsx` calls `update_user_credentials` (`credentials.rs:41-86`), which reads the existing JSON, swaps in the new `credentials` object, and writes back. On the next request the running `openbb-api` server reads the file when constructing `UserSettings`. There is no live push channel — the frontend assumes the change will be visible to Python on the next command.

## The Python REST API surface (for completeness)

Even though the SPA does not consume it, the Python server it spawns is a normal FastAPI app:

- Module: `openbb_platform/core/openbb_core/api/rest_api.py` builds the `app` object.
- Routers attached: `AuthService().router` (DEV_MODE only), `router_system`, `router_coverage`, `router_commands` (`AppLoader.add_routers` at `rest_api.py:74`).
- Default prefix: `/api/v1` (computed in `APISettings.prefix`, `api_settings.py:47`).
- Default CORS: `allow_origins=["*"]`, `allow_methods=["*"]`, `allow_headers=["*"]` (`api_settings.py:11-13`). With `OPENBB_API_AUTH` unset this means **the spawned local server is wide-open to any browser origin**. That is intentional for the Workspace-connector use case but worth keeping in mind for review.
- Launcher: `openbb-platform-api` (entrypoint `openbb_platform_api.main:main`) wraps `app`, adds `/`, `/widgets.json`, `/apps.json`, `/agents.json`, then calls `uvicorn.run("openbb_platform_api.main:app", host, port, **kwargs)`. Defaults: `127.0.0.1:6900` (`main.py:288-331`).
- The `--env_file` switch and any `UVICORN_*` env variables that the Rust wrapper injected are read here.

The user-visible flow ends when the URL appears in the log and `backend-url-discovered` fires; the frontend displays a Workspace setup checklist (see `desktop/src/routes/backends.tsx:2231-2289`) that tells the user to paste the URL into Workspace's "Connect backend" form.

## End-to-end request: starting the OpenBB API from the UI

The clearest end-to-end trace is the "Start" button on the `Backends` page after a fresh install:

1. **UI click** — `BackendServiceItem.onStartStop("start")` in `backends.tsx`. Local state is set to `starting`, the form runs a TS-side `validateCommandInput` mirror of the Rust check, then `invoke("start_backend_service", { id })` (`backends.tsx:2507`).
2. **Rust dispatch** — `main.rs` routes to `start_backend_service` (`backends.rs:639`) which calls `start_backend_service_impl`.
3. **Validation** — `validate_command_input` (`utils/command_sanitizer.rs`) rejects shell-injection patterns. On failure, `backends.json` is updated with `status: "error"` and an error message, the call returns `Err`, and the UI surfaces it.
4. **Activation script** — `start_backend_service_impl` resolves `<install_dir>/conda`, writes `/tmp/backend_start_<id>.sh` (or `.bat`), and `chmod 755` on POSIX. The command becomes `openbb-api --host 127.0.0.1 --port 6900 [--env_file ... --<key> <val>]`.
5. **Spawn** — `cmd.spawn()` runs the script. The PID is stored in `backends.json` (with `started_at`, `status: "running"`); the `Child` handle is moved into `RunningProcesses`.
6. **Python boots** — conda activates, `openbb-platform-api.main` builds `widgets_json`, attaches landing-page/widgets/apps routes, then `uvicorn.run(...)` binds to the port.
7. **Log parsing** — uvicorn prints `INFO:     Started server process [<pid>]` and `INFO:     Uvicorn running on http://127.0.0.1:6900 (Press CTRL+C to quit)`. The Rust readers extract both. After 1.5 s of URL silence, the debounce thread picks the best URL, persists `host` / `port` / `url` to `backends.json`, and emits `backend-url-discovered`.
8. **UI sync** — the `listen("backend-url-discovered", ...)` handler in `backends.tsx:2219` updates that backend's `apiUrl` in component state. A "platform-api-run-once" toast prompts the user to connect Workspace; subsequent runs do not re-toast (gated on `localStorage`).
9. **End state** — the Python process keeps running until `stop_backend_service`, app quit, ctrl-c, or process exit (which the reader threads notice when stdout/stderr close).

## Field-name impedance mismatch

`BackendService` exists in two slightly different shapes — a Rust struct in `backends.rs:31` and a TS interface in `backends.tsx:23`. Rust uses snake_case (`auto_start`, `env_file`, `env_vars`, `working_directory`) with `serde(alias = "envFile", rename = "envFile")` for some fields. The TS interface intentionally declares both:

```ts
envFile?: string;
env_file?: string;
envVars?: Record<string, string>;
autoStart: boolean;
auto_start: boolean;
```

And every read site collapses them:

```ts
backendServices.map((b) => ({
  ...b,
  autoStart: b.auto_start ?? b.autoStart ?? false,
  envFile:   b.env_file   ?? b.envFile,
  apiUrl:    b.url        ?? b.apiUrl,
}))
```

(`backends.tsx:2322-2330`, repeated at line 2365-2372). This is a tech-debt seam: until the wire shape is unified, any new field added on one side must be added on the other and to every coalescing read site.

## Process monitoring substrate

`desktop/src-tauri/src/utils/process_monitor.rs` is the shared logging substrate used by both Jupyter and Backends. It exposes:

- `register_process(storage, &process_id)` — allocate a bounded ring buffer keyed by a string id (`backend-<uuid>` or `jupyter-<env>`).
- `LogEntry { timestamp, content, process_id }` — what every reader thread pushes in.
- `RunningProcesses` (Tauri-managed state) — holds `Child` handles so cleanup is centralised.
- `get_process_logs(...)` — returns the buffered tail; consumed by the `BackendLogsPage` and `JupyterLogsPage` log viewers via `invoke("get_process_logs_history")` and merged with live `process-output` events.

Each log window is a separate webview opened by `open_backend_logs_window` / `open_jupyter_logs_window` and addressed by `WebviewUrl::App("/backend-logs?id=<id>")`. The route component reads the id from the query string, prefills its state from `get_process_logs_history`, and subscribes to `process-output` for new lines.

## Settings translation: when Rust writes Python's config

`helpers.rs:687-940` is unusual: it generates a Python program at runtime, drops it in `temp_dir()`, and runs it inside the just-installed conda env. The program imports `openbb_core.app.service.user_service.UserService` and `system_service.SystemService`, hands their `model_dump_json()` results back through stdout, and writes back into `~/.openbb_platform/{user,system}_settings.json` if the fields are missing.

The effect is that the Rust shell never has to keep an in-sync replica of the OpenBB settings schema — Python remains the schema-of-record, and Rust just guarantees the JSON files exist with the right defaults the first time. Any later credential change goes through the simpler `update_user_credentials` path, which preserves the `credentials` key and leaves everything else alone.

This indirection is a deliberate way to keep the front-of-house Rust agnostic to Pydantic model changes inside `openbb_core`.

## What the frontend does **not** do

It is worth listing the negatives, because they explain a lot of decisions:

- It does not make HTTP requests to the Python `/api/v1` surface. The only `fetch(...)` call discovered in the SPA is in `AddExtensionSelector.tsx` and targets a remote extension catalog, not the Python server.
- It does not embed a Python interpreter or use PyO3. All Python execution is through `Command::spawn`.
- It does not stream structured events from Python. Everything that resembles a signal (URL ready, PID, error) is parsed out of stdout/stderr text with regex.
- It does not own the OpenBB settings schema. Python writes the canonical shape; Rust only patches the `credentials` slot or seeds defaults.

## Risk surface around this integration

For follow-up review the highest-leverage areas to test, harden, or document are:

1. **Log-driven signaling.** URL/PID/error detection depends on log strings that are owned by uvicorn, openbb-platform-api, and the openbb-mcp-server. A future log format change anywhere upstream silently breaks "starting → running" UI transitions. Snapshot tests over canonical log samples + a contract test that runs `openbb-api` headlessly would catch regressions.
2. **Wide-open default CORS + no auth.** The bundled `openbb-api` listens on `127.0.0.1` by default, which is fine, but `cors.allow_origins=["*"]` plus `OPENBB_API_AUTH` defaulting off means any local browser tab can hit it. A reviewer should confirm whether the desktop install ever encourages `--host 0.0.0.0`.
3. **Activation script generation duplication.** The Win/POSIX template strings appear in `start_backend_service_impl`, `execute_in_environment_impl`, and the settings-seeding helper. A change to env var names or conda activation flags must touch all three. Consolidating into a single `build_activation_script(os, env)` helper would shrink the cross-platform surface considerably.
4. **`BackendService` snake/camel duality.** Documented above; worth a single-sided convention (probably snake_case with explicit serde renames) and a TS regenerated type, otherwise any new field is forgotten in one of the four coalescing read sites.
5. **Command sanitiser scope.** `validate_command_input` (`utils/command_sanitizer.rs`) is the security boundary on user-defined backend commands. Backends are user-editable in the UI and stored in `backends.json`, so the sanitiser's allow/deny list is effectively the only thing between a malicious entry and a local exec. Worth fuzzing.
6. **No live push of credential changes.** Updating an API key in the UI does not signal the running `openbb-api`; it relies on Python re-reading `user_settings.json` per request through `UserService`. If `UserService` ever caches, edits would silently not take effect. Worth a contract test.

## File index for this integration

| Concern | Path | Notes |
|---|---|---|
| Rust handler registration | `desktop/src-tauri/src/main.rs:492-550` | Single source of the IPC surface |
| Backend service model + lifecycle | `desktop/src-tauri/src/tauri_handlers/backends.rs` | Spawn, log parse, URL discovery, persistence |
| Jupyter lifecycle | `desktop/src-tauri/src/tauri_handlers/jupyter.rs` | Same pattern with a different default port |
| Conda + env setup | `desktop/src-tauri/src/tauri_handlers/startup.rs`, `environments.rs` | Miniforge install, env.yaml, default services |
| Credentials bridge | `desktop/src-tauri/src/tauri_handlers/credentials.rs` | Edits the `credentials` slot of `user_settings.json` |
| Settings seeder | `desktop/src-tauri/src/tauri_handlers/helpers.rs:687` | Inline Python script that uses `UserService` / `SystemService` |
| Log substrate | `desktop/src-tauri/src/utils/process_monitor.rs` | Ring buffer + RunningProcesses |
| Command sanitiser | `desktop/src-tauri/src/utils/command_sanitizer.rs` | Backend command security check |
| Frontend backends page | `desktop/src/routes/backends.tsx` | Calls every backend handler, listens for `backend-url-discovered` and `process-output` |
| Frontend API keys page | `desktop/src/routes/api-keys.tsx` | Calls `get_user_credentials` / `update_user_credentials` |
| Frontend log viewers | `desktop/src/components/BackendLogsPage.tsx`, `JupyterLogsPage.tsx` | Initial backfill + live event stream |
| Python REST app | `openbb_platform/core/openbb_core/api/rest_api.py` | FastAPI app, default prefix `/api/v1` |
| Python launcher | `openbb_platform/extensions/platform_api/openbb_platform_api/main.py` | `openbb-api` entrypoint, default `127.0.0.1:6900` |
| Python settings shape | `openbb_platform/core/openbb_core/app/service/{user,system}_service.py` | Schema of `user_settings.json` / `system_settings.json` |

## Conclusion

The Python ↔ TypeScript boundary in OpenBBPort is best understood as **a process supervisor, not an HTTP client**. The Tauri Rust core is responsible for installing conda, materialising settings files, launching `openbb-api` / `openbb-mcp` / `jupyter lab` as long-lived child processes inside that env, and reporting their state up to the React SPA through Tauri events. The SPA is a control plane for those processes; the actual HTTP consumer of the Python server is OpenBB Workspace running elsewhere.

The fragile parts of the contract are concentrated in (a) regex-based log parsing for URL/PID/error detection, (b) the dual snake/camel shape of `BackendService`, and (c) the inline Python script that bridges Rust to Pydantic-owned settings models. These are the right places to add tests and consolidate code if this integration grows.
