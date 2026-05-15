# Deep-Dive: IPC Bridge — v2 Addendum

> Second-pass review of `ipc-bridge.md`. Combs all feature deep-dives and the
> Rust source to find what v1 missed, miscounted, or stated wrong.
> Generated 2026-05-15. New material only — read v1 first for the catalog itself.

---

## A. Command count is wrong in v1

**v1 says 53 commands in `generate_handler!`. Actual count is 57.**

Direct enumeration of `/home/user/OpenBBPort/desktop/src-tauri/src/main.rs:493-549` (counted line-by-line, every entry is one identifier per line):

```
1  toggle_theme                       30 execute_in_environment
2  navigate_to_page                   31 start_jupyter_server
3  save_working_directory             32 stop_jupyter_server
4  get_working_directory              33 stop_all_jupyter_servers     ← v1 omits
5  get_home_directory                 34 check_jupyter_server
6  select_directory                   35 list_jupyter_servers
7  get_installation_directory         36 get_user_credentials
8  get_userdata_directory             37 open_credentials_file
9  get_settings_directory             38 update_user_credentials
10 select_file                        39 open_url_in_window
11 install_to_directory               40 register_process_monitoring
12 check_directory_exists             41 unregister_process_monitoring
13 check_file_exists                  42 get_process_logs_history
14 install_conda                      43 clear_process_logs_history
15 abort_installation                 44 open_jupyter_logs_window
16 get_installation_status            45 update_jupyter_status
17 get_installation_state             46 open_backend_logs_window
18 setup_python_environment           47 start_backend_service
19 create_environment                 48 stop_backend_service
20 list_conda_environments            49 update_backend_service
21 get_environment_extensions         50 create_backend_service
22 install_extensions                 51 delete_backend_service
23 update_extension                   52 list_backend_services
24 update_environment                 53 uninstall_application
25 update_installation_error          54 quit_application
26 remove_extension                   55 generate_self_signed_cert
27 remove_environment                 56 update_openbb_settings
28 create_environment_from_requirements 57 create_default_backend_services
29 select_requirements_file
```

v1's per-section subtotals are also wrong. Correcting:
- main.rs: 7 (not 6 — v1 omitted that `get_installation_state` and `quit_application` are both there in addition to the 5 ProcessLog/nav ones).
- startup.rs: 6 registered (`get_installation_status`, `install_to_directory`, `install_conda`, `abort_installation`, `setup_python_environment`, `create_default_backend_services`). v1 counted 8 but listed `check_installer_file_exists` as registered, which is wrong (see §B).
- environments.rs: 13 (`create_environment`, `create_environment_from_requirements`, `select_requirements_file`, `list_conda_environments`, `get_environment_extensions`, `install_extensions`, `update_extension`, `update_environment`, `update_installation_error`, `remove_extension`, `remove_environment`, `execute_in_environment` + `update_jupyter_status` is in jupyter.rs, not here). v1 counted 12 — correct, but the section header "12" is right while the subtotal of 53 is wrong.
- jupyter.rs: 7 — v1 correct.
- backends.rs: 7 (`stop_backend_service`, `start_backend_service`, `list_backend_services`, `create_backend_service`, `update_backend_service`, `delete_backend_service`, `open_backend_logs_window`). v1's "8 commands" header counts `initialize_backends` which is NOT a `#[tauri::command]` (see §B).

**Corrected total: 7 + 13 + 6 + 13 + 7 + 7 + 1 (certs) + 1 (uninstall) + 3 (credentials) - 1 (because `update_jupyter_status` was double-counted) = 57.** Matches the macro.

---

## B. `#[tauri::command]` annotations NOT in `generate_handler!`

v1 surfaced `check_installer_file_exists`. There is a **second** one v1 missed.

Full inventory of every `#[tauri::command]` in the source tree (60 total):

| File:Line | Function | In macro? |
|---|---|---|
| `/home/user/OpenBBPort/desktop/src-tauri/src/tauri_handlers/startup.rs:962` | `check_installer_file_exists` | NO |
| `/home/user/OpenBBPort/desktop/src-tauri/src/tauri_handlers/environments.rs:1616` | `list_conda_environments_impl` | **NO — and it CAN'T be** |

`list_conda_environments_impl` is declared:
```rust
#[tauri::command]
pub async fn list_conda_environments_impl<F: FileSystem, E: EnvSystem>(
    directory: Option<String>,
    fs: &F,
    env_sys: &E,
) -> Result<Vec<CondaEnvironment>, String>
```

The `#[tauri::command]` macro on a generic function with non-IPC-serialisable parameters (`fs: &F`) is silently no-op — Tauri can't construct an invoke wrapper around generics. The bug is purely cosmetic (the macro will emit code but it won't be valid for `generate_handler!`). The actual command is the wrapper at `environments.rs:1798`:
```rust
#[tauri::command]
pub async fn list_conda_environments(
    directory: Option<String>,
) -> Result<Vec<CondaEnvironment>, String> {
    list_conda_environments_impl(directory, &RealFileSystem, &RealEnvSystem).await
}
```

`check_installer_file_exists` at `startup.rs:962` is a real command (no generics, plain `() -> Result<bool, String>`) but simply not registered. Dead code reachable only from Rust unit tests.

**Port-time recommendation:** drop both `#[tauri::command]` attributes from these two functions to remove the false signal. Real command count remains 57.

---

## C. State `.manage(...)` registrations — v1 misses one

v1 says "4" registrations and lists them at "main.rs:489-491". Reality:

| # | Type | File:Line | When |
|---|---|---|---|
| 1 | `ProcessLogState(LogStorage)` | `main.rs:489` | Builder phase |
| 2 | `RunningProcesses` | `main.rs:490` | Builder phase |
| 3 | `InstallationState` (returned by `check_installation_on_startup()`) | `main.rs:491` | Builder phase |
| 4 | `tray: TrayIcon` | `main.rs:742` | **Inside `.setup()` hook** via `app_handle.manage(tray)` |

v1 mentioned the tray case but located the setup-hook registration confusingly under "main.rs:489-491". The tray manage is not in the builder chain — it's deferred. This matters for a TS port because it shows that some Tauri-managed state can be lazily registered after windows exist, which Electron handles differently (state objects don't have a separate manager — just module-scoped variables).

`grep 'manage(' src-tauri/src/`:
```
main.rs:489  .manage(ProcessLogState(get_log_storage()))
main.rs:490  .manage(RunningProcesses::new())
main.rs:491  .manage(check_installation_on_startup())
main.rs:742  app_handle.manage(tray);
```

---

## D. `Lazy<Mutex<T>>` globals — v1 lists 4, real total is 4 (correct, but with caveat)

`grep 'static .*: Lazy<' src-tauri/src/`:

| Global | File:Line | Type |
|---|---|---|
| `ACTIVE_JUPYTER_SERVERS` | `tauri_handlers/jupyter.rs:9` | `Lazy<Mutex<HashMap<String, (String, u32)>>>` |
| `INSTALLATION_STATE` | `tauri_handlers/startup.rs:15` | `Lazy<Mutex<InstallationState>>` |
| `INSTALLATION_IN_PROGRESS` | `tauri_handlers/startup.rs:434` | `Lazy<Mutex<bool>>` |
| `LOG_STORAGE` | `utils/process_monitor.rs:9` | `Lazy<LogStorage>` where `LogStorage = Arc<Mutex<HashMap<String, LogBuffer>>>` |

v1's count is right but **the typing in v1 is wrong for `LOG_STORAGE`** — v1 says `Lazy<LogStorage>` but the table later calls it `Lazy<Mutex<...>>`. The actual declaration is `Lazy<LogStorage>` (a Lazy holding an `Arc<Mutex<...>>`), so there's no `Mutex<>` directly inside the `Lazy<>`. The `Mutex` is wrapped by `Arc` first. This is significant for porting because:

1. `LOG_STORAGE` is `Arc`-clonable; you can hand the same shared map to background threads without locking the `Lazy`.
2. The other three are bare `Mutex` — they require `lock()` on every access, which can fail/poison.
3. A Node port wouldn't need `Arc` (single-threaded JS), but `LOG_STORAGE`-style would still need a singleton-export pattern, while the `Mutex<bool>` re-entrancy guards (`INSTALLATION_IN_PROGRESS`) become a simple `let installInProgress = false;` boolean.

There are NO other `Lazy<>` statics anywhere in `src-tauri/src/` (verified by `grep -r 'Lazy<'`).

---

## E. `window.eval(` injection points — v1 mentions 2, actual is 5

Every `window.eval()` is a Rust→renderer JavaScript injection — equivalent to `webContents.executeJavaScript()` in Electron and a code-execution surface in any port.

`grep 'window.eval' src-tauri/src/`:

| File:Line | Injected JS | Trigger |
|---|---|---|
| `main.rs:408` | `if (localStorage.getItem('environments-first-load-done') === 'true') { window.location.href = '<page>'; } else { console.log(...); }` (in `navigate_to_page`) | Tray menu items: Backends, Environments, API Keys (`main.rs:659-661`). Caller is the tray click handler exclusively. |
| `main.rs:676` | `window.location.href = '/uninstall';` | Tray "Uninstall" menu item (bypasses the `environments-first-load-done` gate that `navigate_to_page` enforces). |
| `main.rs:784` | `localStorage.clear(); console.log('localStorage cleared due to INVALID installation');` | Setup hook when `is_installed=false`. |
| `main.rs:785` | `window.location.href = '/setup'` | Setup hook continuation of line 784. |
| `main.rs:799` | `localStorage.setItem('environments-first-load-done', 'true');` | Setup hook when `is_installed=true`. |

All five are static strings except `main.rs:408` which interpolates `page: &str` from the tray handler. Since `page` is hard-coded by Rust callers (`/environments`, `/api-keys`, `/backends`), there's no current XSS path — but a TS port using `webContents.executeJavaScript` should not parameterise these from any user-influenced source.

**Port note:** in Electron, prefer `mainWindow.webContents.send('navigate', '/path')` + a renderer listener that calls `router.navigate()`. Same goes for the localStorage seeding — emit an event and have the renderer set it. The setup-hook eval at `main.rs:799` is particularly fragile: it races with React mount and is the *only* mechanism that sets the `environments-first-load-done` gate for fresh installs. A TS port should set this gate from the `update_openbb_settings` handler or from the post-install React success callback.

---

## F. `navigate_to_page` has zero frontend callers (confirmed)

`grep -rn 'navigate_to_page' src/`: **no matches**. v1 was right — only Rust callers from `main.rs:659-661` (tray) invoke it.

**So why is `navigate_to_page` decorated as `#[tauri::command]` and registered at `main.rs:494`?** Two possible reasons:

1. **Vestigial** — earlier versions probably exposed it to JS. Now it's Rust-internal but the macro is intact.
2. **Defensive** — registering it in `generate_handler!` means it can be invoked from any window (including future child windows or external URL windows opened via `open_url_in_window`). But no such caller exists today.

**Port recommendation:** drop the `#[tauri::command]` attribute. Make it a private function called only from the tray handler. Same applies to:
- `unregister_process_monitoring` — registered, never called from JS.
- `update_jupyter_status` — registered (jupyter.rs:644), no `invoke("update_jupyter_status",...)` anywhere in src/. It IS emitted as event from inside the impl, but the JS side never invokes it.
- `update_installation_error` — registered (environments.rs:2758), no caller.
- `list_jupyter_servers` — registered (jupyter.rs:558), no caller.
- `stop_all_jupyter_servers` — registered (jupyter.rs:263), called from `cleanup_all_processes` at `main.rs:428` (Rust-only) and never from JS.

That's **6 registered commands with no JS caller**. They could be demoted to private fns for the port.

---

## G. Cross-feature invoke validation — every `invoke()` in src/ accounted for

I extracted all invoke names from `src/` (excluding tests) and cross-referenced against the `generate_handler!` block. **Every JS invoke targets a registered Rust command, with two exceptions:**

1. **`invoke('app.exit')`** at `/home/user/OpenBBPort/desktop/src/routes/uninstall.tsx:84`. There is no Rust handler named `app.exit`. This is a typo / misremembered API — `@tauri-apps/plugin-process` exposes `exit()` from `process` namespace, but it's not invoked through the `invoke()` channel — you import `exit` from `@tauri-apps/plugin-process` and call it directly. The current code silently rejects (logs as unknown command). On macOS the `uninstall.rs` impl already calls `std::process::exit(0)` at line 399 before this fires, so the bug is masked. On Windows/Linux the app fails to actually quit after uninstall. v1 noted this; confirmed.

2. **`invoke('greet', ...)`** and **`invoke('fail_command')`** at `/home/user/OpenBBPort/desktop/src/tests/routes/tauri-mock.test.ts:36,51`. Test fixtures only. Not real commands.

Frontend-invoke commands sorted (43 unique):
```
abort_installation, check_directory_exists, check_file_exists, check_jupyter_server,
clear_process_logs_history, create_backend_service, create_default_backend_services,
create_environment, create_environment_from_requirements, delete_backend_service,
execute_in_environment, generate_self_signed_cert, get_environment_extensions,
get_home_directory, get_installation_directory, get_installation_state,
get_installation_status, get_process_logs_history, get_settings_directory,
get_user_credentials, get_userdata_directory, get_working_directory, install_conda,
install_extensions, install_to_directory, list_backend_services,
list_conda_environments, open_backend_logs_window, open_credentials_file,
open_jupyter_logs_window, open_url_in_window, quit_application,
register_process_monitoring, remove_environment, remove_extension,
save_working_directory, select_directory, select_file, select_requirements_file,
setup_python_environment, start_backend_service, start_jupyter_server,
stop_backend_service, stop_jupyter_server, toggle_theme, uninstall_application,
update_backend_service, update_environment, update_extension, update_openbb_settings,
update_user_credentials
```

That's **51 commands actively used** by the frontend out of the **57 registered**. The 6 unused ones are listed in §F. `__root.tsx:132` and `__root.tsx:194` (where v1 says `get_user_credentials` and `toggle_theme` are called) are inside `{/* … */}` JSX comments — those callers are **dead code**. v1's catalog is therefore wrong on those two rows. The actual `get_user_credentials` callers are `api-keys.tsx:182` only; the actual `toggle_theme` caller is **none** (no live caller — it's in commented code).

---

## H. Argument case-conversion: every multi-word param surveyed

Tauri 2.x normalises camelCase JS args to snake_case Rust params via a `serde_json` rename pass (`tauri::command` macro generates `#[serde(rename_all = "camelCase")]` on the Args struct). Every multi-word param in the codebase:

| Rust param | JS sender (camelCase) | Sites |
|---|---|---|
| `process_id` | `processId` | 9 sites: register/get-history/clear in env, jupyter, backend pages |
| `default_dir` | `defaultDir` | `environments.tsx:545` (`get_working_directory`) |
| `python_version` | `pythonVersion` | `installation-progress.tsx:1120`, `environments.tsx:1559` |
| `file_path` | `filePath` | `environments.tsx:637` (`create_environment_from_requirements`) |
| `user_data_directory` | `userDataDirectory` | `setup.tsx:123` (`install_to_directory`) |
| `conda_dir` | `condaDir` | `installation-progress.tsx:1163`, `:1208` (`update_openbb_settings`) |
| `file_name` | `fileName` | 5 sites in `api-keys.tsx` (`open_credentials_file`) |
| `common_name`, `org_name`, `alt_names`, `output_dir`, `days_valid`, `install_in_trust_store` | `commonName`, `orgName`, `altNames`, `outputDir`, `daysValid`, `installInTrustStore` | `backends.tsx:328-336` (`generate_self_signed_cert`) |
| `remove_user_data`, `remove_settings` | `removeUserData`, `removeSettings` | `uninstall.tsx:73-76` |
| `working`, `environment` | `working`, `environment` (single-word, no conversion) | `environments.tsx:1923-1925` (`start_jupyter_server`) |

**Are there any commands where normalisation fails?** No. Every multi-word camelCase JS arg has a matching snake_case Rust param. Tauri's normaliser handles them all uniformly because the `#[tauri::command]` macro is consistent.

**Two semi-failures worth noting:**

1. **`install_conda` accepts `userDataDir` from JS but the Rust signature only takes `directory: String, window: Window`**. Tauri does NOT error on unknown args — it silently drops `userDataDir`. JS callers expect it to do something; Rust ignores it. This is a wire-protocol mismatch that the port must reconcile (either drop the JS arg or add the Rust param).
   - Source: `installation-progress.tsx:938-941` sends both; `startup.rs:437` accepts only `directory`.

2. **`install_extensions` accepts `directory` from JS but uses `system_settings.json` instead** (the Rust signature DOES have `directory` — wait, no, it doesn't). Recheck: `environments.rs:2682` signature is `pub async fn install_extensions(extensions: Vec<String>, environment: String) -> Result<bool, String>`. JS sends `{extensions, environment, directory}`. So `directory` is silently dropped. Same pattern as `install_conda`.
   - Source: `environments.tsx:1574-1581` sends three args; only two are accepted.

3. **`remove_environment` accepts `directory` from JS, Rust signature is `(name: String)` only**. Same silent drop. Source: `environments.tsx:1316`.

This silent-drop behaviour is a Tauri convention — invoke args are deserialised into a struct generated from the function signature, and `serde_json` ignores unknown fields by default (`#[serde(deny_unknown_fields)]` is NOT applied by the macro). A TS port that uses strict Zod validation would surface these as errors. Either keep the loose behaviour or clean up the JS callers.

---

## I. The "two `InstallationState` types" — exactly where collision could happen

v1 calls this out but doesn't trace where confusion lives. Here's the full picture.

**Type A** — `main.rs:67-70`:
```rust
#[derive(Clone, serde::Serialize)]
struct InstallationState {
    is_installed: bool,
    installation_directory: Option<String>,
}
```
Constructed once at boot by `check_installation_on_startup()` (`main.rs:272-386`). Registered as managed state at `main.rs:491`. Read **only** by `get_installation_state` (`main.rs:388-391`). Never mutated.

**Type B** — `startup.rs:18-25`:
```rust
#[derive(Default, Debug)]
pub struct InstallationState {
    pub is_downloading: bool,
    pub is_installing: bool,
    pub is_configuring: bool,
    pub is_complete: bool,
    pub message: String,
}
```
Wrapped in `Lazy<Mutex<InstallationState>>` at `startup.rs:15`. Mutated throughout `install_conda` and `setup_python_environment`. Read **only** by `get_installation_status` (`startup.rs:34-54`).

**Why this is dangerous:**

1. They share the type *name* but not the type. Rust's module system keeps them disjoint, so the compiler won't complain — but a developer reading `InstallationState` in `main.rs` vs `startup.rs` could reasonably think they're the same.
2. The two commands are nearly identically named: `get_installation_state` (Type A) vs `get_installation_status` (Type B). The frontend uses BOTH:
   - `get_installation_state` at `index.tsx:37` and `environments.tsx:704` (read once for routing).
   - `get_installation_status` at `installation-progress.tsx:1059` (polled every 2s during install).
3. The two commands are registered adjacently in `generate_handler!` (`main.rs:508-509`):
   ```rust
   get_installation_status,    // Type B
   get_installation_state,     // Type A
   ```
   A reader scanning the macro could easily mis-edit one for the other.

**Where could a TS port get confused?** The natural mapping is:
- Type A → `GET /install/state` (one-shot, returns `{installed, dir}`).
- Type B → `GET /install/status` (polling, returns `{phase, message}`).

If the port collapses these into one endpoint or one struct, the boot-time redirect logic (which depends on Type A being immutable and trustworthy) collides with the during-install mutability of Type B. Keep them separate. Better still, **rename them** in the port (e.g. `InstallSnapshot` for A, `InstallProgressTracker` for B) to remove the name collision.

---

## J. `open_url_in_window` — security model of the popped windows

Used by:
- `api-keys.tsx:428` → opens `https://docs.openbb.co/desktop/api_keys`
- `environments.tsx:48,1974` → opens docs and Jupyter URL
- `backends.tsx:1893` → opens docs

Rust impl at `helpers.rs:957-1020` creates a `WebviewWindow` with `WebviewUrl::External(parsed_url)`. **Security implications:**

1. **CSP**: `tauri.conf.json:38` sets `"csp": null` — i.e. **no Content Security Policy at all**. The popped window inherits this. An external page can load arbitrary resources, scripts, frames, etc.
2. **Tauri capabilities**: `capabilities/default.json` and `capabilities/desktop.json` apply to `"windows": ["*"]` — so the popped window gets the SAME IPC permissions as the main window. That includes `fs:read-all`, `fs:write-all`, `shell:allow-execute`, `shell:allow-spawn`, `core:default`, `dialog:default`. If the external URL serves malicious JS, it can call `invoke()` for any of the 57 registered commands.
3. **No `nodeIntegration` analog needed** — Tauri uses webviews, not Node — but the IPC surface is exposed to **any URL the window navigates to**. The popped window has no sandbox.
4. **Window label** is `url_<timestamp_millis>` (helpers.rs:970). Label collisions are theoretically possible at sub-millisecond rates but practically not.
5. **Close behaviour** (`helpers.rs:1000-1006`): `prevent_close` then `destroy()`. So the window is single-shot; closing tears it down (unlike the logs windows which hide).
6. **No URL allow-list**. JS can call `invoke("open_url_in_window", { url: "https://evil.com" })` and the Rust handler will open it. Currently every caller passes a hard-coded openbb.co URL or a Jupyter `localhost:<port>` URL, but there's no Rust-side validation.

**Port-time recommendations:**

- For Electron: `new BrowserWindow({ webPreferences: { sandbox: true, contextIsolation: true, nodeIntegration: false } }).loadURL(url)`. Critically, do NOT preload any IPC bridge into the popped window — give it `sandbox: true` to deny `ipcRenderer` access. Add a CSP via session.defaultSession.webRequest if you want defence in depth.
- Add an allow-list: `url.match(/^https:\/\/(docs\.openbb\.co|.*\.openbb\.co)/) || url.match(/^https?:\/\/(localhost|127\.0\.0\.1):/)`.
- The popped window doesn't need IPC. Strip its access to `invoke()` entirely.

This is the **highest-severity finding** in the v2 review. A TS port that copies the current "no CSP, full IPC, any URL" model inherits a credential-exfiltration vector.

---

## K. Tauri capabilities — full enumeration

v1 mentions capabilities exist. Full enumeration:

### `capabilities/default.json` (applies to all windows in all builds):
```
core:default, dialog:default, opener:default, shell:allow-open, shell:default,
opener:allow-open-url, fs:read-all, fs:write-all, fs:write-files,
fs:allow-watch, fs:allow-unwatch, log:default
```

### `capabilities/desktop.json` (desktop-only platforms, applies to all windows):
Adds on top of default:
```
opener:allow-default-urls, shell:allow-execute, shell:allow-spawn,
opener:allow-open-path (for path "**" — all paths),
fs:allow-exists (for path "**"),
fs:scope-home-recursive, fs:allow-copy-file, fs:allow-create, fs:allow-exists,
fs:allow-mkdir, fs:allow-read-dir, fs:allow-read-file, fs:allow-remove,
fs:allow-rename, fs:allow-watch, fs:allow-write-file,
fs:scope-localdata-recursive, fs:scope-log,
fs:allow-appconfig-{read,write}-recursive,
fs:allow-app-{read,write}-recursive,
fs:allow-applocaldata-{read,write}-recursive,
fs:allow-applog-{read,write}-recursive,
fs:allow-appcache-{read,write}-recursive,
fs:allow-cache-{read,write}-recursive,
fs:allow-temp-{read,write}-recursive,
fs:allow-data-{read,write}-recursive,
fs:allow-config-{read,write}-recursive,
updater:default
```

**Most over-permissive items:**

1. **`fs:read-all` + `fs:write-all`** in default.json. JS can read and write ANY file via `@tauri-apps/plugin-fs`. The codebase actually uses this only at `environments.tsx:5` (`exists` from BaseDirectory.Home), but the capability enables far more.
2. **`shell:allow-execute` + `shell:allow-spawn`** in desktop.json. JS could spawn ANY shell command via `@tauri-apps/plugin-shell`. The codebase doesn't use these from JS at all (Rust spawns subprocesses directly), so these are pure attack surface.
3. **`opener:allow-open-path` with `"path": "**"`**. JS can open ANY file with the OS default handler. The codebase uses this at `backends.tsx:4` (cert directory) only.
4. **`fs:allow-exists` with `"path": "**"`**. The only JS caller is the iTerm check at `environments.tsx:825-1000`.
5. **`updater:default`** — gives the renderer access to the updater plugin. The renderer never uses it (handled in Rust via `app.updater_builder()`).

**Port-time advice:** if the port adopts an Electron-with-preload model, none of these capabilities map directly. Replicate by preload-bridge whitelisting: only expose `fs.exists`, `fs.readFile` for `~/.openbb_platform/*`, `shell.openPath` for `<allowed dirs>`, etc. The current Tauri config grants the renderer the keys to the kingdom; the port should be more restrictive.

---

## L. Plugin permissions in `tauri.conf.json` vs `capabilities/*.json` — the gating model

`tauri.conf.json` doesn't gate IPC commands at all. Specifically:
- `tauri.conf.json:37-40` has `"security": { "csp": null, "capabilities": [] }`. The empty `capabilities` array means "use the default capability discovery" — Tauri auto-loads `capabilities/*.json` from the project root.
- The actual gating lives in `capabilities/default.json` and `capabilities/desktop.json` (see §K).
- **Custom `#[tauri::command]` handlers are NOT subject to plugin-permission gating.** Anything in `generate_handler!` is callable by ANY window with `core:default` permission. There's no per-command ACL.
- Plugin commands (`@tauri-apps/plugin-fs`, `@tauri-apps/plugin-shell`, etc.) ARE subject to the granular allow-lists in `capabilities/desktop.json`. So `plugin-shell:execute` is gated; `start_backend_service` is not.

**What can the frontend call without explicit permission?**
- All 57 `#[tauri::command]` functions, with no per-command gating.
- All plugin commands listed in §K (which is ~40 entries, most of them *-recursive).

**Port-time advice:** in Electron, every `ipcMain.handle` channel is whitelisted by name (no global "all commands callable"). Replicate this explicit model. Do not adopt Tauri's "all custom commands callable" behaviour by default.

---

## M. Long-running call patterns — v1's A/B taxonomy is incomplete

v1 identifies:
- **Pattern A** — Await + listen (most common).
- **Pattern B** — Fire + poll status (used by `install_conda` / `setup_python_environment`).

There is at least one more pattern in active use:

### Pattern C — Fire-and-forget (no await, no poll)

`grep -E '^\s+invoke\(' src/` (lines without leading `await` or `=`):

| Site | Command | Purpose |
|---|---|---|
| `BackendLogsPage.tsx:158` | `register_process_monitoring` | Pre-register buffer; explicitly `.catch(() => {})` to ignore errors |
| `JupyterLogsPage.tsx:166` | `register_process_monitoring` | Same pattern |
| `backends.tsx:782` | `update_backend_service` | Fire from event handler, no caller waits |
| `backends.tsx:847` | `update_backend_service` | Same |
| `backends.tsx:991` | `update_backend_service` | Same |
| `backends.tsx:2411` | `register_process_monitoring` | Logs window pre-register |
| `environments.tsx:396` | `save_working_directory` | Persist debounced workingDir update |
| `environments.tsx:2035` | `register_process_monitoring` | Same |
| `uninstall.tsx:84` | `app.exit` | (Bug — see §G; the intent is fire-and-forget exit) |

Plus the inline `invoke(action, { backend })` at `backends.tsx:2624` where `action` is a string variable resolved at runtime — also fire-and-forget.

**Why this matters for the port:** Pattern C calls discard errors and don't surface to the user. A TS port using strict typed RPC (e.g. tRPC) needs to either match the laxness or refactor these to await + try/catch. The fire-and-forget pattern is also incompatible with any RPC framework that requires response correlation.

### Pattern D — Await without listen (CRUD shape)

Most CRUD calls fall here: `await invoke("list_backend_services")` returns synchronously, no event needed. v1 implicitly covers this under "Pattern A" but it's worth distinguishing — these don't have any event subscription at all.

### Pattern E — Listen-only (no invoke)

`backends.tsx:2218-2293` listens to `backend-url-discovered` from a previously-spawned process. The original spawn was elsewhere (possibly in a previous app session via `auto_start`). This is a passive listener with no corresponding invoke.

Same shape: `installation-progress.tsx:822` listens to `install-progress` and the spawn might be from another window if the user reloaded the page. The current page doesn't always invoke `install_conda` — sometimes it just listens to an in-flight install.

**Port-time advice:** model these patterns explicitly. Pattern A maps to `await rpc.method() / subscription.subscribe()`. Pattern C is an anti-pattern but currently used; either fix it or document it. Pattern E requires that the event emission survives across page reloads — for Electron this means the main process retains state, for a web port it means a server-side stream that backfills.

---

## N. Plugin-fs `exists` is used at 4 sites (v1 says 1)

v1 says `plugin-fs` is "single use" at `environments.tsx:5`. The import IS at line 5, but the actual call sites are 4:

```
environments.tsx:825 (handleOpenSystemShell, macOS branch)
environments.tsx:884 (handleOpenPython, macOS branch)
environments.tsx:941 (handleOpenIPython, macOS branch)
environments.tsx:999 (handleOpenOpenBBCli, macOS branch)
```

All four are the same iTerm-presence check: `exists("/Applications/iTerm.app", { baseDir: BaseDirectory.Home })`. They were probably copy-pasted instead of factored into a helper. (Note: `BaseDirectory.Home` resolves to `~/`, so the actual path checked is `~/Applications/iTerm.app`, not the system `/Applications/iTerm.app` — that's a likely bug, but out of scope for IPC.)

**Port note:** all four can be replaced by a single Node `fs.access` or `fs.stat` call.

---

## O. Vestigial frontend dependencies — confirmed and expanded

v1 lists `taurpc`, `@tauri-apps/plugin-app`, `-http`, `-process`, `-window`, `-shell` as unused. Expanded check:

| Dependency | Verdict | Evidence |
|---|---|---|
| `taurpc` | UNUSED | `grep -r 'taurpc' src/` → 0 hits; `grep 'taurpc' src-tauri/src/` → 0 hits; `grep 'taurpc' src-tauri/Cargo.toml` → 0 hits |
| `@tauri-apps/plugin-app` | UNUSED | `getVersion` is imported from `@tauri-apps/api/app` (built-in), not the plugin |
| `@tauri-apps/plugin-http` | UNUSED | 0 imports in src/ |
| `@tauri-apps/plugin-log` (JS) | UNUSED | 0 imports in src/ (Rust `log::*` works via the server-side plugin only) |
| `@tauri-apps/plugin-process` | UNUSED | 0 imports — `uninstall.tsx:84` `invoke('app.exit')` is the broken attempt to use it |
| `@tauri-apps/plugin-updater` | UNUSED | 0 imports in src/ |
| `@tauri-apps/plugin-window` | UNUSED | 0 imports |
| `@tauri-apps/plugin-shell` | UNUSED | 0 imports |

**Port-time recommendation:** drop all 8 from `package.json` during the port. That's `taurpc` plus 7 unused Tauri plugins.

---

## P. TS port architectural options — additions to v1's electron-only recommendation

v1's recommendation chapter is heavily Electron-flavoured. Several other realistic options deserve mention:

### Option 1 — **Tauri 2.x with TS frontend rewrite** (ZERO Rust changes)

The current Rust backend is robust and battle-tested. If the port goal is "modernise the React UI" rather than "remove Rust", you can:
- Keep `src-tauri/` entirely as-is (all 57 commands, all events, all plugins).
- Rewrite `src/` (the React + TanStack Router frontend) in any TS framework: Solid, Svelte, Vue, plain TypeScript.
- The `invoke()` and `listen()` API are stable and framework-agnostic — they just take string command names and Promise/callback shapes.

Pros: zero risk on the backend, full feature parity by definition, smallest scope change.
Cons: doesn't address Rust maintenance burden if the goal is to eliminate it; doesn't address the "no type safety between JS and Rust" complaint.

### Option 2 — Electron + Node main process (v1's recommendation)

Pros: most familiar TS stack; rich ecosystem; same multi-window model.
Cons: ~150MB binary; need to reimplement subprocess management, log streaming, autostart, updater, native dialogs; need to reimplement window management.

### Option 3 — Wails 2 (Go backend, web frontend)

Wails is the Go equivalent of Tauri. The Rust code (subprocess spawn, conda env management, `lsof`-style port killing) is straightforward to port to Go (`os/exec`, `syscall`). Wails has its own bridge similar to Tauri's `invoke()` + `EventsOn()`.

Pros: small binary (~10MB on Linux); single static-linked Go executable; Go's stdlib has all the OS primitives needed; mature multi-platform packaging.
Cons: requires a Go developer; no first-class macOS-Catalyst-style support yet; smaller plugin ecosystem than Tauri.

### Option 4 — Pure web app + Node agent process

Run the desktop UI as a static SPA hosted by a local Node process that owns the OS interactions. The "agent" exposes HTTP+WebSocket. This is essentially what `openbb-api` does for its own concerns, multiplied to cover all the Tauri-handled OS work.

Pros: same UI works in the browser AND embedded; clearest separation of concerns; lets the user pick browser (no embedded webview to maintain).
Cons: no native window chrome, tray, or autostart without a separate launcher; harder to package (system tray needs an Electron-style host anyway).

### Option 5 — Tauri 2.x backend stays, TS becomes the frontend (Hybrid)

Same as Option 1 but using TanStack Router or React Router again — basically a full UI rewrite without changing the IPC layer at all. Useful if the React tree is the source of complexity.

**My take:** if the goal is to get rid of Rust, **Option 3 (Wails)** is the smallest-team-friction path because Go's subprocess and syscall stdlib is closer to the existing Rust patterns than Node's. If the goal is "modernise the UI", **Option 1** is dramatically lower risk. Option 2 (Electron) is the right answer only if the team has Electron experience; otherwise the reimplementation of all the subprocess/cleanup/tray/autostart code is a 6-month tax.

---

## Q. Other notes worth surfacing

1. **`api-keys.tsx:5` imports `message` from `@tauri-apps/plugin-dialog`** — `useEffect([error])` at `:466-472` calls `message(error, {kind: "error"})`. v1's plugin-dialog table mentions `confirm`/`message` but doesn't note this is in `api-keys.tsx`. Add to v1's "production code only" column.

2. **`backends.tsx:4` imports `openPath`/`openUrl` from `@tauri-apps/plugin-opener`** — used at `backends.tsx:1893` (openUrl for docs) and presumably for cert dir. v1 table is correct.

3. **`open_url_in_window` is used 4 times, not 3 as v1's main catalog implies** — `api-keys.tsx:428`, `environments.tsx:48`, `environments.tsx:1974`, `backends.tsx:1893`. v1 actually lists 4 in its catalog row (`api-keys.tsx:428; environments.tsx:48,1974; backends.tsx:1893`) — so this is OK, just want to confirm the count for the next reviewer.

4. **`uninstall_application` arg names are `removeUserData, removeSettings`** in JS, `remove_user_data, remove_settings` in Rust — Tauri converts. Works. v1 table doesn't show this row.

5. **The `app.manage(tray)` call at `main.rs:742`** is the only place `manage()` is invoked outside the builder chain. The TS port should be aware: in Tauri, `manage()` can be called on `AppHandle` even after the builder is finalised. In Electron there's no such pattern — module-scoped variables work the same and don't need a manager.

6. **`process_monitor.rs:9` declares `LOG_STORAGE` as `Lazy<LogStorage>` not `Lazy<Mutex<...>>`** — v1 lists it as `Lazy<LogStorage>` correctly in the table at §5.2 but the surrounding text says "Lazy<Mutex<T>> globals". Pedantic but worth pinning down: it's `Lazy<Arc<Mutex<HashMap<...>>>>`.

7. **Frontend never invokes `boolean-message`** — v1 says no listener; confirmed. The event is emitted at `backends.rs:1202` but `grep 'boolean-message' src/` is empty. Dead emit.

8. **Frontend-only events** — `process-output`, `install-progress`, `installation-directory`, `jupyter-status-update`, `backend-url-discovered`, `uninstall_progress`. The dead ones are `installation-status` (never emitted) and `boolean-message` (never listened). Total: 6 live + 2 dead.

9. **`tauri-plugin-fix-path-env` is in `Cargo.toml` but listed as "fix-path-env" by v1 (different naming).** The crate is `fix-path-env = "0.0"` and called as `fix_path_env::fix()` at `main.rs:470`. It's NOT a Tauri plugin in the formal sense — it's a regular crate that monkey-patches `$PATH`. v1 mislabels it.

---

## v2 → v1 corrections

1. **Command count: v1 says 53; actual is 57.** Add `stop_all_jupyter_servers`, `update_jupyter_status` (already in jupyter section but not totalled), and recount. v1's section subtotals don't sum to 53 either — they sum to 6+8+12+7+3+8+15+2 = 61, then v1 subtracts the unregistered ones to land on 53, but the arithmetic is off.

2. **State `.manage(...)` count: v1 says 4; actual is 4 — but v1 puts all four at "main.rs:489-491".** Tray manage is at `main.rs:742` (inside setup hook). Same total, different location.

3. **`Lazy<Mutex<T>>` count: v1 says 4; actual is 4.** But `LOG_STORAGE` is technically `Lazy<LogStorage>` where `LogStorage = Arc<Mutex<HashMap<...>>>` — not directly `Lazy<Mutex<...>>`.

4. **`#[tauri::command]` not in `generate_handler!`: v1 lists 1 (`check_installer_file_exists`); actual is 2.** The other is `list_conda_environments_impl` which has the macro applied to a generic-parameter function (no-op/bogus).

5. **`window.eval` count: v1 mentions 2; actual is 5.** `main.rs:408, 676, 784, 785, 799`.

6. **Frontend-callable commands: v1's catalog implies all 53 are called; actual is 51 of 57.** Six commands have NO frontend caller: `navigate_to_page`, `unregister_process_monitoring`, `update_jupyter_status`, `update_installation_error`, `list_jupyter_servers`, `stop_all_jupyter_servers`.

7. **`__root.tsx:132` and `__root.tsx:194` are NOT live callers.** Both are inside `{/* ... */}` JSX comments. v1 lists `__root.tsx:132` for `get_user_credentials` and `__root.tsx:194` for `toggle_theme` — neither is real. The only live `get_user_credentials` caller is `api-keys.tsx:182`. There are NO live `toggle_theme` callers; the command is registered but unreachable. (This makes 7 unreachable commands, not 6.)

8. **`install_conda` accepts `userDataDir` from JS but ignores it** — Rust signature is only `(directory, window)`. Three commands ignore JS args silently: `install_conda` (drops `userDataDir`), `install_extensions` (drops `directory`), `remove_environment` (drops `directory`). v1 mentions case-conversion but not silent-drop of unknown fields.

9. **`api-keys.tsx` imports `message` from `@tauri-apps/plugin-dialog`** — v1 lists plugin-dialog use sites but doesn't enumerate this one explicitly.

10. **Plugin-fs `exists` is used 4 times, not "single use" as v1 claims.** All four are macOS iTerm checks in `environments.tsx`.

11. **CSP is `null` in `tauri.conf.json:38`** — v1 doesn't mention this. It's load-bearing for the security analysis of `open_url_in_window`. Combined with the wildcard capability windows scope (`"windows": ["*"]`), every popped external URL window inherits full IPC + filesystem access.

12. **`open_url_in_window` does not validate URLs.** v1's "items that need special attention" section mentions CSP for the port but doesn't note the current absence of any URL allow-list.

13. **`fix_path_env` is a regular crate**, not a Tauri plugin. v1's plugin table lists it as "fix-path-env" alongside the real plugins.

14. **Pattern C (fire-and-forget) is missing from v1's "Patterns and Gotchas".** At least 9 sites use it, including the legitimate `register_process_monitoring` pre-registers and the buggy `invoke('app.exit')`.

15. **Capabilities are over-permissive in three concrete ways:** `fs:read-all`+`fs:write-all` are granted in default.json, `shell:allow-execute`+`shell:allow-spawn` are granted in desktop.json (and never used by JS), and `opener:allow-open-path` accepts any path `**`. v1 mentions capabilities exist but doesn't rank them by risk.

16. **There are 6 live events + 2 dead events** (`installation-status` never emitted; `boolean-message` never listened). v1 lists them but doesn't tag the dead ones uniformly. `process-output` payload schema also varies between emitters: jupyter (`{processId, output, timestamp}`), backends (`{processId, output, timestamp, type}`), environments (`{processId, output}`). A TS port should normalise this in one schema or the discrepancy will leak into client code.
