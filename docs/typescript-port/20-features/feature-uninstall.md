# Feature: Uninstall

## Purpose
One-click full removal of OpenBB Platform plus (optionally) its data and
settings. The cascade stops every child process, undoes every system
integration, deletes conda envs and the install directory, then arranges
OS-specific post-mortem cleanup of the app bundle/binary itself. Uninstall is
the inverse of `feature-installation.md` — every artifact installation creates
has a deletion step here.

## User flows
1. **Golden path.** User opens tray → clicks `Uninstall` → `/uninstall` page
   loads → reviews three checkboxes (Conda required, user data optional,
   settings optional) → clicks the button → confirm dialog → progress modal
   streams `uninstall_progress` strings → app exits (macOS) / batch window
   appears (Windows) / **app keeps running** (Linux, see bugs).
2. **Not installed.** Tray `Uninstall` is only routable when
   `is_installed = true` — otherwise the menu handler shows an error dialog
   instead of navigating (`main.rs:668-679`).
3. **User cancels confirm.** `confirm()` returns false → nothing happens, page
   stays mounted.
4. **Mid-uninstall failure.** Any step in the cascade returns `Err`; the JS
   promise rejects, the progress modal stays on the last label, the half-deleted
   state on disk is **not rolled back** (see bugs).
5. **Windows: user dismisses the `.bat` window early.** All file deletions
   already ran before the `pause`; closing the window just skips the
   "Press any key" confirmation (`uninstall.rs:801`, `app-shell.v2.md` §11b).

## UI surface
`desktop/src/routes/uninstall.tsx`:
- State (lines 11-19): `isUninstalling`, `removeUserData`, `removeSettings`,
  `uninstallProgress` (string), three displayed dirs, `showProgressDialog`,
  `isModalOpen`.
- On mount (lines 22-37): invokes `get_installation_directory`,
  `get_userdata_directory`, `get_settings_directory` purely for display.
- Event listener (lines 42-50): subscribes to `uninstall_progress` only while
  `isUninstalling`; payload string drives the spinner label.
- `handleUninstall` (lines 59-96): `confirm()` → set state → invoke →
  `setTimeout(2000)` → `invoke('app.exit')` (broken — see bugs).
- Three checkboxes:
  1. **Remove Conda and Environments** — always checked, read-only, required.
  2. **Remove user data** (`removeUserData`) — defaults false.
  3. **Remove application settings** (`removeSettings`) — defaults false.

## Data flow

```mermaid
sequenceDiagram
    participant U as User
    participant Tray as Tray (Rust)
    participant FE as /uninstall (React)
    participant H as uninstall_application
    participant FS as Filesystem
    participant Sh as OS shell
    U->>Tray: Click "Uninstall"
    Tray->>FE: window.eval('location.href=/uninstall')
    FE->>FE: load 3 dirs via invoke
    U->>FE: Tick boxes + click button
    FE->>FE: confirm()
    FE->>H: invoke('uninstall_application', {removeUserData, removeSettings})
    loop 13-step cascade
        H-->>FE: emit('uninstall_progress', label)
    end
    H->>Sh: stop services / disable autostart / conda env remove
    H->>FS: rm install dir, settings, user_data
    alt macOS
        H->>Sh: write /tmp/openbb_uninstall_cleanup.sh & spawn
        H->>H: std::process::exit(0)
    else Windows
        H->>Sh: cmd /C start <visible .bat>
        H-->>FE: Ok(None)
        FE->>FE: setTimeout 2s → invoke('app.exit')  [no handler]
        Sh->>Sh: timeout /t 5 → taskkill /F /IM openbb-platform.exe
    else Linux
        H-->>FE: Ok(None)
        FE->>FE: invoke('app.exit')  [no handler — app stays running]
    end
```

### The 13-step cascade (`uninstall.rs:14-403`)

```mermaid
flowchart TD
    A[1. Stop jupyter + backend services] --> B[2. disable_autostart per-OS]
    B --> C[3. remove_system_integrations<br/>defensive legacy cleanup]
    C --> D[4. Read install dir from<br/>system_settings.json]
    D --> E[5. Remove conda envs<br/>Uninstall-Miniforge3.exe /S or<br/>conda env remove --name X --yes]
    E --> F[6. taskkill /pkill any leftover<br/>conda/python processes]
    F --> G[7. fs::remove_dir_all install dir<br/>fallback to rd /s /q or rm -rf]
    G --> H[8. Strip environments + install_settings<br/>keys from system_settings.json]
    H --> I[9. If removeSettings: nuke ~/.openbb_platform/<br/>else just environments/]
    I --> J[10. If removeUserData: nuke user_data/]
    J --> K[11. Sleep 3s — flush headroom]
    K --> L{OS?}
    L -- Windows --> M[12W. Spawn delayed .bat:<br/>timeout 5 → taskkill →<br/>uninstall.exe /S →<br/>rm LOCALAPPDATA]
    L -- Linux --> N[12L. rm ~/.config/co.openbb.platform]
    L -- macOS --> O[12M. rm ~/Library/Application Support/co.openbb.platform]
    O --> P[13M. Write /tmp cleanup.sh,<br/>spawn, std::process::exit 0]
```

## IPC contract
| Direction | Name | Payload | Returns | Used by |
|---|---|---|---|---|
| invoke | `uninstall_application` | `{ removeUserData: bool, removeSettings: bool }` | `Result<(), String>` (returns `Ok(None)` on Win/Linux; never returns on macOS) | `uninstall.tsx:74` |
| invoke | `get_installation_directory` | — | `string` | `uninstall.tsx:24` |
| invoke | `get_userdata_directory` | — | `string` | `uninstall.tsx:28` |
| invoke | `get_settings_directory` | — | `string` | `uninstall.tsx:32` |
| invoke | `app.exit` | — | **No handler — silent failure** | `uninstall.tsx:84` (typo) |
| event (Rust→JS) | `uninstall_progress` | `string` label | — | `uninstall.tsx:43-49`; emitted at `uninstall.rs:25-26, 450, 470, 535` and others |

## State surfaces
- **React state:** `uninstall.tsx` local state only — no global store. Progress
  label is the only mid-flight UI signal.
- **Rust state:** none persistent; the handler is one-shot and exits the
  process at the end (macOS) or returns and lets a spawned shell script finish
  the job (Windows/Linux/macOS post-mortem).
- **Disk files (all targeted for deletion):**
  - `~/.openbb_platform/` (full tree, conditional on `removeSettings`)
  - `~/.openbb_platform/environments/` (always)
  - `~/.openbb_platform/user_data/` (conditional on `removeUserData`)
  - `~/.openbb_platform/system_settings.json` (read first, then mutated, then deleted with the tree)
  - `<installation_directory>/` (whatever was in `install_settings.installation_directory`)
  - OS-specific app data + bundle (see matrix below).

## Persistence
Nothing is written *to keep*; everything is removed. The only files **written**
during uninstall are throw-away scripts:
- macOS: `/tmp/openbb_uninstall_cleanup.sh` (`uninstall.rs:296-400`)
- Windows: `%TEMP%\openbb_uninstall.bat` (`uninstall.rs:730-835`)

Both delete themselves at the end.

### What gets removed when (matrix)

Rows are user choices; columns are operating systems. Cells list paths
unconditionally removed in addition to the always-removed install dir and
conda envs.

| User choice | macOS extras | Windows extras | Linux extras |
|---|---|---|---|
| **Always (regardless of checkboxes)** | `~/Library/Application Support/co.openbb.platform`, `~/Library/Logs/co.openbb.platform`, `~/Library/Caches/co.openbb.platform`, `~/Library/WebKit/co.openbb.platform`, `~/Library/WebKit/openbb-platform`, `~/Library/Application Scripts/group.co.openbb.platform`, the `.app` bundle, `~/.openbb_platform/environments/` | `%LOCALAPPDATA%\OpenBB Platform`, `%LOCALAPPDATA%\co.openbb.platform`, app binary via `uninstall.exe /S`, `~/.openbb_platform/environments/` | `~/.config/co.openbb.platform`, `~/.openbb_platform/environments/` |
| **+ Remove user data** | `~/.openbb_platform/user_data/` | `~/.openbb_platform/user_data/` | `~/.openbb_platform/user_data/` |
| **+ Remove application settings** | entire `~/.openbb_platform/` (supersedes user_data branch) | entire `~/.openbb_platform/` | entire `~/.openbb_platform/` |

Conda envs and the install dir are removed in every scenario (the "Remove
Conda" checkbox is hard-locked on).

## Error handling
- Each cascade step returns `Result<(), String>`; the first error short-circuits
  the rest of the function and propagates to JS as a rejected promise.
- No try/rollback wrapping — partial state on disk is the user's problem.
- Service-stop calls (step 1) have **no timeout** — a wedged backend blocks the
  cascade indefinitely. Compare to `cleanup_all_processes` in
  `feature-tray-and-autostart.md` which uses 3s+3s/10s nested timeouts.
- Conda env removal (step 5) catches per-env errors and logs them but continues
  through the loop, so one broken env doesn't block uninstall.
- `fs::remove_dir_all` has a retry loop (`uninstall.rs:584-639`) that falls back
  to `rd /s /q` / `rm -rf` if the Rust path fails (typically Windows file locks).
- UX shows the progress label that was last emitted; on error there's no toast
  or recovery — the user is stuck on a half-spinning dialog.

## ▸ Interfaces with
- **depends-on** `feature-tray-and-autostart.md` — tray `Uninstall` item is the
  only entry (`main.rs:668-679`); cascade **step 2** calls `disable_autostart`.
- **depends-on** `feature-backend-services.md` — **step 1** runs
  `stop_all_backend_services` + `stop_all_jupyter_servers`.
- **depends-on** `feature-environments.md` — **step 4** walks
  `<install>/conda/envs/*`.
- **inverse-of** `feature-installation.md` — every artifact installation
  creates has a matching deletion step here; keep the two features paired so
  additions on one side get teardown on the other.
- **shares-state-with** all features via `~/.openbb_platform/` removal and
  child-process shutdown.

## TS port mapping
| Tauri call | TS equivalent | Notes |
|---|---|---|
| `invoke('uninstall_application', …)` | `ipcMain.handle('uninstall:run', …)` | Same shape; resolve once OS-specific cleanup is queued. |
| `window.emit('uninstall_progress', s)` | `webContents.send('uninstall:progress', s)` | `ipcRenderer.on` on FE. |
| `disable_autostart` (per-OS) | `app.setLoginItemSettings({openAtLogin:false})` + Linux `.desktop` rm | See `feature-tray-and-autostart.md`. |
| `taskkill /F /IM …` / `pkill -f …` | `child_process.exec(...)` or `process.kill(pid)` | Keep shell-out for leftover sweep; track PIDs for graceful kill first. |
| `reg delete HKCU\…\Run` (4×5 sweep) | `child_process.exec('reg delete …')` or `winreg` | Defensive — port as-is. |
| `launchctl unload …LaunchAgents/…` | `child_process.exec('launchctl unload …')` | Defensive legacy cleanup. |
| `fs::remove_dir_all` w/ retry + `rd /s /q` / `rm -rf` fallback | `fs.rm(p, {recursive:true, force:true, maxRetries:5, retryDelay:200})` | Node retry handles Windows locks. |
| `Uninstall-Miniforge3.exe /S` | `child_process.spawn(...)` after existence check | — |
| `cmd /C start "title" <bat>` | `child_process.spawn('cmd', ['/C','start',…], {detached:true})` | Keep visible window unless redesigned. |
| `osascript -e 'display notification …'` | Electron `new Notification(...)` | No shell-out needed. |
| `std::process::exit(0)` inside handler | `app.exit(0)` after enqueueing cleanup script | macOS path. |
| `invoke('app.exit')` (typo) | **Drop**; use `invoke('quit_application')` or main-process kill. | See bugs. |

## Known bugs and port-time fixes

> ⚠️ **`invoke('app.exit')` typo (`uninstall.tsx:84`).** No matching Rust handler.
> Silent failure on Linux (app keeps running) and Windows (masked only because
> the `.bat` runs `taskkill`). Works on macOS only because `uninstall.rs:399`
> `std::process::exit(0)` runs first. **Fix:** rename to `quit_application` and
> unify the exit path across all three OSes.

> ⚠️ **Defensive cleanup of never-created artifacts.** macOS LaunchAgents plist,
> Windows registry `Run`/`RunOnce` keys (4×5=20 combos), and Linux
> systemd-user service are all swept by `remove_system_integrations`
> (`uninstall.rs:642-725`) but **never written by current autostart code**
> (AppleScript / `.lnk` / XDG `.desktop` — see `feature-tray-and-autostart.md`).
> **Decision:** keep as legacy-detect for upgraders, else drop.

> ⚠️ **Windows `.bat` opens a visible console window** (`uninstall.rs:815-824`).
> User can dismiss mid-uninstall by clicking X — all destructive operations
> already ran before the `pause`, so only the "Press any key" confirmation is
> skipped. **Decision:** keep visible, or hide (`windowsHide: true`) + replace
> `pause` with a notification.

> ⚠️ **No rollback if any step fails.** Fail-fast leaves disk in whatever state
> the failing step produced. **Fix:** journal completed steps + recovery, or
> surface a clearer "Uninstall incomplete" error.

> ⚠️ **Step 1 has no timeout** on `stop_all_*_services`. A wedged child blocks
> the whole flow. **Fix:** add 3-5 s timeouts matching the shutdown cascade in
> `feature-tray-and-autostart.md`, then fall through to `taskkill`/`pkill`.

## Open questions
- **Dry-run mode?** Preview live conda env names, target paths, and disk size
  per branch before committing. Currently no preview — only this matrix.
- **Back up settings first?** "Remove application settings" is destructive and
  irreversible. Port could write `~/.openbb_platform.bak.<ts>.tar.gz` before
  nuking, at least for API keys and the env catalog.
- **Why a checkbox for required Conda removal?** Drop it (show as text) or
  make it actually optional so conda envs can survive for re-use.
- **Reachable outside the tray?** Currently tray-only. A help-page link or
  command-palette entry would help users who haven't learned quit-to-tray.

## Cross-feature dependencies
- **depends-on** `feature-installation.md` — reads
  `install_settings.installation_directory` (step 3) and deletes
  `system_settings.json` (steps 7-8).
- **depends-on** `feature-backend-services.md` and `feature-jupyter.md` —
  step 1 stops both stacks.
- **depends-on** `feature-environments.md` — step 4 walks
  `<install>/conda/envs/*`.
- **depends-on** `feature-tray-and-autostart.md` — tray is the entry point
  and step 2 calls `disable_autostart`.
- **shares-state-with** every feature via `~/.openbb_platform/` removal.

---

### Sources
- `desktop/src-tauri/src/uninstall.rs` (14-403 main; 296-400 macOS; 642-725 integrations; 730-835 .bat)
- `desktop/src/routes/uninstall.tsx:11-96`
- `desktop/src-tauri/src/main.rs:668-679` (tray entry)
- `raw-deep-dives/app-shell.md` §8; `app-shell.v2.md` §11, §12.
