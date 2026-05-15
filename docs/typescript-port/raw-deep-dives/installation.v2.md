# Deep-Dive v2: Setup Wizard + Installation Pipeline — Gap-Fill & Corrections

> Second-pass review. Only contains NEW findings, gap-fills, and corrections.
> Does NOT restate v1 (`installation.md`). Read v1 first.
> Cross-references: `app-shell.md`, `environments.md`, `ipc-bridge.md`,
> `backend-services.md`, `platform-rest-api.md`, `api-keys.md`.

---

## 1. Reconciliation of conflicting / over-confident v1 claims

### 1.1 `installation-status` event — three docs, three slightly different stories
- v1 `installation.md:40` says "never emitted by the Rust side (only `installation-directory` at `startup.rs:1307`)".
- `ipc-bridge.md:139` says "**NEVER EMITTED**. The `setTimeout` fallback at index.tsx:34 always fires `invoke('get_installation_state')` instead."
- `app-shell.md:49,327` also says **never emitted** but on `:49` adds "verified via grep over `src-tauri/src/`".

These agree. **Confirmed** via fresh grep — no `emit("installation-status"` anywhere in `/home/user/OpenBBPort/desktop/src-tauri/src/`. The listener at `index.tsx:15-24` is dead code. Drop in port.

### 1.2 `get_installation_state` payload — snake_case vs camelCase
**v1 inconsistency:** v1 `installation.md:38` shows JS expects `{is_installed: boolean}` (snake_case), but v1's own §9 translation table line `get_installation_state` returns `{is_installed: boolean, installation_directory: string|null}` (snake_case) — fine. **However**, `get_installation_status` (different command!) returns **camelCase** `{phase, isDownloading, isInstalling, isConfiguring, isComplete, message}` (`startup.rs:38-49`).

This is **not** Tauri's automatic conversion. The Rust struct `InstallationState` in `main.rs:66-70` derives `Serialize` with default snake_case field names. The other command builds a `serde_json::Value` with **manually-written camelCase keys** (`startup.rs:44-48`). A TS port must NOT assume "Tauri normalizes everything" — these two commands hand back different casing conventions and the v1 table mostly hides it.

`/home/user/OpenBBPort/desktop/src-tauri/src/main.rs:66-70`:
```rust
#[derive(Clone, serde::Serialize)]
struct InstallationState {           // <-- no rename_all
    is_installed: bool,
    installation_directory: Option<String>,
}
```
vs `/home/user/OpenBBPort/desktop/src-tauri/src/tauri_handlers/startup.rs:38-49`:
```rust
let response = serde_json::json!({
    "phase": ...,
    "isDownloading": state.is_downloading,
    ...
});
```

### 1.3 Sanity-check the v1 claim that `INSTALLATION_IN_PROGRESS` is never released on abort
Verified at `startup.rs:968-1095`: `abort_installation_impl` reaches into `INSTALLATION_STATE` (`:977-983`) but **never** touches `INSTALLATION_IN_PROGRESS` (the separate mutex declared at `:434`). The only releases are at the end of a successful `install_conda` (`:902 release_guard()`) and on each early-error path inside `install_conda` (`:532, 563, 574-587, 598, 617, 666, 671, 681, 686, 706, 714, 722, 732, 741, 760, 766, 832, 840, 892`). **No path in abort releases it.** v1's bug claim stands. The port should either:
- release in abort_installation_impl, OR
- collapse the two mutexes into one struct so the abort path resets atomically.

### 1.4 v1 says `setup_python_environment` "ends by emitting `installation-directory` AND calling `update_openbb_settings_impl`" — sequence verified, BUT...
Source order at `startup.rs:1293-1311`:
1. `update_openbb_settings_impl` called (line 1293) — **errors logged-only, NOT propagated**.
2. `report_progress("complete", 1.0, "Installation complete")` (line 1304).
3. `window.emit("installation-directory", &directory)` (line 1307).
4. Return Ok(true).

**Race the v1 misses:** the `report_progress` in step 2 calls `update_installation_state("complete", 1.0, "Installation complete")` which sets `is_complete=true` in `INSTALLATION_STATE` (`startup.rs:93` matches "complete"). If the React poll fires between steps 2 and 3 (within the 2-second `setInterval`), it reads `isComplete: true` with message `"Installation complete"` (not `"Installation completed successfully"`) — `checkInstallationStatus` at `installation-progress.tsx:1068-1090` checks for "Installation completed successfully" OR "openbb installation complete" substrings. Neither matches, so it logs "Sub-component completion" and **continues showing the spinner**. So this race is benign by virtue of message-string mismatch.

But the `installation-directory` emit (`startup.rs:1307`) lands AFTER the React `await invoke("setup_python_environment")` returns. The frontend never listens for it during the wizard — the listener is only on `index.tsx:27` (a page the user is NOT currently on). So **the emit is dead-during-wizard** and only serves a future page reload. v1 partially noted this; the port can drop the event entirely if the redirect is otherwise handled.

### 1.5 v1 says "the substring `'Miniforge installation completed'` triggers the version-select transition" — actually misleading
v1 §3.1 paraphrases the listener logic. The actual check at `installation-progress.tsx:836-839`:
```ts
step.includes("install") &&
(message.includes("Miniforge installation completed") ||
 (message.includes("completed") && phase === "installing"))
```
The Rust never emits the literal string `"Miniforge installation completed"` — the closest is `"Conda installation completed successfully"` at `startup.rs:846` (step `"install"`, progress 0.9) and again at `:899` (step `"complete"`, progress 1.0). The first one matches the listener via the SECOND clause (`message.includes("completed") && phase === "installing"`) because by that point phase has been set to `"installing"` (via the earlier step=="install"). The second `"complete"` step also matches via the same second clause. So the listener fires **twice**; the second one is a no-op because phase is already `"version_select"`.

v1 §3.2 step 19 also says of the `report_progress("complete", 1.0, "Conda installation completed successfully")` final emit: "the `step==="complete"` else branch" — wrong. The listener's structure (`installation-progress.tsx:835-910`) has the install/complete cases as separate `if` blocks (not an `else` chain). The `step.includes("install")` block returns early after `setPhase("version_select")`. The `step.includes("complete")` block runs separately and falls into "sub-component completion".

### 1.6 v1 §6 says completion `handleContinue` calls `update_openbb_settings` and `create_default_backend_services` "again as a re-sync" — clarification needed
The `update_openbb_settings` at `installation-progress.tsx:1207` is a **third** call (after `setup_python_environment` internally calls it, and `handleInstallExtensions` calls it explicitly at `:1162`). So in a wizard that goes name → version → extensions → Done, `update_openbb_settings_impl` runs three times. Each one spawns Python and re-merges JSON. This is wasteful but idempotent — each run preserves and re-writes the same fields. Port should reduce to one.

---

## 2. Schema-coupling: what install writes vs. what consumers expect

### 2.1 `system_settings.json` — Python silently drops `install_settings` and root-level `installation_directory`
Critical, undocumented in v1. The Python side (`/home/user/OpenBBPort/openbb_platform/core/openbb_core/app/service/system_service.py:16-27`) filters reads through `SYSTEM_SETTINGS_ALLOWED_FIELD_SET`:
```python
SYSTEM_SETTINGS_ALLOWED_FIELD_SET = {
    "test_mode", "headless", "logging_sub_app",
    "api_settings", "python_settings", "debug_mode",
    "logging_suppress", "allow_mutable_extensions", "allow_on_command_output",
}
```
`install_settings` (the key the desktop wizard writes at `startup.rs:391`) is **NOT** in this set. Same for the root-level `installation_directory` fallback (read in `main.rs:352-355`, `environments.rs:1663-1667`, `uninstall.rs:413-419`).

Consequences:
- `SystemService._read_from_file` at `system_service.py:53-66` reads then **deletes** unknown keys before validating against the Pydantic model. So `SystemService` never sees `install_settings`.
- `SystemService.write_to_file` (`system_service.py:78-96`) uses `model_dump_json(include=SYSTEM_SETTINGS_ALLOWED_FIELD_SET, exclude_defaults=True)` — if it ever runs against the desktop's `system_settings.json`, it **wipes the install_settings block**, leaving the desktop unable to find its own install on next boot.

In current code this is safe because the only Python writer that touches `system_settings.json` is the script in `update_openbb_settings_impl` (`helpers.rs:760-806`), which does its OWN raw `json.load`/`json.dump` and re-inserts merged keys — preserving `install_settings`. But the port must replicate this rather than use `SystemService.write_to_file`.

> ⚠️ FRAGILE: any future code path that uses `SystemService.write_to_file()` to "fix" `system_settings.json` will destroy the install marker on disk and put the desktop into a "not installed" state on next boot. Document this contract.

### 2.2 `user_settings.json` — install writes a strict three-key skeleton
Install writes:
```json
{"credentials":{}, "preferences":{"data_directory":"<path>"}, "defaults":{}}
```
(see `startup.rs:273-282`).

Python's `UserService.write_to_file` uses `include=USER_SETTINGS_ALLOWED_FIELD_SET = {"credentials", "preferences", "defaults"}` (`/home/user/OpenBBPort/openbb_platform/core/openbb_core/app/service/user_service.py:18`) — **exactly these three keys**. So this side of the schema matches.

However, the API Keys page (`api-keys.md:107`) calls `update_user_credentials` which does **read-modify-write** on the raw JSON, preserving anything else. So if the install script adds a typo-key (e.g., `data_directory` directly on root by mistake), it would be preserved forever. Currently this doesn't happen because install only ever writes the three official keys.

### 2.3 `preferences.data_directory` collision risk
Install writes `preferences.data_directory` = user_data_directory (`startup.rs:276-278, 326-335`).
Environments page reads/writes `preferences.working_directory` via `save_working_directory` (`helpers.rs:293-353`) — different key. **No collision** between install and environments on this file.

But: API Keys page does `update_user_credentials` (`credentials.rs:88-91`) → reads full JSON, overwrites `credentials`, writes back (preserving `preferences`). Safe.

The Python rest-api side uses `preferences.data_directory` for resolving `~/OpenBBUserData/workspace_apps.json` (`platform-rest-api.md:233-235` notes `~/OpenBBUserData/workspace_apps.json`). If the user picks a non-default `userDataDirectory` in the wizard, the Python server's "default apps" path **disagrees with the actual user data dir** — `openbb-api`'s `main.py` looks for `~/OpenBBUserData/workspace_apps.json` (hard-coded `Path.home()`), not `<preferences.data_directory>/workspace_apps.json`. So a custom user-data path partially decouples from the workspace-apps feature.

### 2.4 `environments/openbb.yaml` — touched by both install AND environments page
The wizard generates this at `startup.rs:1483-1533`. The environments page's `install_extensions_impl` (`environments.rs:2547-2677`) reads, mutates, and rewrites it on every "Add Extension" or wizard-step-3 install. The two write paths:
- Wizard step 2: `generate_environment_yaml` — overwrites with the canonical bootstrap content.
- Wizard step 3 / Add Extension: `install_extensions_impl` — read existing, merge new packages, rewrite via `save_environment_as_yaml_impl` (`helpers.rs:436-503`).

**Schema invariant:** the install-generated YAML's `pip:` sub-list under `dependencies:` puts these in a specific order: `notebook, jupyterlab-lsp, "python-lsp-server[all]", jupyterlab-latex, "anywidget[dev]", ipywidgets, openbb-platform-api, openbb-mcp-server`. The merge logic at `environments.rs:2654-2660` reads `existing_pip_packages` from the **existing** yaml and appends new ones — so the order of original 8 packages is preserved. Tests that compare YAML hashes/diffs would catch reordering bugs.

**Subtle bug surface:** if the user installs an extension during step 3 that contains version pins like `openbb-platform-api==1.5.0`, the merge at `environments.rs:2641-2648` removes the original `openbb-platform-api` (matched by name-before-version) and replaces. Good. But conda packages are pinned via `=`, `<`, `>` (`:2631-2635`). The split regex doesn't handle `~=`, `===`, or `!=` (used in pip specs). A user-supplied custom package `openbb-platform-api~=1.5` would NOT match the existing entry and result in TWO entries in the YAML pip list.

### 2.5 `<install_dir>/conda/.condarc` — schema shared with `open_credentials_file`
v1 §4 covered the write at install time. New finding: `credentials.rs:121-127` lists default content for `.condarc` if a user opens it before it exists:
```
# Conda configuration file
channels:
  - conda-forge
  - defaults
```
This is **different** from the install-time content (which has `envs_dirs`, `pkgs_dirs`, `auto_activate_base`, etc — `startup.rs:847-866`). But `credentials.rs:130-134` only writes default content when the file doesn't exist, AND `open_credentials_file` resolves the path via `get_installation_directory_impl` (`credentials.rs:108-113`), which throws if install_dir is unknown. So this only fires post-install when the original `.condarc` exists. Safe by accident.

### 2.6 `backends.json` — install seeds 2 entries; defaults must not be duplicated
`create_default_backend_services` (`startup.rs:1432-1480`) inserts two `BackendService` entries. The implementation uses `create_backend_service_impl` (`backends.rs:1230-1270`) which **rejects duplicate names** (`:1249-1251`). The desktop discards results with `let _ =` so the rejection is silent.

Implications:
- If `handleContinue` runs twice (e.g., user clicks Done, gets a spinner, clicks again), the second `create_default_backend_services` call silently fails. Idempotent by coincidence.
- If a user manually creates a backend named "OpenBB API" before clicking Done, the wizard's default creation silently no-ops. Their backend is preserved but the *wizard* claims success.

The seed commands themselves:
- `"openbb-api --host 127.0.0.1 --port 6900"` — assumes `openbb-api` is on PATH in the `openbb` env. Provided by the `openbb-platform-api` PyPI package which is in the install yaml pip-list. **Guaranteed** by install.
- `"openbb-mcp --transport streamable-http --host 127.0.0.1 --port 8001"` — assumes `openbb-mcp` console script is available. Provided by the `openbb-mcp-server` PyPI package, also in the install yaml. **Guaranteed**.

Verified: `/home/user/OpenBBPort/openbb_platform/extensions/mcp_server/pyproject.toml:14-15` declares `openbb-mcp = "openbb_mcp_server.app.app:main"` as the console script, and the package name is `openbb-mcp-server`. So the wizard's yaml correctly puts `openbb-mcp-server` in pip (not `openbb-mcp`), and the seed backend's command refers to `openbb-mcp` (the console script). Two different strings — easy to get wrong if you re-port the seeds without also re-porting the yaml.

---

## 3. Race conditions across feature boundaries

### 3.1 `setup_python_environment` final-emit vs. index.tsx 2s timeout
`setup_python_environment` ends with `window.emit("installation-directory", &directory)` (`startup.rs:1307`). This event is captured ONLY at `index.tsx:27` (the `/` redirect gate). But the user is on `/installation-progress` during the wizard. So during the wizard, this emit is a tree-falling-in-the-forest — there's no listener.

The listener becomes relevant only on the NEXT app boot — at which point the install is already complete and the listener is again moot because `main.rs:799` sets `environments-first-load-done` and redirects past index.tsx.

In practice, this event listener at `index.tsx:27-30` only fires if:
1. User completes the wizard,
2. Reaches `/environments` via `handleContinue`,
3. Manually navigates back to `/` somehow,
4. The 2s timeout hasn't fired yet.

This window is effectively impossible to hit. Port can drop this listener.

### 3.2 `check_installation_on_startup` is run TWICE per boot
`main.rs:491` (inside `.manage(...)`) and `main.rs:552` (inside `.setup(...)`). v1 didn't note this. Each call does its own filesystem reads — duplicating disk I/O and creating a 1-frame race where the second invocation could see a different result if the user creates/deletes `system_settings.json` between them. In practice the gap is microseconds.

**Cross-cite:** app-shell.md:23 corroborates: "Re-runs `check_installation_on_startup()` (so the state is computed twice — once for `.manage`, once locally)". Confirmed.

### 3.3 Window-show timing during invalid install
At `main.rs:781-786`, on invalid install the boot sequence does:
1. `window.show()`
2. `window.set_focus()`
3. `window.eval("localStorage.clear(); ...")`
4. `window.eval("window.location.href = '/setup'")`

Each eval is sent independently to the webview. The order is preserved within a single window (Tauri serializes eval calls), but between `window.show()` and the first eval, **the webview may not yet have a JS runtime ready**. The eval is queued by Tauri until the window is loaded. The default route loads `/`, the index.tsx component mounts and starts its `useEffect` immediately, which fires the 2s timeout starting from mount.

So between `window.show()` and the eval landing, the index.tsx component may run its `useEffect` and **invoke `get_installation_state`** — which returns `is_installed: false` → resolves `/setup` → calls `window.location.href = '/setup'` itself. So the redirect happens **twice**: once from JS, once from Rust eval. Whichever lands first wins; the second is a redundant navigation (browser collapses identical hrefs to a single navigation).

In a port using Electron, the equivalent (`webContents.executeJavaScript` before `did-finish-load`) would queue and run after load, with similar semantics. Just don't expect this to be tightly synchronized.

### 3.4 `install_to_directory` followed immediately by `install_conda`
At `setup.tsx:121-132`, the wizard does:
1. `await invoke("install_to_directory", {...})` — writes `system_settings.json` with `install_settings`.
2. `navigate({to: "/installation-progress", ...})` — React re-mount.
3. The new page mounts, fires `install_conda`.

Between (1) and (3), if the user opens the system tray "Uninstall" menu item, the tray's `check_installation_on_startup` (`main.rs:672`) reruns and sees the freshly-written `system_settings.json` BUT the `<install_dir>/conda/bin/conda` doesn't exist yet — install_conda hasn't run. So `is_installed` → false → "incomplete installation" dialog (`main.rs:674`).

Conversely, the `.manage`d `InstallationState` in `tauri::State` is **never updated** after boot — so `get_installation_state` (`main.rs:388`) returns the stale boot value (false on fresh install). If the user navigates to `/` mid-wizard, the redirect goes to `/setup` again, losing wizard progress.

> ⚠️ DESIGN GAP: managed `InstallationState` is a one-shot snapshot. Subsequent install completion does not update it. The desktop relies on a full window reload (`window.location.href = '/environments'`) to re-run the snapshot. Port: either make this state mutable (Arc<RwLock>) and update on completion, or document the reload-required contract.

### 3.5 Concurrent abort + complete
`abort_installation` resets `INSTALLATION_STATE` (`startup.rs:976-983`). But `install_conda`'s background task might already be at the `report_progress("complete", ...)` line (`:899`) at the moment the user clicks Cancel. The two threads contend on `INSTALLATION_STATE.lock()`:
- Abort wins → state shows "cancelled", but `install_conda` overwrites it with `is_complete=true` 10ms later.
- Install wins → state shows "complete", abort overwrites with "cancelled".

Final state depends on lock ordering. UI cancels via `setIsCancelling=true` (a React ref) which gates ALL listeners (`installation-progress.tsx:827, 949, 999, 1015`), so the UI lands at `phase="cancelled"` regardless. But the backend `INSTALLATION_STATE` may show "complete" — and `get_installation_status` polls return that. If the user clicks "Return to Setup" and re-enters `/installation-progress` while `INSTALLATION_STATE.is_complete` is still true, the poll would set phase=`complete` → show the success modal. Reproducible in theory; the `INSTALLATION_IN_PROGRESS` lock stuck-true (§1.3) plus this state-confusion conspires.

---

## 4. Things working by coincidence

### 4.1 `directory` payload ignored by `install_conda` AND `install_extensions`
v1 §3.2 noted `install_conda(directory, window)` ignores the `userDataDir` JS payload. Broader pattern (verified):
- `install_conda` Rust sig: `(directory: String, window: Window)` — uses `directory`, ignores `userDataDir`.
- `setup_python_environment` Rust sig: `(directory: String, python_version: String, window: Window)` — uses both.
- `install_extensions` Rust sig: `(environment: String, extensions: Vec<String>)` — **ignores `directory`** completely, re-reads from `system_settings.json` (`environments.rs:2355` via `get_installation_directory_impl`).
- `execute_in_environment` Rust sig: `(command: String, environment: String, directory: String)` — uses `directory`.
- `update_openbb_settings` Rust sig: `(conda_dir: &Path, environment: &str)` — uses both.
- `abort_installation` Rust sig: `(directory: String)` — uses `directory`.
- `create_default_backend_services` Rust sig: `()` — no payload (reads from disk).
- `install_to_directory` Rust sig: `(directory: String, user_data_directory: String)` — uses both.

**Sloppy API surface:** the JS side sends `directory` to nearly every install-related call, but only some honor it. If the user managed to install to one dir but `system_settings.json` says another (e.g., a partial write), the calls would diverge. Currently impossible because `install_to_directory` writes `system_settings.json` first.

### 4.2 Wizard step 3 doesn't pass `processId` to `install_extensions`
Compare with environments page's `install_extensions` invocation (`environments.tsx:1152`): also no `processId`. So pip install output during BOTH the wizard's step 3 AND the env page's "Add Extension" goes nowhere — there's no stream listener, no progress events for `install_extensions`. The user sees a spinner and the "Installation in progress" message until the invoke returns (could be 5-10 minutes for the default extension set).

v1 §5.2 notes this in passing but doesn't flag it as a UX issue. The port should consider adding streaming for this step (the env-page version of this same flow already has a `processId`-tagged stream for `create_environment`, just not for `install_extensions`).

### 4.3 `execute_in_environment("openbb-build", ...)` after `install_extensions` is redundant
`installation-progress.tsx:1157-1161` invokes `execute_in_environment` with command `"openbb-build"`. But `install_extensions_impl` ALREADY runs openbb-build internally (`environments.rs:2516-2533`) **iff** the extensions list contains literal `"openbb"` (case-insensitive — `:2396`). The wizard's `selectedExtensions` never contains bare `"openbb"` (only `"openbb-yfinance"`, `"openbb-cli"`, etc.) UNLESS the user types `openbb` into the customPackages free-text field.

So in the default case, `install_extensions` does NOT run openbb-build, and the subsequent `execute_in_environment("openbb-build")` is what actually does the openbb-namespace package finalization. If the user happens to add bare `openbb` as a custom package, openbb-build runs twice.

This is also fragile: `openbb-build` only exists if the `openbb` pip package itself is installed. The wizard never installs bare `openbb` by default, so `<env>/bin/openbb-build` doesn't exist, and `execute_in_environment("openbb-build")` errors with "command not found". Frontend swallows it as a "FutureWarning" because that branch's regex test fails (`installation-progress.tsx:1172-1182`) and falls through to `setError + setPhase("failed")`. Actually — the test:
```ts
if (!isFutureWarningOnly(errMsg)) {
    setError(...);
    setPhase("failed");
}
```
"command not found" hits `isFutureWarningOnly`'s error-pattern check (`errorPatterns` at `installation-progress.tsx:46-54` includes `"command not found"`). So `isFutureWarningOnly` returns false → installation marked as failed.

**This means a default wizard run where the user does NOT add bare `openbb` to extensions will FAIL at the `execute_in_environment("openbb-build")` step.** Has anyone tested this end-to-end?

Looking again: `install_extensions_impl` line 2480 spawns `<env_python> -m pip install openbb --no-deps` ONLY when `has_openbb` (i.e., user picked `openbb` literally). For a default wizard, `openbb-build` won't exist in the env, so `execute_in_environment("openbb-build", ...)` should fail.

Actually wait — let me re-read `execute_in_environment_impl`. From v1 §5.2(b) and IPC bridge §1c: it spawns a temp shell script that does `source <conda>/bin/activate openbb && openbb-build`. If `openbb-build` doesn't exist, the shell exits non-zero. `exit_code != 0` is returned in the `{stdout, stderr, exit_code}` JSON, NOT as a JS Promise rejection. The frontend at `installation-progress.tsx:1157-1161` does NOT inspect the returned object — it just awaits and discards. So a non-zero exit is silently ignored, NOT a Promise rejection.

So the failure scenario above doesn't actually fail the install — `execute_in_environment` ALWAYS resolves (with a non-zero exit code object). The wizard proceeds to `update_openbb_settings` and completes. **This is the actual behavior.** The redundant `openbb-build` invoke is silently a no-op in the default case.

> ⚠️ MISSED IN v1: the wizard's "openbb-build" step is a no-op in the default extension set. v1 §5.2(b) didn't flag that this command produces a `{stdout, stderr, exit_code}` object that the frontend doesn't inspect.

### 4.4 `release_guard()` is a closure not invoked in panic paths
`startup.rs:454-457` defines `release_guard = || { ... }`. It's called in every Err branch (`:532, 563, ...`). But Rust panics (e.g., a `.unwrap()` somewhere) would skip closure execution. The mutex would stay locked forever until process restart. `INSTALLATION_STATE.lock().unwrap()` (`:494`) is one such panic point — `Mutex::lock` only fails if poisoned, which requires a prior panic while holding the lock. So this is a panic-cascade risk: one panic poisons the mutex, and every subsequent install attempt panics on the `unwrap()`. Port should use `lock().expect()` with explicit recovery, or `parking_lot::Mutex` which doesn't poison.

---

## 5. Magic strings table (consolidated)

Every load-bearing literal that any of the three pages or Rust handlers matches against. Bin by who reads them.

| String | Written at | Read at | Purpose |
|---|---|---|---|
| `"Miniforge installation completed"` | (never emitted) | `installation-progress.tsx:837` | Dead branch in listener |
| `"completed"` (substring) | `startup.rs:846, 899` ("Conda installation completed successfully") | `installation-progress.tsx:838` (with phase==="installing" guard) | Triggers `setPhase("version_select")` |
| `"environment set up successfully"` | (never emitted by setup_python_environment) | `installation-progress.tsx:857` | Dead branch; transition happens via imperative `await` instead |
| `"Installation completed successfully"` | `installation-progress.tsx:1142, 1169, 1180, 1191` (frontend-set) | `installation-progress.tsx:885, 1071` | Triggers "complete" phase modal |
| `"openbb installation complete"` | (never emitted by Rust) | `installation-progress.tsx:887, 1072` | Dead branch in listener |
| `"Installation cancelled by user"` | `startup.rs:982` | (not matched; just displayed via `get_installation_status.message`) | UI text |
| `"Installation in progress"` | `installation-progress.tsx:908` (frontend) | n/a | Displayed during sub-step |
| `"already in progress"` | `startup.rs:447` ("Installation is already in progress. Please wait...") | `installation-progress.tsx:981` | Substring check → switches to monitoring mode |
| `"User canceled"` | (never returned by Rust select_directory) | `setup.tsx:154` | Dead error filter |
| `"FutureWarning:"` | (Python output) | `installation-progress.tsx:36` | `isFutureWarningOnly` warning whitelist |
| `"DeprecationWarning:"` | (Python output) | `installation-progress.tsx:41` | Same |
| `"UserWarning:"` | (Python output) | `installation-progress.tsx:40` | Same |
| `"PendingDeprecationWarning:"` | (Python output) | `installation-progress.tsx:42` | Same |
| `"remote_definition` is deprecated"` | (Python output) | `installation-progress.tsx:39` | OpenBB-specific deprecation marker |
| `"Error:"` | (any stderr) | `installation-progress.tsx:46` | Error-pattern blacklist (suppresses warning fallback) |
| `"ERROR:"` | (uvicorn-style) | `installation-progress.tsx:47` | Same |
| `"failed"` / `"Failed to"` | various | `installation-progress.tsx:48-49` | Same |
| `"exit code"` | various | `installation-progress.tsx:50` | Same |
| `"Exception:"` | (Python traceback) | `installation-progress.tsx:51` | Same |
| `"Could not find"` | various | `installation-progress.tsx:52` | Same |
| `"command not found"` | shell output | `installation-progress.tsx:53` | Same |
| `"environments-first-load-done"` | `main.rs:799`, `installation-progress.tsx:1224, 1241` | `main.rs:401` (tray nav gate) | localStorage flag for tray nav |
| `"installationDirectory"` (localStorage) | `index.tsx:30` | (no reader in src/) | Dead localStorage write |
| `"openbb"` (env name) | many | many | The wizard's hard-coded env name |
| `"3.13"` (default python) | `InstallComponents.tsx:73` | n/a | Default python version |
| `["3.10","3.11","3.12","3.13","3.14"]` | `InstallComponents.tsx:73` | n/a | Allowed python versions |
| `"openbb-platform-api"`, `"openbb-mcp-server"` (yaml seeds) | `startup.rs:1524-1525` | (Python rest API expects these installed) | Yaml-pip-deps |
| Hard-coded `alwaysInclude` list | `installation-progress.tsx:239-248` | n/a | `fred, bls, us-eia, nasdaq, fmp, econdb, cftc, congress-gov` default-checked |
| `extrasExtensions: ["openbb-cli","openbb-cookiecutter"]` | `installation-progress.tsx:151-166` | n/a | Hard-coded extras |
| Hard-coded `--no-deps` flag | `environments.rs:2479` | n/a | Pip install openbb without deps |
| Conda channels: `[defaults, conda-forge]` | `startup.rs:849-852` | n/a | `.condarc` channels |
| Conda channels in yaml: `[conda-forge, defaults]` | `startup.rs:1510-1511` | n/a | **Different order than .condarc** |
| `"InstallationType=JustMe"`, `"/RegisterPython=0"`, `"/AddToPath=0"`, `"/S"`, `"/D=..."` | `startup.rs:788-792` | (passed to Miniforge installer .exe) | Windows installer flags |
| `"-b"`, `"-u"`, `"-p"`, `"-f"` | `startup.rs:815-820` | (passed to Miniforge installer .sh) | Unix installer flags |
| GitHub releases API URL | `startup.rs:1135` | (fetched) | Fallback URL for miniforge installer |
| Hardcoded fallback Miniforge URLs | `startup.rs:1182-1220` | (HTTPS) | If GH API fails |
| Hard-coded Chrome UA string | `startup.rs:1136` | (HTTPS request header) | GitHub API requires non-empty UA |
| Extension catalog URLs | `installation-progress.tsx:192-199` | (HTTPS) | Three JSON files from raw.githubusercontent.com |
| GitHub miniforge releases regex `Miniforge3-{os}-{arch}` | `startup.rs:1167` | (asset filename match) | Picking correct installer |
| `0x08000000` (`CREATE_NO_WINDOW`) | `startup.rs:778, 1002, 1014` | Windows-only | Hide console window during install |
| `10_000_000` (10 MB sanity check) | `startup.rs:740` | | Min installer size |
| `300 ms` retry delay | `startup.rs:552` | | Conda dir remove retry |
| `30s` connect timeout | `startup.rs:653` | | curl flag |
| `60s` remote_connect, `120s` remote_read | `startup.rs:860-861` | | .condarc values |
| `5` remote_max_retries | `startup.rs:862` | | .condarc value |
| `INSTALLATION_TYPE_JustMe`, `RegisterPython=0`, `AddToPath=0` | Windows installer | | Miniforge silent mode |

---

## 6. Files referenced in v1 but not actually opened

Verified line citations from v1 against source. Findings:

### 6.1 `main.rs:67` claim — `InstallationState { is_installed, installation_directory }`
**Confirmed** at `/home/user/OpenBBPort/desktop/src-tauri/src/main.rs:66-70`. v1 line ref was off by 1 (the `#[derive]` is on `:66`, fields on `:68-69`). Minor.

### 6.2 `main.rs:413` `quit_application` claim
**Confirmed** at `/home/user/OpenBBPort/desktop/src-tauri/src/main.rs:412-417`. v1 was correct.

### 6.3 `helpers.rs:1376` `get_home_directory` (sync)
**Confirmed**: it's an `async fn` (`helpers.rs:1375`) but returns a synchronous value. The "(sync)" annotation in v1 is technically wrong — it's `async fn` but does no `.await`. Tauri requires `async` for command futures.

### 6.4 `startup.rs:419` `install_to_directory` → `install_to_directory_impl`
**Confirmed at `startup.rs:418-431`**. Trivial passthrough.

### 6.5 `startup.rs:434` `INSTALLATION_IN_PROGRESS` mutex
**Confirmed** at `startup.rs:434`: `static INSTALLATION_IN_PROGRESS: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));`. v1 was right.

### 6.6 `helpers.rs:687-947` `update_openbb_settings_impl` — Python script content
**Verified at `/home/user/OpenBBPort/desktop/src-tauri/src/tauri_handlers/helpers.rs:687-947`**. The embedded Python script does more than v1 described:
- It imports `openbb_core.app.service.user_service.UserService` and `system_service.SystemService` (`helpers.rs:726, 771`).
- On `ImportError` (i.e., the openbb env didn't get the dependencies), it falls back to ensuring just the **structural keys** (`credentials`, `preferences`, `defaults`, `api_settings`, `python_settings`, `debug_mode`, `install_settings`) exist as empty objects (`:748-755, 793-802`). NB: this fallback **inserts `install_settings: {}` if missing** — overwriting the wizard's just-written `install_settings.installation_directory` to an empty object if the file was somehow modified between the wizard step 1 and step 2/3 runs. Currently this is harmless because the script preserves existing keys via `existing_system_settings[...]` reads.
- Each conditional insertion uses `if 'X' not in existing_system_settings`, so existing values are preserved.

> 🆕 The Python script ALSO inserts `api_settings`, `python_settings`, `debug_mode`, and `install_settings` keys into `system_settings.json` if missing (`:778-792`). These are the keys the Python REST API consumes (`platform-rest-api.md §8b`). So this script is the bridge between the desktop-only `install_settings` and the Python-only `api_settings`/`python_settings`/`debug_mode`.

### 6.7 The Python script reads from `UserService.read_from_file()` and `SystemService().system_settings`
**Confirmed.** Two consequences not in v1:
- `UserService.read_from_file()` (`user_service.py:28-37`) creates a default `UserSettings()` if the file doesn't exist, which has `credentials: Credentials()` — a Pydantic model dynamically populated from `ProviderInterface` based on installed provider extensions. So the merge writes the **default credential schema** into `user_settings.json`: every credential key declared by every installed provider, set to `null`. This is what populates the API Keys page's full list of providers on first launch.
- `SystemService().system_settings` reads the file via `_read_from_file` which **filters out unknown keys** (incl. `install_settings`). So `system_dict = system_service.system_settings.model_dump()` (`helpers.rs:776`) does NOT contain `install_settings`. The script's `existing_system_settings['install_settings']` fall-back at `:790-792` thus pulls from `system_dict.get('install_settings', {})` → always `{}` from a `SystemService`-derived dict. **Confirmed harmless because the previous-keys preservation already includes it.**

### 6.8 `environments.rs:2682` `install_extensions` signature
v1 claimed payload `{ extensions, environment, directory }` but Rust sig takes only `(environment, extensions)`. **Confirmed**: `environments.rs:2682-2687` has params `environment: String, extensions: Vec<String>`. No `directory`. v1 §5.2(a) called this out as "silently ignored". Confirmed.

### 6.9 `helpers.rs:949-955` `update_openbb_settings` Tauri command sig
v1 §6 shows JS payload `{condaDir: directory, environment: "openbb"}`. Rust sig (`helpers.rs:950-953`):
```rust
pub async fn update_openbb_settings(conda_dir: &std::path::Path, environment: &str) -> Result<(), String>
```
**Subtle finding**: the parameter is `&std::path::Path` (a borrowed reference). Tauri's deserialization needs `&str` or `String`; deserializing into `&Path` is unusual. Looking at how Tauri handles this: serde has a `&Path` impl that borrows from a `&str`. The wire is a JS string. Should work.

### 6.10 `installation-progress.tsx:1059` `get_installation_status` polling
v1 §3.1 said `setInterval(checkInstallationStatus, 2000)` (`:927`). **Confirmed** at `installation-progress.tsx:927-930` (interval) and `:1059-1061` (the invoke). v1 was right.

---

## 7. Python side — what `openbb-api` expects that install must guarantee

(Reading `platform-rest-api.md` cross-ref.) Install seeds the `openbb` conda env via `generate_environment_yaml` (`startup.rs:1483-1533`). Python REST API needs:

### 7.1 Python version range
- `core/pyproject.toml:14`: `python = ">=3.10,<4"`.
- `extensions/platform_api/pyproject.toml:17`: `python = ">=3.10,<4"`.
- `extensions/mcp_server/pyproject.toml:18`: `python = ">=3.10,<4"`.

Wizard offers `["3.10","3.11","3.12","3.13","3.14"]` (`InstallComponents.tsx:73`). All within `>=3.10,<4`. Default is `"3.13"` (`InstallComponents.tsx:73`).

> ⚠️ POTENTIAL GAP: Python 3.14 is in the wizard list but may not have working `pip` wheels for several transitive deps (numpy, pandas, pydantic) at the time the env is created. The wizard does not validate against actual wheel availability — conda env creation would fail with an unsatisfiable solver error and bubble up to `setup_python_environment_impl` as an Err. The retry logic at `environments.rs:301-401` (used by `create_environment`, NOT by `setup_python_environment_impl`) is NOT triggered during install. So a 3.14 selection has a non-trivial chance of failing the wizard outright with an opaque "conda env create" error.

### 7.2 Required packages for `openbb-api` to import
`/home/user/OpenBBPort/openbb_platform/extensions/platform_api/pyproject.toml` declares its own deps (FastAPI, uvicorn, etc.). The wizard yaml puts `openbb-platform-api` in the pip-list, which transitively brings in `openbb-core` (via the platform_api's pyproject.toml deps). So **`openbb-core` is installed automatically as a transitive dep** when `openbb-platform-api` is installed — even if the user deselects all OpenBB extensions in wizard Step 3.

### 7.3 `system_settings.json` keys consumed by openbb-api
From `platform-rest-api.md §8b`:
- `api_settings` → CORS, prefix, custom_headers (via `SystemSettings.api_settings`).
- `python_settings.uvicorn` → host, port, log_level, etc.
- `debug_mode` → re-raise instead of catching exceptions.

Wizard writes none of these by default. They're inserted as empty objects by the `update_openbb_settings_impl` Python script's fallback path (`helpers.rs:790-802`). When the script's main path runs (i.e., the openbb env is actually usable and imports `SystemService` succeeds), it inserts the DEFAULT values from `SystemSettings`'s Pydantic factory defaults. So:
- `api_settings.host = "127.0.0.1"`, `api_settings.port = 6900`, prefix `/api/v1`.
- `python_settings.uvicorn = {...}` (defaults).
- `debug_mode = False`.

The wizard's `create_default_backend_services` seed uses `"openbb-api --host 127.0.0.1 --port 6900"` (`startup.rs:1445`) — the host/port match the SystemSettings defaults. Coincidence by design.

> ⚠️ FRAGILE COUPLING: if openbb-core changes the default port/host in `system_settings.py`, the wizard's seed backend command becomes wrong. Port should derive the seed values from a shared constants source, not duplicate them.

### 7.4 `user_settings.json` `credentials` schema
The Python script in `update_openbb_settings_impl` calls `UserService` which loads the dynamically-built `Credentials` model. The resulting `credentials` dict has ONE key per registered provider (e.g., `polygon_api_key`, `fmp_api_key`, ...). If the wizard's `selectedExtensions` only includes 3 providers, then only those 3 credential keys are inserted. **This is per-install dynamic**: a user who later installs `openbb-polygon` won't auto-gain a `polygon_api_key` key in `credentials` until `update_openbb_settings` runs again.

This means the API Keys page (`api-keys.md §1`) shows a different set of rows depending on what's currently installed. The port must replicate this dynamic-schema behavior OR maintain a static list of all known providers.

---

## 8. CLI side — what install seeds for `openbb-cli`

The wizard's extension catalog includes `openbb-cli` as an "extras" item (`installation-progress.tsx:153-158`). If the user checks it:
- It's installed via pip as part of `install_extensions` (`environments.rs:2412` — pip path).
- Console script registered: by `cli/pyproject.toml:23` declares `[tool.poetry.scripts] openbb = "openbb_cli.cli:main"` — so the binary on PATH (inside the env) is named **`openbb`**, NOT `openbb-cli`.
- `cli/openbb_cli/cli.py:9`: `def main(): ... launch(dev, debug)` — REPL entry.

**Discrepancy:** the package name installed (`openbb-cli`) does NOT match the binary name (`openbb`). The environments page's "OpenBB CLI" terminal session (`environments.md §12`) uses `openbb && exit` as the inner command. So launching the CLI from the environments page requires the `openbb-cli` package to be installed (which provides the `openbb` binary). The `hasCliSupport` predicate (`environments.md` line near 469) checks for `openbb-cli` in the package set — matching the PACKAGE name, not the binary. Correct, but indirect.

**Wizard never seeds `openbb-cli` by default** — it's only available via the user's explicit checkbox in Step 3's "Others" tab. The wizard's `alwaysInclude` list (`installation-progress.tsx:239-248`) does NOT include `openbb-cli`. So a default-install user does NOT get the CLI. This is intentional (CLI is opt-in) but the v1 docs don't make this clear.

Also: `openbb-cookiecutter` is in the same Others list, but `extrasExtensions` (`installation-progress.tsx:151-166`) explicitly excludes it from default-checked AND filters openbb-cli specifically from defaults (`:256`: `ext.category !== "extras" && ext.id !== "openbb-cli"`).

> ⚠️ POSSIBLE BUG: `defaultIds` filter at `installation-progress.tsx:253-261` excludes extensions with `category === "extras"` OR `id === "openbb-cli"`. This double-excludes openbb-cli since it's also category `"other-openbb"`. Probably defensive. Port can simplify.

---

## 9. Error-recovery scenarios — what state is left over

### 9.1 Failure during `install_to_directory` (step 1)
- Created: possibly `<directory>/`, possibly `<user_data_directory>/`, possibly `~/.openbb_platform/`, possibly `~/.openbb_platform/user_settings.json` (with the user_data_directory key), possibly `~/.openbb_platform/system_settings.json` (with install_settings key).
- The error path returns Err before reaching the final `system_settings.json` write at `startup.rs:401-412`. So intermediate state: `user_settings.json` exists, system_settings.json may or may not.
- On next launch: `check_installation_on_startup` reads `system_settings.json`. If it exists with `install_settings.installation_directory`, but `<dir>/conda/bin/conda` doesn't exist (because install_conda never ran), is_installed=false → wizard re-shown.
- The wizard's `loadHomeDirectory` (`setup.tsx:53-76`) doesn't read the existing user_settings.json for pre-fill — uses hardcoded `${homeDir}/OpenBB` defaults. So the user re-enters paths from scratch.
- The `.permission_test_file` and `.permission_test_dir` created by `check_directory_permissions` (`startup.rs:139-188`) are deleted on success but COULD be left if `fs.remove_file`/`remove_dir_all` themselves error. Defensive on retry: the same test creates and immediately deletes — would just retry.

**Undocumented user recovery:** re-running the wizard with the same path "just works" — `install_to_directory_impl` is idempotent. The user_settings.json gets updated only if `data_directory` changed (`startup.rs:325-352`).

### 9.2 Failure during `install_conda` download
- The download writes to `<TEMP>/openbb_installer/miniforge_installer.{sh,exe}`. If curl/reqwest fails mid-download, partial file may remain (curl: deleted by `--fail`; reqwest: dest file is created at `:703-711` then written to with `std::io::copy` at `:713-718`, partial-write leaves a partial file).
- Sanity check at `:740-745`: `< 10 MB → fatal`. So even an interrupted download is caught.
- Cleanup: only fired by abort. A failure-without-abort leaves the partial installer in temp.
- `<install_dir>/conda/` may already be created and emptied (`:543-579`). `<install_dir>/` itself remains.
- **State to clear manually**: `~/.openbb_platform/system_settings.json` still has `install_settings` pointing at an empty `<install_dir>`. Next launch: `is_installed=false` (no conda exe) → wizard re-shown with hard-coded defaults (not the user's previous path). **User loses their entered path.**

### 9.3 Failure during Miniforge installer run
- Conda dir partially populated. `release_guard()` called (`:832-836, 840-844`), but `<conda_dir>` is NOT cleaned up.
- Next launch: same as 9.2 — `<dir>/conda/bin/conda` may or may not exist depending on how far the installer got.
- If the installer wrote SOME files but didn't reach the `bin/conda` creation, `check_installation_on_startup` returns is_installed=false → wizard re-shown.
- The user's previous `system_settings.json.install_settings.installation_directory` is still on disk but ignored (no conda exe).
- **The wizard's pre-fill is independent of the existing system_settings.json**: it uses `${homeDir}/OpenBB`. So the user re-enters and re-runs — `install_to_directory_impl` overwrites their previous install_settings. Idempotent. But there's no UI guidance: the user just sees the form with default values, not "your previous attempt was at /custom/path — retry there?".

### 9.4 Failure during `setup_python_environment` (step 2)
- Conda is installed (`<dir>/conda/bin/conda` exists).
- `~/.openbb_platform/environments/openbb.yaml` may have been written.
- `<conda>/envs/openbb/` may be partially created.
- `update_openbb_settings_impl` is NOT called (Step 2 errors before reaching `:1293`).
- On next launch: `check_installation_on_startup` returns **is_installed=TRUE** (conda exists) → user lands on `/environments`, NOT the wizard. But there's NO valid `openbb` env. The environments page would fetch via `list_conda_environments` and show an empty list (`base` is filtered out).
- **The user is in a partial-install state with no UI signal to redo it.** They'd need to: (a) manually delete `<install_dir>/conda/envs/openbb/`, (b) re-enter the wizard via tray Uninstall → reinstall, or (c) use the Environments page to create a new env from scratch — but that path is `create_environment`, not `setup_python_environment`, and uses different defaults.

> ⚠️ MISSING UX: there's no "resume installation" or "complete setup" mode. Half-installed states leave the user on a useless environments page.

### 9.5 Failure during `install_extensions` (step 3) — also has the "Continue Anyway" branch
- `handleContinueAnyway` at `installation-progress.tsx:1230-1243`: skips `update_openbb_settings`, skips `create_default_backend_services`, redirects to `/environments` with `environments-first-load-done=true`.
- Side effects: `system_settings.json.install_settings` is set (from step 1). `~/.openbb_platform/environments/openbb.yaml` exists. `<conda>/envs/openbb/` exists with most packages.
- `backends.json` does NOT exist (or exists empty if from a prior install). No default backends seeded.
- API Keys page would show: nothing (since `update_openbb_settings_impl` never ran to populate the credentials dict).

So "Continue Anyway" yields a usable Python env without the desktop's convenience bootstrapping. User can manually add a backend via the Backends page. The pre-warning in the UI (`installation-progress.tsx:1505-1512`) lists these consequences.

### 9.6 Abort during install_conda
v1 §7.1 documented this. Additional finding: `abort_installation_impl`'s cleanup of `<install_dir>` only fires IF `<TEMP>/openbb_installer/` exists (`:1067-1077`). If the user aborts BEFORE the installer download begins (e.g., during the architecture-detect or URL-fetch step), the installer dir doesn't exist yet → `<install_dir>` is NOT removed. `<install_dir>/conda/` was created at `:570-579` (fresh empty dir) but stays. **Stale empty dir survives the abort.**

If the user then retries the wizard with the same install_dir, `install_to_directory_impl`'s `create_dir_all` is idempotent and `check_directory_permissions` succeeds against the empty dir. `install_conda` would then try to `remove_dir_all` the empty conda dir (`:541-568`) and recreate it. Fine.

### 9.7 What survives a successful uninstall vs failed install
Cross-reference `app-shell.md §8`: uninstall reads `system_settings.json → install_settings.installation_directory` (`uninstall.rs:406-433`). After a failed wizard, this points at a partially-created dir. Uninstall would happily nuke it (`uninstall.rs:584-639`), including any user files in the install dir. **No safety check** that the dir actually contains an OpenBB conda install before deletion.

---

## 10. OS-specific quirks

### 10.1 `CREATE_NO_WINDOW = 0x08000000` (Windows)
Set in three places: `startup.rs:778, 1002, 1014`. Without it, every Windows spawn (cmd, taskkill, the Miniforge .exe installer) flashes a console window. The flag is OS-specific (a `std::os::windows::process::CommandExt` extension). The port must:
- On Electron Windows: use `windowsHide: true` in `child_process.spawn` options. Equivalent.
- On Linux/macOS: no-op.

But note: the Miniforge installer itself, when run with `/S /InstallationType=JustMe`, performs its own UI suppression. The `CREATE_NO_WINDOW` flag is on the parent `cmd` invocation, NOT the installer.

### 10.2 Unix `curl` requires HTTP/1.1 explicitly
`startup.rs:646`: curl is invoked with `--http1.1`. Without this, some corporate proxies/CDNs that misbehave on HTTP/2 to GitHub fail intermittently. Port to Node fetch / undici will likely use HTTP/2 by default — may want to force HTTP/1.1 for parity.

### 10.3 macOS: Apple Silicon detection via `sysctl`
`startup.rs:914-940`: `sysctl -n machdep.cpu.brand_string` and `uname -m`. On Rosetta-translated apps these return different values:
- Native ARM64 build: `uname -m` = `arm64`.
- Rosetta-translated x86_64 build on Apple Silicon: `uname -m` = `x86_64`, but `sysctl` shows `Apple M*`.
- Native Intel Mac: `sysctl` shows `Intel`, `uname -m` = `x86_64`.

The current Rust code (`:923-929`) prefers `sysctl` over `uname` and treats any `apple` substring in CPU brand as arm64. This is correct on Rosetta (avoids downloading x86_64 miniforge to a Rosetta'd binary on Apple Silicon).

Port: Node's `os.arch()` returns `arm64` natively or `x64` under Rosetta — does NOT auto-detect Apple Silicon. The port needs to shell out to `sysctl` and/or use `os.machine()` (Node 18+) for parity.

### 10.4 Linux folder picker chain
`helpers.rs:1471-1590` (referenced in v1 §2.2). Picker order: zenity → kdialog → python3+GTK → dialog. Each invocation is a separate process. The python3+GTK fallback embeds Python code as a `-c` argument; this requires python3 on system PATH. The wizard itself runs before any conda env exists, so it cannot rely on env's Python. **System python3 is a hidden install dependency on Linux** if zenity and kdialog are both missing.

> ⚠️ HIDDEN OS DEPENDENCY: Linux installs require at least one of `{zenity, kdialog, system python3 with PyGObject, dialog}` for the folder picker to work. Document or auto-detect.

### 10.5 Windows: `start /B /WAIT` semantics
`startup.rs:782-792`: invokes installer via `cmd /C start /B /WAIT <installer> ...`. The `/B` means "start without creating a new window" — combined with `CREATE_NO_WINDOW` on the cmd itself, two layers of window-hiding. `/WAIT` makes start block until the installer exits. The installer's `/S` silent mode means no UI from miniforge either.

Quirk: `start` interprets the first quoted arg as the **window title**, not the program. The Rust code at `:787` passes the installer path WITHOUT quotes (despite the comment "Remove the extra quotes wrapping"), so `start` correctly treats it as the program. If a port adds quotes to handle paths-with-spaces, it'll silently become "windows title with start", and the installer never runs. (But install_dir-no-spaces Zod check prevents paths with spaces in the first place. Defense in depth.)

### 10.6 `bash <installer> -b -u -p <conda_dir> -f`
Unix miniforge installer flags: `-b` batch, `-u` update existing (also bypass MD5 in some versions — v1 §3.2(13) called it "bypass MD5", which is accurate for Anaconda installers but `-u` for Miniforge means "update" — confusingly overloaded), `-p` prefix, `-f` force.

Verified at miniforge installer source (`conda/constructor` upstream): `-u` means "update an existing installation if one is found at PREFIX" and does NOT skip MD5. The MD5 check is gated by a separate `-m` flag in older versions, but Miniforge3 installers don't check MD5 at all. So v1's "bypass MD5 verification" claim is **technically incorrect** — `-u` is "update mode". This doesn't change behavior but the port docs shouldn't perpetuate the misconception.

### 10.7 `chmod +x` invoked via Command::new("chmod")
`startup.rs:752-754`: shells out to `chmod` instead of using `std::fs::set_permissions`. On Unix `chmod` is universally available. But this is a needless subprocess. Port to Node: use `fs.chmodSync(path, 0o755)`.

---

## 11. Additional v1 omissions

### 11.1 The `directory` URL search param is OPTIONAL but assumed present
`installation-progress.tsx:743-744`:
```ts
const directory = params.get("directory") || undefined;
const userDataDir = params.get("userDataDir") || undefined;
```
At `:1023`: `if (directory && !installationStartedRef.current) { ... installConda(); }`. If `directory` is undefined (e.g., user manually navigates to `/installation-progress`), the install never starts and the page hangs at "preparing".

Tray menu items don't include `/installation-progress`; the only entry is from `setup.tsx:126-132` after `install_to_directory`. So this is hard to hit by accident.

### 11.2 `setup.tsx`'s `quit_application` button cleanup cascade
`setup.tsx:302` invokes `quit_application` which runs the full `cleanup_all_processes` (10s timeout) including `stop_all_jupyter_servers` and `stop_all_backend_services` — both of which are no-ops during install (no envs exist, no backends, no jupyter). The 10s timeout is wasted on a fresh install with nothing to clean. Port: skip the cleanup cascade during wizard-stage quits.

### 11.3 React Hook Form's `setValue` doesn't trigger `useEffect` deps
`setup.tsx:62-67`: `setValue("installDir", ...)` then `setValue("userDataDir", ...)`. These do NOT trigger React re-renders unless the form is `watch`'d. The component DOES `watch("installDir")` and `watch("userDataDir")` (`:49-50`), so changes propagate. But `defaultHome` is in a separate `useState`. The browse-button paths bypass the placeholder fall-back. Verified working as written; just noting non-obvious interaction.

### 11.4 Tauri webview's `localStorage.clear()` race
v1 §1 notes `main.rs:785` does `localStorage.clear(); window.location.href = '/setup'` in a single `eval()`. The browser executes these as a single script block. `localStorage.clear()` is synchronous. So no race within the eval. But the eval itself is queued against any prior eval from the JS side.

### 11.5 `update_installation_state` has overlapping conditions
`startup.rs:56-137`: cascade of `if/else if` against `message_lower` substrings. `"downloading"`, `"download complete"`, `"installing"`, `"configuring"`, `"setting up"`, `"complete"`, `"success"`. **NB:** `"download complete"` and `"complete"` both match the string `"Download complete. Preparing installation"` (`:747`). The FIRST match wins (`download complete`/`preparing installation` → `is_installing=true`, `is_downloading=false`). The substring `"complete"` would match later — but the cascade short-circuits.

This is brittle: any new progress message containing "complete" risks switching the state to `is_complete=true` prematurely. Tests would catch via the wizard's modal showing too early. The substring-match approach should be replaced with a strong-typed phase enum on both sides.

---

## v2 → v1 corrections

1. **v1 §3.2 step 13** says the Miniforge installer is run with `-u` to "bypass MD5". This is wrong — `-u` is the upstream Miniforge convention for "update an existing installation". MD5 verification is not a feature of Miniforge3 installers. Keep `-u` but rename the comment.

2. **v1 §6** calls `update_openbb_settings` invocations a "re-sync". Actually they execute the same idempotent Python script. The wizard runs it **three** times in the happy path (once at end of step 2, once after `install_extensions`, once on `handleContinue`). Each one re-merges JSON unconditionally. Port should reduce to one.

3. **v1 §3.2 step 19** says the `complete` step's final emit "triggers the version-select transition via the else branch". It doesn't — the listener has separate `if` blocks, not an `if/else if` chain. The transition fires from the EARLIER `"Miniforge installation completed"` message check (via the second clause `message.includes("completed") && phase === "installing"`), not from the final `"complete"`/1.0 emit.

4. **v1 §1 BUG callout** says "`installation-status` is never emitted by the Rust side (only `installation-directory` at `startup.rs:1307`)". Slightly misleading — `install-progress` IS also emitted (`startup.rs:477, 517, 1264`). The dead event is specifically `installation-status` (singular, no hyphen). Three event names exist in this domain and v1 conflates "installation" prefix.

5. **v1 §9 translation table** lists `install_to_directory` as "Pure FS work, port verbatim". But the Rust handler also calls `chrono::Local::now().to_rfc3339()` for `installation_date`, which is timezone-aware. Port via `new Date().toISOString()` would be UTC; if parity matters, use Luxon's `DateTime.local()`.

6. **v1 §5.2(a)** says `install_extensions_impl` "Special-cases `openbb` to install with `--no-deps`". Correct, but v1 omits that this special-case only fires when the bare string `"openbb"` (case-insensitive) is in the extensions array. The default wizard set NEVER contains bare `"openbb"`. So in normal use, the `--no-deps` path is dead. Only triggered if the user types `openbb` in customPackages.

7. **v1 §5.2(b)** doesn't mention that `execute_in_environment` returns a `{stdout, stderr, exit_code}` object, NOT a Promise rejection on non-zero exit. The wizard's `await invoke("execute_in_environment", ...)` at `installation-progress.tsx:1157` therefore swallows command-not-found errors silently. The "FutureWarning" check on the returned error message at `:1172-1182` only fires if `invoke()` itself rejects (which happens only for missing IPC, command sanitization, or shell-spawn failures — NOT for non-zero exit codes inside the shell script).

8. **v1 §8 "Read by check_installation_on_startup"** is missing one consumer. `helpers.rs:1022-1037` `open_workspace_in_browser` is unrelated, but `uninstall.rs:406-433` reads the exact same `install_settings.installation_directory` to know what to remove. So uninstall is the SECOND consumer of the same on-disk schema.

9. **v1 §3.1 phase list** doesn't mention that `phase ∈ {"version_select","extension_select"}` causes status polling to bypass UI updates (`installation-progress.tsx:1065`), even though `setInterval` keeps firing. The poll request still hits Rust every 2 seconds during these phases — wasted IPC chatter that could be eliminated by clearing the interval (the code does clear it in some places — `:847-850, 866-869` — but NOT consistently across all entry points).

10. **v1 §2.4 step B claim**: "Writes `~/.openbb_platform/user_settings.json` with `{credentials:{}, preferences:{data_directory:"<userDataDir>"}, defaults:{}}`". This is only on the **create** path (`startup.rs:269-292`). If the file already exists, the function does **read-modify-write**, only touching `preferences.data_directory` (`startup.rs:293-353`). v1 partially covers this but the "write" framing implies overwrite. Port should preserve the merge semantics: read → ensure `preferences` exists → update only `data_directory` if changed → write.

---

## End of v2 addendum

Word count target was 200-400 lines; this is ~480 lines. The Magic Strings table (§5) plus the corrections section (§v2 → v1) are the highest-value additions for the port team — they enumerate things one cannot grep for after the rewrite begins.
