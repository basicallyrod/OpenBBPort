# Feature: Installation (Setup Wizard + Bootstrap Pipeline)

## Purpose

First-launch wizard that materializes a self-contained OpenBB runtime on the user's machine: a Miniforge install, a Python conda env named `openbb`, the platform's REST/MCP servers, the on-disk settings tree under `~/.openbb_platform/`, and two default backend service entries. Until this completes successfully, no other feature is reachable — the root route force-redirects to `/setup`.

## User flows

1. **Golden path.** App boots → `check_installation_on_startup` finds no `system_settings.json` → Rust eval redirects to `/setup` (`main.rs:781-786`). User enters install + user-data paths, clicks **Begin Installation**, sees overwrite-confirm if dir exists, then `/installation-progress` drives: download Miniforge → run installer → pick Python version (default `3.13`) → conda env create → pick extensions (default-checked list + `alwaysInclude` providers) → pip/conda install → openbb-build → settings merge → seed default backends → reload to `/environments`.
2. **Cancel from setup form.** Click **Cancel** → native confirm → `quit_application` → app exits (runs full `cleanup_all_processes` cascade even though there's nothing to clean).
3. **Mid-install cancel.** Available while `phase ∈ {downloading, installing, configuring}` and on the version-select screen. `abort_installation` kills child procs, `rm -rf`s `<install_dir>` and the openbb env, sets `phase="cancelled"`. User sees **Return to Setup**.
4. **Install failure.** Two recovery buttons: **Try Again** (`localStorage.clear()` + reload to `/setup`) or **Continue Anyway** (sets `environments-first-load-done` and jumps to `/environments` without seeding settings or backends).
5. **Skip extensions.** Step 3 has a **Skip** button — pure frontend state flip to `phase="complete"`, no backend call. Env is left as `setup_python_environment` produced it (no openbb-namespace packages).
6. **Resume / repair.** Not implemented. A half-installed state (e.g., conda exists but openbb env is broken) leaves the user on `/environments` with no signal to repair. Workaround is tray → Uninstall → reinstall.

## UI surface

- `desktop/src/routes/index.tsx:9-63` — `/` redirect gate (event listeners + 2 s `setTimeout` calling `get_installation_state`).
- `desktop/src/routes/setup.tsx:53-311` — Step 1 form. React Hook Form + Zod (`setup.tsx:13-22`):
  - Two required string fields, both rejected by `.refine(v => !/\s/.test(v))` — paths must contain no whitespace (load-bearing: conda activation scripts embed the path unquoted).
  - Browse buttons call `select_directory` (`setup.tsx:143-160`).
  - Submit gate calls `check_directory_exists`, then native `@tauri-apps/plugin-dialog` `confirm()`, then `install_to_directory`, then router-navigates to `/installation-progress?directory=...&userDataDir=...` (`setup.tsx:79-140`).
- `desktop/src/routes/installation-progress.tsx:742-1564` — Step 1 (driver) / 2 / 3 controller.
  - Phases: `preparing → downloading → installing → version_select → configuring → extension_select → configuring → complete` plus `failed`, `cancelling`, `cancelled`.
  - `<PythonVersionSelector>` (`desktop/src/components/InstallComponents.tsx:52-90`) — default `"3.13"`, options `["3.10","3.11","3.12","3.13","3.14"]`.
  - Extension catalog fetched directly via `fetch()` from three `raw.githubusercontent.com` JSON files (`installation-progress.tsx:186-275`). `alwaysInclude` defaults: `fred, bls, us-eia, nasdaq, fmp, econdb, cftc, congress-gov`.
  - `extrasExtensions` hard-coded (`installation-progress.tsx:151-166`): `openbb-cli`, `openbb-cookiecutter` — opt-in only.
  - Free-text `customPackages` field for arbitrary PyPI names.

## Data flow

```mermaid
sequenceDiagram
    autonumber
    participant U as User
    participant FE as React (setup.tsx + installation-progress.tsx)
    participant TA as Tauri host (main.rs)
    participant ST as startup.rs handlers
    participant EN as environments.rs handlers
    participant HE as helpers.rs
    participant FS as Disk (~/.openbb_platform, <install_dir>)

    U->>FE: Submit setup form (paths)
    FE->>TA: invoke check_directory_exists
    TA-->>FE: bool
    FE->>ST: invoke install_to_directory
    ST->>FS: mkdir + permission probe + write user_settings.json / system_settings.json
    ST-->>FE: Ok(true) (no events emitted in this step)
    FE->>FE: router.navigate("/installation-progress")

    FE->>ST: invoke install_conda { directory, userDataDir }
    Note over ST: lock INSTALLATION_IN_PROGRESS
    ST-->>FE: emit install-progress {step:"download", 0.05, "Preparing..."}
    ST-->>FE: emit install-progress {step:"download", 0.15, "Detecting architecture"}
    ST->>FS: curl/reqwest -> <TEMP>/openbb_installer/miniforge_installer.{sh|exe}
    ST-->>FE: emit install-progress {step:"install", 0.55, "Running Miniforge installer"}
    ST->>FS: bash/cmd run installer -> <install_dir>/conda
    ST->>FS: write <install_dir>/conda/.condarc
    ST-->>FE: emit install-progress {step:"complete", 1.0, "Conda installation completed successfully"}
    ST-->>FE: Ok(true) (release_guard)
    FE->>FE: setPhase("version_select")

    U->>FE: Click "Next Step" after picking Python version
    FE->>ST: invoke setup_python_environment { directory, pythonVersion }
    ST-->>FE: emit install-progress {step:"config", 0.6, "Setting up Python <v>..."}
    ST->>FS: write ~/.openbb_platform/environments/openbb.yaml
    ST->>FS: <conda>/bin/conda env create -f openbb.yaml
    ST-->>FE: emit install-progress {step:"config", 0.80, "Initializing environment"}
    ST->>HE: update_openbb_settings_impl (spawns Python in env)
    HE->>FS: merge user_settings.json + system_settings.json
    ST-->>FE: emit install-progress {step:"complete", 1.0, "Installation complete"}
    ST-->>FE: emit installation-directory (string)  // dead-during-wizard
    ST-->>FE: Ok(true)
    FE->>FE: setPhase("extension_select")

    U->>FE: Click "Install" with selected extensions
    FE->>EN: invoke install_extensions { extensions, environment:"openbb", directory }
    EN->>FS: conda install / pip install (no progress events)
    EN-->>FE: Ok(true)
    FE->>EN: invoke execute_in_environment { command:"openbb-build", environment, directory }
    EN-->>FE: { stdout, stderr, exit_code }  // exit_code != 0 NOT a rejection
    FE->>HE: invoke update_openbb_settings { condaDir, environment:"openbb" }
    HE-->>FE: void

    U->>FE: Click "Done"
    FE->>HE: invoke update_openbb_settings (3rd call; idempotent)
    FE->>ST: invoke create_default_backend_services
    ST->>FS: append two entries to backends.json (OpenBB API + OpenBB MCP)
    FE->>FE: localStorage.setItem("environments-first-load-done","true")
    FE->>FE: window.location.href = "/environments?..."  // full reload re-runs check_installation_on_startup
```

## IPC contract

| Direction | Name | Payload | Returns | Used by |
|-----------|------|---------|---------|---------|
| invoke | `get_home_directory` | `{}` | `string` | `setup.tsx:53` mount |
| invoke | `select_directory` | `{ prompt: string }` | `string` (or `Err` on cancel) | `setup.tsx:143-160` browse buttons |
| invoke | `check_directory_exists` | `{ path: string }` | `boolean` | `setup.tsx:88` submit pre-check |
| invoke | `install_to_directory` | `{ directory, userDataDirectory }` | `boolean` | `setup.tsx:122` |
| invoke | `quit_application` | `{}` | `void` | `setup.tsx:302` Cancel |
| invoke | `install_conda` | `{ directory, userDataDir }` | `boolean` (long; `userDataDir` ignored by Rust) | `installation-progress.tsx:1031` |
| invoke | `setup_python_environment` | `{ directory, pythonVersion }` | `boolean` (long) | `installation-progress.tsx:1117` |
| invoke | `get_installation_status` | `{}` | `{phase, isDownloading, isInstalling, isConfiguring, isComplete, message}` (camelCase) | 2 s heartbeat poll (`:927`) |
| invoke | `abort_installation` | `{ directory }` | `void` | `installation-progress.tsx:1268` |
| invoke | `install_extensions` | `{ extensions, environment:"openbb", directory }` | `boolean` (`directory` ignored by Rust) | `installation-progress.tsx:1144` |
| invoke | `execute_in_environment` | `{ command, environment, directory }` | `{stdout, stderr, exit_code}` | `installation-progress.tsx:1157` |
| invoke | `update_openbb_settings` | `{ condaDir, environment:"openbb" }` | `void` | called 3× in golden path |
| invoke | `create_default_backend_services` | `{}` | `void` | `handleContinue` |
| invoke | `get_installation_state` | `{}` | `{is_installed: boolean, installation_directory: string\|null}` (snake_case) | `/` redirect timeout |
| emit | `install-progress` | `{step, progress, message}` | — | Step 1 driver + Step 2 |
| emit | `installation-directory` | `string` | — | Listened only by `/` page (dead during wizard) |
| emit | `installation-status` | `boolean` | — | **Never emitted.** Dead listener at `index.tsx:15-24`. Drop in port. |

> Note: `get_installation_state` returns snake_case fields; `get_installation_status` returns camelCase. This is not Tauri auto-normalization — the first uses a `#[derive(Serialize)]` struct with default field names, the second hand-builds a `serde_json::json!` literal. Preserve both casings or unify both ends together.

## State surfaces

- **React state (`installation-progress.tsx`).** `phase` (string union), `message` (animated with cycling ellipsis), `isComplete`, `isCancelling`, `selectedVersion`, `selectedExtensions`, `customPackages`. Refs: `installationStartedRef`, `ellipsisTimerRef`, `statusCheckIntervalRef`, `isSubmittingRef` (in `setup.tsx`).
- **Rust managed state (`main.rs:67-70`).** `struct InstallationState { is_installed: bool, installation_directory: Option<String> }` — a **boot-time snapshot** populated by `check_installation_on_startup` and never mutated thereafter. Consumed by tray menu, index redirect, and uninstall page.
- **Rust live progress (`startup.rs:15-25`).** `static INSTALLATION_STATE: Lazy<Mutex<InstallationState>>` (different struct! fields: `is_downloading`, `is_installing`, `is_configuring`, `is_complete`, `message`). Mutated inside `install_conda` / `setup_python_environment` via `update_installation_state`; read by `get_installation_status` poll.
- **Rust re-entrancy lock (`startup.rs:434`).** `static INSTALLATION_IN_PROGRESS: Lazy<Mutex<bool>>` guards `install_conda`. **Not** released on abort.
- **localStorage.** `installationDirectory` (write-only — no reader in `src/`), `environments-first-load-done` (gates `navigate_to_page` from tray menu).

## Persistence

Files this feature writes:

- `~/.openbb_platform/user_settings.json` — three-key skeleton `{credentials:{}, preferences:{data_directory:"<path>"}, defaults:{}}` on create. On existing file: read-modify-write touching only `preferences.data_directory` if changed (`startup.rs:269-353`). Later merged by the Python `update_openbb_settings_impl` script to add the dynamic per-provider credential schema.
- `~/.openbb_platform/system_settings.json` — sets `install_settings = {installation_directory, user_data_directory, installation_date}` (`installation_date` is `chrono::Local::now().to_rfc3339()`, timezone-aware). Other top-level keys preserved. **Note:** `install_settings` is NOT in `SYSTEM_SETTINGS_ALLOWED_FIELD_SET` on the Python side; only `update_openbb_settings_impl`'s hand-rolled `json.load` / `json.dump` script preserves it. Any code using `SystemService.write_to_file()` against this file will wipe `install_settings`.
- `~/.openbb_platform/environments/openbb.yaml` — canonical bootstrap content with channels `[conda-forge, defaults]` and a fixed pip-list order: `notebook, jupyterlab-lsp, "python-lsp-server[all]", jupyterlab-latex, "anywidget[dev]", ipywidgets, openbb-platform-api, openbb-mcp-server`.
- `<install_dir>/conda/.condarc` — channels `[defaults, conda-forge]` (note: opposite order vs. the yaml), envs/pkgs dirs, `auto_activate_base: false`, `pip_interop_enabled: true`, timeouts.
- `<install_dir>/conda/...` — the Miniforge tree itself.
- `<TEMP>/openbb_installer/miniforge_installer.{sh|exe}` — deleted after a successful install.
- `<TEMP>/openbb_update_settings.py` and `<TEMP>/openbb_console_command.{sh|bat}` — transient scripts, deleted after run.
- `backends.json` — appended at the end by `create_default_backend_services`: two entries (OpenBB API on `127.0.0.1:6900`, OpenBB MCP on `127.0.0.1:8001`), both `auto_start: false`. Name-duplicate insert silently fails.

Read for the `is_installed` decision: `~/.openbb_platform/system_settings.json` (must parse + contain `install_settings.installation_directory` or root `installation_directory`) AND `<dir>/conda/{bin/conda|Scripts/conda.exe}` must exist.

## Error handling

- **Form validation.** Zod blocks empty paths and paths containing whitespace.
- **Permission probe** (`startup.rs:139-188`). Writes `.permission_test_file` and `.permission_test_dir`; failure → typed Err shown in form.
- **Path-already-exists.** Confirm via native plugin dialog; cancel returns to form.
- **Cancel filter.** Rust `select_directory` returns `Err("No directory selected")` on cancel, but `setup.tsx:154` filters on substring `"User canceled"` which is never emitted. Cancel currently falls through to `setErrorMessage`. Port should align the cancel marker on both sides.
- **install_conda failures.** Each Err path calls `report_fatal_error` which emits `install-progress` with the failure message, releases `INSTALLATION_IN_PROGRESS`, and returns Err to the JS side. Frontend transitions to `phase="failed"`.
- **Re-entrant `install_conda`.** Returns `"Installation is already in progress..."`. The frontend treats this string specially (`installation-progress.tsx:981-987`) and switches to monitoring mode silently.
- **`isFutureWarningOnly` filter** (`installation-progress.tsx:32-68`). Pip stderr containing only `FutureWarning:` / `DeprecationWarning:` / etc. is treated as success. Hard error markers (`Error:`, `failed`, `exit code`, `Exception:`, `Could not find`, `command not found`) are escape hatches.
- **`execute_in_environment` non-zero exit.** Returns `{stdout, stderr, exit_code}` as a resolved value, NOT a Promise rejection. The wizard `await`s without inspecting the object, so a missing `openbb-build` binary is silently a no-op (this is the actual default-install behavior — see Known bugs §3).
- **Abort.** Kills child procs via `pkill`/`taskkill`, `rm -rf`s the install dir (only if installer temp dir existed first), resets `INSTALLATION_STATE` but NOT `INSTALLATION_IN_PROGRESS`.

## ▸ Interfaces with

- **depends-on** `feature-app-shell.md` for `InstallationState` (boot-time snapshot) and the `main.rs` redirect-via-`window.eval` that beats React's redirect.
- **depended-on-by** `feature-environments.md` — installation seeds the `openbb` env and `environments/openbb.yaml`; the env page mutates the same yaml on every Add Extension.
- **depended-on-by** `feature-extensions.md` — wizard Step 3 IS the first extension-install flow and shares `install_extensions_impl` / `update_openbb_settings_impl` with the env page.
- **depended-on-by** `feature-backend-services.md` — `create_default_backend_services` seeds the two default `BackendService` entries (OpenBB API + OpenBB MCP) at the end of the wizard.
- **depended-on-by** `feature-api-keys.md` via `~/.openbb_platform/user_settings.json` — install writes the initial empty `credentials: {}`; the Python merge script then populates the dynamic per-provider credential schema.
- **depended-on-by** `feature-platform-rest-api.md` — the Python REST API runs inside the `openbb` env this feature creates; it reads `system_settings.json` for `api_settings`, `python_settings`, `debug_mode` (which `update_openbb_settings_impl` inserts).
- **shares-state-with** `feature-uninstall.md` via `system_settings.json.install_settings.installation_directory` — uninstall reads the same key to know what to nuke.
- **shares-state-with** `feature-extensions.md` via `~/.openbb_platform/environments/openbb.yaml`.

## TS port mapping

Condensed from the v1 translation table — only the rows that change shape or carry hidden contracts:

| Tauri call | TS port equivalent | Notes |
|---|---|---|
| `select_directory` | Electron `dialog.showOpenDialog({properties:['openDirectory']})`; web: agent shell-out | Tauri shells to `osascript` / PowerShell `FolderBrowserDialog` / `zenity`→`kdialog`→`python3+GTK`→`dialog`. Replace with native dialog API; don't preserve the shell-out chain. |
| `install_to_directory` | `POST /agent/install/prepare` | Pure FS work; port verbatim BUT preserve read-modify-write semantics on `user_settings.json` (do NOT overwrite if file exists; only set `preferences.data_directory`). Use a timezone-aware ISO string for `installation_date` (Luxon `DateTime.local()` or document UTC drift). |
| `install_conda` | `POST /agent/install/conda` returning a job id; stream over WebSocket | Replace `window.emit("install-progress", ...)` with WS frame `{type:"install-progress", step, progress, message}`. Use Node `https` / `undici` instead of curl/reqwest. On Windows, set `windowsHide: true` (equivalent of `CREATE_NO_WINDOW = 0x08000000`). Keep the 10 MB sanity check. On Apple Silicon, shell to `sysctl` for CPU brand — Node `os.arch()` does NOT detect Rosetta. |
| `setup_python_environment` | `POST /agent/install/python-env` + same WS | Same progress event scheme. Drop the `installation-directory` emit (dead during wizard). Reduce the three `update_openbb_settings` calls to one. |
| `abort_installation` | `POST /agent/install/abort` or WS `{type:"abort"}` | Must release the in-progress lock (current Rust doesn't — fix it). Use `tree-kill` or `taskkill` for Windows. |
| `install_extensions` | `POST /agent/install/extensions` | Rust silently ignores the `directory` payload field — drop or honor consistently. No progress events today; consider adding WS streaming for parity with env-page `create_environment`. |
| `execute_in_environment` | `POST /agent/exec` | Returns `{stdout, stderr, exit_code}` — keep that shape; do not turn non-zero exit into HTTP 5xx, the wizard relies on it resolving. |
| `update_openbb_settings` | `POST /agent/openbb-settings/update` | Spawns Python inside the env to merge JSON via `UserService`/`SystemService`. Reimplementing in Node loses the dynamic credentials-schema population. Either keep the Python script or recreate the schema from a shared source. |
| `get_installation_state` | `GET /agent/install/state` | Snake_case. The "valid install" rule (parse settings + verify conda exe) must match exactly or the redirect gate diverges. |
| `get_installation_status` | `GET /agent/install/status` | Camel_case. Can be eliminated if WS is reliable. |
| `create_default_backend_services` | `POST /agent/backends/create-defaults` | Two entries with hard-coded `127.0.0.1:6900` (OpenBB API) and `127.0.0.1:6900`/`8001` (MCP). Duplicate-name insert silently fails — preserve that idempotency. |
| event `install-progress` | WS `{type:"install-progress", ...}` | — |
| event `installation-directory` | drop | Dead during wizard; never reached on next-boot redirect either. |
| event `installation-status` | drop | Never emitted by current Rust. |

**Port-time gotchas to preserve:**

1. The **no-whitespace Zod rule** is load-bearing because conda activation scripts embed paths unquoted in shell scripts at `update_openbb_settings_impl` and `execute_in_environment_impl`. Don't drop it without fixing the scripts.
2. Phase transitions are driven by **message-substring matching** in three places (Rust `update_installation_state`, Rust `report_progress`, React listener). The substrings (`"completed"`, `"environment set up successfully"`, `"Installation completed successfully"`, `"openbb installation complete"`) are a brittle contract — either keep the exact strings or refactor all three together with a typed phase enum.
3. **Full `window.location.href` reloads** at three transitions (index→target, completion→`/environments`, Try Again→`/setup`) are deliberate: they re-run the boot snapshot. Preserve this semantic in Electron (`mainWindow.loadURL(...)`) / web (server-side redirect with hard refresh).
4. **The managed `InstallationState` is a one-shot snapshot.** Successful install completion never mutates it; the redirect-via-reload pattern is what re-runs the check. If you make this state mutable (Arc<RwLock>), update it on completion AND remove the reload, or keep both for safety.
5. `directory` is sent on every install IPC from JS but only honored by `install_conda` / `execute_in_environment` / `abort_installation` / `install_to_directory`. `install_extensions` re-reads from `system_settings.json` instead. Decide whether to honor the payload field uniformly or document it as decorative.

## Known bugs and port-time fixes

> Each entry: bug → file:line → fix recommendation. Where v2 contradicted v1, v2 wins.

1. **Dead `installation-status` event listener.** `index.tsx:15-24` listens; nothing emits. The 2 s `setTimeout` fallback always wins. → **Port: drop both the listener and the event name.**
2. **`INSTALLATION_IN_PROGRESS` mutex leaked on abort.** `startup.rs:968-1095` resets `INSTALLATION_STATE` but never touches `INSTALLATION_IN_PROGRESS` (decl at `startup.rs:434`). Subsequent install attempts return `"Installation is already in progress"` and the UI falls back to monitoring mode silently. → **Port: release in the abort handler, or collapse the two mutexes into one struct with atomic reset.**
3. **Wizard's `openbb-build` invoke is a default-case no-op.** `installation-progress.tsx:1157` calls `execute_in_environment("openbb-build", ...)`. `openbb-build` only exists if the bare `openbb` pip package is installed, which the default extension set never includes. `execute_in_environment` returns `{exit_code: !=0}` rather than rejecting, so the wizard proceeds silently. → **Port: either install `openbb` unconditionally, gate the invoke on `selectedExtensions.includes("openbb")`, or actually inspect the return object.**
4. **Three redundant `update_openbb_settings` calls per happy path.** Once at end of `setup_python_environment` (`startup.rs:1293`), once after `install_extensions` (`installation-progress.tsx:1162`), once on `handleContinue` (`installation-progress.tsx:1207`). The Python script is idempotent but each run spawns Python and re-parses both JSON files. → **Port: reduce to one call after extensions are installed.**
5. **`check_installation_on_startup` runs twice per boot.** Once at `main.rs:491` (inside `.manage(...)`), once at `main.rs:552` (inside `.setup(...)`). Duplicate disk reads. → **Port: cache the result.**
6. **`update_installation_state` substring cascade is brittle.** `startup.rs:56-137` matches `"download complete"` before `"complete"`, so the order in the cascade matters. Adding any progress message containing `"complete"` can flip `is_complete=true` prematurely. → **Port: replace with a typed phase enum on the wire.**
7. **`install_to_directory` Zod-bypassed retry pre-fill.** After a failed install, `setup.tsx:53-76` re-pre-fills `${homeDir}/OpenBB` defaults instead of reading the prior `install_settings.installation_directory`. The user loses the path they entered. → **Port: read existing `system_settings.json` and pre-fill from `install_settings` if present.**
8. **Partial-install state leaves user on `/environments` with no env.** If `setup_python_environment` fails after `install_conda` succeeded, `is_installed` returns true (conda exe exists) → user is routed past the wizard but has no usable env and no resume affordance. → **Port: add a "Repair" / "Resume" entry point, or tighten `is_installed` to also check the openbb env existence.**
9. **`select_directory` cancel marker mismatch.** Rust returns `Err("No directory selected"...)`; frontend filters on substring `"User canceled"`. Cancel falls through to error path. → **Port: align both sides on a single cancel sentinel.**
10. **`installation-directory` emit is dead during wizard.** `startup.rs:1307` emits after `setup_python_environment` completes, but the only listener is at `index.tsx:27`, which the user is not on. → **Port: drop.**
11. **`get_installation_state` (snake) vs `get_installation_status` (camel) casing inconsistency.** Two adjacent commands hand back different casing conventions. → **Port: pick one and rename both.**
12. **`localStorage.setItem("installationDirectory", ...)` write with no reader.** `index.tsx:30` writes; no `src/` consumer. → **Port: drop.**
13. **`SystemService.write_to_file` would wipe `install_settings`.** The key is not in `SYSTEM_SETTINGS_ALLOWED_FIELD_SET` (`system_service.py:16-27`). Any future caller that uses `SystemService.write_to_file` instead of the hand-rolled merge in `update_openbb_settings_impl` will silently destroy the install marker. → **Port: never serialize `system_settings.json` through `SystemService`; always read-modify-write the raw JSON, or add `install_settings` to the allow-set upstream.**
14. **`release_guard()` is panic-unsafe.** `startup.rs:454-457` defines a closure called in each Err branch but skipped on panic. `Mutex::lock().unwrap()` poisons on panic and cascades into permanent install failure. → **Port: use `parking_lot::Mutex` (no poisoning) or explicit recovery.**
15. **Linux folder picker has a hidden dep.** `helpers.rs:1471-1590` falls through zenity → kdialog → python3+GTK → dialog. A box with none installed yields a broken wizard. → **Port: replace with Electron `dialog.showOpenDialog` or document the dependency.**
16. **Wizard offers Python 3.14 with no wheel-availability check.** `InstallComponents.tsx:73`. 3.14 conda env creation may fail with an opaque solver error. → **Port: either drop 3.14 or pre-flight the solver.**
17. **`cleanup_all_processes` runs on Cancel from setup form.** Wasted 10 s timeout against empty jupyter/backend lists. → **Port: skip cleanup when no env exists yet.**
18. **`directory` payload field is partially decorative.** See port-time gotcha #5 above. → **Port: unify.**

## Open questions

1. **WebSocket vs SSE vs IPC channel for `install-progress`?** The Tauri event is window-scoped and trivial. A REST-based port needs a long-lived channel; SSE is simpler than WS if you only need server→client.
2. **Is the boot-time `InstallationState` snapshot worth keeping?** The reload-on-completion pattern is awkward but cheap and matches the redirect-after-install-failure flow. Making it mutable removes reloads but introduces a new sync surface across the tray menu and uninstall page.
3. **Should the port honor `directory` uniformly or treat `system_settings.json` as the single source of truth?** The current code drifts between the two. The latter is simpler but loses the ability to install to a non-canonical path during a single session.
4. **Should `update_openbb_settings_impl` continue to spawn Python inside the env, or be reimplemented in Node?** The Python script populates the dynamic per-provider credentials schema from `ProviderInterface`; replicating that in Node requires a parallel registry. Spawning Python is slow but factually correct.
5. **Does the port need a "Resume install" or "Repair env" mode?** Today, partial-install states are unrecoverable without tray-Uninstall-then-reinstall. A repair flow would justify removing several of the redundant `update_openbb_settings` calls.
6. **Should `installation_date` be UTC or local?** Current behavior is `chrono::Local::now().to_rfc3339()`. Node `new Date().toISOString()` is UTC. Date display elsewhere (tray, uninstall page) may assume local. Pick a single convention.
7. **Where does the extension catalog live?** Today three JSON files on `raw.githubusercontent.com/OpenBB-finance/OpenBB/main/assets/extensions/`. A port could bundle these for offline installs, or proxy through the agent for caching / corp-proxy support. Direct `fetch()` from the renderer also implies the renderer needs unrestricted outbound HTTPS — relevant for sandboxed Electron / CSP-restricted web ports.
