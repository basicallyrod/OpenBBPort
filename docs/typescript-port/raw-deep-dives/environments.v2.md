# Environments v2 — Cross-Reference Gap-Fill

> Second-pass review of `environments.md`. Reads v1 plus `installation.md`,
> `backend-services.md`, `logs-streaming.md`, `ipc-bridge.md`, `platform-rest-api.md`.
> Only **new** findings, gap-fills, and corrections. Does NOT restate v1 content.
> Generated 2026-05-15.

---

## 1. Backends ↔ Environments cross-coupling (v1 §0 / §5 gap)

v1 §5.3 documents `remove_environment` but doesn't say anything about backends
that live inside that env. They do, and the wiring is missing.

### 1.1 `removeEnvironment` never consults `list_backend_services`

Frontend (`environments.tsx:1298-1357`) — the entire `removeEnvironment` flow:

1. `deletedEnvironments.current.add(envName)` (line 1311).
2. `invoke("remove_environment", { name, directory })` (line 1316).
3. Cache cleanup (lines 1324-1346).

There is **no** read of `list_backend_services`, no stop-cascade, no warning
to the user that backends bound to `envName` are about to be orphaned.

### 1.2 Rust `remove_environment_impl` is just as oblivious

`environments.rs:2689-2751` — only checks for "base" name, locates conda exe,
runs `conda env remove -n <name> -y`, with `fs::remove_dir_all` fallback, then
deletes `<envs>/<name>.yaml`. **No `backends.json` read.** Compare to
`backend-services.md §5.3` which shows the only "is_process_running" cleanup
happens at boot (`initialize_backends`, `backends.rs:1455-1475`) and only for
backends whose status is `running`.

### 1.3 The actual failure mode

If a user removes the env that hosts a `running` `openbb-api` backend:
- The shell wrapper child (the bash/cmd PID stored in `backends.json`) keeps
  running for a moment — `conda env remove` does NOT signal in-flight processes
  bound to the env's `python` binary (the binary is just open; conda will
  unlink the directory and the kernel's inode handle keeps the binary alive
  until exit).
- Logs continue streaming via the `backend-<id>` channel. The frontend log
  heuristics (`backends.tsx:656-801`) will eventually see the python process
  die with an `ImportError` (modules vanish) or simply EOF, and the
  `process-output` listener won't trigger any status transition — it watches
  for `ERROR:` / `Traceback` / `address already in use`, not "wrapper exited
  silently" (see `backend-services.md §5.3` ⚠️ "no proactive running→error
  transition").
- The next user restart attempt fails with `"Conda executable not found"` or
  `"Failed to activate environment"` (`backends.rs:872-879`).

### 1.4 Port-time requirements

A correct port MUST:
1. Read `backends.json` before `conda env remove`. If any entry has
   `environment === envName`:
   - For `status === "running"`: call `stop_backend_service` (port-kill +
     wrapper-kill, see `backend-services.md §5.2`).
   - For all: either reject deletion with a friendly error listing them, or
     auto-rebind them to `base` and warn.
2. After deletion, sweep `backends.json` to clear `host/port/url/pid/status`
   on any matching entries.

The same logic applies to **jupyter**: `ACTIVE_JUPYTER_SERVERS` (`jupyter.rs:9`)
holds `env → (url, pid)` and v1 §11 does not say
`removeEnvironment` consults this. It doesn't. The deletion will succeed even
with a jupyter process holding files in the env directory; on macOS/Linux the
process survives until its `kill -9` (which never comes because the entry
in the static map is now orphaned from any UI control).

---

## 2. `env-extensions-cache` schema MISMATCH between consumers

v1 §1.2 documents the writer schema:
```ts
{ [env]: { extensions: Extension[]; pythonVersion: string } }
```

But `backends.tsx:2156-2168` reads `cache[name].path`:
```ts
function loadEnvironmentsFromCache(): Environment[] {
    const cached = localStorage.getItem("env-extensions-cache");
    if (!cached) return [];
    try {
        const cache = JSON.parse(cached);
        return Object.keys(cache).map((name) => ({
            name,
            path: cache[name].path || "",   // ← never written
        }));
    } catch { return []; }
}
```

`environments.tsx` writes the cache at 6 sites (`:510-535, 1071-1084,
1407-1416, 1489-1494, 1602-1608, 1325-1335`) — none write `path`. The
`Environment` type at `environments.tsx:17-21` requires `path: string`, but
the cache only stores `{extensions, pythonVersion}`. `environments.tsx`
derives `path` at read time:
```ts
path: `${installDir}/conda/envs/${name}`   // environments.tsx:470
```

`backends.tsx` therefore always gets `path: ""` for cached envs. Harmless
today because `backends.tsx` never **uses** `env.path` after the
`Environment[]` is built (only `env.name`, verified via grep). But it's a
schema lie that a TS port must either repair or pave over.

### 2.1 Port recommendation

Either:
- (a) Make `environments.tsx` write `path` into the cache so the schema
  matches its consumers, OR
- (b) Drop `path` from the `Environment` interface in `backends.tsx` (it's
  unused).

`(b)` is cheaper; the cache only needs `{extensions, pythonVersion}` to be
useful to both pages.

---

## 3. `EnvironmentCreationContext` consumers — only 2 (v1 §1.1 claim verified)

v1 §1.1 speculated "other pages / app chrome (sidebar, terminal, settings,
etc., which presumably consume `useEnvironmentCreation`)". Grep across
`desktop/src/` shows only **two** consumers:

| File | Line(s) | Role |
|---|---|---|
| `routes/__root.tsx` | 12, 32, 38 | `NavLink` reads `isCreatingEnvironment` and renders a non-clickable `<div>` when truthy, locking all 3 nav tabs |
| `routes/environments.tsx` | 11, 235, 381 | Sole writer — mirrors `isCreateModalOpen \|\| creationLoading \|\| creatingFromRequirements` into the context |

There is NO third consumer (no tray, no setup, no api-keys, no backends).
`app-shell.md §3` confirms `__root.tsx:31-49` is the only reader. The
provider lives in `RootWithProvider` (`__root.tsx:258-264`) — a single
`useState<boolean>(false)` (`EnvironmentCreationContext.tsx:24`).

**Implication for port:** the lock is purely UI sugar — it does NOT block the
backend, the Tauri tray menu, or `window.eval('window.location.href=...')`
calls from `navigate_to_page` (`main.rs:393-410`). A user clicking the tray
"Environments" menu item during create will navigate away despite the lock.

---

## 4. `list_conda_environments` YAML cleanup — no orphan consumers found

v1 §2.3 flags the destructive scan at `environments.rs:1746-1792` and warns
ports must replicate it. Cross-check across the codebase for any consumer
that depends on orphan YAMLs existing:

| Consumer | Behaviour without YAML |
|---|---|
| `get_environment_extensions_impl` (`environments.rs:1815-1846`) | Falls back to `system_settings.json["environments"][name]["extensions"]`. If that's also missing → returns `{ extensions: [] }`. Tolerant. |
| `install_extensions_impl` (`environments.rs:2549-2677`) | If YAML missing, **skips** the YAML merge silently with `log::warn!` (line 2673-2675). The packages still install successfully. |
| `update_environment_impl` (`environments.rs:2789-2790`) | **Hard-fails** with `"Environment YAML file not found for {environment}"`. After cleanup an env whose YAML was orphaned could be deleted by listing but updating fails. |
| `remove_extension_impl` (`environments.rs:2126-...`) | Parses `<envs>/<env>.yaml`. If missing, the env still gets removed via conda but YAML mutations are skipped silently. |
| `uninstall.rs:184-192` | Iterates `system_settings.json["environments"]` and deletes referenced YAMLs explicitly. Independent path; does not rely on orphans. |

**Conclusion**: the cleanup is safe — every consumer either tolerates missing
YAML or relies on a complementary mutation that already keeps YAML+conda
in sync. The cleanup IS, however, **silently destructive on first list call
after install** if the user manually placed a YAML under `~/.openbb_platform/
environments/<x>.yaml` without the corresponding conda env existing
(common pattern when copying YAMLs between machines). The port should
preserve this behaviour but log loudly.

---

## 5. Jupyter cross-doc reconciliation (logs-streaming §7 vs environments §11)

| Concern | `environments.md §11` | `logs-streaming.md §7` | Source of truth |
|---|---|---|---|
| LogStorage key | `jupyter-${envName}` (§11.3) | `jupyter-<env>` (§7.6 table) | **Consistent.** Built client-side at `environments.tsx:1918, 2032`; built server-side at `jupyter.rs:105`. |
| RunningProcesses key | "(none — port-kill instead)" (§11.5 implied) | Same — explicit "(none)" (§7.6) | **Consistent.** Jupyter NEVER calls `RunningProcesses::add_process` (verified `jupyter.rs:1-660`). The stored PID is in the static `ACTIVE_JUPYTER_SERVERS` map only. |
| Window label | `jupyter-logs-<env>` (§11.5) | `jupyter-logs-<env>` (§7.6) | **Consistent.** `jupyter.rs:587`. |
| URL extraction regexes | Three patterns + fallback (§11.3) | Three patterns + fallback (§7.2) | **Consistent in count and order** but env v1 omits the trailing-punctuation strip `.,)]}` (`jupyter.rs:27-29`). |
| Timeout | "30s" (§11.3) | "30 seconds" (§7.2) | **Consistent.** `jupyter.rs:198`. |
| Stop signal sequence | Documented as SIGTERM → 2s wait → SIGKILL (§11.4) | Same (§7.4) | **Consistent.** `jupyter.rs:391-417`. |
| Shutdown trigger string | "Shutting down on /api/shutdown request" (§11.4) | Same (§7.5) | **Consistent.** Matched at `environments.tsx:2053` and `JupyterLogsPage.tsx:213-240`. |

### 5.1 One small inconsistency (logs-streaming over-claims)

`logs-streaming.md §7.4` says `lsof -ti tcp:<port> -sTCP:LISTEN`. The Rust
code at `jupyter.rs:332-380` actually uses `lsof -ti tcp:<port>` **without**
`-sTCP:LISTEN` for Jupyter (which targets ALL connections on the port, not
just listeners) — that filter is only used by `backends.rs:380` for backend
stop. logs-streaming.md conflated the two. **Port the version-difference:**
Jupyter kills all PIDs with any TCP socket on that port; backends only kill
the listening process. This matters because a stale jupyter notebook tab
with an active connection will be in `lsof -ti` output too.

### 5.2 Process-output payload divergence

v1 §0.1 already notes Jupyter payloads include `timestamp` but env-create
payloads don't. `logs-streaming.md §3.2` adds detail: Jupyter `process-output`
emits **also lack** the `"type"` discriminator that backends emit. The
unified Jupyter payload is `{ processId, output, timestamp }` (no `type`);
backends emit `{ processId, output, timestamp, type }` (`backends.rs:997-1002`).
The "shutdown completion" event Jupyter emits at `jupyter.rs:476-482` is
the only Jupyter `process-output` that DOES include `"type": "system"`.

Port implication: don't infer Jupyter status from `type` — it's not there.

---

## 6. `execute_in_environment` — two call sites, signature parity check

v1 §12 implies the command is shared. Verified call sites:

| Site | File:line | Args sent |
|---|---|---|
| Terminal: System Shell, Python, IPython, OpenBB CLI (5 variants per OS = up to 15 sites) | `environments.tsx:819, 848, 854, 878, 907, 913, 964, 973, 1022, 1031` | `{ command, environment: "base", directory: installDir }` |
| Install Step 3: `openbb-build` | `installation-progress.tsx:1157` | `{ command: "openbb-build", environment: "openbb", directory }` |

Rust signature (`environments.rs:3235-3248`):
```rust
pub async fn execute_in_environment(
    command: String,
    environment: String,
    directory: String,
) -> Result<serde_json::Value, String>
```

**Both call sites match the 3-arg signature.** No drift. But there's a
subtle semantic divergence:

### 6.1 The two sites use the inner shell differently

For terminal sessions, the `command` is itself a full shell command like
`start cmd.exe /k "cd /d <workdir> && ..."` (Windows) or
`osascript -e '<applescript>'` (macOS). The Windows branch detects
`is_shell_command` at `environments.rs:3058-3065` and routes through a
generated `.bat` (`openbb_start_command.bat`) — see v1 §12.

For installation, `command` is just `"openbb-build"` — a plain executable.
The Windows branch then hits the **else** at `environments.rs:3162-3175`
which uses `python -c "{command}"` — meaning on Windows during install,
`openbb-build` is invoked as **`python -c "openbb-build"`**, which would
fail because `openbb-build` is not valid Python syntax.

Re-reading: `if is_shell_command` (line 3066) — `openbb-build` doesn't match
any of the `start `, `cmd.exe`, `powershell`, `bash`, `.bat`, `.sh` literals.
So on Windows install step 3 it falls to the `python -c` branch which **will
fail**. This is likely a latent Windows-install bug — but the install step
also runs `install_extensions` which itself shells out to `openbb-build`
directly (`environments.rs:2516-2533`) so the build runs successfully through
that path; the `execute_in_environment("openbb-build")` call at
`installation-progress.tsx:1157` is essentially a no-op (or error-throw) on
Windows, swallowed by the wrapping try/catch.

### 6.2 macOS/Linux behaviour for the install site

Unix uses `sh <script>` (`environments.rs:3215-3219`), and the script does
`source <conda>/bin/activate <env>` then runs the command verbatim — so
`openbb-build` resolves via PATH after activation. **Works correctly on
Unix.**

**Port-time recommendation**: drop the platform-specific `is_shell_command`
heuristic and use a unified "write a script, exec the script" path for both
call sites (Unix already does this; Windows should too). The current logic
is a footgun.

---

## 7. `update_openbb_settings` — every caller mapped

v1 only mentions this as called from installation. Full call graph:

| Caller | File:line | When |
|---|---|---|
| Frontend: install Step 3 success | `installation-progress.tsx:1162-1165` | After `install_extensions` + `execute_in_environment("openbb-build")` |
| Frontend: install completion `handleContinue` | `installation-progress.tsx:1207-1210` | When user clicks "Done" on success modal — **re-sync** |
| Rust: `setup_python_environment` (Step 2) | `startup.rs:1293` | After `conda env create -f <yaml>`; errors are logged-only |

Total: 3 callers. Two from `installation-progress.tsx`, one Rust-internal.
**No call from `environments.tsx`** — the Environments page never invokes
`update_openbb_settings`. So after creating a new env (or installing
extensions into an existing one), `~/.openbb_platform/{user,system}_settings.json`
is NOT re-synchronised against the env's installed `openbb-core`. This is
intentional: the wizard's `update_openbb_settings` is what writes the initial
defaults (api_settings, python_settings) by importing the env's
`openbb_core.app.service.system_service.SystemService`. Once that exists,
subsequent env mutations don't replay it. Side effect: if a user creates a
new env with a NEWER `openbb-core` that adds new system_settings keys, those
defaults won't appear in `system_settings.json` until either the user
re-installs or a hand-edit is done.

### 7.1 Signature parity

Rust (`helpers.rs:950-955`):
```rust
pub async fn update_openbb_settings(
    conda_dir: &Path,
    environment: &str,
)
```

Tauri auto-converts JS `condaDir` → Rust `conda_dir`. Both JS callers send
`{ condaDir: directory, environment: "openbb" }` where `directory` is the
**install dir** (e.g. `~/OpenBB`), NOT the `<install>/conda` subdir. The Rust
`update_openbb_settings_impl` (`helpers.rs:687-698`) detects this:
```rust
let conda_dir = if conda_dir.file_name() == Some(OsStr::new("conda")) {
    conda_dir.to_path_buf()
} else {
    conda_dir.join("conda")
};
```

So the param is tolerantly named; callers can pass either form. Port note:
this normalisation is load-bearing.

---

## 8. Concurrent operation matrix (v1 §6.5 expansion)

v1 §6.5 notes "NO backend serialization" for `install_extensions`. The full
matrix of concurrent operations on the same env:

| Op A | Op B | Same env | Failure mode | Has frontend guard? |
|---|---|---|---|---|
| `install_extensions` | `install_extensions` | yes | YAML rewrite race (conda lockfile protects data) | `installExtensionsLoading` button-disable only (`environments.tsx:282`) |
| `install_extensions` | `remove_extension` | yes | Same | Same |
| `install_extensions` | `update_extension` | yes | YAML inconsistent | Same |
| `install_extensions` | `update_environment` | yes | conda holds its own lock at `<env>/conda-meta/`; one will block | `isUpdatingEnvironment` set (`environments.tsx:782-805`) but doesn't disable extension buttons |
| `create_environment` | `install_extensions` (on **different** env) | no | independent | none needed |
| `create_environment` | `install_extensions` on the **about-to-be-created** env | yes-ish | `install_extensions` would error: "Environment '{name}' does not exist - Python executable not found at:..." (`environments.rs:2379-2383`) | None — relies on the env not being in `environments` state yet |
| `create_environment` | `create_environment` same name | YES (CRITICAL) | Second one's "env already exists" block (`environments.rs:194-222`) runs `conda env remove` while the first is still mid-`env update --prune`. Catastrophic. | `envCreatedRef.current` flag (`environments.tsx:2199-2204`) — single-flight only for the current page mount. A second window would not be guarded. |
| `remove_environment` | `install_extensions` same env | yes | `install_extensions` fails after env vanishes mid-install | `deletedEnvironments.current` ref (`environments.tsx:1311`) blocks subsequent UI requests, but NOT in-flight `install_extensions` |
| Two browser tabs / two log windows | any | yes | No cross-window mutex. Tauri single-instance plugin (`main.rs:476-481`) prevents 2 app processes, BUT the Tauri webview windows (jupyter-logs, backend-logs, url_*) all share the same JS context per window — they don't share state | None |

**Port recommendation:** add a per-env coarse-grained lock at the IPC handler
level (not the JS level). Either a `Mutex<HashSet<EnvName>>` of busy envs that
the handlers acquire/release with a "busy, try later" error, or a
`tokio::sync::Mutex` per env. The current setup leaks race-windows because
the JS guards are window-local and the Rust handlers are stateless.

---

## 9. Timeouts that v1 missed or under-described

v1 §10.3 mentions a single 5-minute timeout on `update_environment` conda
install. Full catalog of timeouts in the env subsystem:

| Timeout | Location | What it guards | Behaviour on breach |
|---|---|---|---|
| 5 min on `conda install` | `environments.rs:2876` | `update_environment_impl` conda step | `child.kill() + child.wait()`; returns `(None, "", "Timed out")`; **continues to pip step anyway** (no early-return — see `:2922-2935`) |
| ~~5 min on pip in `update_environment`~~ | **NONE** | `update_environment_impl` pip step uses straight `.output()` (`:2971-2975`) | Hangs indefinitely if pip stalls |
| 30s on Jupyter URL extraction | `jupyter.rs:198` | `start_jupyter_server` waiting for URL | `process.kill()`; returns `Err` |
| 2s after SIGTERM before SIGKILL | `jupyter.rs:404` | Jupyter stop, per PID | `kill -9` |
| 2s after spawn before deleting temp `.bat` | `environments.rs:3136` | Windows `execute_in_environment` "start" branch | File rm |
| 30s "starting → error" | `environments.tsx:1832` | UI polling; if `jupyterStatus[env] === "starting"` and `Date.now() - startTime > 30000` | UI flips to `error` |
| 3s status polling interval | `environments.tsx:1862` | `setInterval(checkStatus, 3000)` for `check_jupyter_server` | self-clears when no envs need polling |
| 5min sessionStorage cleanup | `environments.tsx:799` | `setTimeout(...)` 300000 ms to remove stale `updating-env-<name>` flag | Clears `setIsUpdatingEnvironment` |
| 5s create-cancel grace | `environments.tsx:1712-1720` | After `handleAbortInstallation`, modal stays for 5s | Modal closes; backend continues |
| 500ms directory validation debounce | `environments.tsx:358` | `check_directory_exists` for working-directory input | Re-validates |
| 45s URL confirmation failsafe | `backends.tsx:804-813` | NOT env-specific but documented in `backend-services.md §5.3.3` | Hides spinner, status stays `running` |

**Critical port note:** the pip-install in `update_environment` has NO
timeout. PyPI / corporate proxy / network stalls can hang the entire UI's
update flow. Port should add a matching 5-min timeout (or longer) using
`tokio::time::timeout` around the pip step.

---

## 10. Why each race-condition ref exists (v1 §1 expansion)

v1 lists the refs without explaining the bugs. After studying the call graph:

### 10.1 `deletedEnvironments: Set<string>` (`environments.tsx:375`)

**Bug class fixed**: TOCTOU between `remove_environment` invoke and several
parallel sources of `setEnvironments`:
- `fetchEnvironments` (mount + retry button)
- `updateCacheAfterBackendOperation` (called after EVERY mutation)
- The 3s `setInterval` jupyter status poll re-reads `environments` array
- Concurrent `create_environment` for the same env name (rare but possible
  from clicking the modal's submit twice fast)

Without this set, the user sees the env disappear from the UI, then 200ms
later it re-appears (re-fetch raced ahead of the conda removal), then
disappears again. The set is added BEFORE the invoke (`:1311`) so all
intermediate fetches filter it out (`:421-426, 502-503, 2384`). It's
removed AFTER `updateCacheAfterBackendOperation` (`:1350`) so the cache
write doesn't accidentally re-add it.

### 10.2 `envCreatedRef: boolean` (`environments.tsx:376`)

**Bug class fixed**: double-fire of `safeCreateEnvironment` from the modal's
submit button. React 18 strict-mode double-invokes effects, AND
`onInstallExtensions` (`InstallComponents.tsx`) could be triggered by a fast
Enter+click combo. The ref ensures the second call is a no-op
(`:2200-2204`). It's reset on `cancel`, `success`, and `error` (`:1626,
1662, 1669`).

### 10.3 `creationWarningRef: string | null` (`environments.tsx:320`)

**Bug class fixed**: state updates after modal close. If extension install
emits a warning AFTER the user closed the create modal, calling
`setCreationWarning` won't show anything (the warning UI only renders inside
the modal scope). The ref stashes the warning, and a separate effect
(`:2206-2211`) reads it once the modal is gone and re-injects via
`setCreationWarning`. This separates "warning origin time" from "warning
display time".

### 10.4 `createEnvironmentRef: (exts?) => Promise<void>` (`environments.tsx:377`)

**Bug class fixed**: stale closure capture. `safeCreateEnvironment` is a
`useCallback` with `[]` deps; without the ref it would close over the
initial `createEnvironment` and never see updated React state (`installDir`,
`newEnvName`, etc.). The ref is refreshed every render (`:2195-2197`).
Same trick as React's escape hatch for "I need the latest version of this
callback inside a stable identity".

### 10.5 `hasLoadedEnvironments: boolean` (`environments.tsx:403`)

**Bug class fixed**: cache-vs-fetch race on mount. `loadEnvironmentsFromCache`
(`:451-486`) sets it to `true` after seeding cached envs; `fetchEnvironments`
(`:406-434`) checks it (`:411`) to decide whether to "loading=true" the UI
or seed the cache first. Without it the screen flashes blank between cache
load and fetch.

### 10.6 `jupyterUrlRef: { [env]: string | null }` (`environments.tsx:300`)

Used instead of state to avoid re-rendering the entire env grid every time
the polling effect updates a single URL. The URL is read at button-click
time (`:1901-1903`), so freshness on click is what matters, not on every
poll tick.

### 10.7 `activeServers: Set<string>` (`environments.tsx:301`)

Used for the unmount snapshot to sessionStorage (`:1873-1887`). Maintained
in parallel to `jupyterStatus['running']` so unmount can serialize quickly
without iterating the status object.

---

## 11. What `openbb-build` actually does (v1 §6.4 / §10.3 expansion)

v1 references `openbb-build` 4 times without explaining the action. From
`openbb_platform/core/pyproject.toml:31`:
```toml
openbb-build = "openbb_core.build:main"
```

Implementation: `openbb_platform/core/openbb_core/build.py`:
1. Runs `<sys.executable> -c "import openbb"` as a subprocess (line 23-28).
   This triggers the auto-build path inside `openbb_core.app.static.package_builder.PackageBuilder.auto_build()`
   (`package_builder.py:151-169`) which compares installed extensions
   against `<openbb_pkg>/assets/reference.json` and rebuilds the static
   `openbb.<extension>` module tree if they differ.
2. If `"Building"` was NOT found in the subprocess stdout, it imports
   `openbb` directly and calls `openbb.build()` (line 56-62).
3. The actual rebuild work in `PackageBuilder.build()`
   (`package_builder.py:171-230`):
   - Acquires an exclusive flock on `<openbb>/static/.build.lock`
     (`package_builder.py:179-183`) — concurrent `openbb-build` calls fail
     with `BlockingIOError → RuntimeError("Another build process is running
     and has locked <path>")`.
   - Wipes `<openbb>/static/assets/` and `<openbb>/static/package/`
     (`_clean`, `:232-241`).
   - Re-generates a Python module per `path_list` entry by introspecting
     every registered router's command signature and synthesising matching
     code (`ModuleBuilder.build`, ~3000 lines deep in the file).
   - Writes a `reference.json` snapshot of installed extensions.
   - Optionally runs `black` + `ruff` (`_run_linters`, `:312-317`).

**Why it matters for the port:**
1. **It's not a server-start.** `openbb-build` is purely a codegen step that
   produces the `from openbb import obb` Python SDK. It is REQUIRED before
   anyone can `import openbb` in user code; the API server (`openbb-api`)
   does NOT need it (the API server hits the FastAPI app directly via
   `openbb_platform_api.main:app` — see `platform-rest-api.md §1`).
2. **It's idempotent and self-locked.** Two concurrent calls won't corrupt
   state — the second errors out. The desktop already calls it twice during
   install (once inside `install_extensions_impl` if `openbb` was selected
   as an extension, once via `execute_in_environment` in
   `installation-progress.tsx:1157`); both will succeed because the second
   call's `auto_build()` sees the up-to-date reference.json and is a no-op.
3. **It can take 30-90 seconds** on a fully-loaded extension install. Port's
   UX must keep a spinner/progress through this — the current frontend just
   waits for the invoke to resolve with no streaming.
4. **For the TS port (Strategy A / B)** — if you keep Python (B), call this
   verbatim post-install. If you reimplement in TS (A), you must reproduce
   the `assets/reference.json` artifact OR forgo `import openbb` in user
   code entirely. There is no shortcut.

---

## 12. `new_conda_command` env var clearing — the WHY (v1 §0.3)

v1 documents the three vars are unset (`CONDA_DEFAULT_ENV`, `CONDA_PREFIX`,
`CONDA_SHLVL`) but doesn't explain why. The reason is environment
**leakage from the parent process**:

1. **When the desktop app is launched from a terminal that already had a
   conda env activated**, those three vars are inherited via `std::process::
   Command`'s default env-inheritance.
2. Conda's activation scripts check them at start-up — if `CONDA_SHLVL > 0`
   they refuse to activate again at the same level or unstack the wrong
   env on deactivate. Concretely, `conda activate openbb` will read
   `CONDA_PREFIX` as the "currently active env" and reverse-engineer the
   stack from there.
3. The desktop runs `<conda>/bin/conda env update -n openbb -f ...` from
   a process where `CONDA_PREFIX=/Users/me/miniconda3/envs/some-other-env`
   would cause conda to write `<some-other-env>/conda-meta/history` entries
   for the openbb env's installs. Cross-env pollution.
4. macOS-specific: the GUI-launched app gets `$PATH` patched by
   `fix_path_env::fix()` (`main.rs:470`) which slurps the user's shell init;
   if that init does `conda activate <env>` then those three vars are now
   in `std::env` for the lifetime of the app.

**Port implication:** any TS port using Node `child_process.spawn` must
explicitly do:
```ts
const env = { ...process.env };
delete env.CONDA_DEFAULT_ENV;
delete env.CONDA_PREFIX;
delete env.CONDA_SHLVL;
env.CONDA_ROOT = condaDir;
env.CONDA_ENVS_PATH = path.join(condaDir, 'envs');
env.CONDA_PKGS_DIRS = path.join(condaDir, 'pkgs');
env.CONDARC = path.join(condaDir, '.condarc');
spawn(condaExe, args, { env });
```

The same applies to `backend-services.md §5.1` step 7 (backend start script)
and `installation.md §4`'s `update_openbb_settings` script — all three sites
unset the same three vars in the shell prelude. Verified:
- `helpers.rs:161-163` — `new_conda_command` (`unset` via `env_remove`)
- `helpers.rs:863-865, 899-901` — `update_openbb_settings_impl` bash script
- `backends.rs:840-842 (Win), 880-883 (Unix)` — `start_backend_service_impl` script
- `environments.rs:3084-3086 (Win), 3187-3189 (Unix)` — `execute_in_environment_impl`

Six separate inline copies. Port should centralise this in one helper.

---

## 13. Smart retry loop in `create_environment` — third retry semantics (v1 §3.3 gap)

v1 §3.3 documents the regex set but doesn't answer:
- **Max retry count?** None. The loop at `environments.rs:305-401` is
  `loop { ... break on success; abort on no-progress; else retry }`. As long
  as each iteration removes at least one package, it will keep going.
- **Bound on iterations?** Implicit: each iteration removes ≥1 entry from
  `conda_packages` or `pip_packages`. Worst case = total package count.
  For the default openbb install (~30 packages), bound is ~30 iterations.
  No `Instant` timeout — could plausibly take 5-15 minutes if each retry
  redoes the full `conda env update --prune` round-trip (~30s).
- **What if removal makes the env unusable?** The loop only removes
  packages from the YAML-generation lists, not from the already-created
  conda env. After the first iteration the env exists with Python
  installed but no extra packages; the loop body re-runs
  `conda env update -n <name> -f <yaml> --prune`. The `--prune` flag will
  **remove** packages from the env that are no longer in the YAML — so if
  the retry strips `numpy` because of UnsatisfiableError, then strips
  `pandas` because pandas depends on numpy and its version constraint
  cascades, then strips `openbb` because it depends on pandas... the user
  ends up with a Python-only env that "succeeds" but has nothing useful in
  it. The frontend treats this as success (`environments.tsx:1626` →
  `envCreatedRef.current = false; setCreationLoading(false);`) and writes
  the (empty) extension cache.
- **Port-time recommendation**: cap iterations to `min(8, len(packages))`
  and surface "could not resolve dependencies; environment created without
  packages X, Y, Z" as a warning instead of silent success.

The `--prune` flag's destructive interaction with the retry loop is the
most subtle correctness issue in the entire env subsystem — a port that
omits `--prune` to "make it safer" will instead leak old packages between
retries.

---

## 14. Cross-doc dependency corrections

v1's "Cross-feature dependencies" footer lists:
- `depended-on-by feature-backend-services.md`

This understates the coupling. After cross-checking, the true relationship is
**shares-state-with** (bidirectional), because:
1. Backends READ `list_conda_environments` (`backends.tsx:2377`) and
   `env-extensions-cache` localStorage (`backends.tsx:2156-2168`) — pure read.
2. Backends WRITE NOTHING to env state. ✓ (one-way)
3. Environments NEVER consult `backends.json` (verified §1 above) — but they
   SHOULD, on `remove_environment`. This is a **missing edge** the port
   must add.

Also missing from v1:
- **shares-state-with `feature-platform-rest-api.md`** beyond the API
  package. Specifically:
  - Both `~/.openbb_platform/system_settings.json` AND `user_settings.json`
    are read by `openbb-api` at every request (`platform-rest-api.md §4d`,
    `§8b`). The Environments page never writes these. The credential side
    is `feature-api-keys.md`'s job.
  - Re-syncing settings on env update is a hole: see §7 above.

---

## v2 → v1 corrections

| v1 claim | Correct version |
|---|---|
| §0.4 / §11.3 — Jupyter port "not chosen by us — Jupyter picks" | Correct, but v1 missed that `start_jupyter_server` does NOT bind/check the port; only `jupyter lab` decides. The port the desktop "remembers" is parsed from the URL string (`jupyter.rs:496-531`), so a Jupyter `lab` that prints a different URL than it actually binds (rare bug in older versions) would mis-track. |
| §2.3 — "Note: TS handler signature in main.rs binds only `name` — but the frontend ALSO sends `directory`" (`remove_environment`) | Verified at `environments.rs:2754-2755` — `remove_environment(name: String)`. Frontend `environments.tsx:1316-1319` does send `directory`. v1 correctly flags this. **No correction needed**; flagging for emphasis. |
| §6.3 — "directory is ignored" for `install_extensions` | Verified — Rust signature is `install_extensions(environment, extensions)` (`environments.rs:2681-2687`). Frontend sends `directory` from THREE sites (`environments.tsx:1378, 1577`; `installation-progress.tsx:1155`). All three ignored. v1 correctly flags. |
| §10.3 — "5-minute timeout" implies BOTH conda and pip are bounded | **WRONG**. Only the conda step (`environments.rs:2864-2935`) has the 5-min `tokio::task::spawn_blocking` timeout. The pip step (`environments.rs:2971-2975`) uses straight `.output()` with no timeout. Port must add one. |
| §1.1 — "It exists so other pages / app chrome (sidebar, terminal, settings, etc., which presumably consume `useEnvironmentCreation`)" | **OVERSPECIFIED**. Only `__root.tsx` consumes it (single `NavLink` component). No sidebar, no terminal, no settings page reads it. v1's speculation is unsupported. |
| §6.4 — "After install: reads existing `<env>.yaml`, extracts python version + existing conda/pip packages, merges new packages... rewrites YAML" | Correct, but v1 misses that `install_extensions_impl` does **NOT** read `system_settings.json["environments"][name]["extensions"]` as a YAML fallback the way `get_environment_extensions_impl` does (`environments.rs:1839-1843`). So if the YAML is missing AND `install_extensions` is called, the install succeeds but no YAML is written (silent `log::warn!` at `:2673-2675`); subsequent `update_environment` then fails with "Environment YAML file not found". A two-step inconsistency. |
| §11.5 (`open_jupyter_logs_window`) "on close it `hide()` instead of destroying" | Correct; matches `logs-streaming.md §6.2`. v1's description is accurate. |
| §15 translation table — `install_extensions` row says "`POST /environments/:env/extensions`" | Reasonable, but doesn't mention that the port also needs an idempotent retry mechanism because the Rust handler is itself non-idempotent (conda's lockfile is, but the YAML merge writes are last-writer-wins). |
| §15 translation table — does not include `openbb-build` as a step in `install_extensions` | The Rust handler DOES invoke `openbb-build` internally when `extensions` contains `"openbb"` (`environments.rs:2516-2533`). Port must replicate or the static SDK won't regenerate. |
| Top of doc — "Scope: TS files under `desktop/src/...`, Rust handlers..." | v1 implicitly treats `helpers.rs` as out-of-scope but several env operations (`get_environments_directory_impl`, `save_environment_as_yaml_impl`, `update_openbb_settings_impl`, `new_conda_command`) live there. v2 made them explicit in §6, §7, §12. |

