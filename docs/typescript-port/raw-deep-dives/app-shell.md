# Deep-Dive: App Shell — Root Layout, Index Redirect, Tray, Autostart, Updater, Uninstall

> Raw findings from Wave 1 agent. Source of truth for:
> - `20-features/feature-tray-and-autostart.md`
> - `20-features/feature-uninstall.md`
> - portions of `10-architecture/architecture-overview.md`
>
> Generated 2026-05-15.

## 1. App Boot Sequence (`src-tauri/src/main.rs`)

Order of operations on cold start:

| Step | Code | What |
|---|---|---|
| 1 | `main.rs:469` | Sets Windows subsystem to `windows` in release (`#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` line 2) so no console pops up. |
| 2 | `main.rs:470` | `fix_path_env::fix()` — patches `$PATH` for macOS GUI launches (LaunchServices doesn't inherit shell env). Crate from `tauri-apps/fix-path-env-rs`. |
| 3 | `main.rs:471` | `init_process_monitoring()` — initializes the global `LogStorage` for child-process log capture (`utils/process_monitor.rs`). |
| 4 | `main.rs:473-488` | `tauri::Builder::default()` registers plugins in this order: `updater`, `opener`, `single_instance` (with focus-existing callback), `fs`, `shell`, `persisted_scope`, `log` (clear targets, then add Stdout + Stderr), `dialog`. |
| 5 | `main.rs:489-491` | `.manage(...)` registers three managed states: `ProcessLogState(get_log_storage())`, `RunningProcesses::new()`, and **`check_installation_on_startup()` — this runs synchronously here, so the `InstallationState` is populated BEFORE setup runs**. |
| 6 | `main.rs:492-550` | `.invoke_handler(generate_handler![...])` — registers all 50+ commands (toggle_theme, navigate_to_page, install/setup, environments, jupyter, credentials, backends, uninstall, quit, etc.). |
| 7 | `main.rs:551` | `.setup(|app_handle| { … })` — invoked once after Tauri windows are created. Inside setup: |
|   7a | `main.rs:552` | Re-runs `check_installation_on_startup()` (so the state is computed twice — once for `.manage`, once locally). |
|   7b | `main.rs:554-567` | Reads `.show_on_restart` flag in `~/.openbb_platform/.show_on_restart`. Deletes it and remembers boolean. |
|   7c | `main.rs:569-579` | If installed, **spawn async task with 100ms delay** then `initialize_backends(...)` to start any auto-start backend services. |
|   7d | `main.rs:581-583` | `window.set_menu(Menu::new(handle)?)` — installs an empty top menu. |
|   7e | `main.rs:585-599` | Reads autostart status (per-OS `is_autostart_enabled`), defaulting to `true` on error. |
|   7f | `main.rs:602-631` | Builds tray menu items (Open, Open Workspace, separator, Backends, Environments, API Keys, separator, Start at Login [check item, initial state from 7e], separator, Check for Updates, Uninstall, Quit). |
|   7g | `main.rs:635-742` | Builds `TrayIconBuilder` with default window icon, tooltip "Open Data Platform - By OpenBB", attaches the menu and `on_menu_event` handler (see §4). `app_handle.manage(tray)` keeps it alive. |
|   7h | `main.rs:744-760` | Attaches `on_window_event` to main window: `CloseRequested` ⇒ `window.hide()` + `api.prevent_close()` (close-to-tray). On macOS, sets the NSWindow background to opaque black via `objc2_app_kit`. |
|   7i | `main.rs:762-771` | `ctrlc::set_handler` — on SIGINT, builds a fresh tokio runtime, runs `cleanup_all_processes`, then `app.exit(0)`. |
|   7j | `main.rs:773-777` | macOS only: `utils::app_termination::setup_termination_handler` — registers an Obj-C observer on `NSApplicationWillTerminateNotification` (see `utils/app_termination.rs:50-93`). |
|   7k | `main.rs:779-786` | If **not installed**: shows + focuses window, evals `localStorage.clear()`, then `window.location.href = '/setup'`. |
|   7l | `main.rs:787-808` | If **installed**: spawns background update check after 3s sleep (`background_update_check`), then evals `localStorage.setItem('environments-first-load-done', 'true')`. If `show_after_update`, calls `window.show()`. Always `window.set_focus()`. |
| 8 | `main.rs:811` | `.build(generate_context!())` — produces the running app. On error, exits with code 1. |
| 9 | `main.rs:816-857` | `.run(|app_handle, event| …)` — global event loop hook (see §7 for cleanup cascade). |

Note: window is created with `"visible": false` in `tauri.conf.json:17`, so visibility is entirely driven by setup-step 7k/7l decisions.

---

## 2. The Redirect Decision (`src/routes/index.tsx`)

The "/" route intends to be event-driven but in practice falls back to invoke. Sequence (`index.tsx:6-77`):

1. `Base()` mounts, `loading=true` (line 7).
2. `useEffect` fires on mount (line 9). Creates a `redirectPromise` (line 13).
3. Subscribes to two Tauri events:
   - `installation-status` (line 15) — `boolean` payload. **NEVER actually emitted by the Rust side** (verified via grep over `src-tauri/src/`). Dead listener.
   - `installation-directory` (line 27) — `string` payload. Emitted from `tauri_handlers/startup.rs:1307` after a successful conda install. Stores into `localStorage["installationDirectory"]`.
4. After **2000 ms timeout** (line 34), invokes `get_installation_state` (`main.rs:388-391`), which returns the managed `InstallationState` struct populated at step 5 of boot.
5. If `state.is_installed` → `resolve("/environments")`; else `resolve("/setup")`. On invoke error → `resolve("/setup")`.
6. `redirectPromise.then((target) => window.location.href = target)` — full document navigation, not router-internal. This causes a hard reload.

In practice, on a fresh Tauri start the "/" route is never visited, because `main.rs:785` already evals `window.location.href = '/setup'` for non-installed and the installed path doesn't redirect at all (the user lands on whatever the start route is). The "/" route exists primarily for the post-install reload flow where the URL gets reset.

`InstallationState` struct (`main.rs:66-70`):
```rust
struct InstallationState {
    is_installed: bool,
    installation_directory: Option<String>,
}
```
Populated by `check_installation_on_startup()` (`main.rs:272-386`) which:
- Reads `$HOME/.openbb_platform/system_settings.json`.
- Looks for `install_settings.installation_directory` (line 328) OR top-level `installation_directory` (line 352).
- Validates by checking `<install_dir>/conda/{Scripts/conda.exe | bin/conda}` exists.
- `is_installed=true` only if conda binary is present.

---

## 3. Root Layout (`src/routes/__root.tsx`)

### Tabs and route mapping
Three nav tabs at `__root.tsx:237-239`:

| Tab Label | `to` path | Notes |
|---|---|---|
| Backends | `/backends` | `backends.tsx` |
| Environments | `/environments` | search params `{directory: undefined, userDataDir: undefined}` (line 238) |
| API Keys | `/api-keys` | `api-keys.tsx` |

Tab visibility: hidden when on `/jupyter-logs`, `/backend-logs`, `/setup`, or `/installation-progress` (`__root.tsx:103-108`, `shouldHideNav` flag, line 231).

### Navigation lock during environment creation
The exact mechanism (`__root.tsx:31-49`):
- `NavLink` calls `useEnvironmentCreation()` to read `isCreatingEnvironment` from context.
- If `isCreatingEnvironment === true && !isCurrentPage`, render a non-clickable `<div>` with `cursor-not-allowed opacity-50` + `tabIndex={-1}` instead of the `<button>`. The current page tab still renders as a div (also disabled), so navigation is fully locked.
- Active-state click handler at `__root.tsx:51-55` does `e.preventDefault()`, `setSelectedTab(to)` (immediate UI update), then `router.navigate({to, search})`.

Provider chain (`__root.tsx:258-264`): `RootWithProvider` wraps `Root` in `<EnvironmentCreationProvider>` (`contexts/EnvironmentCreationContext.tsx:23-31`). The provider just holds `useState<boolean>(false)` and exposes `{isCreatingEnvironment, setIsCreatingEnvironment}`. `environments.tsx` flips it during create operations.

### Window chrome
- Title bar: `tauri.conf.json:27` sets `"titleBarStyle": "Transparent"`. macOS window background is overridden to opaque black via Obj-C in `main.rs:752-759` (using `objc2_app_kit::NSColor::colorWithRed_green_blue_alpha(0,0,0,1)`).
- Window effects (`tauri.conf.json:29-34`): `["titlebar", "mica"]` — Mica blur on Windows 11.
- `decorations: true`, `acceptFirstMouse: true`, `backgroundThrottling: "disabled"`.
- `theme: "Dark"` forced; theme toggle code in root is commented out (`__root.tsx:115-207`).

### Header and footer
- Header: `ODPLogo` left, `OpenBBLogo` + `<ShowVersion/>` right (`__root.tsx:215-228`).
- Footer: copyright "Copyright © 2025 OpenBB Inc." (`__root.tsx:249-253`).

### Misc
- Backspace globally `preventDefault`'d outside form fields (`__root.tsx:78-98`) — prevents back-navigation.
- `selectedTab` state synced with `router.state.location.pathname` via `useEffect` (`__root.tsx:110-112`).

---

## 4. System Tray Menu (`main.rs:602-742`)

Built in setup (step 7f). Menu item ordering: Open, Open Workspace, sep, **Backends, Environments, API Keys** (note: tray order is Backends-Environments-API Keys), sep, Start at Login, sep, Check for Updates, Uninstall, Quit.

| ID | Label | Handler | What it does |
|---|---|---|---|
| `open` | "Open Window" | `main.rs:652-657` | `window.show().unwrap(); window.set_focus().unwrap()`. |
| `open_workspace` | "Go to Workspace" | `main.rs:658` | Calls `open_workspace_in_browser()` (`helpers.rs:1022-1037`) — runs `open`/`xdg-open`/`cmd /c start` against `https://pro.openbb.co`. **System browser, not embedded.** |
| `open_environments` | "Environments" | `main.rs:659` | `navigate_to_page(handle, "/environments")` |
| `open_api_keys` | "API Keys" | `main.rs:660` | `navigate_to_page(handle, "/api-keys")` |
| `open_backends` | "Backends" | `main.rs:661` | `navigate_to_page(handle, "/backends")` |
| `check_updates` | "Check for Updates" | `main.rs:662-667` | Spawns `trigger_update_dialog(handle)` — calls `check_and_apply_update(app, true)` so user always sees a dialog. |
| `uninstall` | "Uninstall" | `main.rs:668-679` | Show+focus window; if `!is_installed` show error dialog; else `window.eval("window.location.href = '/uninstall';")`. |
| `start_at_login` | "Start at Login in Background" (CheckMenuItem) | `main.rs:680-735` | Reads current state per-OS, toggles, calls per-OS `enable_autostart`/`disable_autostart`, then `start_at_login_item.set_checked(target_state)`. |
| `quit` | "Quit" | `main.rs:642-651` | Builds fresh tokio Runtime, blocks on `cleanup_all_processes`, then `app.exit(0)`. |

### Frontend-navigation mechanism (key finding)
**No event emission, no deep links.** Tray uses `navigate_to_page` (`main.rs:393-410`):
```rust
window.show(); window.set_focus();
window.eval(format!(r#"
  if (localStorage.getItem('environments-first-load-done') === 'true') {{
      window.location.href = '{page}';
  }} else {{
      console.log('Navigation prevented: environments-first-load-done not set');
  }}
"#));
```
Two important details:
- It's a `window.eval` injection of raw JS that does a hard `location.href` assignment (not router.navigate, so the router is rebuilt on each tray click).
- The `environments-first-load-done` localStorage gate prevents tray navigation before the app has finished its first valid environments load — preventing tray clicks from interrupting setup. The flag is set in `main.rs:799` after a valid install detection.

The "Uninstall" tray item also uses `window.eval` (`main.rs:676`), bypassing the localStorage gate.

---

## 5. Autostart

Toggle persistence is per-OS, with no shared state file — the OS itself is the source of truth. Each backend implements `is_autostart_enabled / enable_autostart / disable_autostart`.

### macOS (`utils/autostart/macos_autostart.rs`)
- Uses `osascript` shelling to AppleScript against `System Events → login items`.
- `get_app_path` (line 7-43): walks up to 5 parent dirs looking for `.app` extension; falls back to raw exe path.
- `enable_autostart` (line 46-108): runs check script first; if absent, `make new login item at end with properties {path:..., hidden:false, name:"OpenBB Platform"}`.
- `disable_autostart` (line 111-169): scans login items, removes any whose path contains the app path or whose name matches "OpenBB Platform"/"Open Data Platform"/"app"/"openbb-platform". Iterates in reverse to preserve indices.
- `is_autostart_enabled` (line 172-215): same path-contains check, returns bool.
- **Does NOT use a `~/Library/LaunchAgents/*.plist` file** despite the uninstall code mentioning one (`uninstall.rs:697-704` references `com.openbb.platform.plist` but it's never created — it's defensive cleanup of legacy installs).

### Windows (`utils/autostart/windows_autostart.rs`)
- Uses **`%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup\<package_name>.lnk`** (line 150-160). Package name comes from `app_handle.package_info().name` = `"openbb-platform"` (Cargo `[package].name`).
- `enable_autostart` (line 15-133): builds a `.lnk` shell shortcut via raw COM (`CoCreateInstance(CLSID_ShellLink)` → `IShellLinkW::SetPath(exe)` → `IShellLinkW::SetShowCmd(SW_SHOW)` → `IPersistFile::Save(lnk_path)`). Manual `CoUninitialize` on every error path.
- `disable_autostart` (line 135-148): just `fs::remove_file` on the .lnk path.
- `is_autostart_enabled` (line 5-13): `shortcut_path.exists()`.
- **NOT the registry `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` approach**, although uninstall (`uninstall.rs:646-672`) defensively cleans those keys too.

### Linux (`utils/autostart/linux_autostart.rs`)
- XDG autostart: writes a `.desktop` file at `~/.config/autostart/<package_name>.desktop` (line 81-94, uses `dirs::config_dir()`).
- Desktop entry contents (line 32-44):
  ```
  [Desktop Entry]
  Type=Application
  Name=openbb-platform
  Exec="<path/to/exe>"
  Terminal=false
  X-GNOME-Autostart-enabled=true
  ```
- Sets file mode 0o755 via `PermissionsExt` (line 54-65).
- `is_autostart_enabled` = file exists.

---

## 6. Updater (`main.rs:100-270`)

Plugin: `tauri-plugin-updater` (registered `main.rs:474`). Capability `updater:default` (`capabilities/desktop.json:71`).

### Endpoint
Hard-coded twice (somewhat redundantly):
- `tauri.conf.json:60-63`: `"endpoints": ["https://github.com/OpenBB-finance/OpenBB/releases/download/ODP/latest.json"]`.
- `main.rs:108`: same URL inside `check_and_apply_update`. The Rust code uses the runtime `.endpoints(vec![url])` builder which **overrides** the conf value.

### Signature verification
Pubkey embedded in `tauri.conf.json:63` (minisign, base64-wrapped). The updater plugin enforces verification automatically — any update artifact must be signed with the matching private key.

### Flow
`check_and_apply_update(app, always_prompt: bool)` at `main.rs:100-262`:
1. Build HTTP headers: `User-Agent: ODP-Updater` (line 115), `X-App-ID: <get_or_create_app_id()>` (line 132-134). The app_id is created/persisted via `helpers.rs:1598-1655` (UUIDv4 stored in app data dir).
2. `app.updater_builder().headers(headers).endpoints(vec![url]).build()`.
3. `updater.check().await`:
   - `Ok(Some(update))`: `dialog().message("A new version (X) is available. ...").buttons(YesNo).show(...)`. On Yes:
     - Spawn async task; show + focus main window.
     - `update.download_and_install(|_,_|{}, ||{}).await` — empty progress callbacks.
     - On success, write `~/.openbb_platform/.show_on_restart` flag (line 204-208), call `app.request_restart()` (line 214). Restart cascade is then handled in the global RunEvent loop (see §7).
   - `Ok(None)`: if `always_prompt`, show "You are already running the latest version".
   - `Err(e)`: log; if `always_prompt`, show error dialog.
4. `trigger_update_dialog` = `always_prompt: true` (called from tray "Check for Updates").
5. `background_update_check` = `always_prompt: false` (called 3s after boot for installed users, `main.rs:792-796`).

The `.show_on_restart` flag is read in setup (`main.rs:554-567`) — when present, the file is deleted and `window.show()` is forced after restart so the user sees the new version.

---

## 7. Cleanup on Quit

### `cleanup_all_processes` (`main.rs:419-467`)
Called from: tray Quit, Ctrl-C handler, macOS `applicationWillTerminate`, `quit_application` command, and the global `RunEvent::ExitRequested` hook.

Cascade with **outer 10s overall timeout** wrapping inner per-step timeouts:
1. `tokio::time::timeout(10s, async { ... })` outer wrap (line 423-453).
2. Inner step A: `tokio::time::timeout(3s, stop_all_jupyter_servers(handle))` (line 426-435).
3. Inner step B: `tokio::time::timeout(3s, stop_all_backend_services(handle, &RealFileSystem, &RealEnvSystem, &RealFileExtTrait))` (line 437-451).
4. Each step logs the outcome (`Ok(Ok)` success, `Ok(Err(e))` inner error, `Err(_)` timed out).
5. Outer timeout: logs "Cleanup process timed out after 10 seconds" if breached.
6. **Windows-only** (line 460-464): extra 500ms `tokio::sleep` to let GDI/UI handles clean up before exit.

### Window close vs app quit
- `WindowEvent::CloseRequested` (`main.rs:746-751`): `window.hide()` + `api.prevent_close()`. Clicking the X / Cmd+W just hides to tray; the process keeps running.
- True quit only via tray Quit, Ctrl-C, or system terminate.

### Ctrl-C (`main.rs:762-771`)
`ctrlc::set_handler` builds a brand-new `tokio::runtime::Runtime`, blocks on `cleanup_all_processes(handle)`, then `app.exit(0)`. Building a runtime inside the SIGINT handler is a code smell (could panic if called from inside an async context) but works for a process-wide signal.

### macOS termination (`utils/app_termination.rs`)
Registers Obj-C observer (`OBBAppTerminationObserver`) for `NSApplicationWillTerminateNotification`. On fire (line 27-41): builds new tokio Runtime, blocks on `cleanup_all_processes`. **Does not call `exit()`** to avoid interfering with macOS app shutdown. Uses a `static mut APP_HANDLE_PTR` (Box::leaked AppHandle) — this is the only way to bridge Obj-C callbacks back to the Rust app handle.

### Global RunEvent loop (`main.rs:816-857`)
- On every `ExitRequested`: prevents exit, builds Runtime, blocks on cleanup.
- Tracks `RESTART_EXIT_CODE` via `AtomicBool` to differentiate restart from normal quit. > ⚠️ BUG: `is_restart_requested` is created fresh per-event (line 817), so the atomic flag never persists across events. The detection happens at line 821 inside the same event arm that uses it on line 833 — so it does work for the same-event case but the `Arc` is pointless.
- On normal exit: `std::process::exit(0)`.
- On restart: `tauri::process::restart(&app.env())`.
- On `RunEvent::Exit` with restart flag: `app.cleanup_before_exit()` then `tauri::process::restart`.
- macOS `RunEvent::Reopen` (clicking dock icon) (line 844-848): `window.show() + set_focus()`.

### `quit_application` command (`main.rs:412-417`)
Frontend-callable: runs `cleanup_all_processes` then `app.exit(0)`.

---

## 8. Uninstall Flow

### Frontend (`src/routes/uninstall.tsx`)
Reached via tray "Uninstall" → `window.eval('window.location.href = '/uninstall'')` (`main.rs:676`).

State (line 11-19): `isUninstalling`, `removeUserData`, `removeSettings`, `uninstallProgress`, three displayed dirs (`installationDirectory`, `userDataDirectory`, `settingsDirectory`), `showProgressDialog`, `isModalOpen`.

On mount (line 22-37): invokes `get_installation_directory`, `get_userdata_directory`, `get_settings_directory` to display them.

Listens for `uninstall_progress` event (line 42-50, only while uninstalling) — payload string updates the spinner label.

Handler `handleUninstall` (line 59-96):
1. `confirm()` dialog via `@tauri-apps/plugin-dialog`.
2. Sets `isUninstalling=true`, shows progress dialog.
3. `invoke('uninstall_application', { removeUserData, removeSettings })`.
4. After resolve, sleep 2s, then `invoke('app.exit')` — > ⚠️ BUG: this is a likely-broken invoke target (no Rust handler named `app.exit` exists; should be `quit_application`). The macOS path inside the Rust handler already does `std::process::exit(0)`, so on macOS the JS `setTimeout` never matters.
5. Three checkboxes: "Remove Conda and Environments" (Required, always checked, readonly), "Remove user data", "Remove application settings".

### Backend (`src-tauri/src/uninstall.rs`)
`uninstall_application(app_handle, window, remove_user_data, remove_settings)` at line 14-403. Uses `window.emit("uninstall_progress", ...)` throughout.

Cascade:
1. **Stop services** (line 30-45): `stop_all_jupyter_servers`, `stop_all_backend_services` — no timeouts here, blocking awaits.
2. **Remove system integrations** (line 50-82):
   - Per-OS `disable_autostart` (mac/win/linux).
   - `remove_system_integrations()`:
     - Windows (line 642-691): `reg delete` against four startup keys × five entry names; scans `%LOCALAPPDATA%\…\Startup` for shortcuts containing "openbb".
     - macOS (line 693-705): `launchctl unload` + remove `~/Library/LaunchAgents/com.openbb.platform.plist` (defensive — never created by current autostart code).
     - Linux (line 707-725): stops/disables `systemctl --user openbb-platform.service`, removes `~/.config/systemd/user/openbb-platform.service`, `daemon-reload`.
3. **Read installation dir** from `~/.openbb_platform/system_settings.json` `install_settings.installation_directory` (line 406-433).
4. **Remove conda envs** (line 436-582): on Windows, prefer running `<install>/conda/Uninstall-Miniforge3.exe /S`; otherwise iterate `<install>/conda/envs/*` (skip `base`) calling `conda env remove --name <env> --yes`.
5. **Force kill** any conda/python processes (`taskkill` on Windows, `pkill -f` on Unix) (line 111-130).
6. **`fs::remove_dir_all` install dir** with retries (line 584-639): falls back to `rd /s /q` (Windows) or `rm -rf` (Unix).
7. **Update settings JSON** (line 149-206): removes `environments` key (and the YAML files they point to) and `install_settings` key from `system_settings.json`.
8. If `remove_settings`: nuke whole `~/.openbb_platform/`. Else, just nuke `~/.openbb_platform/environments/`.
9. If `remove_user_data`: nuke `~/.openbb_platform/user_data/`.
10. Sleep 3s.
11. Windows: `run_windows_system_uninstaller` (line 730-835) — writes a delayed `.bat` to `%TEMP%\openbb_uninstall.bat` that waits for the app process to exit, kills any leftover, then runs `uninstall.exe /S` and removes `%LOCALAPPDATA%\OpenBB Platform` and `%LOCALAPPDATA%\co.openbb.platform`. Spawns it via `cmd /C start "OpenBB Platform Final Cleanup" <bat>` — visible window so user sees it.
12. Non-Windows: directly removes platform-specific application data dir:
    - macOS: `~/Library/Application Support/co.openbb.platform`
    - Linux: `~/.config/co.openbb.platform`
13. macOS only (line 296-400): generates `/tmp/openbb_uninstall_cleanup.sh` that waits for the app process to die (via `pgrep -f`), then `rm -rf` the `.app` bundle plus `~/Library/Logs/co.openbb.platform`, `~/Library/Caches/co.openbb.platform`, `~/Library/WebKit/co.openbb.platform`, `~/Library/WebKit/openbb-platform`, `~/Library/Application Scripts/group.co.openbb.platform`. Shows an `osascript` notification on success. Then closes window and `std::process::exit(0)`.

Post-uninstall: app is gone; `~/.openbb_platform/` is partially or fully cleared depending on flags; the `.app`/`uninstall.exe` removes the binaries.

---

## 9. Single-Instance

Yes. Plugin `tauri-plugin-single-instance` registered at `main.rs:476-481`. The callback fires when a second instance launches and is invoked **inside the original instance**:
```rust
.plugin(tauri_plugin_single_instance::init(|app, _, _| {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}))
```
Second instance exits immediately; first instance brings its window forward. `_, _` args are CLI args and CWD (unused — no deep-link handling).

**No deep-link plugin registered, no `tauri-plugin-deep-link`.** Custom URL schemes are not handled.

---

## Cross-cutting maps

### Plugins → consumers
| Plugin | Frontend usage |
|---|---|
| `tauri-plugin-updater` | `main.rs::check_and_apply_update`; tray "Check for Updates"; auto-runs 3s after boot. |
| `tauri-plugin-opener` | Likely in environments/api-keys pages (open paths/URLs). |
| `tauri-plugin-single-instance` | Rust-only. |
| `tauri-plugin-fs` | All routes touching files (api-keys reads/writes credentials, etc.). |
| `tauri-plugin-shell` | Backend/jupyter spawn flows. |
| `tauri-plugin-persisted-scope` | Persists user-granted FS scopes across runs (env file picks). |
| `tauri-plugin-log` | All Rust `log::` macros routed to Stdout+Stderr. |
| `tauri-plugin-dialog` | `uninstall.tsx` (`confirm`); `main.rs` updater dialogs. |

### Events emitted/listened
| Event | Emitter (file:line) | Listener |
|---|---|---|
| `installation-status` | **never emitted** | `index.tsx:15` (dead) |
| `installation-directory` | `tauri_handlers/startup.rs:1307` | `index.tsx:27` (writes localStorage) |
| `uninstall_progress` | `uninstall.rs:25-26` (closure), `uninstall.rs:450,470,535` | `uninstall.tsx:43` |

There are surely more events emitted from environments/backends/jupyter, but they're outside scope.

---

## TS Port Translation Table (Tauri → Electron)

| Concern | Tauri (current) | Electron equivalent |
|---|---|---|
| **Process boot** | `tauri::Builder + setup hook` | `app.whenReady().then(() => …)` in `main.ts`; create `BrowserWindow` with `show: false`. |
| **Managed state** (`InstallationState`) | `.manage(…)` + `tauri::State<T>` parameter | Module-scoped `let state: InstallationState`; expose via `ipcMain.handle('get_installation_state', () => state)`. |
| **Invoke handler** (`#[tauri::command]`) | `invoke_handler![generate_handler!]` | `ipcMain.handle('command_name', async (e, args) => …)` per command. |
| **Tray menu** | `TrayIconBuilder` + `MenuItemBuilder` + `CheckMenuItemBuilder` | `new Tray(iconPath)`, `Menu.buildFromTemplate([{label, click, type:'checkbox', checked}])`. Use `tray.setContextMenu()`. CheckMenuItem ⇒ `{type:'checkbox', checked}`. |
| **Tray → frontend nav** | `window.eval(\`window.location.href='/path'\`)` gated by localStorage | `mainWindow.webContents.executeJavaScript(\`window.location.href='/path'\`)` OR cleaner: `mainWindow.webContents.send('navigate', '/path')` and listen with `ipcRenderer.on` then call `router.navigate`. Keep the localStorage gate or move to a Redux/Zustand `bootReady` flag. |
| **Window close→hide** | `WindowEvent::CloseRequested` + `api.prevent_close()` + `window.hide()` | `mainWindow.on('close', e => { if (!app.isQuitting) { e.preventDefault(); mainWindow.hide(); } })`. Set `app.isQuitting=true` in tray Quit handler before `app.quit()`. |
| **Updater** | `tauri-plugin-updater` + GitHub `latest.json` + minisign | `electron-updater` (autoUpdater) with `provider: 'github'`. Sign with code-signing cert (mac/win) — minisign equivalent does not exist; rely on platform code-signing + `app-update.yml` SHA512. Use `autoUpdater.checkForUpdates()` + `update-available`/`update-downloaded` events. The `.show_on_restart` flag pattern can stay as-is (write to userData dir). |
| **Single-instance lock** | `tauri-plugin-single-instance` | `const got = app.requestSingleInstanceLock(); if (!got) app.quit(); else app.on('second-instance', () => { mainWindow.show(); mainWindow.focus(); })`. |
| **Autostart — macOS** | Pure `osascript` → System Events login items | `app.setLoginItemSettings({openAtLogin: true, openAsHidden: false})`. Avoid the AppleScript dance; Electron wraps `SMLoginItemSetEnabled`. |
| **Autostart — Windows** | COM `IShellLinkW` writing `Startup\openbb-platform.lnk` | `app.setLoginItemSettings({openAtLogin: true, path: process.execPath, args: ['--autostart']})` — Electron uses the registry under the hood. Or keep the Startup-folder approach via `node-windows` if you want the `.lnk` shortcut explicitly. |
| **Autostart — Linux** | `~/.config/autostart/openbb-platform.desktop` | No `app.setLoginItemSettings` on Linux; manually write the same `.desktop` file. Reuse the exact format from `linux_autostart.rs:32-44`. |
| **Graceful shutdown** | `cleanup_all_processes` with nested `tokio::time::timeout` (3s/3s/10s) | `app.on('before-quit', async (e) => { e.preventDefault(); await Promise.race([cleanup(), wait(10_000)]); app.exit(0); })`. Use `Promise.race` for timeouts; track child processes so you can `kill('SIGTERM')` then `SIGKILL` on timeout. |
| **Ctrl-C** | `ctrlc::set_handler` building fresh runtime | `process.on('SIGINT', async () => { await cleanup(); app.exit(0); })`. Node has native signal handling — no runtime build needed. |
| **macOS `applicationWillTerminate`** | Custom Obj-C observer in `app_termination.rs` | `app.on('will-quit', async (e) => { e.preventDefault(); await cleanup(); app.exit(0); })` — Electron exposes this directly. |
| **macOS reopen** | `RunEvent::Reopen` shows window | `app.on('activate', () => { if (BrowserWindow.getAllWindows().length === 0) createWindow(); else mainWindow.show(); })`. |
| **Window background (macOS opaque black)** | `objc2_app_kit::NSWindow::setBackgroundColor` | `new BrowserWindow({backgroundColor: '#000000', vibrancy: undefined})`. Avoid `transparent: true` to keep opaque. |
| **Mica / titleBarStyle** | `tauri.conf.json` `windowEffects: ["titlebar","mica"]`, `titleBarStyle: "Transparent"` | `new BrowserWindow({titleBarStyle: 'hiddenInset', backgroundMaterial: 'mica' /* Win11 */, vibrancy: 'sidebar' /* macOS */})`. |
| **Restart after update** | `tauri::process::restart(&env)` | `autoUpdater.quitAndInstall()` or `app.relaunch(); app.exit(0)`. |
| **Path env fix** | `fix_path_env::fix()` for macOS GUI launches | npm `fix-path` package, called once before any child_process.spawn. |
| **Capabilities/permissions** | `capabilities/*.json` | Not applicable — Electron has no per-window permission ACLs. Rely on `contextIsolation: true`, `nodeIntegration: false`, and a typed preload bridge with explicit IPC method whitelist. |
| **`window.eval` for JS injection** | `window.eval(js)` from Rust | `webContents.executeJavaScript(js)`. Prefer `webContents.send(channel, payload)` + `ipcRenderer.on` listener. |
| **Process monitor / log capture** | `utils/process_monitor.rs` (custom in-mem ring buffer) | Wrap `child_process.spawn`, pipe stdout/stderr through Node streams into a per-pid bounded array. |
| **Persisted FS scope** | `tauri-plugin-persisted-scope` | Custom: persist allowed paths to userData JSON; check on each FS call. |
| **Updater app-id header** | Custom `X-App-ID` from `get_or_create_app_id()` | Same — generate UUID once, persist to `app.getPath('userData')/.app_id`, send via custom request headers in autoUpdater config. |

---

## Key Files (absolute paths)

- `/home/user/OpenBBPort/desktop/src-tauri/src/main.rs` — boot, tray, updater, cleanup, window event hooks (859 lines)
- `/home/user/OpenBBPort/desktop/src-tauri/tauri.conf.json` — window config + updater endpoint + pubkey
- `/home/user/OpenBBPort/desktop/src-tauri/Cargo.toml` — plugin versions, package name `openbb-platform`
- `/home/user/OpenBBPort/desktop/src-tauri/capabilities/{default,desktop}.json` — permission ACLs
- `/home/user/OpenBBPort/desktop/src-tauri/src/uninstall.rs` — full 836-line uninstall cascade
- `/home/user/OpenBBPort/desktop/src-tauri/src/utils/autostart/{macos,windows,linux}_autostart.rs` — three OS-specific autostart impls
- `/home/user/OpenBBPort/desktop/src-tauri/src/utils/app_termination.rs` — macOS Obj-C terminate observer
- `/home/user/OpenBBPort/desktop/src-tauri/src/tauri_handlers/startup.rs:1307` — only emitter of `installation-directory` event
- `/home/user/OpenBBPort/desktop/src-tauri/src/tauri_handlers/helpers.rs:1022` — `open_workspace_in_browser` (system browser opener for `https://pro.openbb.co`)
- `/home/user/OpenBBPort/desktop/src/main.tsx` — React entry, TanStack Router setup
- `/home/user/OpenBBPort/desktop/src/routes/__root.tsx` — layout, nav lock, tabs
- `/home/user/OpenBBPort/desktop/src/routes/index.tsx` — redirect logic ("/")
- `/home/user/OpenBBPort/desktop/src/routes/uninstall.tsx` — uninstall UI
- `/home/user/OpenBBPort/desktop/src/contexts/EnvironmentCreationContext.tsx` — provider used by NavLink to lock nav

## Notable findings (gotchas for the port)

1. **`installation-status` event is dead** (`index.tsx:15`) — Rust never emits it; the 2-second invoke fallback is what actually decides routing.
2. **Tray nav uses `window.eval` + localStorage gate** — no Tauri events, no deep links. Port can switch to IPC + router.
3. **`InstallationState` is computed twice** at boot (line 491 for `.manage`, line 552 inside setup) — port can compute once.
4. **`uninstall.tsx:84` calls `invoke('app.exit')`** which has no matching handler in Rust — silent failure on non-macOS (works on macOS only because `uninstall.rs:399` `process::exit(0)` runs first).
5. **`is_restart_requested` AtomicBool** in run-event closure is recreated every event (`main.rs:817`) — pointless `Arc`; only works because detection + use are in the same event.
6. **macOS uninstall plist** at `~/Library/LaunchAgents/com.openbb.platform.plist` is referenced for cleanup but never created by autostart code — defensive cleanup of legacy installs.
7. **No deep-link plugin** — single-instance second-arg (CLI args) is discarded.
8. **Window `visible: false`** at startup; everything depends on Rust setup hook explicitly calling `window.show()`.
9. **Pure-OSAScript autostart on macOS** (no `LaunchAgents` plist) — port should use Electron's `setLoginItemSettings` instead.

---

## Cross-feature dependencies

- **depends-on** `feature-installation.md` for `InstallationState` (boot-time read of `system_settings.json` + conda existence check)
- **depended-on-by** every feature for nav routing, tray entry points, single-instance behavior, graceful shutdown timing
- **shares-state-with** `feature-backend-services.md` and `feature-jupyter.md` via the cleanup cascade (3s each timeout)
- **shares-state-with** `feature-environments.md` via `EnvironmentCreationContext` nav lock
- **shares-state-with** `feature-api-keys.md` via tray "API Keys" menu item
