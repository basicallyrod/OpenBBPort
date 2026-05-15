# State and Storage

Operational map of every place data lives in OpenBBPort: filesystem,
process memory, browser storage, and the cross-process contracts that
connect them. When something gets stale, when a write races, or when the
port has to pick a new storage primitive — this is the reference.

## Filesystem layout

Three on-disk roots. Together they form the entire persistent surface.

```
~/.openbb_platform/                       # platform settings tree (HOME-based)
├── system_settings.json                  # install marker + python_settings + api_settings
├── user_settings.json                    # credentials + preferences + defaults + id
├── mcp_settings.json                     # MCP server config (created on demand)
├── .env                                  # OPENBB_* env-var overrides (dotenv)
├── .show_on_restart                      # 1-byte flag — post-update window show
├── .cli.env                              # CLI settings overlay (Settings.set_item)
├── .cli.his                              # prompt-toolkit history (sanitized)
├── environments/
│   ├── openbb.yaml                       # bootstrap env (channels, deps, pip list)
│   └── <env>.yaml                        # per-env manifest (one per managed env)
└── user_data/                            # per-user OpenBB data dir (preferences.data_directory)

<install_dir>/                            # user-chosen install location
├── conda/                                # Miniforge tree
│   ├── bin/conda  or  Scripts/conda.exe
│   ├── envs/<env>/                       # actual conda envs
│   ├── pkgs/
│   └── .condarc                          # channels + envs/pkgs dirs + timeouts
└── backends/
    └── backends.json                     # backend service definitions

<userDataDir>/                            # OS-managed app data dir
├── .app_id                               # UUIDv4 sent as X-App-ID to updater
└── (Tauri-internal IPC + webview cache)

<TEMP>/                                   # OS temp dir (transient)
├── openbb_installer/miniforge_installer.{sh|exe}
├── openbb_update_settings.py
├── openbb_console_command.{sh|bat}
├── backend_start_<id>.{sh|bat}           # conda activation wrapper (per backend)
└── openbb_uninstall.bat                  # Windows post-mortem cleanup script
```

Per-path detail follows. Atomicity column: ★ = atomic-rename used; flock =
file lock; — = none (plain `fs::write` open→truncate→write).

| Path | Writer(s) | Reader(s) | Format | Atomicity | Notes |
|---|---|---|---|---|---|
| `~/.openbb_platform/system_settings.json` | `install_to_directory` (`feature-installation.md:128-131`); `update_openbb_settings_impl` (Python merge); `list_conda_environments` cleanup pass (`feature-environments.md:242-247`); uninstall step 8 (strips `environments`+`install_settings`) | `check_installation_on_startup` (every boot, twice); every backend spawn for `installation_directory`; every env handler; tray Uninstall; `<install>/backends/` resolution; Python REST `SystemService` singleton | JSON `{install_settings, api_settings, python_settings, debug_mode, logging_sub_app, environments}` | — | `install_settings` is **NOT** in `SYSTEM_SETTINGS_ALLOWED_FIELD_SET`; only the hand-rolled merge preserves it. Any use of `SystemService.write_to_file()` wipes the install marker (`feature-installation.md:208`). |
| `~/.openbb_platform/user_settings.json` | `install_to_directory` (initial `{credentials:{}, preferences:{data_directory}, defaults:{}}`); `update_user_credentials` (`feature-api-keys.md:75-89`); theme toggle (only writer with flock — `helpers.rs:177-286`); Python `UserService` writes via API Keys IPC | `get_user_credentials` (mount); Python REST `UserSettings()` constructor on **every request** (`feature-platform-rest-api.md:179-184`); CLI `obb.user` (read only) | JSON `{credentials, preferences, defaults, id}` | — for credentials/install; flock for theme | Credentials writer takes **no lock** → races with Python re-read (transient-400 window) and theme toggle (`feature-api-keys.md:230-234`). |
| `~/.openbb_platform/.env` | `feature-api-keys.md` "Open File" → external editor; CLI `Settings.set_item` writes `~/.openbb_platform/.cli.env` (separate file) | Python `Env()` singleton snapshots `os.environ` once at module import (`feature-platform-rest-api.md:441-443`); conda activation scripts | dotenv `OPENBB_*=...` lines | — | Mutations require REST/MCP **restart** to take effect; no FS-watcher trigger. |
| `~/.openbb_platform/.show_on_restart` | Updater after successful download (`feature-tray-and-autostart.md:91-92`) | `main.rs:554-567` at boot, then `unlink` | 1-byte truthy flag | — | Missing `~/.openbb_platform/` swallows the write silently → post-update window stays hidden. |
| `~/.openbb_platform/.cli.env` | CLI `Settings.set_item` → `dotenv.set_key` (`feature-cli-repl.md:149-150`) | CLI `Settings.__init__` | dotenv | — | Two concurrent CLI sessions race; loser's edits lost (`feature-cli-repl.md:236-240`). |
| `~/.openbb_platform/.cli.his` | prompt-toolkit `FileHistory` per accepted line; passwords masked to `********` | CLI Ctrl-R reverse search | line-oriented | append-mostly | — |
| `~/.openbb_platform/environments/openbb.yaml` | `setup_python_environment` (wizard); `save_environment_as_yaml_impl` on every extension mutation (`feature-extensions.md:130-148`); `remove_environment` deletes | `conda env create -f` / `conda env update -f --prune`; `create_environment_from_requirements`; `update_environment` rebuild | conda env YAML | — | Last-writer-wins on extension installs (`feature-extensions.md:250-251`). Missing YAML → install succeeds but YAML merge silently skipped; later `update_environment` hard-fails (`feature-extensions.md:252-254`). |
| `~/.openbb_platform/environments/<env>.yaml` | same as above for managed envs | same | conda env YAML | — | `list_conda_environments` **deletes** any YAML whose stem isn't a directory under `<conda>/envs/` (`feature-environments.md:242-247`). |
| `~/.openbb_platform/user_data/` | populated by user / OpenBB plugins | Workspace `apps.json` (auto-creates `workspace_apps.json` here) | tree | — | Conditional removal on uninstall (`removeUserData` checkbox, `feature-uninstall.md:139-143`). |
| `<install_dir>/conda/.condarc` | `install_conda` (`feature-installation.md:131`); `feature-api-keys.md` "Open File" → external editor (auto-create-with-stub bug, `feature-api-keys.md:266-268`) | every conda invocation | YAML | — | Channel order opposite to `openbb.yaml`: `[defaults, conda-forge]` vs `[conda-forge, defaults]` — load-bearing for pin resolution. |
| `<install_dir>/conda/envs/<env>/` | `conda create` / `conda env update --prune` (`feature-environments.md:73-83`) | every spawn that activates the env | conda tree | conda lockfile | conda's own concurrency control is the only thing keeping it consistent. |
| `<install_dir>/backends/backends.json` | `create_default_backend_services` (install completion); `create_backend_service` / `update_backend_service` / `delete_backend_service`; `start/stop_backend_service` (status, pid, url, host, port, started_at); log-reader URL-discovery thread (writes host/port/url) | `list_backend_services`; `initialize_backends` 100 ms after Tauri setup (`feature-backend-services.md:42-44`); every per-row start/stop | JSON array of `BackendService` | flock exclusive on write; **NO LOCK on read** (`feature-backend-services.md:251`) | Reads race writes. Log-reader URL-discovery writes also race user edits. |
| `<userDataDir>/.app_id` | `get_or_create_app_id()` first call | every updater HTTP request (`X-App-ID` header) | UUIDv4 string | — | One-shot; persists. |
| `<TEMP>/backend_start_<id>.{sh|bat}` | `start_backend_service_impl` per start | bash/cmd | conda-activation script | per-id, NOT per-invocation | Rapid restart can have a stale 5 s timer delete the script mid-execution (`feature-backend-services.md:255`). |
| `<TEMP>/openbb_installer/miniforge_installer.{sh|exe}` | `install_conda` download | `bash <installer>` / `cmd /c <installer>` | platform installer | — | Deleted after successful install. |

## In-memory state

### Rust globals (`Lazy<Mutex<T>>`)

| Symbol | Location | Type | Holds | Writers | Readers |
|---|---|---|---|---|---|
| `INSTALLATION_STATE` | `startup.rs:15-25` | `Lazy<Mutex<InstallationState>>` | `{is_downloading, is_installing, is_configuring, is_complete, message}` — **live progress mirror** for the wizard | `update_installation_state` inside `install_conda` / `setup_python_environment` (`feature-installation.md:121`) | `get_installation_status` (2 s heartbeat poll from the renderer) |
| `INSTALLATION_IN_PROGRESS` | `startup.rs:434` | `Lazy<Mutex<bool>>` | re-entrancy guard for `install_conda` | acquired at install start; released by `release_guard()` on Err | re-entry check at install start |
| `LOG_STORAGE` | `process_monitor.rs:7-9` | `Lazy<Arc<Mutex<HashMap<String, LogBuffer>>>>` | per-process ring buffer (10k lines each); keys `backend-<uuid>`, `jupyter-<env>`, `create-env-<name>-<ts>`, `requirements-<name>-<ts>` | log-reader OS threads on every line (except env-create, which only emits — `feature-logs-streaming.md:208-214`) | `get_process_logs_history`, `clear_process_logs_history`, `register_process_monitoring` |
| `ACTIVE_JUPYTER_SERVERS` | `jupyter.rs:9-10` | `Lazy<Mutex<HashMap<String, (String, u32)>>>` | env-name → (jupyter_url, pid_of_conda_wrapper) | `start_jupyter_server` insert; `stop_jupyter_server` remove | `check_jupyter_server`, `list_jupyter_servers`, app-quit cleanup |

### `tauri::State` registrations (`main.rs:489-491`)

| State | Type | Holds | Writers | Readers |
|---|---|---|---|---|
| `InstallationState` (managed) | `InstallationState { is_installed: bool, installation_directory: Option<String> }` | **boot-time snapshot** of installation status | populated **once** by `check_installation_on_startup`; never mutated after (`feature-installation.md:120`) | `get_installation_state` (`/` redirect timeout, tray Uninstall gate, env page `installDir` fallback) |
| `ProcessLogState` | wrapper around `LOG_STORAGE` global | same as `LOG_STORAGE` | same | same |
| `RunningProcesses` | `Arc<Mutex<HashMap<String, std::process::Child>>>` | tracked-child handles **keyed by bare backend UUID** (no prefix) | `start_backend_service_impl` insert; `stop_backend_service_impl` / `delete_backend_service` remove | shutdown cleanup cascade; per-row stop |

> ⚠️ BUG: `LOG_STORAGE` key is `backend-<uuid>` but `RunningProcesses` key is
> bare `<uuid>` — wrap in `processIdForLogs(uuid)` /
> `processIdForKill(uuid)` helpers in the port
> (`feature-logs-streaming.md:106-109`, `feature-backend-services.md:240-241`).

> ⚠️ BUG: `INSTALLATION_IN_PROGRESS` is leaked on abort. `abort_installation`
> resets `INSTALLATION_STATE` but never touches `INSTALLATION_IN_PROGRESS` —
> subsequent install attempts silently return "Installation is already in
> progress" (`feature-installation.md:197`).

> ⚠️ BUG: `InstallationState` (managed) is computed twice at boot — once at
> `main.rs:491` inside `.manage(...)`, once at `main.rs:552` inside
> `.setup(...)` (`feature-installation.md:200`).

> ⚠️ BUG: `unregister_process_monitoring` has zero callers. Buffers for
> deleted backends and removed envs persist until app shutdown
> (`feature-logs-streaming.md:222-226`).

## In-renderer state (localStorage)

| Key | Writer | Reader | Schema | TTL / invalidation |
|---|---|---|---|---|
| `environments-first-load-done` | `main.rs:799` on cold-boot-installed; `installation-progress.tsx:1224,1241` on wizard complete; `installation-progress.tsx:1248` "Try Again" wipe | `navigate_to_page` gate (`feature-tray-and-autostart.md:111-128`); UI conditional branches | string `"true"` or unset | Cleared by Try-Again; never set on a failed install. |
| `installationDirectory` | `index.tsx:27` when (dead) `installation-directory` event fires | **no reader in `src/`** (`feature-installation.md:207`) | string path | — |
| `env-extensions-cache` | `environments.tsx` × 6 sites: `updateCacheAfterBackendOperation`, `refreshEnvironmentUIState`, `createEnvironment` success, extension-mutation handlers (`feature-environments.md:159-163`) | environments page mount (read-through cache); `backends.tsx:2156-2168` reads `cache[name].path` (never written — schema mismatch) | `{[envName]: {extensions: Extension[], pythonVersion: string}}` (no `path`) | No TTL. Wiped by Refresh button + reload; per-env evicted on `remove_environment`; full wipe on `createEnvironmentFromRequirements` success. |
| `jupyter-shutdown-<env>` | `JupyterLogsPage.tsx:229` when stdout matches `Shutting down on /api/shutdown request` | `environments.tsx:2107-2153` storage listener + on-mount scan; sets `jupyterStatus[env] = 'stopped'`, then unlinks the key | `Date.now()` timestamp string | 60 s freshness window; key removed after consumption (`feature-jupyter.md:178-182`). Clock-jump brittle. |
| Tauri's auto-managed window-state (positions etc.) | tauri-plugin-window-state | same | plugin-internal | — |

> ⚠️ BUG: `env-extensions-cache` schema mismatch — consumer reads `path`,
> writers never populate it. Harmless today (field unused after build) but a
> schema lie (`feature-environments.md:265-268`).

> ⚠️ BUG: `installationDirectory` is write-only. Drop in the port
> (`feature-installation.md:207`).

> ⚠️ BUG: `jupyter-shutdown-<env>` uses localStorage as cross-window IPC. A
> system clock jump breaks the 60 s freshness check. Replace with
> `BroadcastChannel` in the port (`feature-logs-streaming.md:260-263`).

## Cross-process state sharing

The full cross-process matrix of who-reads-what and what's cached vs.
re-read.

| File | Desktop Rust | Python REST (`openbb-api`) | Python MCP (`openbb-mcp`) | Python CLI (`openbb`) | Cache behavior |
|---|---|---|---|---|---|
| `system_settings.json` | R/W (every backend spawn re-reads for `installation_directory`) | R via `SystemService()` **singleton** — module-import-time, never reloaded (`feature-platform-rest-api.md:170-176`) | R same singleton, separate process | R via `SystemService()` singleton; CLI temporarily mutates `logging_sub_app` (`feature-cli-repl.md:152-154`) | Python: process-lifetime singleton, restart required. Rust: re-read per command. |
| `user_settings.json` | R via `get_user_credentials`; R/W via `update_user_credentials`; R for `installation_directory` resolution (theme toggle) | **R on every request** via `UserSettings()` default of hidden auth dep (`feature-platform-rest-api.md:179-184`) | same | R via `obb.user` at SDK init; not reloaded | Python REST: **per-request disk read** — hot path. Writes take effect without restart. **Non-atomic write opens a transient-400 race window** (`feature-api-keys.md:222-228`). |
| `.env` | R/W via external editor only | R **once** at module import via `Env()` singleton (`dotenv.load_dotenv` then `os.environ` snapshot) (`feature-platform-rest-api.md:441-443`) | same | same | Edits via API Keys UI require **restart** of REST/MCP/CLI to take effect. No mtime watcher. |
| `backends.json` | R/W (exclusive flock on write, **NO lock on read**) | not read | not read | not read | Log-reader thread writes URL/host/port out-of-band — races user edits and `list_backend_services` reads. |
| `<env>.yaml` | R/W on env CRUD + extension mutations | not read | not read | not read | Last-writer-wins; missing YAML cascades to `update_environment` failure. |
| `.condarc` | written once at install; opened by user via API Keys page | not read | not read | not read | conda CLI reads on every spawn. |
| `mcp_settings.json` | API Keys page opens for editing (writes empty `{}` if missing — mismatch with Python `MCPService` ~30-field default, `feature-api-keys.md:269-271`) | not read | R via `MCPService()` at boot | not read | Python singleton; restart required. |

### The credentials-reload-on-every-request behavior

This is the single most important cross-process contract.

The REST server's per-route auth dependency `__authenticated_user_settings`
defaults to a fresh `UserSettings()` constructor call when auth is off
(`feature-platform-rest-api.md:179-184`). `UserSettings.__init__` does
`json.load(open("~/.openbb_platform/user_settings.json"))`. So **every
request** — including the hot OBBject pipeline — hits the filesystem for a
JSON parse.

Implication for the port:

- **Pro:** API key edits take effect instantly. The UI doesn't need to
  restart anything.
- **Con (hot-path cost):** Even unauthenticated requests pay a stat + open +
  read + parse on every call.
- **Con (race window):** `update_user_credentials` does `std::fs::write`
  (open → `O_TRUNC` → write → close) **without a flock**
  (`feature-api-keys.md:222-230`). If a Python request lands during the
  zero-byte window, the `json.load` raises `JSONDecodeError`, which becomes a
  500 with no auto-retry. Frequency is low but reproducible under heavy
  load + simultaneous editing.

> ⚠️ BUG: port-time fix is two-part: (a) `fs.writeFile('.tmp')` +
> `fs.rename` for atomicity; (b) flock both reader and writer for the
> read-modify-write window. Mtime cache in the reader optional.

## State precedence

When multiple sources set the same value, what wins. The
`openbb-api` server is the canonical precedence chain
(`feature-platform-rest-api.md:267-269`):

```
CLI flag  >  ENV var (process)  >  ~/.openbb_platform/.env  >  system_settings.json:python_settings.uvicorn  >  hard-coded default
```

Worked example: the bound port for `openbb-api`.

| Source | Value | Read at |
|---|---|---|
| `--port 6900` on command line | `6900` | uvicorn argparse |
| `OPENBB_API_PORT=7000` in process env | `7000` | launcher `os.environ` (`feature-platform-rest-api.md:262-265`) |
| `UVICORN_PORT=7100` in `~/.openbb_platform/.env` | `7100` | `dotenv.load_dotenv` then `os.environ` |
| `system_settings.json:python_settings.uvicorn.port` | `7200` | `SystemService` boot singleton |
| default | `6900` | hard-coded |

Two-layer wrinkle from the **desktop side**: `feature-backend-services.md`'s
`UVICORN_*`→`--<flag>` translation **prepends** flags onto the user's
command string but **skips if the flag is already present**
(`feature-backend-services.md:298-330`). So:

- `command = "openbb-api --port 6900"`, env_file has `UVICORN_PORT=7000` →
  `--port 6900` already present → env var silently ignored.
- `command = "openbb-api"`, env_file has `UVICORN_PORT=7000` → translated to
  `--port "7000"` (no shell escaping — command-injection vector).

The desktop user-visible result is: **whatever `--port` value is literally
in the backends.json `command` string wins**, falling back through the
chain only if that flag is absent.

> ⚠️ BUG: the silent auto-increment in Python `check_port` shadows the whole
> chain. If the chosen port is busy, uvicorn binds the next free one and
> only stdout reports the truth (`feature-platform-rest-api.md:405-410`).
> The desktop's URL-discovery log reader catches this and rewrites
> `backends.json`, so the per-row "URL" pill is correct — but
> `backend.port` and `backend.command` then disagree.

## Cleanup matrix

Uninstall offers three choices: Conda (always — required), Remove user data,
Remove application settings. Cross-product with three OSes
(`feature-uninstall.md:139-143`):

| | macOS | Windows | Linux |
|---|---|---|---|
| **Always (any choice)** | `<install_dir>/`, `~/.openbb_platform/environments/`, `~/Library/Application Support/co.openbb.platform`, `~/Library/Logs/co.openbb.platform`, `~/Library/Caches/co.openbb.platform`, `~/Library/WebKit/co.openbb.platform`, `~/Library/WebKit/openbb-platform`, `~/Library/Application Scripts/group.co.openbb.platform`, the `.app` bundle | `<install_dir>/`, `~/.openbb_platform/environments/`, `%LOCALAPPDATA%\OpenBB Platform`, `%LOCALAPPDATA%\co.openbb.platform`, app binary via `uninstall.exe /S` | `<install_dir>/`, `~/.openbb_platform/environments/`, `~/.config/co.openbb.platform` |
| **+ Remove user data** | `~/.openbb_platform/user_data/` | `~/.openbb_platform/user_data/` | `~/.openbb_platform/user_data/` |
| **+ Remove application settings** (supersedes user data) | entire `~/.openbb_platform/` | entire `~/.openbb_platform/` | entire `~/.openbb_platform/` |
| **Always — defensive legacy sweep** | `~/Library/LaunchAgents/com.openbb.platform.plist`, `osascript` login-item delete | 20 `HKCU\…\Run`/`RunOnce` key×name combos via `reg delete`; `%APPDATA%\…\Startup\openbb-platform.lnk` | `~/.config/systemd/user/openbb-platform.service`, `~/.config/autostart/openbb-platform.desktop` |
| **Always — process cleanup (step 1)** | `stop_all_jupyter_servers` + `stop_all_backend_services` (no timeout — `feature-uninstall.md:217-218`); `pkill conda`, `pkill python` mop-up | same + `taskkill /F /IM conda.exe`, `taskkill /F /IM python.exe` | same |
| **Post-mortem self-delete** | `/tmp/openbb_uninstall_cleanup.sh` spawned + `std::process::exit(0)` | `%TEMP%\openbb_uninstall.bat` visible window: `timeout /t 5` → `taskkill /F /IM openbb-platform.exe` → `uninstall.exe /S` → cleanup | **NONE — app stays running** (`feature-uninstall.md:14-15`) |

> ⚠️ BUG: the Linux post-mortem path is broken because
> `invoke('app.exit')` (`uninstall.tsx:84`) has no Rust handler. The macOS
> path works only because Rust `std::process::exit(0)` runs first; Windows
> works only because the `.bat` taskkills the process
> (`feature-uninstall.md:194-197`).

> ⚠️ BUG: the defensive legacy sweep on each OS targets artifacts the
> **current** autostart code never creates (`feature-tray-and-autostart.md:212-218`).
> Keep as legacy-detect for upgraders from older versions; the port should not
> implement creation of these.

> ⚠️ BUG: step 1 has no timeout. A wedged backend blocks the entire
> cascade indefinitely — compare to `cleanup_all_processes` which uses
> nested 3 s / 3 s / 10 s timeouts (`feature-uninstall.md:217-218`).

## Port checklist

State surfaces most likely to drift if the port team doesn't consciously
preserve them. Each row is a "if you don't think about this, you'll silently
break something" warning.

1. **`system_settings.json:install_settings` is not in
   `SYSTEM_SETTINGS_ALLOWED_FIELD_SET`.** Reading the file through Python
   `SystemService.write_to_file()` wipes the install marker, after which
   the boot redirect sends the user back to `/setup`. Always read-modify-write
   the raw JSON when touching install state (`feature-installation.md:208`).
2. **`UserSettings` is re-read on every Python request.** A non-atomic
   write to `user_settings.json` opens a transient-400 window. Port: atomic
   rename + flock both sides. Optional: mtime cache in the reader to drop the
   per-request disk hit (`feature-api-keys.md:222-228`,
   `feature-platform-rest-api.md:413-415`).
3. **`localStorage["env-extensions-cache"]` is the read-through cache for
   the environments page.** Six writers, no TTL. `backends.tsx` reads
   `cache[name].path` that no writer populates. Port: move to IndexedDB
   with a versioned schema and migrations; populate `path` from
   `list_conda_environments` or drop the consumer's read
   (`feature-environments.md:138-156, 265-268`).
4. **`process-output` payload schema diverges per producer.** Jupyter emits
   `{processId, output, timestamp}` (+`type:"system"` on stop), backends emit
   `{processId, output, timestamp, type}`, environments emit
   `{processId, output}` (no timestamp, no type). Every consumer destructures
   `type` away anyway. Port: unify the payload, drop the dead discriminator
   (`feature-logs-streaming.md:80-94`).
5. **`backends.json` is read without a lock.** Log-reader thread writes
   URL/host/port out-of-band, racing user edits and per-row reads. Port:
   shared lock on read, exclusive on write — or move to SQLite
   (`feature-backend-services.md:251`).
6. **Process-id key prefixing is asymmetric.** `LOG_STORAGE` key is
   `backend-<uuid>`; `RunningProcesses` key is bare `<uuid>`. Wrap in
   helpers in the port (`feature-logs-streaming.md:106-109`).
7. **`InstallationState` is a one-shot snapshot, not a reactive store.**
   The redirect-via-reload pattern is what re-runs the check after install
   completion. If the port makes it mutable, update on completion AND
   remove the reloads — or keep both (`feature-installation.md:188`).
8. **Wizard's `directory` payload is partially decorative.** Only honored
   by `install_conda` / `execute_in_environment` / `abort_installation` /
   `install_to_directory`. `install_extensions` re-reads from
   `system_settings.json` instead. Port: pick one and document
   (`feature-installation.md:190`).
9. **Conda activation cannot be inherited — must be a generated script.**
   `child_process.spawn` can't enter a conda env; the wrapper `.sh`/`.bat`
   that sources `conda.sh` and runs `conda activate <env>` is mandatory.
   Use per-invocation random suffix to avoid the rapid-restart deletion
   race (`feature-backend-services.md:354-355`,
   `feature-environments.md:218-238`).
10. **`backgroundThrottling: "disabled"`** in `tauri.conf.json:15` is
    load-bearing for tray-mode polling. Electron equivalent:
    `webPreferences.backgroundThrottling: false`
    (`feature-tray-and-autostart.md:219-220`).
11. **Jupyter is killed by port, not PID.** `conda run` adds a wrapper layer;
    the stored PID is the conda wrapper, not the server. Stop extracts the
    port from the scraped URL and kills whatever is listening
    (`feature-jupyter.md:140-146`). If the port drops `conda run` and invokes
    `<env>/bin/jupyter` directly, this can collapse to `process.kill(pid)`.
12. **`.env` changes require restart.** `Env()` snapshots `os.environ` at
    import; the API Keys UI lets users edit `.env` but never restarts the
    server. Port: file-watch + restart prompt
    (`feature-platform-rest-api.md:441-443`).
13. **`workspace_apps.json` is auto-created by a GET.** `Python /apps.json`
    handler writes `[]` if the file is missing — a side effect of a read.
    Port: initialize at install time (`feature-platform-rest-api.md:429-431`).
14. **MCP server makes outbound HTTP back to REST.** Every MCP tool calls
    `127.0.0.1:6900/api/v1/...`. If REST stops or its port drifts, every
    MCP tool 502s. The UI doesn't couple their lifecycles
    (`feature-platform-rest-api.md:445-447`). Port: surface this dependency
    in the UI or merge protocols (Strategy A).
15. **Credentials on Unix inherit umask 0644.** No `chmod 0o600` after the
    write — secrets are world-readable on Linux. Port: explicit chmod
    after rename (`feature-api-keys.md:228-231`).
