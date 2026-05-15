# Deep-Dive: Setup Wizard + Installation Pipeline

> Raw findings from Wave 1 agent. Source of truth for `20-features/feature-installation.md`
> and `20-features/feature-extensions.md`. Generated 2026-05-15.

## 0. Cast of characters

Frontend (TanStack Router, single SPA window):
- `/home/user/OpenBBPort/desktop/src/routes/index.tsx` — `/` redirect gate
- `/home/user/OpenBBPort/desktop/src/routes/setup.tsx` — Step 1 form
- `/home/user/OpenBBPort/desktop/src/routes/installation-progress.tsx` — Steps 1–3 driver
- `/home/user/OpenBBPort/desktop/src/components/InstallComponents.tsx:52` — `PythonVersionSelector`

Rust (Tauri commands + global state):
- `/home/user/OpenBBPort/desktop/src-tauri/src/main.rs` — `InstallationState` (managed state), `check_installation_on_startup`, `quit_application`, `navigate_to_page`, `invoke_handler` registry at `main.rs:492–550`
- `/home/user/OpenBBPort/desktop/src-tauri/src/tauri_handlers/startup.rs` — `INSTALLATION_STATE` (global mutex), `INSTALLATION_IN_PROGRESS` mutex, `install_to_directory`, `install_conda`, `setup_python_environment`, `abort_installation`, `get_installation_status`, `create_default_backend_services`
- `/home/user/OpenBBPort/desktop/src-tauri/src/tauri_handlers/helpers.rs` — `get_home_directory`, `select_directory`, `check_directory_exists`, `get_installation_directory`, `update_openbb_settings`, `FileSystem`/`EnvSystem` traits
- `/home/user/OpenBBPort/desktop/src-tauri/src/tauri_handlers/environments.rs` — `install_extensions` (`:2682`), `execute_in_environment` (`:3235`)

Two parallel Rust state objects (do not confuse them):
1. `main.rs:67` `struct InstallationState { is_installed: bool, installation_directory: Option<String> }` — managed via `tauri::State`, populated **once** at app startup by `check_installation_on_startup()` (`main.rs:272-386`). Read by `get_installation_state` (`main.rs:388`). Never mutated after startup.
2. `startup.rs:15-25` `static INSTALLATION_STATE: Lazy<Mutex<InstallationState>>` with fields `is_downloading / is_installing / is_configuring / is_complete / message: String`. This is the live progress mirror, mutated from inside `install_conda` and `setup_python_environment`. Read by `get_installation_status` (`startup.rs:34`).

Plus `startup.rs:434` `static INSTALLATION_IN_PROGRESS: Lazy<Mutex<bool>>` — a re-entrancy guard for `install_conda`.

---

## 1. ENTRY: `/` route redirect gate (`index.tsx`)

**Trigger**: app loads at `/`. Component mounts (`index.tsx:9`).

**Two parallel paths race; first to resolve wins**:

a) Event listeners registered immediately (`index.tsx:15-31`):
- `listen<boolean>("installation-status", ...)` → resolve `/environments` if `true`, `/setup` if `false`.
- `listen<string>("installation-directory", ...)` → write to `localStorage.installationDirectory`.

b) 2-second `setTimeout` fallback (`index.tsx:34-50`) calls `invoke<{is_installed: boolean}>("get_installation_state")` → resolves to `/environments` or `/setup`. On any error, defaults to `/setup`.

> ⚠️ BUG: `installation-status` is **never emitted by the Rust side** (only `installation-directory` at `startup.rs:1307`). So the timeout always wins. The event listener exists but is dead code.

Resolution then triggers `window.location.href = targetRoute` (`index.tsx:63`) — full page reload.

**Server-side gate (parallel)**: `main.rs:779-786` runs *during Tauri setup* and, if `check_installation_on_startup()` returned `is_installed=false`, calls `window.eval("localStorage.clear(); window.location.href = '/setup'")` directly. So in practice the backend force-navigates to `/setup` before React even renders `/`. Conversely, valid installation triggers `localStorage.setItem('environments-first-load-done', 'true')` (`main.rs:799`).

### What "valid installation" means (`main.rs:272-386`)

1. `~/.openbb_platform/system_settings.json` must exist and parse as JSON (env var `HOME` or `USERPROFILE` for cwd).
2. JSON must contain either `install_settings.installation_directory` or top-level `installation_directory` (string).
3. The conda exe at `<dir>/conda/bin/conda` (Unix) or `<dir>/conda/Scripts/conda.exe` (Windows) must exist on disk.

If any of those fails, `is_installed = false` → setup is shown.

---

## 2. STEP 1 — `/setup` form (`setup.tsx`)

### 2.1 Mount: load default home

`setup.tsx:53-76` `useEffect`:
- **Invoke**: `get_home_directory` → `string`.
- **Rust**: `helpers.rs:1376` (sync). Returns `env_sys.home_dir()` (`std::env::home_dir().unwrap()` at `helpers.rs:166-168`).
- **State update**: sets `defaultHome` and pre-fills form via React Hook Form `setValue` with `${homeDir}/OpenBB` and `${homeDir}/OpenBBUserData` (Windows path-sep variant detected via `navigator.userAgent.includes("Windows")`).
- **Errors**: caught, displayed via `setErrorMessage`.

### 2.2 Browse buttons (`setup.tsx:143-160`)

- **Invoke**: `select_directory` with `{ prompt: string }` → `string`.
- **Rust**: `helpers.rs:1594`. Spawns OS-native folder picker — **not the Tauri dialog plugin**:
  - macOS (`helpers.rs:1389-1427`): `osascript -e <AppleScript>` invoking `choose folder` with home as default.
  - Windows (`helpers.rs:1429-1469`): `powershell -Command <script>` using `System.Windows.Forms.FolderBrowserDialog`, with `CREATE_NO_WINDOW` flag.
  - Linux (`helpers.rs:1471-1590`): tries `zenity --file-selection --directory`, then `kdialog --getexistingdirectory`, then a Python GTK FileChooserDialog, then `dialog --inputbox`. Returns trimmed stdout.
- **On cancel**: returns `Err("No directory selected"...)`. Frontend swallows on substring match `"User canceled"` (note: the Rust never returns that exact phrase, so cancel falls through to the error path setting `errorMessage`).
- **State**: `setValue(field, selectedDir, { shouldValidate: true })` triggers Zod re-validation.

### 2.3 Zod validation (`setup.tsx:13-22`)

```ts
const formSchema = z.object({
  installDir: z.string().min(1, "Installation directory is required")
    .refine(v => !/\s/.test(v), { message: "Path cannot contain spaces" }),
  userDataDir: z.string().min(1, "User data directory is required")
    .refine(v => !/\s/.test(v), { message: "Path cannot contain spaces" }),
});
```

Two fields, both required, **no whitespace** anywhere in the path (this is enforced because conda activation scripts later embed the path unquoted in shell commands).

### 2.4 Submit — "Begin Installation" (`setup.tsx:79-114`)

Re-entrancy guarded by `isSubmittingRef.current`.

Step A — overwrite check:
- **Invoke**: `check_directory_exists` with `{ path: string }` → `boolean`.
- **Rust**: `helpers.rs:1365` → `Path::new(&path).exists()`.
- If `true`: shows native dialog via `confirm()` from `@tauri-apps/plugin-dialog` (`setup.tsx:97-100`). On "no" → reset guard, return.

Step B — `proceedWithInstallation` (`setup.tsx:117-140`):
- `setIsLoading(true)`.
- **Invoke**: `install_to_directory` with `{ directory: string, userDataDirectory: string }` → `boolean`.
- **Rust**: `startup.rs:419` → `install_to_directory_impl` (`startup.rs:192-416`). This is **prep only**, not the Conda install. It:
  1. Trims paths; rejects empties.
  2. `fs.create_dir_all` for both directories if missing.
  3. `check_directory_permissions` (`startup.rs:139`) on each: writes `.permission_test_file`, reads it, deletes it; creates `.permission_test_dir`, removes it. Failure → `"... directory is not writable: ..."` etc.
  4. Resolves `$HOME` (or `$USERPROFILE`) → builds `~/.openbb_platform`, `mkdir -p`.
  5. Writes `~/.openbb_platform/user_settings.json` with `{credentials:{}, preferences:{data_directory:"<userDataDir>"}, defaults:{}}`. If file already exists, parses it, ensures `preferences` key exists, updates only `data_directory` if it changed (`startup.rs:295-353`).
  6. Writes `~/.openbb_platform/system_settings.json` — read existing or `{}`, set/overwrite key `install_settings = {installation_directory, user_data_directory, installation_date: chrono::Local::now().to_rfc3339()}`. Other top-level keys in system_settings are preserved.
- **No events emitted** during this step.
- On success: navigate to `/installation-progress` with `?directory=...&userDataDir=...` via TanStack Router (`setup.tsx:126-132`).
- On failure: `setErrorMessage(...)`, releases guard.

### 2.5 Cancel button (`setup.tsx:294-311`)

- Confirms via `confirm()` plugin dialog.
- **Invoke**: `quit_application` (no payload).
- **Rust**: `main.rs:413`. Awaits `cleanup_all_processes(app_handle)` (stops all Jupyter servers w/ 3s timeout, all backend services w/ 3s timeout, both wrapped in 10s outer timeout — `main.rs:419-467`), then `app_handle.exit(0)`.

---

## 3. STEP 1 (cont.) — `install_conda` driven from `/installation-progress`

The component reads search params (`installation-progress.tsx:742-744`) and starts on `useEffect` (`:814-1052`).

### 3.1 Event listener setup

Before invoking, registers `listen<InstallProgress>("install-progress", ...)` (`installation-progress.tsx:821`). Payload schema (Rust `startup.rs:27-32`):
```ts
interface InstallProgress { step: string; progress: number; message: string; }
```

Listener (`:823-911`) routes by `step`/`message` substrings:
- `step.includes("install") && message.includes("Miniforge installation completed" | "completed")` → `setPhase("version_select")` and pause status polling.
- `step.includes("config") && message.includes("environment set up successfully")` → `setPhase("extension_select")`.
- `step.includes("download")` → `setPhase("downloading")`.
- `step.includes("install")` → `setPhase("installing")`.
- `step.includes("config")` → `setPhase("configuring")`.
- `step.includes("complete")` → if message contains "Installation completed successfully" / "openbb installation complete" → `setPhase("complete")`, else just update message.

Status polling: `setInterval(checkInstallationStatus, 2000)` (`:927`).
- **Invoke**: `get_installation_status` → `InstallationStatus` JSON (`startup.rs:34-54`).
- Used as a fallback/heartbeat; ignored when `phase === "version_select" | "extension_select"`.

### 3.2 The `install_conda` invoke

- **Invoke**: `install_conda` with `{ directory: string, userDataDir: string }` → `boolean`.
   - NB: `userDataDir` is in the payload but the Rust signature `install_conda(directory: String, window: Window)` (`startup.rs:437`) **ignores it**.
- **Rust** (`startup.rs:437-905`):

  Re-entrancy: locks `INSTALLATION_IN_PROGRESS`. If already `true` returns `Err("Installation is already in progress...")`. Frontend treats that error specially (`installation-progress.tsx:981-987`) — switches to monitoring mode silently.

  Two closures: `report_progress(step, progress, message)` and `report_fatal_error(message)`. Both:
  - Mutate `INSTALLATION_STATE` (set boolean phase flags from `step`/`message` substring match — `startup.rs:485-508`).
  - `window.emit("install-progress", &InstallProgress {...})`.

  Steps:
  1. `report_progress("download", 0.05, "Preparing installation directory")`.
  2. Create `<directory>/`, then nuke and recreate `<directory>/conda/` (3 retries with 300 ms sleep — `startup.rs:540-579`).
  3. `report_progress("download", 0.15, "Detecting system architecture")` → `detect_architecture` (`startup.rs:909`). On macOS prefers `sysctl -n machdep.cpu.brand_string` and `uname -m` to detect Apple Silicon vs Rosetta. Otherwise maps `std::env::consts::ARCH`.
  4. `fetch_miniforge_installer_url(arch)` (`startup.rs:1103-1225`): `reqwest` GET `https://api.github.com/repos/conda-forge/miniforge/releases` (with hard-coded Chrome UA), sorts by `published_at`, picks first asset matching `Miniforge3-{os}-{arch}.{ext}`. Falls back to `releases/latest/download/Miniforge3-{os}-{arch}.{ext}` URL.
  5. `report_progress("download", 0.2, "Using installer: <url>")`.
  6. Create `<TEMP>/openbb_installer/`. Installer path: `miniforge_installer.sh` (Unix) or `.exe` (Windows). Removes any existing.
  7. `report_progress("download", 0.25, "Downloading Miniforge installer")`.
  8. **Download** (`startup.rs:644-719`):
     - Unix: `curl --http1.1 -L -o <path> --fail --retry 3 --connect-timeout 30 --silent --show-error <url>`.
     - Windows: `reqwest::get(url).await.bytes().await` → `std::io::copy` to file.
  9. Sanity check: file size ≥ 10 MB else fatal (`startup.rs:740-745`).
  10. `report_progress("install", 0.5, "Download complete. Preparing installation")`.
  11. Unix: `chmod +x <installer>` (`startup.rs:751-769`).
  12. `report_progress("install", 0.55, "Running Miniforge installer")`.
  13. **Run installer** (`startup.rs:772-823`):
      - Windows: `cmd /C start /B /WAIT <installer> /InstallationType=JustMe /RegisterPython=0 /AddToPath=0 /S /D=<conda_dir>` with `creation_flags(0x08000000)`.
      - Unix: `bash <installer> -b -u -p <conda_dir> -f` (batch mode, **bypasses MD5**, force).
  14. Non-zero exit → fatal with stdout+stderr.
  15. `report_progress("install", 0.9, "Conda installation completed successfully")`.
  16. Writes `<conda_dir>/.condarc` (`startup.rs:847-876`):
      ```
      channels: [defaults, conda-forge]
      envs_dirs: [<conda_dir>/envs]
      pkgs_dirs: [<conda_dir>/pkgs]
      auto_activate_base: false
      pip_interop_enabled: true
      remote_connect_timeout_secs: 60 / read 120 / max_retries 5
      ```
  17. Removes installer file.
  18. Verifies `<conda_dir>/{bin/conda | Scripts/conda.exe}` exists; missing → fatal.
  19. `report_progress("complete", 1.0, "Conda installation completed successfully")` — note this triggers `is_complete=true` in `INSTALLATION_STATE` but the **frontend listener only marks "complete" if message contains "Installation completed successfully"** — this message says "Conda installation completed successfully", so it does NOT trigger the success modal. The substring `"Miniforge installation completed"` triggers the version-select transition; this final message does too via the `step==="complete"` else branch.
  20. Releases `INSTALLATION_IN_PROGRESS`. Returns `Ok(true)`.

- **React side**: when invoke returns and current phase is not `version_select|configuring|complete`, force-transitions to `version_select` (`installation-progress.tsx:952-968`).

---

## 4. STEP 2 — Python version selection

**UI** (`installation-progress.tsx:1362-1368`): `<PythonVersionSelector onSelectVersion={handleVersionSelect} />`. Defaults to `"3.13"`, options `["3.10","3.11","3.12","3.13","3.14"]` (`InstallComponents.tsx:73`). Calls back with the chosen string only — no invoke yet.

**"Next Step" button** (`installation-progress.tsx:1544-1551`) → `handleVersionNext` (`:1111-1134`):
- `setPhase("configuring")`.
- **Invoke**: `setup_python_environment` with `{ directory: string, pythonVersion: string }` → `boolean`.
- **Rust** (`startup.rs:1228-1313`):
  1. Local closure `report_progress(step, progress, message)` calls `update_installation_state` AND `window.emit("install-progress", ...)`.
  2. `report_progress("config", 0.6, "Setting up Python <v> environment")`.
  3. `validate_conda_installation(<conda_dir>)` (`startup.rs:1317-1332`) — checks executable presence.
  4. `prepare_environment` (`startup.rs:1334-1394`): if `<conda_dir>/envs/openbb` exists, runs `conda env remove -n openbb -y` (`config 0.65`). Then `conda install -n base -c conda-forge conda conda-libmamba-solver --solver=classic -y --quiet` (`config 0.7`, "Updating conda to latest version"). Failures here are logged but non-fatal.
  5. `generate_environment_yaml` (`startup.rs:1483-1533`): writes `~/.openbb_platform/environments/openbb.yaml`:
      ```yaml
      name: openbb
      channels: [conda-forge, defaults]
      dependencies:
        - python={version}
        - nodejs
        - pip
        - setuptools
        - pip:
          - notebook
          - jupyterlab-lsp
          - "python-lsp-server[all]"
          - jupyterlab-latex
          - "anywidget[dev]"
          - ipywidgets
          - openbb-platform-api
          - openbb-mcp-server
      ```
  6. `create_environment_from_yaml` (`startup.rs:1396-1427`): `report_progress("config", 0.80, "Initializing environment")` then `conda env create -f <yaml> -y`. Failure here is **fatal** (returns Err).
  7. Calls `helpers::update_openbb_settings_impl(<conda_dir>, "openbb", ...)` — see §6 for what that does. Failures are logged-only.
  8. `report_progress("complete", 1.0, "Installation complete")` — note: substring "Installation complete" without "completed successfully" — this puts INSTALLATION_STATE.is_complete=true via `update_installation_state` (`startup.rs:93` checks "complete" or "success"), but the React listener is in `step.includes("complete")` branch and the message check `"Installation completed successfully" | "openbb installation complete"` fails → falls through to the "sub-component completion" log, no UI complete. Phase remains `configuring` until invoke returns.
  9. `window.emit("installation-directory", &directory)` — only event of this name in the entire codebase. Index.tsx listener catches this if user reloads.
  10. Returns `Ok(true)`.

  > ⚠️ The listener check `step.includes("config") && message.includes("environment set up successfully")` (`installation-progress.tsx:855-872`) never matches — Rust never emits that message. The `setPhase("extension_select")` for this transition therefore happens via the imperative `await` then `setPhase("extension_select")` at `installation-progress.tsx:1124`.

- **React side after invoke returns**: `setPhase("extension_select")` and `setMessage("Select extensions to install")`. On error: `setError(...)`, `setPhase("failed")`.

`new_conda_command` (`helpers.rs:154-165`) wraps the conda exe with these env vars on every spawn:
- `CONDA_ROOT=<conda_dir>`
- `CONDA_ENVS_PATH=<conda_dir>/envs`
- `CONDA_PKGS_DIRS=<conda_dir>/pkgs`
- `CONDARC=<conda_dir>/.condarc`
- Removes `CONDA_DEFAULT_ENV`, `CONDA_PREFIX`, `CONDA_SHLVL`.
- Windows: `CREATE_NO_WINDOW`.

---

## 5. STEP 3 — Extension selection & installation

### 5.1 Extension catalog fetch (frontend-only)

`installation-progress.tsx:186-275` — direct `fetch()` (no Tauri) to:
- `https://raw.githubusercontent.com/OpenBB-finance/OpenBB/refs/heads/main/assets/extensions/provider.json`
- `.../extensions/router.json`
- `.../extensions/obbject.json`

Each entry is shape `ExtensionSource { packageName, reprName?, description?, credentials?: string[], instructions?: string|null }` (`installation-progress.tsx:23-29`). Default-selected: any extension with no credentials (excluding `extras`/`openbb-cli`) plus a hard-coded `alwaysInclude` list (`fred, bls, us-eia, nasdaq, fmp, econdb, cftc, congress-gov` — `:239-260`).

Plus two hard-coded `extrasExtensions`: `openbb-cli`, `openbb-cookiecutter`.

User can add/remove free-text PyPI packages (`customPackages`).

### 5.2 "Install" button → `handleInstallExtensions` (`installation-progress.tsx:1137-1184`)

`allPackages = [...selectedExtensions, ...customPackages]`. Three serial invokes:

a) **Invoke**: `install_extensions` with `{ extensions: string[], environment: "openbb", directory: string }` → `boolean`.
   - **Rust**: `environments.rs:2682` → `install_extensions_impl` (`:2344-2679`). Note the param `directory` from JS is **silently ignored** — the impl re-reads from `system_settings.json` via `get_installation_directory_impl`.
   - Splits packages: prefix `conda:` → conda install, otherwise pip. Special-cases `openbb` to install with `--no-deps`, then runs `<env>/bin/openbb-build` directly.
   - Spawns:
     - `conda install -y -n openbb <conda_pkgs...>` if any.
     - `<env>/bin/python -m pip install <pip_pkgs...>` if any.
     - `<env>/bin/python -m pip install openbb --no-deps` if openbb in list, then `<env>/bin/openbb-build`.
   - Non-zero exits return Err (except openbb, which is just warned).
   - Updates the YAML file at `~/.openbb_platform/environments/openbb.yaml` with the new pip dependency list.

b) **Invoke**: `execute_in_environment` with `{ command: "openbb-build", environment: "openbb", directory: string }` → `{stdout, stderr, exit_code}` (serde_json::Value).
   - **Rust**: `environments.rs:3235` → builds a temp shell script (`<TEMP>/openbb_console_command.sh` or `.bat`) that exports `CONDA_ROOT/.../CONDARC`, sources `<conda>/bin/activate openbb`, then runs the command. Spawns via `sh <script>` / `cmd.exe /c <script>`. Cleans up. Returns stdout/stderr/exit_code.

c) **Invoke**: `update_openbb_settings` with `{ condaDir: string, environment: "openbb" }` → `void`.
   - **Rust**: `helpers.rs:950` → `update_openbb_settings_impl` (`:687-947`).
   - Writes `<TEMP>/openbb_update_settings.py` with a Python script that:
     - imports `openbb_core.app.service.user_service.UserService` and `system_service.SystemService`,
     - merges any existing keys from `~/.openbb_platform/user_settings.json` and `system_settings.json`,
     - re-writes both with pretty JSON.
   - Builds a shell script (bash/cmd) that sets the same `CONDA_ROOT/...` env, `source <conda>/bin/activate openbb`, then `python <script>`.
   - Failure is logged as a warning — `Ok(())` is always returned.

On any of those returning a JS error: if `isFutureWarningOnly(errMsg)` (`installation-progress.tsx:32-68` — checks for "FutureWarning:" / "DeprecationWarning:" etc, *and* lack of error markers like "Error:", "failed", "exit code") → still mark `setPhase("complete")`. Otherwise `setError(...)`, `setPhase("failed")`.

### 5.3 "Skip" button (`installation-progress.tsx:1187-1193`)

Pure frontend: clears arrays, `setPhase("complete")`. **No backend call** — the env stays as `setup_python_environment` left it (no extensions, no openbb).

---

## 6. COMPLETION — "Done" / `handleContinue` (`installation-progress.tsx:1196-1226`)

Triggered from the success modal.

a) **Invoke**: `update_openbb_settings` with `{ condaDir: directory, environment: "openbb" }` (same as above; called again as a re-sync). Errors swallowed.

b) **Invoke**: `create_default_backend_services` (no payload).
   - **Rust**: `startup.rs:1432` → creates two `BackendService` records via `create_backend_service_impl`:
     - "OpenBB API" — command `openbb-api --host 127.0.0.1 --port 6900`, env `openbb`, `auto_start: false`.
     - "OpenBB MCP" — command `openbb-mcp --transport streamable-http --host 127.0.0.1 --port 8001`, env `openbb`, `auto_start: false`.
   - Errors swallowed.

c) `localStorage.setItem("environments-first-load-done", "true")` — this is the same flag set by `main.rs:799` for already-installed users; it gates `navigate_to_page` (`main.rs:401-405`).

d) `window.location.href = "/environments?directory=...&userDataDir=..."` — full reload (deliberate, to re-run `main.rs`'s `check_installation_on_startup` & populate `tauri::State<InstallationState>`).

---

## 7. ERROR / CANCEL paths

### 7.1 Mid-install Cancel (`handleCancel`, `installation-progress.tsx:1253-1284`)

Available during `phase ∈ {downloading, installing, configuring}` (button rendered at `:1411-1420`) and on the version-select step (`:1540`).

1. `setIsCancelling(true)`, `setPhase("cancelling")`. The `isCancelling` ref blocks all event/poll-driven UI updates throughout the file.
2. Clears the status-poll interval.
3. **Invoke**: `abort_installation` with `{ directory: string }` → `void`.
4. **Rust** (`startup.rs:1098-1101` → `abort_installation_impl` `:968-1095`):
   - Resets `INSTALLATION_STATE` flags to all-false, `message = "Installation cancelled by user"`.
   - Process-killing:
     - Windows: `taskkill /F /FI "WINDOWTITLE eq *<dir>*openbb*" /IM cmd.exe` and same for `*conda*` / `python.exe` (`startup.rs:986-1016`).
     - Unix: `pkill -f "<dir>/conda.*install"`, `pkill -f "source.*<dir>/conda.*"`, `pkill -f "bash.*openbb_install.*<dir>.*"` (`:1018-1045`).
   - If `<dir>/conda/envs/openbb` exists, `rm -rf` it.
   - If `<TEMP>/openbb_installer/` exists, rm it AND `rm -rf <dir>` (note: the entire install dir is wiped — `:1073-1076`).
   - Removes leftover temp scripts: `openbb_install_extensions.{sh,bat}`, `openbb_install_packages.{sh,bat}`, `openbb_get_versions.{sh,bat}`.
5. Frontend: `setPhase("cancelled")`, "Return to Setup" button (`:1556-1564`) navigates to `/setup`.
6. > ⚠️ BUG: `INSTALLATION_IN_PROGRESS` mutex is **not** released here. If user retries without restarting the app, `install_conda` returns `"Installation is already in progress..."` and the frontend silently falls back to monitoring mode. Stale guard.

### 7.2 Install failure ("Try Again" / "Continue Anyway" — `installation-progress.tsx:1228-1250`, render at `:1490-1532`)

- "Try Again": `localStorage.clear()`, `window.location.href = "/setup"`. No backend call.
- "Continue Anyway": `localStorage.setItem("environments-first-load-done","true")`, redirect to `/environments`. No `update_openbb_settings`, no `create_default_backend_services`. UI warning enumerates the consequences.

### 7.3 React state map

```
phase: "preparing" → "downloading" → "installing" → "version_select"
   → "configuring" → "extension_select" → "configuring" → "complete"

failure branches: "failed" (with error string)
cancel branches: "cancelling" → "cancelled"
```

Other relevant React state:
- `message: string` (animated with `ellipsis` cycling `'' → '.' → '..' → '...'`).
- `isComplete: boolean` (gates the success modal).
- `isCancelling: boolean` (gates everything).
- `selectedVersion: string|null`.
- `selectedExtensions: string[]`, `customPackages: string[]`.
- `installationStartedRef`, `ellipsisTimerRef`, `statusCheckIntervalRef` (refs, not state).

---

## 8. Files written / read by the wizard

Written:
- `~/.openbb_platform/user_settings.json` (created/updated by `install_to_directory_impl` and `update_openbb_settings_impl`)
- `~/.openbb_platform/system_settings.json` (created/updated by `install_to_directory_impl` and `update_openbb_settings_impl`)
- `~/.openbb_platform/environments/openbb.yaml` (`generate_environment_yaml`, `install_extensions_impl`)
- `<directory>/conda/.condarc` (post-install in `install_conda`)
- `<directory>/conda/...` (the entire Miniforge tree)
- `<TEMP>/openbb_installer/miniforge_installer.{sh,exe}` (deleted after install)
- `<TEMP>/openbb_update_settings.py` (deleted after run)
- `<TEMP>/openbb_console_command.{sh,bat}` (deleted after run)
- `<userDataDirectory>/` (created if missing; not populated)

Read by `check_installation_on_startup` to decide redirect:
- `~/.openbb_platform/system_settings.json` for `install_settings.installation_directory`
- `<directory>/conda/{bin/conda|Scripts/conda.exe}` (existence check)

External processes spawned: `curl`, `bash`, `cmd`, `chmod`, `osascript`, `powershell`, `zenity`, `kdialog`, `python3` (Linux folder picker), `<conda>/bin/conda`, `<env>/bin/python`, `<env>/bin/openbb-build`, `pkill`, `taskkill`, `sysctl`, `uname`. Plus HTTPS to api.github.com (releases) and raw.githubusercontent.com (extension catalog).

---

## 9. TS port translation table

| Tauri invoke | Payload (JS) | Returns | Equivalent TS-port endpoint | Notes |
|---|---|---|---|---|
| `get_home_directory` | `{}` | `string` | `os.homedir()` (Electron renderer) or `GET /agent/home-dir` | Pure read, no side effects. |
| `select_directory` | `{ prompt?: string }` | `string` | Electron: `dialog.showOpenDialog({properties:['openDirectory']})`. Web: native `<input webkitdirectory>` or remote agent that shells out. | Tauri uses native OS shell-outs; in Electron use the built-in dialog API. |
| `check_directory_exists` | `{ path: string }` | `boolean` | `fs.existsSync(path)` / `GET /agent/exists?path=` | Trivial. |
| `install_to_directory` | `{ directory, userDataDirectory }` | `boolean` | `POST /agent/install/prepare` | Permission-test + write user/system settings JSON. Pure FS work, port verbatim. |
| `quit_application` | `{}` | `void` | `app.quit()` (Electron) / `POST /agent/quit` | Run cleanup of background services first. |
| `install_conda` | `{ directory, userDataDir }` | `boolean` (long-running) | `POST /agent/install/conda` returning a job-id; stream progress over WebSocket / SSE / `ipcMain.on` | Replace `window.emit("install-progress", ...)` with WebSocket message of shape `{type:"install-progress", step, progress, message}`. Also need download (use `node-fetch` / `https.get` instead of curl/reqwest), shell-out to bash/cmd for the installer. Honor `INSTALLATION_IN_PROGRESS`-equivalent in the agent. |
| `get_installation_status` | `{}` | `{phase, isDownloading, isInstalling, isConfiguring, isComplete, message}` | `GET /agent/install/status` | Heartbeat poll; can be eliminated if WebSocket is reliable. |
| `setup_python_environment` | `{ directory, pythonVersion }` | `boolean` (long-running) | `POST /agent/install/python-env` + same WebSocket | Same progress event scheme. Final emit `installation-directory` becomes a WS message of the same shape `{type:"installation-directory", payload:string}`. |
| `abort_installation` | `{ directory }` | `void` | `POST /agent/install/abort` or WS `{type:"abort"}` | Must release the in-progress lock (Tauri impl currently doesn't — fix that). Use `tree-kill` npm or platform pkill/taskkill. |
| `install_extensions` | `{ extensions: string[], environment: "openbb", directory }` | `boolean` (long-running) | `POST /agent/install/extensions` | Note Rust ignores `directory`. Long-running; consider WS progress. |
| `execute_in_environment` | `{ command, environment, directory }` | `{stdout, stderr, exit_code}` | `POST /agent/exec` | Use a shell script with the same env-var preamble (`CONDA_ROOT`/etc.). Echoes stdout/stderr verbatim. |
| `update_openbb_settings` | `{ condaDir: string, environment: string }` | `void` | `POST /agent/openbb-settings/update` | Spawns Python inside the env to merge JSON. Could be reimplemented in Node directly if you don't care about pulling defaults from `openbb_core`. |
| `create_default_backend_services` | `{}` | `void` | `POST /agent/backends/create-defaults` | Writes the OpenBB API + MCP service entries. Trivial JSON write to backends config. |
| `get_installation_state` | `{}` | `{is_installed: boolean, installation_directory: string\|null}` | `GET /agent/install/state` | The "valid install" check: parse `~/.openbb_platform/system_settings.json` and verify `<dir>/conda/{bin\|Scripts}/conda{,.exe}` exists. |

| Tauri event | Payload | Equivalent TS-port channel |
|---|---|---|
| `install-progress` | `{step:string, progress:number, message:string}` | WebSocket `{type:"install-progress", ...}` or Electron `ipcRenderer.on('install-progress')` |
| `installation-directory` | `string` | WebSocket `{type:"installation-directory", payload:string}` — emitted at end of `setup_python_environment` |
| `installation-status` | `boolean` | **Dead in source.** Safe to drop. |

| Frontend dialog API (Tauri plugin) | TS port |
|---|---|
| `confirm(msg, {title, kind})` from `@tauri-apps/plugin-dialog` | Electron: `dialog.showMessageBoxSync({type:'warning', buttons:['OK','Cancel'], message})`. Web: custom modal. |

### Critical port-time gotchas to preserve behavior

1. **Path-no-spaces Zod rule** is load-bearing because conda activation scripts embed paths unquoted. Don't drop it without also fixing the shell scripts in `update_openbb_settings_impl` and `execute_in_environment_impl`.
2. **Installation directory is ignored** by `install_conda` (uses arg) but `install_extensions` and `execute_in_environment` re-read it from `system_settings.json` — the `directory` payload field is partially decorative.
3. **Cancel does not release the in-progress lock** in current Rust (`abort_installation_impl` resets `INSTALLATION_STATE` but not `INSTALLATION_IN_PROGRESS`). Frontend tolerates this via the "already in progress" silent fallback. Decide whether to fix or replicate.
4. **Two parallel "InstallationState" types** must both exist: a snapshot taken at app boot (used for redirect) and a live phase mirror (used during install). Keeping them separate matches the current contract; collapsing them risks breaking the boot-time redirect.
5. **Phase transitions are driven by message-substring matching** in three places (Rust `update_installation_state`, Rust `report_progress` in `install_conda`, React `install-progress` listener). Keep the exact substrings (`"Miniforge installation completed"`, `"environment set up successfully"`, `"Installation completed successfully"`, `"openbb installation complete"`) or refactor all three together.
6. **Index page race**: server-side `window.eval('window.location.href = /setup')` in `main.rs:785` runs before React mounts and beats the React redirect. In an Electron port replicate via `mainWindow.loadURL('file://.../setup')` from the main process. In a web port the server has to send the right initial route.
7. **Full `window.location.href` reloads** (rather than `navigate({to:...})`) are used at 3 transitions: index→target, completion→/environments, try-again→/setup. They're load-bearing because they re-run the boot snapshot of the install state. Preserve this in the port.

---

## Cross-feature dependencies

- **depends-on** `feature-logs-streaming.md` for `install-progress` event semantics (modeled the same way as `process-output`) — but installation uses its OWN event channel (`install-progress`), not the global one.
- **depends-on** `feature-environments.md` for the final environment created (the "openbb" env) — installation seeds `~/.openbb_platform/environments/openbb.yaml` which the Environments page reads later.
- **depended-on-by** `feature-environments.md`, `feature-backend-services.md`, `feature-api-keys.md`, `feature-platform-rest-api.md` — installation is the precondition for all of them.
- **depended-on-by** `feature-extensions.md` (Step 3 of the wizard IS the first extension-install flow).
- **shares-state-with** `feature-backend-services.md` — completion writes default services via `create_default_backend_services`.
- **shares-state-with** `feature-app-shell.md` — `InstallationState` in main.rs is consumed by tray menu and index redirect.
- **shares-state-with** `feature-api-keys.md` via `~/.openbb_platform/user_settings.json` — installation writes the initial empty `credentials: {}`.
