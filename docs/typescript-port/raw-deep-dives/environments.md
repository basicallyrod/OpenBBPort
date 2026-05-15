# Deep-Dive: Environments Page (Conda Env Management + Extensions + Jupyter)

> Raw findings from Wave 1 agent. Source of truth for `20-features/feature-environments.md`,
> `20-features/feature-extensions.md`, and `20-features/feature-jupyter.md`.
> Generated 2026-05-15.

Scope: TS files under `/home/user/OpenBBPort/desktop/src/...`, Rust handlers under `/home/user/OpenBBPort/desktop/src-tauri/src/tauri_handlers/{environments,jupyter,helpers}.rs`, plus `src-tauri/src/utils/process_monitor.rs` and the registration manifest in `src-tauri/src/main.rs:492-550`.

---

## 0. Cross-cutting infrastructure

### 0.1 `process-output` event (the only streaming channel)
- Emitter: `app_handle.emit("process-output", { processId, output, timestamp? })`. Defined in:
  - `environments.rs:60-66` (stdout) and `environments.rs:82-89` (stderr) inside `run_command_with_logging` — payload is `{ processId: String, output: String }` with NO timestamp.
  - `jupyter.rs:148-154` (Jupyter stdout), `jupyter.rs:188-193` (Jupyter stderr), `jupyter.rs:476-482` (shutdown notice). Jupyter payloads include `timestamp: chrono::Utc::now().timestamp_millis()` and the shutdown one also adds `"type": "system"`.
- Subscriber pattern in TS: every site filters by `processId` because the event channel is global. `environments.tsx:600-624` (requirements logs), `environments.tsx:1521-1547` (create-env logs), `environments.tsx:2046-2076` (Jupyter shutdown listener).
- Lines are pre-cleaned in Rust: `clean_output_line` (`environments.rs:14-34`) strips ANSI escape sequences (`\x1B\[[0-9;]*[a-zA-Z]`), processes backspaces, and keeps only the segment after the last `\r` (carriage return collapsing for progress bars). Empty cleaned lines are dropped, never emitted.
- Frontend de-duplicates "prefix repeats" — if the previous log starts with `prefix:` and the new one starts with the same `prefix:`, it overwrites the last line in-place (`environments.tsx:604-621` and again `1525-1544`). This is what makes pip's "Collecting numpy..." progress not spam the log box.

### 0.2 Process registration / log buffer (`process_monitor.rs`)
- Global singleton `LOG_STORAGE: Arc<Mutex<HashMap<String, LogBuffer>>>` (line 9). Each `LogBuffer` is a `VecDeque<LogEntry>` capped at 10,000 entries (`process_monitor.rs:68`); `LogEntry { timestamp, content, process_id }`.
- Two state stores live in Tauri-managed state:
  - `ProcessLogState(LogStorage)` — registered at `main.rs:73`.
  - `RunningProcesses(Arc<Mutex<HashMap<String, Child>>>)` — `process_monitor.rs:106`. NOTE: the Environments page never spawns processes that get tracked here; only `RunningProcesses::add_process` would. The Jupyter handler keeps its own static `ACTIVE_JUPYTER_SERVERS` (`jupyter.rs:9-10`) as `Mutex<HashMap<String, (url, pid)>>`, so for the Environments page `RunningProcesses` is effectively unused.
- Tauri commands wrapping the buffer (registered in `main.rs:532-535`):
  - `register_process_monitoring(processId)` -> bool (`main.rs:75-78`) — creates an empty 10k-deep `LogBuffer` for that id. Idempotent: returns false if already registered.
  - `unregister_process_monitoring(processId)` -> bool.
  - `get_process_logs_history(processId, count?)` -> `Vec<LogEntry>`.
  - `clear_process_logs_history(processId)` -> bool.
- Both Jupyter and create-env flows call `register_process(&log_storage, &process_id)` from Rust (`environments.rs:131`, `jupyter.rs:108`) AND the frontend separately calls `register_process_monitoring` (`environments.tsx:633`, `1919`, `2035`). Double-registration is harmless (idempotent).

### 0.3 `RealEnvSystem::new_conda_command` (`helpers.rs:154-165`)
Every conda/pip subprocess goes through this builder. It sets:
- `CONDA_ROOT`, `CONDA_ENVS_PATH`, `CONDA_PKGS_DIRS`, `CONDARC` (relative to `<install>/conda`)
- Removes `CONDA_DEFAULT_ENV`, `CONDA_PREFIX`, `CONDA_SHLVL` from inherited env
- On Windows applies `CREATE_NO_WINDOW` (0x08000000) flag

### 0.4 The `~/.openbb_platform/` directory layout the page reads
- `system_settings.json` — `install_settings.installation_directory` is the source of truth for `<install>` (read in `environments.rs:165-174` and many others).
- `user_settings.json` — `preferences.working_directory` (`helpers.rs:338`), `preferences.chart_style/table_style` (theme).
- `environments/<env>.yaml` — per-environment authoritative YAML, written by `save_environment_as_yaml_impl` (`helpers.rs:436-503`). Used by update flow & extension YAML mutation.
- `<install>/conda/` — Miniconda/Mamba install. `bin/conda` (Unix) or `Scripts/conda.exe` (Windows). `envs/<name>/` for each environment. `pkgs/`, `.condarc`, `condabin/` configured.
- `<install>/Jupyter/jupyter_config|jupyter_data|jupyter_runtime` — set as env vars when launching Jupyter (`jupyter.rs:88-93`).

---

## 1. Page lifecycle / state map (`environments.tsx`)

Top-level component: `EnvironmentsPage` at `environments.tsx:233`. Keys to know:

| State | Where set | Purpose |
|---|---|---|
| `installDir: string \| null` | `environments.tsx:694-739` (from URL search `directory` or `get_installation_state` then fallback `get_home_directory + "/OpenBB"`) | All conda/pip paths derive from this |
| `currentWorkingDir` | `environments.tsx:543-553` (init via `get_working_directory`), saved via `save_working_directory` effect at `:394-400` | The cwd Jupyter and shell sessions launch in |
| `environments: Environment[]` | `fetchEnvironments` (`:406-434`), `loadEnvironmentsFromCache` (`:451-486`), `updateCacheAfterBackendOperation` (`:489-539`) | List of `{name, pythonVersion, path}`; "base" is always filtered out |
| `extensions: Extension[]` | `refreshEnvironmentUIState` (`:1043-1097`), `showExtensions` (`:1731-1763`) | Currently-displayed environment's package list |
| `environmentPackages: { [env]: Set<string> }` | populated from cache in `:1100-1183`, `:2001-2014` | In-memory lower-cased package-name set; backs `hasJupyterSupport`/`hasIPythonSupport`/`hasCliSupport` |
| `jupyterStatus: { [env]: 'stopped'\|'starting'\|'stopping'\|'running'\|'error' }` | `:1791-1870`, etc. | Per-env Jupyter lifecycle |
| `jupyterUrlRef.current[env]` | useRef (`:300`) | Current URL per env; not state to avoid re-renders |
| `activeServers: Set<string>` (ref) | `:301` | Used to persist "active" snapshot to sessionStorage on unmount (`:1873-1887`) |
| `deletedEnvironments: Set<string>` (ref) | `:375` | Acts as a tombstone so concurrent fetches/listeners don't re-add an env mid-deletion |
| `envCreatedRef`, `creationWarningRef`, `createEnvironmentRef` | `:376-377`, `:320` | Guards against double-fire; allows safe re-entry |
| `isCreatingEnvironment` (Context) | `EnvironmentCreationContext.tsx` | Locks UI elsewhere during create (see §1.1) |

### 1.1 `EnvironmentCreationContext`
- File: `contexts/EnvironmentCreationContext.tsx`
- Single bool `isCreatingEnvironment` + setter, no persistence.
- `environments.tsx:380-382` mirrors `isCreateModalOpen || creationLoading || creatingFromRequirements` into the context.
- It exists so other pages / app chrome (sidebar, terminal, settings, etc., which presumably consume `useEnvironmentCreation`) can disable navigation/quit while a long-running env operation is in flight. The Environments page itself does NOT read the value back — the context is purely for cross-page UI locking.

### 1.2 `ENV_EXTENSIONS_CACHE_KEY` localStorage cache
- Key constant: `"env-extensions-cache"` (`environments.tsx:14`).
- Schema:
  ```ts
  {
    [envName: string]: {
      extensions: Extension[];        // {package, version, install_method, channel}
      pythonVersion: string;
    }
  }
  ```
- **No TTL**. Cache only invalidated by:
  - "Refresh" button — `EnvironmentActionButtons.handleUpdateAndReload` (`environments.tsx:106-109`) does `localStorage.removeItem("env-extensions-cache")` then `window.location.reload()`.
  - End of `createEnvironmentFromRequirements` (`:654`) wipes the entire cache.
  - Per-env delete: `removeEnvironment` (`:1324-1346`) deletes only that env's entry.
- Writers (per-env merges):
  - `updateCacheAfterBackendOperation` (`:510-535`) — adds entries for new envs (extensions: []) and removes entries for envs no longer present in backend list.
  - `refreshEnvironmentUIState` (`:1071-1084`) — overwrites `cache[env].extensions`.
  - `handleInstallExtensions` warning branch (`:1407-1416`).
  - `handleUpdateExtension` (`:1489-1494`).
  - `createEnvironment` success path (`:1602-1608`).
- Readers: `loadEnvironmentsFromCache` (`:451-486`) builds `Environment[]` from cache before backend; `hasJupyterSupport/hasIPythonSupport/hasCliSupport` (`:1195-1295`); `showExtensions` reads cache only and never touches backend (`:1744-1755`).

---

## 2. List environments — `list_conda_environments`

### 2.1 Frontend trigger
- Initial mount: `:542-558` calls `loadEnvironmentsFromCache()` which reads localStorage, builds mock `Environment[]` with `path = ${installDir}/conda/envs/${name}`, and only falls back to `fetchEnvironments` if cache is empty.
- Manual refresh button → `handleUpdateAndReload` (`:106-109`) clears cache + reloads page.
- After every mutation: `updateCacheAfterBackendOperation` (`:489-539`) re-invokes `list_conda_environments` and merges result into cache.
- Error retry: `:2376-2398`.

### 2.2 Invoke
```ts
invoke<Environment[]>("list_conda_environments", { directory: installDir })
```
Return shape (Rust `CondaEnvironment` at `environments.rs:107-113`, `serde(rename = "pythonVersion")`):
```ts
{ name: string; pythonVersion: string; path: string }[]
```

### 2.3 Rust handler (`environments.rs:1617-1803`)
- If `directory` given → `<directory>/conda`. Else reads `~/.openbb_platform/system_settings.json` and pulls `install_settings.installation_directory` (or root-level `installation_directory` as fallback at `:1663-1667`).
- Errors out if `<install>/conda` doesn't exist.
- Walks `<conda>/envs/`: every directory entry whose name doesn't start with `.` is treated as an env.
- For each, calls `get_environment_python_version_impl(path, fs, env_sys)` (`helpers.rs:526-622`):
  1. Tries `<env>/pyvenv.cfg`, parses `version = X.Y.Z`, returns `X.Y`.
  2. Else verifies `<env>/[bin/]python[.exe]` exists.
  3. Else last-resort: walks back to find the conda root and runs `conda list -n <env> --json` parsing `python` package version.
- **YAML cleanup pass** (`:1746-1792`): scans `~/.openbb_platform/environments/*.yaml` and **deletes** any YAML file whose stem isn't in the actual env directory list. Also strips that env from `system_settings.json["environments"]`. So `list_conda_environments` is destructive — TS ports must replicate this if they want the YAML directory clean.
- No conda CLI is invoked for the listing itself (filesystem scan only). Only `conda list` is run if pyvenv.cfg parsing fails.

### 2.4 State updates
- Filters out `name.toLowerCase() === "base"` and any name in `deletedEnvironments.current` (`:421-426`).
- Sets `setEnvironments(...)`, marks `hasLoadedEnvironments.current = true`.
- After backend call also updates the cache: adds new envs with `{extensions: [], pythonVersion}`; removes stale.

### 2.5 Errors
- Backend errors propagate as strings; UI shows `setEnvironmentsError` with a Retry button (`:2372-2398`).

---

## 3. Create environment — full pipeline

### 3.1 Trigger UI
- "New Environment" button → `EnvironmentActionButtons.showCreateEnvironment` (`:118-125`) → `showCreateEnvironment` (`:1766-1777`) which resets all create-state and sets `isCreateModalOpen=true`, `createStep="name"`.
- The 3-step modal is rendered later in the file: name → python version (`PythonVersionSelector` at `AddExtensionSelector.tsx:51-96`) → extensions (`ExtensionSelector` from `InstallComponents.tsx`).
- Step 3 calls `onInstallExtensions(extensionsToInstall)` which is wired to `safeCreateEnvironment` (`:2199-2204`) → `createEnvironment` (`:1510-1698`).

### 3.2 `createEnvironment` flow (`environments.tsx:1510-1698`)
1. Generates `processId = "create-env-${envName}-${Date.now()}"`.
2. `await listen<{processId, output}>("process-output", ...)` — pushes lines into `creationLogs`, with the prefix-collapsing dedupe.
3. `setCreationLoading(true); setCreationLogs([])`.
4. **Step 1**: `invoke("create_environment", { name, pythonVersion, extensions: [], directory, processId })` — passes empty extensions on purpose.
5. **Step 2** (only if extensions selected): `invoke("install_extensions", { extensions, environment, directory })` — note this is a SEPARATE Tauri command (no `processId`, no streaming).
6. After both succeed, `invoke("get_environment_extensions", { name })` to repopulate cache.
7. If err: classify with `isFutureWarningOnly` / `isPipSubprocessError` (`:79-94`). FutureWarning-only is treated as success.
8. `finally`: if cancelled (`deletedEnvironments.current` contains it), invoke `remove_environment` for cleanup.

### 3.3 Rust `create_environment` (`environments.rs:118-436`)
- Always force-adds `openbb-platform-api` and `openbb` to extensions list.
- Loads `~/.openbb_platform/system_settings.json` to find `install_settings.installation_directory` and locates `conda` binary.
- If env already exists: runs `conda env remove -n <name> -y` (line 199).
- Splits incoming extensions:
  - `conda:<channel>:<pkg>` → channel + package (channels map at `conda_channels_map`, defaults seeded with `defaults` and `conda-forge`).
  - `conda:<pkg>` → adds to `conda-forge`.
  - everything else → pip packages, except literal `openbb` (handled separately downstream).
- `conda create -n <name> python=<version> -y` (line 269-276). Output streamed via `run_command_with_logging` → `process-output` events.
- Then writes a YAML to `~/.openbb_platform/environments/<name>.yaml` (`save_environment_as_yaml_impl` at `helpers.rs:436-503`) with channels, conda deps, and a `pip:` sub-list.
- `conda env update -n <name> -f <yaml> --prune` (line 320-330).
- **Smart retry loop** (`:301-401`): if YAML update fails, regex-matches stderr for:
  - `UnsatisfiableError: ... - <pkg>` → conda incompatibility
  - `PackagesNotFoundError: ... - <pkg>` → conda missing
  - `No matching distribution found for <pkg>` → pip

  Removes the offending package from both lists, regenerates YAML, retries. Aborts if no progress can be made.
- Final `save_environment_as_yaml_impl` writes the cleaned YAML.

### 3.4 Step 2 — `install_extensions` (covered fully in §6).

### 3.5 Cancellation
- "Cancel Installation" button → `handleAbortInstallation` (`:1701-1721`):
  - Sets `isCancellingCreation=true`, adds env name to `deletedEnvironments.current`.
  - 5-second `setTimeout` then closes modal — the backend keeps running, but the `finally` block will see the tombstone and call `remove_environment`.

### 3.6 Frontend cache writes
- On success: `cache[env] = { extensions, pythonVersion }`, save (`:1602-1608`); also seeds `environmentPackages[env]` (`:1611-1617`).

### 3.7 Warnings
- `isFutureWarningOnly` (`:79-89`): error must contain `"FutureWarning:"` and NOT contain `"Error:"`, `"failed"`, or `"Pip subprocess error:"`. Treated as non-fatal — env is considered created, cache repopulated, modal closes normally.
- `isPipSubprocessError` (`:91-94`): substring check for `"Pip subprocess error:"` — surfaces the error as-is so the user can see the pip log.

---

## 4. Import from YAML / requirements.txt / pyproject.toml

### 4.1 File picker
- "Import Environment" button → `handleRequirementsFileSelect` (`:560-589`).
- `invoke<string>("select_requirements_file")` (no args).
- Frontend extracts file extension; only `.txt|.toml|.yml|.yaml` accepted.
- Empty string return = user cancelled.

### 4.2 Rust `select_requirements_file` (`environments.rs:1365-1614`)
Per-platform native dialog:
- macOS: `osascript -e 'tell application "System Events" ... choose file ... of type {"txt","toml","yml","yaml"}'`. Errors prefixed `"ERROR: "`. User-cancel (errNum -128) returns empty.
- Windows: PowerShell `System.Windows.Forms.OpenFileDialog`, `creation_flags(0x08000000)` (no console window). Cancel returns empty.
- Linux fallback chain: `zenity --file-selection` → `kdialog --getopenfilename` → embedded Python+GTK script via `python3 -c '...'` → `dialog --inputbox`. Each tries the next on failure; cancel → empty string.

### 4.3 Create-from-requirements
- Frontend modal: `:2942-3099`. User enters env name (regex `^[a-z0-9-]+$`).
- "Create Environment" → `createEnvironmentFromRequirements` (`:591-691`):
  1. processId = `requirements-${envName}-${Date.now()}`
  2. Listen on `process-output` (same dedupe pattern).
  3. `invoke("register_process_monitoring", { processId })`.
  4. `invoke("create_environment_from_requirements", { name, filePath, directory: installDir, processId })`.
  5. On success: clear entire cache (`localStorage.removeItem("env-extensions-cache")`) — comment at `:654` is misleading, it actually wipes everything.
  6. On cancel: invoke `remove_environment` cleanup.

### 4.4 Rust `create_environment_from_requirements` (`environments.rs:438-1363`)
Three branches by extension:
- **`pyproject.toml`**: parses with `toml` crate. Detects PEP 621 (`[project]`) or Poetry (`[tool.poetry]`). Extracts `requires-python` (regex `[>=<~!]*([0-9]+\.[0-9]+)`), `dependencies`, `packages`. Translates Poetry's `^` and `~` semver to `>=,<` ranges (`:624-668`). Treats setup.py / src/ / .py files as indicator of "installable Python project" → ends with `pip install -e .` step (`:1210-1326`).
- **`requirements.txt`**: line-by-line; `python==X.Y` extracts python version; everything else becomes a pip dep. If there's a setup.py/pyproject.toml in the same dir, marks as installable.
- **`.yaml/.yml`** (conda env file): parses `channels`, `dependencies` (string entries → conda; `pip:` mapping → pip).

After parsing:
- Default python version 3.12 if not detected; clamped to range 3.10-3.13.
- For pyproject.toml: writes a temp shell script (Bash on Unix, batch on Windows) that sets all `CONDA_*` env vars, removes any old env, runs `conda create -n <name> python=<v> pip -y`, sources the activate script, then `pip install -r <temp_reqs.txt>`. Streamed via `run_command_with_logging`.
- For txt/yaml: writes YAML via `save_environment_as_yaml_impl`, then `conda env create -f <yaml> -y`.
- If `is_installable_project`: second temp script does `conda activate <env> && cd <project> && pip install -e .`.
- Final `save_environment_as_yaml_impl` writes a "canonical" YAML.

---

## 5. Remove environment

### 5.1 Trigger
- Per-env trash icon → `setEnvironmentToRemove + setShowEnvironmentRemoveConfirmation(true)` (`:2466-2479`).
- Confirmation modal calls `removeEnvironment(name)` (`:1298-1358`).

### 5.2 Frontend
- Adds env to `deletedEnvironments.current` BEFORE invoking — this is a critical race-condition fix so any in-flight `fetchEnvironments` won't re-show it.
- `invoke("remove_environment", { name: envName, directory: installDir })`.
- Deletes `cache[envName]` from localStorage.
- Calls `updateCacheAfterBackendOperation()` for fresh listing.

### 5.3 Rust `remove_environment` (`environments.rs:2689-2756`)
- Refuses to remove "base".
- Reads installation_directory from system_settings.
- `<conda>/bin/conda env remove -n <name> -y`. If it fails, falls back to `fs.remove_dir_all(<conda>/envs/<name>)`.
- Also deletes `~/.openbb_platform/environments/<name>.yaml`.
- **Note**: TS handler signature in main.rs binds only `name` — but the frontend ALSO sends `directory`. The Rust function ignores the extra param (Tauri tolerates extras silently for typed handlers).

---

## 6. Install extensions — `install_extensions`

### 6.1 Trigger UI flows
Two entry points:
- During env creation, step 3 of the modal — `safeCreateEnvironment` → `createEnvironment` step 2.
- On an existing env: `Manage Extensions` panel → "Add Extension" tab → `AddExtensionSelector` → `handleInstallExtensions` (`environments.tsx:1361-1427`).

### 6.2 The two selector components
There are **two** versions of the picker UI:
- `components/AddExtensionSelector.tsx` — used for ADDING to existing env (`environments.tsx:2576-2581`).
- `components/InstallComponents.tsx` (`ExtensionSelector`) — used during the 3-step CREATE flow.

Both are nearly identical and:
- Fetch three JSON files from `https://raw.githubusercontent.com/OpenBB-finance/OpenBB/main/assets/extensions/{provider,router,obbject}.json` directly via `fetch()`. Schema: `{ packageName, reprName?, description?, credentials?, instructions? }[]`. Result is cached only in component state — NOT in localStorage (so a fresh fetch happens every time the modal opens).
- Hard-coded `extrasExtensions` adds: `openbb-mcp-server`, `pywry`, `openbb-cli`, plus `openbb-cookiecutter` (only in InstallComponents).
- Categories tabs: `conda`, `extras` (PyPI), `provider`, `router`, `other-openbb`. The first two have free-form input fields; the others have checkbox lists.
- Conda input encodes as `conda:<channel>:<pkg>` — `addCondaPackage` at `AddExtensionSelector.tsx:203-213` builds `${channel}:${pkg}`, then `handleInstallExtensions` (`:393-420`) prepends `"conda:"` so the wire format is `conda:<channel>:<pkg>`. PyPI packages are sent as plain strings. OpenBB extensions are sent as their package name (`openbb-yfinance`, etc.).
- `installedPackages: Set<string>` is passed in (lower-cased) and used to filter `provider`/`router`/`other-openbb` so already-installed ones don't appear.
- "Setup instructions" (markdown from JSON) rendered with `react-markdown`.

### 6.3 Frontend invoke
```ts
invoke("install_extensions", {
  extensions: string[],   // e.g. ["openbb-yfinance", "conda:conda-forge:numpy", "pandas"]
  environment: string,
  directory: string,      // accepted by frontend but Rust signature only takes (environment, extensions)
})
```
Returns `boolean`. (NB: as with `remove_environment`, `directory` is ignored by the Rust signature — Rust gets install dir from `system_settings.json`.)

### 6.4 Rust `install_extensions` (`environments.rs:2344-2687`)
- Resolves env's python: `<conda>/envs/<env>/[bin/]python[.exe]` (or `<conda>/python.exe` for base). Errors if missing.
- Splits extensions:
  - Strings starting with `conda:` → strip prefix, add to conda packages array (NOTE: handler keeps the channel-qualified portion e.g. `conda-forge:numpy` after stripping just the leading `conda:`).
  - `openbb` (case-insensitive) → handled separately at the end.
  - Everything else → pip packages.
- Conda batch install: `conda install -n <env> -y <pkgs...>` (or `conda install -y <pkgs>` if env=="base").
- Pip batch install: `<env_python> -m pip install <pkgs...>`.
- OpenBB special case: `<env_python> -m pip install openbb --no-deps`. Then runs `<conda>/envs/<env>/[bin|Scripts]/openbb-build[.exe]` (this is what generates the `openbb` namespace package's modules).
- After install: reads existing `<env>.yaml`, extracts python version + existing conda/pip packages, merges new packages (replacing duplicates by name part before `=<>`), rewrites YAML via `save_environment_as_yaml_impl`.
- Errors: returns formatted string `"Failed to install pip packages: \nStdout: ...\nStderr: ..."` — frontend sees raw text, classifies, surfaces.

### 6.5 Concurrent operations
There is NO backend serialization. Concurrent `install_extensions` calls would race on the same conda env (conda's own lockfile would protect data, but the YAML rewrite is racy). Frontend prevents this loosely:
- `installExtensionsLoading` boolean disables buttons (`:282-283`).
- The "Cancel" button (`handleCancelExtensionInstall` at `:1726-1728`) only hides the modal — it does NOT stop the backend ("The process will complete and then be cleaned up").
- `setExtensionSelectorKey((prev) => prev + 1)` (`:1385`) forces a fresh selector state after install.

---

## 7. Get / view extensions — `get_environment_extensions`

### 7.1 Trigger
- "Extensions" button on env card → `showExtensions(envName)` (`:1731-1763`) — but this only reads localStorage; never invokes backend.
- Backend invoke happens from:
  - `refreshEnvironmentUIState` after install/remove/update/create (`:1043-1097`).
  - `loadOrCreateCache` initial mount, only for envs missing from cache or missing `pythonVersion` (`:1100-1183`).
  - Manual Retry button on extensions panel error (`:2632-2657`).

### 7.2 Invoke
```ts
invoke<{ extensions: Extension[] }>("get_environment_extensions", { name: envName })
```
where `Extension = { package: string; version: string; install_method: "pip"|"conda"; channel: string }`.

### 7.3 Rust handler (`environments.rs:1805-2049`)
- Locates `<envs>/<name>.yaml` first; if YAML missing, falls back to `system_settings.json["environments"][name]["extensions"]` if present, else returns `{ extensions: [] }`.
- If YAML exists, runs `<conda>/[bin/]conda list --name <name> --json` directly via `new_conda_command` (the script written to disk earlier in the function appears unused — only the direct CLI call is what actually executes).
- Skips python/pip/setuptools.
- For each package: `channel == "pypi"` → `install_method="pip"`, `package = name`. Else `install_method="conda"`, `package = "<channel>:<name>"`.
- **Sort order**: `openbb` first, then `openbb-core`, then `openbb-platform-api`, then other `openbb-*` alphabetically, then everything else alphabetically (`:2001-2041`).
- Returns `serde_json::json!({ "extensions": extensions })`.

---

## 8. Remove single extension

### 8.1 Trigger
- Trash icon on `ExtensionRow` (`:213-227`) → `setExtensionToRemove + setShowRemoveConfirmation`.
- Confirmation modal (`:2794-2848`) → `handleRemoveExtension(extensionInfo, activeEnv)` (`:1430-1459`).

### 8.2 Invoke
```ts
invoke("remove_extension", { package: packageName, environment: envName, directory: installDir })
```
Note `package` includes `<channel>:` prefix for conda items.

### 8.3 Rust handler (`environments.rs:2051-2250`)
- `(removal_method, package_name)` derived from `package`: if it contains `:`, split → `("conda", &package[idx+1..])`. Else `("pip", package)`.
- Verifies env's python exists.
- conda → `<conda>/bin/conda remove -n <env> <pkg> -y`.
- pip → `<env_python> -m pip uninstall <pkg> -y`.
- After removal: parses `<envs>/<env>.yaml`, drops matching deps from either the top-level `dependencies:` array (conda) or the `pip:` sub-list (pip), writes back.

### 8.4 Frontend after-effect
- `refreshEnvironmentUIState(envName)` → re-fetches `get_environment_extensions`, updates cache.

---

## 9. Update single extension

### 9.1 Trigger
- Refresh icon on `ExtensionRow` → `handleUpdateExtension(packageName)` (`:1461-1507`). Sets `updatingExtension=packageName`.

### 9.2 Invoke
`invoke("update_extension", { package, environment, directory })`.

### 9.3 Rust (`environments.rs:2252-2342`)
- First tries `<env_python> -m pip install --upgrade <pkg>`.
- If pip exits non-zero: tries `<conda>/bin/conda install -n <env> <pkg> -y` (or `install <pkg> -y` for base).
- No YAML update on update (only install/remove update YAML).

---

## 10. Update entire environment

### 10.1 Trigger
- Refresh icon on env card → `updateEnvironment(envName)` (`:752-779`).
- Sets sessionStorage key `updating-env-<name>` so a refresh mid-update keeps showing the spinner. 5-minute stale timeout (`:782-805`).

### 10.2 Invoke
`invoke("update_environment", { environment, directory })`.

### 10.3 Rust (`environments.rs:2772-3042`)
- Reads `<envs>/<env>.yaml` for the canonical package lists.
- Skips `python|pip|nodejs|setuptools` from conda upgrades.
- Conda: `conda install -n <env> -y <pkg_names...>`. Spawns with `Stdio::piped`, runs in `tokio::task::spawn_blocking` with a **5-minute timeout** that kills the child on overrun.
- Pip: `<env_python> -m pip install --upgrade <pkgs...>`.
- If any upgraded package is `openbb` or `openbb-*`, runs `openbb-build` again.

### 10.4 Frontend after-effect
- `refreshEnvironmentUIState(envName)` re-fetches extensions.

---

## 11. Jupyter integration

### 11.1 Status state machine
`jupyterStatus[envName]` ∈ `{undefined, 'starting', 'stopping', 'running', 'stopped', 'error'}`.

### 11.2 Polling effect (`:1791-1870`)
- Runs every 3 seconds, but only polls envs whose status is `starting`, `stopping`, or undefined.
- For each, `invoke<JupyterStatus>("check_jupyter_server", { environment })`.
- Result `{ running: bool, url?: string }`:
  - If running → `setJupyterStatus(env, 'running')`, store URL in `jupyterUrlRef`, add to `activeServers`.
  - If currently `running`/`stopping` and now not running → `'stopped'`.
  - If `'starting'` for >30s → `'error'`.
- Self-clears interval when no envs need polling.

### 11.3 Start: `start_jupyter_server`
Frontend: `startJupyterLab` (`:1890-1942`):
1. If already `running` → `openJupyterWindow(url)` (no new spawn).
2. processId = `jupyter-${envName}` — note **stable, not Date.now()-suffixed**, because the page expects the buffer to persist.
3. `invoke("register_process_monitoring", { processId })`.
4. `invoke<JupyterStatus>("start_jupyter_server", { environment, directory: installDir, working: workDir })`.
5. On success: store url, set status `'running'`, open the window with `?token=launcher` appended.

Rust (`jupyter.rs:36-261`):
- Checks `ACTIVE_JUPYTER_SERVERS` (static `Mutex<HashMap<env, (url, pid)>>`); short-circuits if entry exists.
- Builds command: `<conda>/bin/conda run -n <env> --no-capture-output jupyter lab --no-browser --notebook-dir <working>`.
- Sets envs: `JUPYTER_CONFIG_DIR=<install>/Jupyter/jupyter_config`, `JUPYTER_DATA_DIR=...jupyter_data`, `JUPYTER_RUNTIME_DIR=...jupyter_runtime`.
- Spawns with piped stdout/stderr. PID captured.
- Two threads pipe stdout/stderr → log buffer + `process-output` event (with timestamp).
- An mpsc channel (`tokio::sync::mpsc::channel(100)`) carries lines back to the awaiter; the URL extractor regex set:
  - `https?://[^\s]+token=[^\s]+`
  - `https?://(?:localhost|127\.0\.0\.1):[0-9]+[^\s]*`
  - `http://[^:\s]+:[0-9]+[^\s]*`
  - Plus a fallback substring search for `http://...localhost...lab|8888`.
- Up to 30s timeout. If found: store `(url, pid)` in static map, return `{ url, already_running, status: "running", process_id }`. If not: kill the process, return `Err`.
- **Port assignment**: not chosen by us — Jupyter picks. The static map records `(url, pid)`; port is later extracted from the URL string when stopping.

### 11.4 Stop: `stop_jupyter_server`
Frontend: `stopJupyterServer` (`:1945-1969`) — also called when the secondary "Jupyter logs" window observes `"Shutting down on /api/shutdown request"` from process output (`:2046-2076`).

Rust (`jupyter.rs:291-485`):
- Removes entry from static map. If not present → error.
- Extracts port from URL via regexes (`extract_port_from_url` at `:496`).
- **Windows**: `cmd /c netstat -ano | findstr :<port> | findstr LISTENING` → parses last column as PID → `taskkill /F /PID <pid>` for each.
- **Unix**: `lsof -ti tcp:<port> -sTCP:LISTEN` → for each PID `kill -15 <pid>`, sleep 2s, `kill -0 <pid>` to check, `kill -9 <pid>` if still alive. Fallback if `lsof` errors: `fuser -k <port>/tcp`.
- Emits a synthetic shutdown line on `process-output` with `"type": "system"`.

### 11.5 Open log window
- "Logs" button → `viewJupyterLogs(envName)` (`:1981-1990`) → `invoke("open_jupyter_logs_window", { environment })`.
- Rust (`jupyter.rs:582-641`): creates a Tauri webview window labeled `jupyter-logs-<env>`, navigates to `/jupyter-logs?env=<env>`. On close it `hide()` instead of destroying. Reads logs via `get_process_logs_history` separately.

### 11.6 Open Jupyter URL
- `openJupyterWindow(url)` → `invoke("open_url_in_window", { url })` → `helpers.rs:958-1020` opens external URL in a new Tauri webview window (1200x800, label `url_<timestamp>`).

### 11.7 Cross-window jupyter status sync
The page also listens for:
- Tauri events (`process-output` with shutdown text — `:2046-2076`).
- DOM `message` events from child windows (`:2089-2104`) for `{type: 'jupyter-status-update', environmentName, status}`.
- `storage` events (`:2107-2126`) for keys like `jupyter-shutdown-<env>` written by other windows (with timestamp; only honored if <60s old).

---

## 12. Terminal / Python / IPython / OpenBB CLI sessions (`EnvironmentActions.tsx` + handlers)

`EnvironmentActions.tsx` is purely presentational: opens a modal listing 5 actions and calls callbacks supplied by the parent.

For each action the parent builds a platform-specific command string and invokes:
```ts
invoke("execute_in_environment", { command, environment: "base", directory: installDir })
```
Note: `environment` is hardcoded to `"base"` — the env switch happens INSIDE the command via `conda activate`/`activate.bat`/`source activate`.

Rust `execute_in_environment` (`environments.rs:3044-3248`):
- Windows: detects `start ` to mean "open new window", writes a `.bat` file under `temp_dir`, sets all `CONDA_*` env vars, calls `conda init cmd.exe`, `conda activate base`, `conda activate <env>`, then runs the command. Auto-deletes batch file 2s after spawn.
- Unix: writes a `.sh` script with the conda env vars and `source <conda>/bin/activate <env>`, sets `0o755`, runs via `sh <script>`, deletes.
- Returns `{ stdout, stderr, exit_code }`.

The exact CLIs invoked from `environments.tsx`:
- Windows: `start cmd.exe /k "cd /d <workdir> && \"<conda>\\Scripts\\activate.bat\" <env> && <inner>"`
- macOS: `osascript -e '<applescript>'` — uses iTerm if `~/Applications/iTerm.app` exists (checked via `@tauri-apps/plugin-fs.exists` with `BaseDirectory.Home` at `:825-827`), else Terminal.app. Pre-escapes via `escapeAppleScriptString` (`:96-97`).
- Linux: `x-terminal-emulator -e "..."`.

Inner commands per action:
- System Shell — just activate + drop into shell.
- Python — `python -i` (plus `-c "from openbb import obb; print(obb)"` only if env name is literally `openbb`).
- IPython — same with `ipython -i`.
- OpenBB CLI — `openbb && exit`.

Each action button has a `disabled` predicate based on `hasJupyterSupport`, `hasIPythonSupport`, `hasCliSupport` — those check the cached package set for `notebook|jupyter|jupyterlab`, `ipython`, `openbb-cli` respectively.

---

## 13. Misc supporting commands

| Command | Frontend caller | Rust impl | Purpose |
|---|---|---|---|
| `check_directory_exists` | debounced effect at `:341-360` | `helpers.rs:1359-1366` | Validates manual cwd entry |
| `select_directory` | `selectWorkingDirectory` (`:436-448`) | `helpers.rs:1380-1595` | Native folder picker (osascript / PowerShell / zenity/kdialog/python+gtk) |
| `save_working_directory` | useEffect (`:394-400`) | `helpers.rs:293-353` | Writes `preferences.working_directory` to user_settings.json |
| `get_working_directory` | `:545-553` | `helpers.rs:355-401` | Reads + validates dir exists, else returns default |
| `get_installation_state` | `:704` | `main.rs:389` | Returns `{ is_installed, installation_directory }` from managed Tauri state |
| `get_home_directory` | `:719` | `helpers.rs:1369-1377` | `$HOME` or `$USERPROFILE` |
| `open_url_in_window` | `openDocumentation`, `openJupyterWindow` | `helpers.rs:958-1020` | Opens external URL in new webview window |

---

## 14. Error / warning handling reference

- `extractStderr` (`environments.tsx:58-76`) — splits Rust formatted error strings:
  - Looks for `"Stderr:"` and pulls everything between it and end-of-string OR `"Exit code:"`/`"Stdout:"`.
  - Special-case for `"Pip subprocess error:"` + `"Stdout:"` — uses Stdout instead (because pip writes the actual error to stdout).
- `isFutureWarningOnly` (`:79-89`) — `FutureWarning:` and not (`Error:`|`failed`|`Pip subprocess error:`).
- `isPipSubprocessError` (`:91-94`) — `"Pip subprocess error:"` substring.
- Error banners in JSX: `creationWarning` (`:2349-2370`), `requirementsError`/`requirementsWarning` (`:2974-2988`), `extensionsError` (`:2617-2658`), `extensionRemoveError` (`:2659-2670`), `updateExtensionError` (`:2671-2682`), `updateEnvironmentError` (`:2754-2775`), `removeEnvironmentError` (`:2777-2789`).

---

## 15. TS port translation table

| Tauri command | Args (TS) | Returns | Equivalent IPC/HTTP design | External CLI invoked |
|---|---|---|---|---|
| `list_conda_environments` | `{ directory?: string }` | `{name, pythonVersion, path}[]` | `GET /environments?installDir=...` | None directly (FS scan); fallback `conda list -n <env> --json` for python version |
| `get_environment_extensions` | `{ name }` | `{ extensions: Extension[] }` | `GET /environments/:name/extensions` | `conda list --name <env> --json` |
| `create_environment` | `{ name, pythonVersion, extensions, directory, processId }` | `bool` | `POST /environments` + WebSocket/SSE on `/processes/:id/stream` | `conda env remove -n <name> -y`; `conda create -n <name> python=<v> -y`; `conda env update -n <name> -f <yaml> --prune` |
| `create_environment_from_requirements` | `{ name, filePath, directory, processId }` | `bool` | `POST /environments/from-file` + stream | `conda create`, `conda env create -f`, `pip install -r`, `pip install -e .`; plus shell scripts |
| `install_extensions` | `{ extensions, environment, directory }` | `bool` | `POST /environments/:env/extensions` | `conda install -n <env> -y <pkgs>`; `<env_python> -m pip install <pkgs>`; `<env_python> -m pip install openbb --no-deps`; `openbb-build` |
| `update_extension` | `{ package, environment, directory }` | `bool` | `PATCH /environments/:env/extensions/:pkg` | `<env_python> -m pip install --upgrade <pkg>`; fallback `conda install -n <env> <pkg> -y` |
| `remove_extension` | `{ package, environment, directory }` | `bool` | `DELETE /environments/:env/extensions/:pkg` | `conda remove -n <env> <pkg> -y` OR `<env_python> -m pip uninstall <pkg> -y` |
| `update_environment` | `{ environment, directory }` | `bool` | `POST /environments/:env/update` | `conda install -n <env> -y <pkgs>` (5-min timeout); `<env_python> -m pip install --upgrade <pkgs>`; `openbb-build` |
| `remove_environment` | `{ name, directory }` | `bool` | `DELETE /environments/:name` | `conda env remove -n <name> -y`; fallback rm -rf |
| `select_requirements_file` | `{}` | `string` (path or empty) | Native file picker via Electron/web `<input>` or system dialog API | `osascript`/`powershell`/`zenity`/`kdialog`/`python3 -c '...'`/`dialog` |
| `select_directory` | `{ prompt? }` | `string` | Native folder picker | same family |
| `check_directory_exists` | `{ path }` | `bool` | `HEAD /fs?path=` | None |
| `save_working_directory` | `{ path }` | `bool` | `PUT /preferences/working_directory` | None |
| `get_working_directory` | `{ defaultDir }` | `string` | `GET /preferences/working_directory` | None |
| `get_installation_state` | `{}` | `{ is_installed, installation_directory }` | `GET /installation/state` | None |
| `get_home_directory` | `{}` | `string` | `GET /system/home` | None |
| `execute_in_environment` | `{ command, environment, directory }` | `{ stdout, stderr, exit_code }` | `POST /shell/exec` (with caveat — opens external terminal windows) | `cmd.exe`/`bash`/`sh`; transitively `osascript`, `x-terminal-emulator`, `python`, `ipython`, `openbb` |
| `start_jupyter_server` | `{ environment, directory, working }` | `{ url, already_running, status, process_id? }` | `POST /jupyter/:env/start` + WebSocket `/jupyter/:env/logs` | `conda run -n <env> --no-capture-output jupyter lab --no-browser --notebook-dir <wd>` |
| `stop_jupyter_server` | `{ environment }` | `bool` | `POST /jupyter/:env/stop` | Windows: `netstat -ano`, `taskkill /F /PID`. Unix: `lsof -ti tcp:<port> -sTCP:LISTEN`, `kill -15`, `kill -0`, `kill -9`, fallback `fuser -k` |
| `check_jupyter_server` | `{ environment }` | `{ running, url?, status, environment, process_id? }` | `GET /jupyter/:env/status` | None (in-memory map) |
| `list_jupyter_servers` | `{}` | `{ servers: [...] }` | `GET /jupyter` | None |
| `open_jupyter_logs_window` | `{ environment }` | `void` | New browser window/route | None |
| `open_url_in_window` | `{ url, title? }` | `void` | `window.open(url)` or new BrowserWindow | None |
| `register_process_monitoring` | `{ processId }` | `bool` | Implicit (server creates buffer on first stream subscription) | None |
| `unregister_process_monitoring` | `{ processId }` | `bool` | DELETE on the buffer | None |
| `get_process_logs_history` | `{ processId, count? }` | `LogEntry[]` | `GET /processes/:id/logs?count=N` | None |
| `clear_process_logs_history` | `{ processId }` | `bool` | `DELETE /processes/:id/logs` | None |

### External CLI tools the port needs to spawn/wrap

(Combining all observed callsites in environments.rs / jupyter.rs / helpers.rs):

- **conda** (`<install>/conda/[bin|Scripts]/conda[.exe]`):
  - `env remove -n <env> -y`
  - `create -n <env> python=<v> [pip] -y`
  - `env create -f <yaml> -y`
  - `env update -n <env> -f <yaml> --prune`
  - `install [-n <env>] [<pkgs>] -y`
  - `remove [-n <env>] <pkg> -y`
  - `list --name <env> --json`
  - `run -n <env> --no-capture-output jupyter lab --no-browser --notebook-dir <wd>`
  - `init cmd.exe` (Windows shell scripts)
  - `activate <env>` (via `condabin/conda.bat` or `bin/activate`)
- **pip** (always invoked as `<env>/[bin|Scripts]/python[.exe] -m pip`):
  - `install <pkgs>`
  - `install --upgrade <pkgs>`
  - `install -r <requirements.txt>`
  - `install openbb --no-deps`
  - `install -e .`
  - `uninstall <pkg> -y`
- **openbb-build** (`<env>/[bin|Scripts]/openbb-build[.exe]`) — no args.
- **jupyter** — only via `conda run -n <env> jupyter lab`.
- **mamba** — NOT used anywhere (no references in any handler).
- **System utilities for kills/picker/terminal**:
  - Windows: `cmd.exe`, `powershell` (file dialog), `netstat`, `taskkill`.
  - Unix: `osascript` (macOS dialogs + iTerm/Terminal AppleScript), `lsof`, `kill`, `fuser`, `sh`, `bash`, `python3` (only as a host for embedded GTK file picker on Linux), `zenity`, `kdialog`, `dialog`, `x-terminal-emulator`, `open` (mac browser), `xdg-open` (linux browser).

### Key porting notes

1. **The `process-output` event channel is global**; every consumer must filter by `processId`. A typed-per-process WebSocket would be a cleaner replacement.
2. **The localStorage cache (`env-extensions-cache`) has no TTL** and is hand-merged. A keyed-store (IndexedDB / SWR) with a versioned schema would be cleaner. Schema: `{[env]: {extensions: Extension[], pythonVersion: string}}`.
3. **`deletedEnvironments` ref + `envCreatedRef` + `creationWarningRef` + `createEnvironmentRef`** patterns are workarounds for race conditions between async invokes, the polling effect, the listen subscription, and the cancel button. A proper TS port should model these as state machines (XState or reducer) per environment.
4. **`EnvironmentCreationContext` only locks UI elsewhere** — port it as a global "busy" signal that other routes can subscribe to.
5. **`list_conda_environments` is destructive** — it deletes orphaned `<env>.yaml` files. If the port wants to be idempotent, separate the listing from the cleanup.
6. **`install_extensions` ignores the `directory` field** in the Rust signature even though the frontend sends it (Tauri silently drops unknown args). Same for `remove_environment`. The frontend should be cleaned up to match.
7. **Frontend prefix-collapsing dedupe of log lines** (`if lastLog.split(':')[0] === newLog.split(':')[0] then replace`) is what makes pip progress bars not flood the UI; replicate this in any TS log viewer.
8. **Jupyter port is auto-assigned by Jupyter itself**; the Rust handler pulls it back out of the URL when stopping. The port is never hard-coded.
9. **The fetched extensions JSON URLs (`https://raw.githubusercontent.com/.../assets/extensions/{provider,router,obbject}.json`)** are public — port can call them directly from the frontend (CORS works) or proxy through the backend.
10. **Nothing currently uses `RunningProcesses`** for env operations — it's defined for tracking spawned processes but only Jupyter uses its own static map. A clean port should pick one approach (probably `RunningProcesses` style) and use it everywhere.

---

## Cross-feature dependencies

- **depends-on** `feature-installation.md` — every conda path comes from `system_settings.json` written at install; install seeds the `openbb` env that this page edits.
- **depends-on** `feature-logs-streaming.md` — all subprocess output flows through the shared `process-output` event with `processId` filtering.
- **depended-on-by** `feature-backend-services.md` — backends spawn inside conda envs listed here; selecting an env in the backend form pulls from this list.
- **depended-on-by** `feature-jupyter.md` (this same doc covers Jupyter; the feature-jupyter.md writer should source §11 in particular).
- **depended-on-by** `feature-extensions.md` — extension install/remove/update flows live here.
- **shares-state-with** `feature-app-shell.md` via `EnvironmentCreationContext` (cross-page nav lock).
- **shares-state-with** `feature-platform-rest-api.md` — the `openbb-platform-api` PyPI package is what the Python REST server needs; install includes it by default.
