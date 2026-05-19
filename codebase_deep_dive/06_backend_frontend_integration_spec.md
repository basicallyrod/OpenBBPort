# 06 — Backend ↔ Frontend Integration Spec Sheet

Compact, tabular reference for the Python ↔ TypeScript integration in OpenBBPort. Companion to `05_backend_frontend_integration.md`.

## 1. System identity

| Property | Value |
|---|---|
| Frontend runtime | Tauri 2.x desktop (Rust core + system webview) |
| Frontend UI | React 18 + TypeScript 5.9, Vite 7, TanStack Router 1.131 |
| UI lib | `@openbb/ui-pro` 0.6.10, Tailwind 3.4 |
| Backend runtime | CPython (conda env `openbb`, Python 3.10–4) |
| Backend framework | FastAPI + Uvicorn |
| Process model | Rust supervises Python as long-lived child processes |
| Direct HTTP from SPA → Python | **None** |
| Real HTTP client of Python API | OpenBB Workspace (external, browser/cloud) |
| Package name (npm) | `openbb-platform` 1.0.2 |
| Package name (PyPI) | `openbb-platform-api` 1.3.5, `openbb-core` ^1.6.4 |

## 2. Default network endpoints (Python services)

| Service | Command | Default bind | Default port | Transport | Source |
|---|---|---|---|---|---|
| OpenBB API | `openbb-api --host 127.0.0.1 --port 6900` | 127.0.0.1 | 6900 | HTTP (FastAPI / uvicorn) | `startup.rs:1445` |
| OpenBB MCP | `openbb-mcp --transport streamable-http --host 127.0.0.1 --port 8001` | 127.0.0.1 | 8001 | HTTP streamable | `startup.rs:1464` |
| Jupyter Lab | `jupyter lab --no-browser --notebook-dir <working>` | 127.0.0.1 | 8888 (auto) | HTTP + WS | `jupyter.rs:75` |
| FastAPI default prefix | — | — | — | `/api/v1` (`APISettings.prefix`) | `api_settings.py:47` |

## 3. Default security posture

| Control | Default | Source |
|---|---|---|
| `cors.allow_origins` | `["*"]` | `api_settings.py:11` |
| `cors.allow_methods` | `["*"]` | `api_settings.py:12` |
| `cors.allow_headers` | `["*"]` | `api_settings.py:13` |
| `OPENBB_API_AUTH` | disabled | env var, see `rest_api.py:24` |
| Bind interface | loopback only (`127.0.0.1`) | service commands |
| TLS | optional self-signed via `generate_self_signed_cert` Tauri command | `utils/certs.rs` |
| Command sanitiser | `validate_command_input` runs before every `start_backend_service` and `create_backend_service` | `utils/command_sanitizer.rs` |

## 4. Filesystem state contract

| Path | Format | Schema owner | Read by Rust | Write by Rust | Read by Python | Write by Python |
|---|---|---|---|---|---|---|
| `~/.openbb_platform/user_settings.json` | JSON | `UserService` (Python) | ✓ | ✓ (credentials slot) | ✓ | ✓ (seeded once) |
| `~/.openbb_platform/system_settings.json` | JSON | `SystemService` (Python) | ✓ | ✓ (seeded once) | ✓ | ✓ |
| `~/.openbb_platform/environments/openbb.yaml` | YAML | Rust (generated) | – | ✓ | input only | – |
| `<install_dir>/backends/backends.json` | JSON | Rust (`BackendService`) | ✓ | ✓ | – | – |
| `<install_dir>/conda/.condarc` | YAML | conda | – | ✓ | – | – |
| `~/.openbb_platform/.env` | dotenv | user-edited | – | – | ✓ (`openbb_core.env.Env`) | – |
| `~/.openbb_platform/mcp_settings.json` | JSON | MCP server | – | create-on-open | ✓ | ✓ |
| `~/.openbb_platform/.show_on_restart` | flag | Rust | ✓ | ✓ | – | – |

`<install_dir>` is the directory chosen by the user during setup, persisted into `system_settings.json.install_settings.installation_directory`.

## 5. Tauri IPC surface (TS → Rust commands)

All commands registered in `desktop/src-tauri/src/main.rs:492-550`.

### 5.1 Installation / lifecycle

| Command | Args | Returns | Source |
|---|---|---|---|
| `install_to_directory` | `directory, window` | `Result<bool>` | `startup.rs:419` |
| `install_conda` | `directory, window` | `Result<bool>` | `startup.rs:437` |
| `abort_installation` | – | `Result<()>` | `startup.rs` |
| `get_installation_status` | – | status JSON | `startup.rs` |
| `get_installation_state` | – | `InstallationState` | `main.rs:388` |
| `setup_python_environment` | `directory, python_version, window` | `Result<bool>` | `startup.rs:1228` |
| `create_default_backend_services` | – | `Result<()>` | `startup.rs:1432` |
| `uninstall_application` | … | `Result<()>` | `uninstall.rs` |
| `quit_application` | – | `Result<()>` | `main.rs:412` |
| `navigate_to_page` | `page` | `()` | `main.rs:394` |

### 5.2 Conda environments

| Command | Args | Returns | Source |
|---|---|---|---|
| `create_environment` | name, python_version, ... | `Result<bool>` | `environments.rs:419` |
| `create_environment_from_requirements` | … | `Result<bool>` | `environments.rs:1346` |
| `select_requirements_file` | – | `Result<String>` | `environments.rs:1612` |
| `list_conda_environments` | – | `Vec<Environment>` | `environments.rs:1799` |
| `get_environment_extensions` | `name` | `Value` | `environments.rs:2047` |
| `install_extensions` | env, list | `Result<Value>` | `environments.rs:2682` |
| `update_extension` | env, name | `Result<Value>` | `environments.rs:2329` |
| `remove_extension` | env, name | `Result<bool>` | `environments.rs:2237` |
| `remove_environment` | `name` | `Result<bool>` | `environments.rs:2754` |
| `update_environment` | env, directory | `Result<bool>` | `environments.rs:3040` |
| `update_installation_error` | `error` | `Result<()>` | `environments.rs:2759` |
| `execute_in_environment` | command, env, directory | `Result<Value>` | `environments.rs:3235` |

### 5.3 Backend services

| Command | Args | Returns | Source |
|---|---|---|---|
| `list_backend_services` | – | `Vec<BackendService>` | `backends.rs:1225` |
| `create_backend_service` | `BackendService` | `BackendService` | `backends.rs:1273` |
| `update_backend_service` | `BackendService` | `BackendService` | `backends.rs:1346` |
| `delete_backend_service` | `id` | `Result<()>` | `backends.rs:1381` |
| `start_backend_service` | `id` | `BackendService` | `backends.rs:640` |
| `stop_backend_service` | `id` | `Result<()>` | `backends.rs:562` |
| `open_backend_logs_window` | `id` | `Result<()>` | `backends.rs:1537` |

### 5.4 Jupyter

| Command | Args | Returns | Source |
|---|---|---|---|
| `start_jupyter_server` | env, directory, working | `Value` | `jupyter.rs:254` |
| `stop_jupyter_server` | env | `Result<()>` | `jupyter.rs` |
| `stop_all_jupyter_servers` | – | `Result<()>` | `jupyter.rs:264` |
| `check_jupyter_server` | env | bool / url | `jupyter.rs` |
| `list_jupyter_servers` | – | list | `jupyter.rs` |
| `open_jupyter_logs_window` | env | `Result<()>` | `jupyter.rs` |
| `update_jupyter_status` | env, status | `Result<()>` | `jupyter.rs` |

### 5.5 Credentials & settings files

| Command | Args | Returns | Source |
|---|---|---|---|
| `get_user_credentials` | – | `Value` (full `user_settings.json`) | `credentials.rs:37` |
| `update_user_credentials` | `credentials: Value` | `bool` | `credentials.rs:89` |
| `open_credentials_file` | `file_name?` | `bool` (opens in OS editor) | `credentials.rs:184` |
| `update_openbb_settings` | conda_dir, env | `Result<()>` | `helpers.rs:687` |

`open_credentials_file` accepts: `user_settings.json` (default), `system_settings.json`, `mcp_settings.json`, `.env`, `.condarc`.

### 5.6 Process monitoring

| Command | Args | Returns | Source |
|---|---|---|---|
| `register_process_monitoring` | `process_id` | `bool` | `main.rs:75` |
| `unregister_process_monitoring` | `process_id` | `bool` | `main.rs:80` |
| `get_process_logs_history` | `process_id, count?` | `Vec<LogEntry>` | `main.rs:85` |
| `clear_process_logs_history` | `process_id` | `bool` | `main.rs:95` |

`process_id` convention: `backend-<uuid>` or `jupyter-<env_name>`.

### 5.7 Filesystem / shell helpers

| Command | Returns |
|---|---|
| `get_home_directory` | `String` |
| `get_installation_directory` | `String` |
| `get_userdata_directory` | `String` |
| `get_settings_directory` | `String` |
| `get_working_directory` | `String` |
| `save_working_directory` | `bool` |
| `select_directory({ defaultPath? })` | `String` |
| `select_file({ filter? })` | `String` |
| `check_directory_exists({ path })` | `bool` |
| `check_file_exists({ path })` | `bool` |
| `open_url_in_window({ url, ... })` | `()` |
| `toggle_theme({ theme })` | `()` |
| `generate_self_signed_cert(...)` | `()` |

## 6. Tauri events (Rust → TS)

| Event name | Payload | Emitter | Subscriber |
|---|---|---|---|
| `process-output` | `{ processId, output, timestamp, type }` | `backends.rs:1003`, `jupyter.rs:154/193` | `BackendLogsPage.tsx`, `JupyterLogsPage.tsx`, `routes/backends.tsx:665` |
| `backend-url-discovered` | `{ id, url }` | `backends.rs:1103` | `routes/backends.tsx:2219` |
| `boolean-message` | `{ message }` | `backends.rs:1202` | UI generic |
| `install-progress` | `InstallProgress { step, progress, message }` | `startup.rs:1264` | `routes/installation-progress.tsx` |
| `installation-directory` | `String` | `startup.rs:1307` | setup flow |
| `uninstall_progress` | `{ step, progress, message }` | uninstall flow | `routes/uninstall.tsx:43` |

## 7. `BackendService` data contract

Persistence: `<install_dir>/backends/backends.json` (array).

| Field | Type | Rust (snake) | TS (camel) | Notes |
|---|---|---|---|---|
| id | string (UUID v4) | `id` | `id` | required |
| name | string | `name` | `name` | unique per config |
| command | string | `command` | `command` | validated by sanitiser |
| environment | string | `environment` | `environment` | conda env name |
| working_directory | string? | `working_directory` | `working_directory` | optional |
| env_file | string? | `env_file` ↔ alias `envFile` | `envFile` / `env_file` | path to `.env` |
| env_vars | map<string,string>? | `env_vars` ↔ alias `envVars` | `envVars` | inline overrides |
| auto_start | bool | `auto_start` | `autoStart` / `auto_start` | TS coalesces both |
| status | enum | `status: string` | `"running" \| "stopped" \| "starting" \| "stopping" \| "error"` | |
| host | string? | `host` | `host` | filled after URL detection |
| port | u16? | `port` | `port` | filled after URL detection |
| url | string? | `url` | `url` / `apiUrl` | filled after URL detection |
| pid | u32? | `pid` | `pid` | uvicorn PID, not script PID |
| started_at | RFC3339 string? | `started_at` | `startedAt` | |
| error | string? | `error` | `error` | populated on failure |

TS coalescing convention (`routes/backends.tsx:2322-2330`):
```ts
autoStart: b.auto_start ?? b.autoStart ?? false
envFile:   b.env_file   ?? b.envFile
apiUrl:    b.url        ?? b.apiUrl
```

## 8. Log-driven signal extraction (regex contract)

| Signal | Regex / pattern | Source line | Action |
|---|---|---|---|
| Real Python PID | `Started server process \[(\d+)\]` | uvicorn stdout | persist `pid` |
| URL candidates | `https?:\/\/(?:localhost\|\d{1,3}(?:\.\d{1,3}){3})(?::\d+)?[^\s]*` | uvicorn / Jupyter | collect, debounce 1500 ms, pick via `select_best_url` |
| Command not found | line ends with `: command not found` | shell stderr | set `status=error`, kill process |
| Traceback | line contains `Traceback` | Python stderr | start TS-side buffer, flush after 2 s, set `status=error` |
| Address in use | line contains `address already in use` or `ERROR:` | Python stderr | TS stops backend, sets `status=error` |
| Jupyter URL | `(http://[^:\s]+:[0-9]+[^\s]*)` | jupyter stdout | resolve `extract_jupyter_url` |

`select_best_url(urls, original_log_line)` prefers MCP streamable URLs when the source line contains `"streamable-http"`.

## 9. Conda activation contract

| Element | Value | Source |
|---|---|---|
| Installer source | Miniforge (URL fetched at runtime) | `startup.rs:595` |
| Conda root | `<install_dir>/conda/` | `startup.rs:1278` |
| Default env name | `openbb` | `startup.rs:1343`, `1448` |
| Env vars exported | `CONDA_ROOT`, `CONDA_ENVS_PATH`, `CONDA_PKGS_DIRS`, `CONDARC`, `PATH` | `backends.rs:859-867` |
| Env vars unset before activate | `CONDA_DEFAULT_ENV`, `CONDA_PREFIX`, `CONDA_SHLVL` | same |
| POSIX activation | `source <conda>/etc/profile.d/conda.sh; conda activate <env>` | `backends.rs:870-877` |
| Windows activation | `call <conda>\condabin\conda.bat activate <env>` | `backends.rs:822-830` |
| Script lifetime | written to `temp_dir()/backend_start_<id>.{sh,bat}`, deleted ~5 s after spawn | `backends.rs:953-958` |
| `openbb-api` arg rewrite | appends `--env_file <path>` and translates `UVICORN_<KEY>=<val>` → `--<key> "<val>"` | `backends.rs:776-806` |

## 10. Conda env.yaml pip deps (seeded at install)

```yaml
name: openbb
channels: [conda-forge, defaults]
dependencies:
  - python={python_version}
  - nodejs
  - pip
  - setuptools
  - pip:
      - notebook
      - jupyterlab-lsp
      - python-lsp-server[all]
      - jupyterlab-latex
      - anywidget[dev]
      - ipywidgets
      - openbb-platform-api
      - openbb-mcp-server
```

Source: `startup.rs:1507-1527`.

## 11. Python REST routers attached

Source: `openbb_platform/core/openbb_core/api/rest_api.py:74`.

| Router | Always on | Notes |
|---|---|---|
| `router_commands` | yes (if has routes) | all `@router.command(model=...)` endpoints |
| `router_coverage` | yes (if commands present) | provider/extension coverage |
| `router_system` | DEV_MODE only | system endpoints |
| `AuthService().router` | DEV_MODE only | auth |

Extra routes added by `openbb-platform-api` wrapper (`platform_api/main.py`):

| Path | Method | Returns |
|---|---|---|
| `/` | GET | landing page HTML |
| `/widgets.json` | GET | widgets manifest (cached after FIRST_RUN unless EDITABLE) |
| `/apps.json` | GET | merged user + default apps |
| `/agents.json` | GET | agents manifest (empty by default) |

## 12. Route ↔ command matrix (TS frontend)

| Route | File | Primary Tauri commands invoked |
|---|---|---|
| `/` (index) | `routes/index.tsx` | – |
| `/setup` | `routes/setup.tsx` | `get_home_directory`, `select_directory`, `check_directory_exists`, `install_to_directory`, `quit_application` |
| `/installation-progress` | `routes/installation-progress.tsx` | listens `install-progress`, `installation-directory` |
| `/environments` | `routes/environments.tsx` | `list_conda_environments`, `create_environment`, `install_extensions`, `remove_environment`, `start_jupyter_server`, `execute_in_environment` |
| `/backends` | `routes/backends.tsx` | `list_backend_services`, `create_backend_service`, `update_backend_service`, `delete_backend_service`, `start_backend_service`, `stop_backend_service`, `open_backend_logs_window`, `generate_self_signed_cert` |
| `/backend-logs` | `routes/backend-logs.tsx` | `register_process_monitoring`, `get_process_logs_history`, `clear_process_logs_history`; listens `process-output` |
| `/api-keys` | `routes/api-keys.tsx` | `get_user_credentials`, `update_user_credentials`, `open_credentials_file`, `open_url_in_window` |
| `/jupyter-logs` | `routes/jupyter-logs.tsx` | same monitor set; listens `process-output` |
| `/uninstall` | `routes/uninstall.tsx` | `get_installation_directory`, `get_userdata_directory`, `get_settings_directory`, `uninstall_application`; listens `uninstall_progress` |

## 13. Process monitor (Rust)

| Element | Type | Notes |
|---|---|---|
| Storage | `Arc<Mutex<HashMap<process_id, RingBuffer<LogEntry>>>>` | `utils/process_monitor.rs` |
| `LogEntry` | `{ timestamp: i64 (ms), content: String, process_id: String }` | |
| Tauri-managed state | `RunningProcesses` | holds `Child` handles for centralised cleanup |
| Cleanup triggers | `quit_application`, ctrl-c handler, macOS `applicationWillTerminate`, `RunEvent::ExitRequested` | `main.rs:419-457`, `762-771`, `825-833` |
| Cleanup timeout | 10 s outer, 3 s per service | `main.rs:423-451` |

## 14. URL discovery timing

```
t = 0    : start_backend_service spawned
t ≈ 0–3s : conda activation, Python import
t ≈ 3s   : uvicorn prints "Started server process [PID]"  → PID persisted
t ≈ 3s   : uvicorn prints "Uvicorn running on http://..." → URL collected
t + 1500 ms after last URL line: debounce fires → select_best_url → persist host/port/url → emit backend-url-discovered
TS receives backend-url-discovered → updates state → shows "connect to Workspace" toast (once per service kind, gated by localStorage)
```

## 15. Top integration risks (one line each)

1. URL/PID/error detection relies on uvicorn log strings; upstream format changes break UI transitions silently.
2. `cors.allow_origins=["*"]` + auth disabled by default — anything that can reach `127.0.0.1:6900` from a browser tab can call the API.
3. Conda activation script template is duplicated in three places (`start_backend_service_impl`, `execute_in_environment_impl`, settings-seeder).
4. `BackendService` carries both snake_case and camelCase fields; every read site must coalesce; easy to forget when adding fields.
5. `validate_command_input` is the only sanitiser between user-supplied backend commands and `bash -c`/`cmd /c`; worth fuzzing.
6. Credential updates do not push to Python; relies on `UserService` re-reading `user_settings.json` per request.
7. Settings-seeder runs Python from a heredoc inline string at install time — any breakage between Rust and `openbb_core` Pydantic models surfaces only at first launch.

## 16. Quick reference: where to look for each concern

| If you need to change… | Edit… |
|---|---|
| The list of default Python services | `desktop/src-tauri/src/tauri_handlers/startup.rs:1437` |
| The conda environment dependency list | `desktop/src-tauri/src/tauri_handlers/startup.rs:1507-1527` |
| How the Python process is launched (script template) | `desktop/src-tauri/src/tauri_handlers/backends.rs:809-903` |
| How a URL is parsed out of logs | `desktop/src-tauri/src/tauri_handlers/backends.rs:1028-1119` |
| How URL is chosen among candidates | `desktop/src-tauri/src/tauri_handlers/backends.rs:580-637` |
| The IPC surface (allow/deny commands) | `desktop/src-tauri/src/main.rs:492-550` |
| How credentials are written to disk | `desktop/src-tauri/src/tauri_handlers/credentials.rs:41-86` |
| How Python settings are seeded | `desktop/src-tauri/src/tauri_handlers/helpers.rs:687-940` |
| The Python REST app entrypoint | `openbb_platform/core/openbb_core/api/rest_api.py` |
| The `openbb-api` CLI | `openbb_platform/extensions/platform_api/openbb_platform_api/main.py` |
| Default API prefix `/api/v1` | `openbb_platform/core/openbb_core/app/model/api_settings.py:47` |
| Default CORS / auth defaults | `openbb_platform/core/openbb_core/app/model/api_settings.py:11`, `rest_api.py:24` |
