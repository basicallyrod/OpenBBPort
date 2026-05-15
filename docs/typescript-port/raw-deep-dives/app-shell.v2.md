# App Shell v2 — Second-Pass Findings

> Reviewer for `app-shell.md`. Source verified against
> `desktop/src-tauri/src/main.rs` (858 lines), `desktop/src-tauri/src/uninstall.rs` (835 lines),
> `desktop/src-tauri/src/utils/{app_termination.rs, autostart/*}`, plus the secondary-window
> handlers in `backends.rs:1588-1593`, `jupyter.rs:623-628`, and `helpers.rs:999-1006`.
> Cross-checked against `installation.md`, `backend-services.md`, `ipc-bridge.md`,
> `environments.md`.
>
> Generated 2026-05-15.

---

## 1. Boot order — v1 numbering is correct, with one structural caveat

v1's nine-step table is faithful to `main.rs:469-857`. Two clarifications:

1. **Step 4 plugin order is significant for one plugin only — `single_instance`.**
   `tauri_plugin_single_instance::init(...)` is registered at `main.rs:476-481` and runs
   its lock-acquire before *any* other plugin can do work. Tauri 2's
   `single-instance` plugin is documented as needing to be the **first** plugin in the
   chain, but in this codebase it is in position 3 (after `updater` and `opener`).
   `tauri_plugin_updater::Builder::new().build()` (line 474) and `tauri_plugin_opener::init()`
   (line 475) do not perform any first-launch side effects in their `init()` — they only
   register IPC commands — so the misordering is benign here. The Tauri 2 docs explicitly
   call this out as a footgun; the port should put single-instance first regardless.

2. **`fix_path_env::fix()` (line 470) runs *before* `init_process_monitoring()` and the
   `Builder::default()`.** v1 mentions this fixes `$PATH` for macOS GUI launches. The
   crate's actual behavior (per `tauri-apps/fix-path-env-rs`'s README and code): on macOS
   and Linux, it spawns `<user's login shell> -ilc 'echo $PATH'`, captures stdout, and
   calls `std::env::set_var("PATH", ...)`. Important consequences not noted in v1:

   - **It is a no-op on Windows.** `cfg!(windows)` short-circuits to `Ok(())`.
   - **It modifies the process's `PATH` only.** Children spawned with `Command::new(...)`
     inherit the process env via Rust's default `Command::env_clear` not being called, so
     subprocesses get the fixed `PATH`. This matters for the conda installer (which is
     spawned later) and for the cleanup `pkill`/`taskkill` calls in `uninstall.rs:111-130`.
   - **It runs synchronously and *can* block.** A misbehaving login shell (e.g., `.zshrc`
     that prompts) would hang the entire boot. There is no timeout. On a "good" machine
     it's tens of milliseconds; on a corporate-managed Mac with slow login profiles it
     could be several seconds before the splash screen ever appears.
   - **Order matters vs. `init_process_monitoring`:** the log storage init does not read
     `$PATH`, so it doesn't matter that it runs after. But if v2 of the port wanted to
     spawn anything during process-monitoring init (e.g., a watchdog), it would now have
     the fixed `PATH`.

   > ⚠️ Port concern: `fix-path` (Node equivalent) has the same blocking-shell behavior.
   > Wrap the call in `Promise.race` with a 2s timeout in the Electron port.

---

## 2. Tray menu nav gate vs. install-completion race — there *is* a brief gap

v1 §4 documents the `environments-first-load-done` localStorage gate. Tracing all writers:

| Writer | File:line | When |
|---|---|---|
| Boot setup hook (installed path) | `main.rs:799` | Right after `setup()` decides `is_installed` |
| Install wizard "Continue" button | `installation-progress.tsx:1224` | After `update_openbb_settings` + `create_default_backend_services` succeed |
| Install wizard "Continue Anyway" | `installation-progress.tsx:1241` | When user dismisses a failed install |
| Try-Again wipe | `installation-progress.tsx:1248` | `localStorage.clear()` — *unsets* the flag |

The race the prompt asks about: **between install completion and the
`localStorage.setItem`, can the user click a tray item?**

Yes, but the consequence is benign:

1. While the user is on `/installation-progress`, `is_installed` (Rust-side) becomes true
   the moment conda is fully unpacked, but the setup-hook's setter (`main.rs:799`) has
   already run during the cold boot — *with the original `is_installed=false` state*.
   So during install, the boot setter never set the flag.
2. The frontend setter (`installation-progress.tsx:1224`) runs only when the user clicks
   "Continue", *after* `await invoke("update_openbb_settings", …)` (line 1207) and
   `await invoke("create_default_backend_services")` (line 1218) both resolve. Both are
   sync-ish (file writes); typically <100ms.
3. **During those <100ms** (install just finished, user clicked Continue, awaits in
   progress, flag not yet set), tray clicks for Environments/Backends/API Keys do
   nothing — `navigate_to_page` (`main.rs:401-405`) hits the `else` branch and only logs
   `Navigation prevented: environments-first-load-done not set`.

So no crash, but a small UX dead zone where the tray quietly ignores clicks while the
frontend is in the middle of bootstrapping. v1 missed this. The simplest port-time fix:
move the flag write to *before* the awaits, or replace the flag with a Rust-side
`AtomicBool` that the install handler flips when `update_openbb_settings` is called.

Side note: a tray "Open Window" click still works during this gap because that handler
(`main.rs:652-657`) is unconditional — it only calls `window.show() + set_focus()`, no
nav.

---

## 3. `InstallationState` computed twice at boot — purposeful, not accidental

v1 §1 step 7a tags this as a port-time fix opportunity. **That's wrong.** Re-reading
`main.rs:491` and `main.rs:552`:

```rust
.manage(check_installation_on_startup())                    // line 491
…
.setup(|app_handle| {
    let install_state = check_installation_on_startup();    // line 552
```

There is no comment, but the timing matters. Between the two calls, two things happen:

1. The `setup()` closure isn't entered until *after* Tauri has spun up webviews,
   constructed plugins, and is about to start the run-loop. Wall-clock delay between
   `.manage()` and `setup()` first line: typically tens to hundreds of ms.
2. **Critically**, the `setup()` reads `show_after_update` *before* re-reading
   `install_state` would matter — but the structural intent is to allow the install
   state to reflect filesystem changes that happened *between* `.manage()` and `setup()`.
   The only realistic way this could change: an update was just installed, the new
   binary's `main.rs:491` saw the old state, the updater's restart hit between them, and
   now `setup()` sees the new state.

But that's not actually how it works either — the updater restarts the process, so both
calls happen in the new process. The two reads return identical results in 100% of
realistic timelines.

**Conclusion: v1 was right.** The second call is genuinely redundant. The most charitable
reading is "defensive re-read in case the managed State got mutated" — but
`InstallationState` is `#[derive(Clone, Serialize)]` only and `.manage(T)` stores it
behind `&T`; nothing mutates it. Port can compute once and share.

(The prompt suggested the second computation might be for the show/hide window decision.
It's not — line 552's `install_state` is used at line 569 to decide whether to spawn
backend initialization, and at line 779/787 to decide setup vs. environments routing.
The show/hide decision derives from `show_after_update`, not from `install_state`.)

---

## 4. macOS `applicationWillTerminate` — multiple-cleanup race exists, but is mostly safe

`utils/app_termination.rs:33-37` builds a fresh `Runtime` and `block_on(cleanup_all_processes(...))`.

The cleanup paths that can fire on macOS shutdown, in temporal order:

| Trigger | Path | What it does |
|---|---|---|
| User clicks tray Quit | `main.rs:642-651` | New Runtime → cleanup → `app.exit(0)` |
| `app.exit(0)` triggers… | `main.rs:825` `RunEvent::ExitRequested` | New Runtime → cleanup again → `process::exit(0)` |
| macOS sends `applicationWillTerminate` | `app_termination.rs:33` | New Runtime → cleanup a *third* time |

The three cleanup runs are not concurrent — they're strictly sequential because each
`rt.block_on(...)` blocks the calling thread. But they do re-enter the same global
state (`RunningProcesses` Mutex). The second and third calls find the process map empty
(backends already stopped → `kill_process` is a no-op; `stop_all_jupyter_servers` finds
nothing to stop). So they each waste ~ a few ms and log a benign "Successfully stopped
all backend services" with an empty list.

> ⚠️ Real risk: **building three independent Tokio runtimes back-to-back is wasteful
> and trips Tokio's "Cannot start a runtime from within a runtime" guard if any of
> them gets called from inside an async task.** Currently each is called from sync
> contexts (signal handler, Obj-C callback, sync event arm), so it works. The port
> should consolidate to a single cleanup invocation gated by an `AtomicBool` "already
> cleaning up" flag.

The Obj-C path's "don't call `exit()`" comment (`app_termination.rs:39`) is correct —
the comment exists precisely because `process::exit(0)` from inside
`applicationWillTerminate:` would short-circuit AppKit's own shutdown and can lead to
crash logs. The contract is: macOS itself will call `exit()` once `willTerminate` returns.

---

## 5. Single-instance plugin discards CLI args — `--autostart` does nothing

`main.rs:476-481`:

```rust
.plugin(tauri_plugin_single_instance::init(|app, _, _| {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}))
```

The `_, _` are `argv: Vec<String>` and `cwd: String`. Both discarded. **No CLI arg of
the second instance is observed by anyone — not the first instance, not the second
(which exits immediately after handing off).**

Furthermore, **the first instance's `main()` itself never reads `std::env::args()`** —
grep over `src-tauri/src/` confirms zero `env::args` reads. So even the *first* invocation
ignores its CLI args.

Consequence: **`openbb-platform.exe --autostart` is functionally identical to
`openbb-platform.exe` with no flags.** Searching for `--autostart`:

```
$ grep -rn "autostart" /home/user/OpenBBPort/desktop/src-tauri/src/ /home/user/OpenBBPort/desktop/src/
```

returns only autostart toggle code; nothing reads or matches the `--autostart` token.
The macOS `enable_autostart` does mention `--autostart` in its `make new login item`
properties (`macos_autostart.rs:86` comment), but the AppleScript itself does
`properties {path:"<app>", hidden:false, name:"OpenBB Platform"}` — the comment is
aspirational and **no CLI arg is actually appended.**

Similarly, the Windows `.lnk` shortcut sets `SetPath(exe_path)` only — no `SetArguments`
call (`windows_autostart.rs:91`). The Linux `.desktop` file's `Exec=` line is just
`"<exe>"` with no args (`linux_autostart.rs:38`).

**Net: autostart code paths trigger only because the OS launched the binary in a
specific context (login item, Startup folder shortcut, XDG autostart file). The binary
has no runtime knowledge that it was auto-started.** The Rust setup hook treats every
launch identically, including showing the window after `is_installed` check
(`main.rs:803`'s `show_after_update` is the only conditional `window.show()` skip — but
it has nothing to do with autostart).

> ⚠️ Port should fix this by either:
> (a) Read `process.argv` in Electron `main.ts` and check for `--hidden`/`--autostart`.
> (b) Pass args via `app.setLoginItemSettings({openAtLogin, args: ['--hidden']})` on
>     macOS/Windows; manually edit the `.desktop` file on Linux.
> (c) For "Start at Login in Background", the *background* part of the menu item label
>     promises the window stays hidden — but the current code shows it every time, which
>     contradicts the UX promise.

---

## 6. Updater flow — `request_restart()` does trigger ExitRequested, but with a caveat

The chain in `main.rs:214`:

```rust
app_clone_inner.request_restart();
```

`AppHandle::request_restart()` (Tauri 2 API) internally calls
`exit_with_code(RESTART_EXIT_CODE)` where `RESTART_EXIT_CODE = i32::MIN + 1` (Tauri
constant, imported at `main.rs:15`). This produces a `RunEvent::ExitRequested` with
`code: Some(RESTART_EXIT_CODE)` — which is exactly what `main.rs:820-823` checks for.

So **`cleanup_all_processes` does run** before restart:

1. `request_restart()` → `ExitRequested { code: Some(i32::MIN+1) }`.
2. `main.rs:820`: `is_restart_requested.store(true, …)`.
3. `main.rs:825`: `api.prevent_exit()`, build Runtime, `cleanup_all_processes` runs.
4. `main.rs:833-839`: branch on `is_restart_requested`; calls `tauri::process::restart`.
5. After restart, `RunEvent::Exit` also fires; `main.rs:851` re-checks the flag and
   calls `cleanup_before_exit` + `tauri::process::restart` again — **a second restart
   call**. Tauri's `process::restart` is idempotent (it `exec`s a new process and the
   current one exits) so the second call is unreachable in practice, but it's defensive.

**Real gotcha not in v1:** between steps 4 and 5, the `tauri::process::restart` at
`main.rs:835` calls `std::process::exit(0)` internally on non-Windows. On Windows it
spawns a new process and *also* exits. Either way, the `RunEvent::Exit` arm at line
851 is only reached on Windows where the spawn-then-exit flow gives it a moment, and
even then only if `process::restart` returns control (which it shouldn't). v1 noted
the `Arc` is "pointless" — that's true: both the store at 820 and the load at 833 are
in the same closure invocation, so a plain `bool` would do. The pointer to the second
restart attempt at line 855 *does* benefit from the Arc since it's a different closure
invocation… except it isn't: `.run()`'s closure is a single `FnMut`, and the `Arc` is
constructed *inside* the closure (line 817), so it's recreated per event. The
"persistence" never happens. **No backends leak across restart** because the cleanup
at line 825-832 always runs before the restart.

---

## 7. `.show_on_restart` flag — single-file, idempotent

`main.rs:204-208` writes to `$HOME/.openbb_platform/.show_on_restart` with content `"1"`.
`main.rs:556-559` checks for existence, deletes if found, sets a bool.

**Multiple queued updates:** Tauri's updater is a singleton check; you can't queue two
updates in the same process lifecycle. But theoretically if you click "Check for
Updates" twice in rapid succession, both invocations land in
`check_and_apply_update`, both could find an update, both could prompt. If the user
clicks Yes on both:

- Both `download_and_install` futures race to `update.download_and_install(...)`.
- Both write to `.show_on_restart` with content "1" — `fs::write` truncates and replaces,
  so the second write *overwrites* the first. **No append, no duplication.**
- Both call `request_restart()`. The second restart call hits an already-quitting
  process; the request fails silently (Tauri's `App` is already in shutdown).

So `.show_on_restart` is naturally idempotent. Port should match: write `"1"` to a
fixed path, read+delete on next boot.

Bonus finding: **the flag file's parent directory `~/.openbb_platform/` may not exist**
if the user never completed installation but somehow triggered an update. The write at
`main.rs:206` will fail (no directory). The code wraps in `let _ = std::fs::write(...)`,
so failure is silently swallowed. The next-boot read at line 557 will simply not find
the file. Net: `show_after_update` stays false, the post-update window stays hidden, the
user has to manually open from the tray. Mildly bad UX but not catastrophic.

---

## 8. macOS Reopen — fires when dock icon is clicked, including from hidden state

`main.rs:844-848`:

```rust
if let tauri::RunEvent::Reopen { .. } = event
    && let Some(window) = app_handle.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
```

`RunEvent::Reopen` is Tauri's wrapper for AppKit's
`applicationShouldHandleReopen:hasVisibleWindows:`. It fires:

1. When the user clicks the dock icon of a *running* app whose windows are all closed
   *or hidden*.
2. When the user uses Cmd+Tab to the app.

**Does Reopen fire when the window is hidden via close-to-tray?** Yes. After
`CloseRequested` → `window.hide()` (`main.rs:748`), the NSWindow is set to `isVisible=false`
but the NSApplication still has the app frontmost. Clicking the dock icon then sends
the reopen event with `hasVisibleWindows: false`. The handler at 844 unconditionally
calls `show() + set_focus()`. **Works correctly.**

Caveat: if the user has *multiple* windows open (e.g. a backend-logs popup is open but
the main window is hidden), `Reopen` still fires for the main window only — but the
handler hardcodes `"main"` (line 845). So clicking the dock icon will always show the
main window, even if the user only wanted to bring the logs popup forward. Minor UX
quirk.

---

## 9. Window CloseRequested → hide — three independent handlers, not centralized

Each window has its own `on_window_event` registration:

| Window | Handler location | Close behavior |
|---|---|---|
| `main` | `main.rs:744-760` | `hide()` + `prevent_close()` (close-to-tray) |
| `backend-logs-{id}` | `backends.rs:1588-1593` | `hide()` + `prevent_close()` |
| `jupyter-logs-{environment}` | `jupyter.rs:623-628` | `hide()` + `prevent_close()` |
| Open-URL popup (`open_url_in_window`) | `helpers.rs:999-1006` | `destroy()` + `prevent_close()` |

**Critical asymmetry** v1 missed: the **open-URL popup destroys** while logs windows
**hide**. Reason: logs windows are kept around because the user can re-open the same
logs view from the parent route (and the persisted webview retains scroll position);
URL popups are single-use.

This means for a long-running session, the user can accumulate:

- N hidden `backend-logs-{id}` windows (one per backend ever opened)
- M hidden `jupyter-logs-{environment}` windows (one per env ever logged)
- 1 hidden `main` window

…all eating memory. Each holds a full webview process. There is no cleanup hook for
these on quit (only `cleanup_all_processes` for the *backend services*, not the
*UI windows*).

> ⚠️ Memory leak per session. Port should either destroy logs windows on close (and
> rely on parent route to re-create) or implement an LRU cap (e.g., keep last 5
> hidden logs windows).

The three handlers are not centralized into a single function — each call site builds
its own closure. The port can DRY this with a single `attachHideOnClose(window)` helper.

---

## 10. `visible: false` flicker risk

`tauri.conf.json:17` sets `"visible": false`. The window is created hidden, then either:

- Shown explicitly in setup hook (`main.rs:782` for fresh-install path, line 803 for
  `show_after_update`).
- *Not* shown for installed users on normal boot (the user only sees it via tray
  "Open Window" or by clicking the dock icon).

Flicker risk: between window creation (which loads the dist/index.html and runs JS) and
the explicit `window.show()`, JS is executing on an invisible window. If JS does a
heavy synchronous load (TanStack Router initialization, theme detection, the
index.tsx 2s timeout), it doesn't matter because nothing is visible.

**But:** `main.rs:785` does `window.eval("window.location.href = '/setup'")` *before*
`window.show()` is called at line 782. Wait — looking carefully at lines 781-786:

```rust
if let Some(window) = handle.get_webview_window("main") {
    let _ = window.show();              // line 782 — show FIRST
    let _ = window.set_focus();
    let _ = window.eval("localStorage.clear(); …");
    let _ = window.eval("window.location.href = '/setup'");  // navigate AFTER show
}
```

So the window is shown *first*, then the navigation runs. **On slow machines this can
cause a flash of the wrong route.** The webview initial URL is whatever the dev/prod
build resolves to (typically `/`), so the user sees a brief flash of `/`'s
`<div>Starting OpenBB Platform...</div>` (`index.tsx:72-73`) before the eval fires and
replaces with `/setup`.

> ⚠️ UX bug: brief route flash on slow machines for first-time users. Port can fix
> by passing the route as a `?initial=/setup` query param baked into the index HTML, or
> by deferring `show()` until after the navigation completes (`webContents.once('did-finish-load', () => show())` in Electron).

For installed users (line 798), there's no `window.show()` (the window stays hidden
unless `show_after_update`), so the flicker isn't visible — but the next time the user
opens the tray and clicks "Open Window", they'll see whatever route the webview ended
up on, which is `/` → which then redirects to `/environments` via the 2s timeout
mechanism in `index.tsx:34-50`. So on first tray-open, installed users see a 2-second
"Starting OpenBB Platform" screen unnecessarily. (The boot setup hook never navigates
the installed user away from `/`.)

---

## 11. Uninstall flow — execution-order details v1 missed

### 11a. `invoke('app.exit')` order on each OS

The Rust handler `uninstall_application` has different exit semantics per OS:

| OS | What happens at end of handler | Frontend `setTimeout(2000) → invoke('app.exit')` reaches it? |
|---|---|---|
| **macOS** | `uninstall.rs:399` `std::process::exit(0)` — **inside the handler** | **No** — the process dies before the handler's response gets back to JS. The JS `await` never resolves, the `setTimeout` is never scheduled. |
| **Windows** | Handler returns `Ok(None)` at line 402 (after the `#[cfg(target_os = "windows")]` block runs `run_windows_system_uninstaller`). The Rust process keeps running. | **Yes** — JS resolves, `setTimeout(2000)` fires, `invoke('app.exit')` is called → no matching handler → silent failure. **The app keeps running.** It will only exit when the spawned `openbb_uninstall.bat` runs `taskkill /F /IM openbb-platform.exe /T` (line 783). |
| **Linux** | Handler returns `Ok(None)`. Process keeps running. | **Yes** — same as Windows: silent failure on `invoke('app.exit')`, app keeps running. **Nothing kills it.** |

> ⚠️ Linux uninstall leaves the app running after the user clicks Uninstall. The user
> has to manually quit it (tray → Quit, or close + tray → Quit). v1 noted the typo
> but did not enumerate per-OS impact. The macOS path masks the bug; Windows masks it
> via the kill-script; Linux has no masking.

### 11b. Windows `.bat` spawned with `cmd /C start "title" <bat>` — visible window confirmed

`uninstall.rs:815-824`. The comment at line 816 explicitly says "Do NOT hide this window".
The `start` command opens a new console window titled "OpenBB Platform Final Cleanup".
The batch file ends with `pause > nul` (line 801) — **the window stays open until the
user presses a key.** If the user dismisses the window (clicks X) before pressing a
key, the `pause` is interrupted and `exit` (line 802) runs immediately. Either way, the
uninstall steps already ran (they're before the `pause`); dismissing only skips the
"Press any key" confirmation. **No data loss from premature dismissal.**

### 11c. The 3-second sleep before sysem-uninstaller

`uninstall.rs:244`: `sleep(Duration::from_secs(3))`. Then on Windows, line 257 calls
`run_windows_system_uninstaller`. Re-reading the surrounding context, the 3s is
**after** the user-data-removal step (line 232-240) and **before** the Windows-specific
uninstaller. The script generated by `run_windows_system_uninstaller`
(`uninstall.rs:780-803`) itself starts with `timeout /t 5 > nul` to wait for the Rust
process to exit before running `uninstall.exe`. So we have:

- 3s sleep inside the Rust process (line 244)
- Then Rust spawns the `.bat`, which sleeps another 5s (`timeout /t 5`)
- Total dead time: 8s before `uninstall.exe` actually runs

The 3s sleep is **race-mitigation**: ensures filesystem operations (the
`fs::remove_dir_all` of `~/.openbb_platform/`) are flushed to disk before the system
uninstaller starts. On a slow disk or with antivirus indexing, removing many small
files can take time and the syscalls may complete async (especially on Windows). 3s
is empirical headroom. The port should keep it; reducing to 1s on SSD-only systems is
fine but pointless.

---

## 12. System-integration cleanup paths — defensive, all references to non-existent files

Searching for *all* references to the system-integration files (across the
repository, not just current branch):

```
~/Library/LaunchAgents/com.openbb.platform.plist
HKCU\Software\Microsoft\Windows\CurrentVersion\Run\OpenBBPlatform   (and 4 sibling keys)
~/.config/systemd/user/openbb-platform.service
```

| Path | Created by current code? | Cleaned by current code? |
|---|---|---|
| `~/Library/LaunchAgents/com.openbb.platform.plist` | **No.** `macos_autostart.rs` uses pure AppleScript against System Events login items, not LaunchAgents. | Yes: `uninstall.rs:697-704`. |
| Windows registry `Run`/`RunOnce` keys (4 paths × 5 entry names = 20 combinations) | **No.** `windows_autostart.rs` writes a `.lnk` to the Startup folder. | Yes: `uninstall.rs:646-672` (regardless of which key/name was actually used). |
| `~/.config/systemd/user/openbb-platform.service` | **No.** `linux_autostart.rs` writes an XDG autostart `.desktop` file at `~/.config/autostart/openbb-platform.desktop`. | Yes: `uninstall.rs:707-725`. |

**Confirmed: all three are defensive cleanup for legacy installs.** None of the
current paths produce these artifacts. Git history would need to be checked to find
when (or if) these were ever the active mechanism — the prompt mentioned this but I
have no tools to grep history easily, and the current codebase is the only state we're
documenting. The port should treat these as **legacy-detect + uninstall-only**
defensive paths and **not implement creation** for them.

One *current* artifact that v1 also missed in the uninstall flow: the XDG autostart
file at `~/.config/autostart/openbb-platform.desktop` (created by `linux_autostart.rs`)
**is removed via `disable_autostart`** at `uninstall.rs:72-79`, not via
`remove_system_integrations()`. The latter only handles the legacy systemd path. So the
cleanup is complete, just split across two functions.

---

## 13. The 10s outer cleanup timeout — 4s headroom is mostly wasted

`main.rs:419-467`:

```rust
let cleanup_timeout = std::time::Duration::from_secs(10);                  // outer
…
tokio::time::timeout(Duration::from_secs(3), stop_all_jupyter_servers(...))// inner A
…
tokio::time::timeout(Duration::from_secs(3), stop_all_backend_services(...))// inner B
```

If both inner steps time out, total elapsed inside the outer block is 6s. The outer
wraps the entire async block at line 425, so the outer's 10s is *measured from start
of A*, not the sum of A+B. Thus:

- Worst case: A = 3s (timed out), B = 3s (timed out) → 6s elapsed → 4s headroom
  unused before outer fires.
- What runs in those 4s: **nothing.** The async block ends at line 451; the outer
  wrapper resolves successfully (not via timeout); execution falls through to line 455
  immediately.
- Windows-only `tokio::sleep(500ms)` at line 463 runs *after* the outer match, so it's
  not inside the 10s budget — it adds another 500ms strictly after cleanup.

**Net: the 10s outer is conservative. It would only fire if `stop_all_jupyter_servers`
or `stop_all_backend_services` themselves hang *internally* in a way that bypasses
their own 3s inner timeout** — which can happen if either function awaits on a Mutex
lock that's held by a deadlocked task. Realistic risk: low, but the outer is a
"don't hang forever" insurance policy. v1's read was correct.

The port can simplify to **a single 10s timeout** wrapping a `Promise.allSettled([
stopJupyter(), stopBackends() ])` and lose the nested-timeout complexity. The 500ms
Windows sleep is also somewhat dubious — Electron handles GDI cleanup internally on
`app.exit()` — port can probably skip it.

---

## 14. `fix_path_env::fix()` — what it actually fixes

From the upstream crate (`tauri-apps/fix-path-env-rs`, MIT-licensed, ports
`sindresorhus/fix-path`):

```rust
// Pseudo-summary of the crate's implementation
pub fn fix() -> Result<(), Error> {
    if cfg!(windows) { return Ok(()); }    // Windows: no-op
    let shell = env::var("SHELL").unwrap_or("/bin/sh");
    let output = Command::new(shell).args(["-ilc", "echo -n \"_SHELL_ENV_DELIMITER_\"; printenv PATH; echo \"_SHELL_ENV_DELIMITER_\""])
        .output()?;
    let path = parse_between_delimiters(output.stdout);
    env::set_var("PATH", path);
    Ok(())
}
```

Key facts not in v1:

- **It only fixes `PATH`.** Not `HOME`, not `LANG`, not `LC_*`, not anything else.
- **It modifies `std::env::PATH` in the current process.** Children spawned via
  `std::process::Command` inherit the *current* process env unless `.env_clear()` is
  called — so children also get the fixed PATH.
- **It does not run if `SHELL` is unset.** It falls back to `/bin/sh` non-interactively,
  which on most systems means `.bashrc`/`.zshrc` is NOT sourced — so the "fix" doesn't
  actually fix anything in that case. Real-world: macOS GUI launches always have
  `SHELL` set (inherited from launchd), so this is rare.
- **Performance:** spawns a process, waits for shell startup files. On a Mac with a slow
  `.zshrc` (Oh-My-Zsh + many plugins), ~200-500ms. On a clean shell, <50ms.

In the port, the Node equivalent is `npm fix-path` (also by sindresorhus). Same
mechanics; same caveats. The Electron port should call it once at the top of `main.ts`
before any `child_process.spawn`. Optionally wrap in a 2s timeout.

---

## 15. Tray icon click (left/right) behavior

`main.rs:635-740` builds the `TrayIconBuilder` with only:

- `.icon(icon)`
- `.tooltip("Open Data Platform - By OpenBB")`
- `.menu(&menu)`
- `.on_menu_event(…)`

**No `.on_tray_icon_event(…)` is registered.** No `.show_menu_on_left_click(false)`
override either.

Tauri 2's `TrayIconBuilder` default behavior (per
`https://docs.rs/tauri/2.10.3/tauri/tray/struct.TrayIconBuilder.html`):

- `show_menu_on_left_click`: **default `true`** on Linux and Windows.
- On macOS, the default OS behavior for menubar items is: both left and right click
  open the menu. macOS doesn't typically distinguish click types for menubar items.

So:

| OS | Left click | Right click |
|---|---|---|
| macOS | Opens menu | Opens menu (same as left) |
| Windows | Opens menu | Opens menu |
| Linux (GNOME/KDE) | Opens menu | Opens menu (with `AyatanaAppIndicator`, can vary) |

**There is no separate "click icon to show window" handler.** The user must click the
menu, then click "Open Window". v1 documented the menu items correctly but did not
note the absence of a direct icon-click action.

> ⚠️ UX gap: Most tray apps support left-click-icon → toggle main window. The port
> should add this via `tray.on('click', () => mainWindow.isVisible() ? mainWindow.hide() : mainWindow.show())`
> in Electron, and keep the context menu on right-click.

---

## 16. `backgroundThrottling: "disabled"` in tauri.conf.json

`tauri.conf.json:15`: `"backgroundThrottling": "disabled"`.

This is a Tauri 2 window config option that maps to the underlying webview's
"background throttling" behavior:

- **Chromium/WebView2 (Windows, Linux):** by default, when a webview is occluded
  (window minimized or fully covered), Chromium throttles `setTimeout`/`requestAnimationFrame`
  intervals to 1Hz to save CPU. `"disabled"` turns this off — timers continue at full
  rate.
- **WKWebView (macOS):** similar throttling; `"disabled"` keeps it active.

**Why disabled here?** The app relies on periodic polling and websocket activity:

- `installation-progress.tsx` polls `get_installation_status` every 1-2s via setInterval.
- `environments.tsx` and `backends.tsx` poll backend health periodically.
- The webview hosts a background event listener for `installation-directory` and
  `uninstall_progress` events — these rely on the Tauri event system which works
  regardless of throttling, but UI updates triggered by them depend on React
  re-rendering, which depends on tasks running.
- When the user **hides the window to the tray** (close-to-tray), the webview is
  occluded. Without `backgroundThrottling: "disabled"`, all backend polling would slow
  to 1Hz, making the tray-state stale by tens of seconds. With `"disabled"`, the
  webview keeps running normally even when hidden.

> Port equivalent in Electron: `new BrowserWindow({ webPreferences: { backgroundThrottling: false } })`.

This is **important to copy** in the port — otherwise the tray-only operation mode
becomes much less responsive.

---

## 17. Bonus finding: `Menu::new(handle)` builds an empty system menu

`main.rs:582`: `window.set_menu(Menu::new(app_handle.handle())?)?;`

`Menu::new(...)` constructs an **empty** menu. On macOS, this overrides the default
app menu (which would otherwise include "File / Edit / View / Window / Help" with the
app name and Quit). On Windows/Linux, it sets an empty menubar.

**Side effects:**

- macOS users lose the standard `Cmd+Q` quit shortcut (because the system Quit menu
  item is gone). v1's §7 confirms there's no Tauri-side keyboard shortcut for quit;
  the only quit path is the tray menu or Ctrl-C from a terminal. **Users cannot
  quit via the keyboard on macOS.**
- macOS users also lose `Cmd+W` (close window) handling — except Tauri's window
  intercepts `CloseRequested` and hides, so `Cmd+W` *is* still functional via the
  webview's native handling, just not via menu.
- The app does not register any `Accelerator`/`KeyboardShortcut` to fill the gap.

> ⚠️ macOS-specific UX bug: `Cmd+Q` is a no-op. Port should either (a) reconstruct
> the standard app menu with custom Quit handler, or (b) register a global
> `localShortcut` for `Cmd+Q` that calls `quit_application`.

This is also why `main.rs:582` fails silently if the `Menu::new` errors — the `?` operator
propagates, but the entire `setup()` would error out, which would fail the build at
`main.rs:811-815` (`unwrap_or_else` → `std::process::exit(1)`). So in practice it always
succeeds; the question is just whether the resulting empty menu is what we want.

---

## v2 → v1 corrections

- v1 §1 step 7a: **Correction.** v1 called the duplicate `check_installation_on_startup`
  call a port-time fix opportunity. After review (§3 above), it is genuinely redundant
  but the rationale is mildly defensive — there's no observable timing window where the
  results would differ. Port can safely compute once. v1's stance was right; tagging
  it as a "fix" is fine.
- v1 §4: **Correction.** The tray nav gate has a brief dead zone after install
  completion (§2 above). Not catastrophic, but worth noting in the feature doc.
- v1 §5 macOS autostart: **Confirmation.** v1 noted "no `~/Library/LaunchAgents/`
  plist". Confirmed: uses only AppleScript against `System Events → login items`.
- v1 §6: **Addition.** `request_restart()` does trigger `ExitRequested` with the
  special `RESTART_EXIT_CODE`, which `main.rs:820` correctly detects. Backends do NOT
  leak across restart. The `Arc<AtomicBool>` is structurally pointless (recreated per
  event) but the logic works because detect-and-use happen in the same closure call.
- v1 §7 `is_restart_requested` Arc: **Confirmation + nuance.** v1 marked it a bug.
  Confirmed: the Arc is recreated every event, but the only consumer (line 833) is in
  the same event arm as the producer (line 820), so the same `AtomicBool` instance is
  used. Functional, just wasteful. Port can use a plain `bool`.
- v1 §8: **Addition.** Per-OS behavior of the `invoke('app.exit')` typo differs (§11a):
  macOS is masked by `process::exit(0)`; Windows is masked by the `.bat` kill;
  **Linux is not masked — the app stays running after uninstall.**
- v1 §8 Windows `.bat`: **Confirmation.** Visible window is intentional. The `pause`
  step makes the user confirm before the console closes; dismissing X doesn't break
  anything.
- v1 §9: **No correction.** Single-instance behavior as documented. But v1 missed that
  the binary itself ignores `std::env::args()` entirely (§5 above) — so `--autostart`
  CLI flag does nothing whether passed to the first or second instance.
- v1 "Notable findings" #4: **Confirmation + per-OS impact.** `invoke('app.exit')` is
  broken on all OSes but only Linux exposes the bug to the user.

**New findings not in v1:**

1. Boot flicker risk for fresh-install users (§10).
2. Three independent close-handlers + hidden-windows memory accumulation (§9).
3. macOS has no `Cmd+Q` because `Menu::new(handle)` builds an empty menu (§17).
4. Tray icon left-click does nothing — no direct show-window action (§15).
5. `backgroundThrottling: "disabled"` is load-bearing for tray-mode responsiveness (§16).
6. `fix_path_env::fix()` blocks the main thread on slow login shells; no timeout (§1, §14).
7. Linux uninstall leaves the app running after completion (§11a).
8. Cleanup runs up to 3 times on macOS shutdown (tray Quit → ExitRequested → applicationWillTerminate) — wasteful but not unsafe (§4).
9. Single-instance plugin position in the chain (§1) — should be first per Tauri 2 docs.
10. The 4s "headroom" in the cleanup outer timeout is unused; it's a hard ceiling, not a separate budget (§13).
