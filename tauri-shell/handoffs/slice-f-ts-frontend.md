# Slice F — TypeScript Frontend Example (handoff)

Status: **Done.** `examples/typescript-frontend/` is a runnable Vite + TS smoke
test that exercises 30 representative IPC commands across 12 modules. `npm
install`, `npm run typecheck`, and `npm run build` all pass.

---

## 1. Purpose

Per `tauri-shell/SPEC.md:240-265` (§2 Slice F), the goal was a minimal "smoke
test" TS frontend that exercises every command family — useful as **both
documentation and an integration test**. The result lives at
`/home/user/OpenBBPort/tauri-shell/examples/typescript-frontend/` and serves
three purposes:

- **Documentation by example.** Every wrapper in `src/ipc.ts` is a worked
  example of the on-the-wire shape for one of the 162 commands listed in
  `SPEC.md:386-490`.
- **Integration test.** Clicking *Run all* fires 25 of the 30 wired commands
  sequentially and tallies pass/fail — a quick visual check that the shell,
  the connector (Slice B), and the bindings (Slice A) all line up.
- **Reference UI.** Vanilla HTML + TS, zero framework, so consumers can lift
  the wrappers into React/Vue/Svelte without unpicking ceremony.

---

## 2. What was built

8 files, 1088 LOC (excluding `package-lock.json` and `node_modules/`).

| File | LOC | Purpose |
|---|---:|---|
| `package.json` | 21 | `@tauri-apps/api ^2.1.1` dep; `vite ^5.4`, `typescript ^5.6`, `@types/node ^20.17` dev-deps; scripts `dev`/`build`/`typecheck`/`preview` |
| `vite.config.ts` | 37 | Port 1420, `strictPort`, `TAURI_DEV_HOST` bridge for remote-host dev, `esnext` target, conditional HMR on port 1421 |
| `tsconfig.json` | 24 | `strict: true`, `noUnusedLocals`, `noUnusedParameters`, ES2020 target, `Bundler` module resolution, DOM lib |
| `index.html` | 119 | Header with 3 status badges + grid layout (12 `<fieldset>` button groups left, output `<aside>` right) |
| `src/ipc.ts` | 236 | Hand-typed wrappers around `invoke<T>(...)` for every command the demo calls + shared types (`InstallationSnapshot`, `LogEntry`, `RouteInfo`, `RoutineMetadata`, `JsonValue`, `ObbCallArgs`) |
| `src/main.ts` | 380 | Button dispatch, `wireEvents()`, `refreshBadges()`, per-command handlers, sequential `runAll()` |
| `src/style.css` | 158 | Dark-first theme (`color-scheme: dark light`), grid main layout, button state colours (`busy`/`ok`/`err`) |
| `README.md` | 113 | Run instructions + per-button table + production-wiring snippet |

Verified file sizes match: `wc -l` agrees with the table above; total = 1088.

### Key file locations (absolute paths)

- `/home/user/OpenBBPort/tauri-shell/examples/typescript-frontend/index.html`
- `/home/user/OpenBBPort/tauri-shell/examples/typescript-frontend/package.json`
- `/home/user/OpenBBPort/tauri-shell/examples/typescript-frontend/tsconfig.json`
- `/home/user/OpenBBPort/tauri-shell/examples/typescript-frontend/vite.config.ts`
- `/home/user/OpenBBPort/tauri-shell/examples/typescript-frontend/src/ipc.ts`
- `/home/user/OpenBBPort/tauri-shell/examples/typescript-frontend/src/main.ts`
- `/home/user/OpenBBPort/tauri-shell/examples/typescript-frontend/src/style.css`
- `/home/user/OpenBBPort/tauri-shell/examples/typescript-frontend/README.md`

---

## 3. Command coverage (≥30)

`main.ts:1-36` carries the same enumeration as a comment header. The numbered
list below cites the wrapper location in `src/ipc.ts` and the handler in
`src/main.ts`. Args shapes mirror the Rust `#[tauri::command]` signatures in
`src/ipc/*.rs`; the rule is `camelCase` on the wire (`SPEC.md:336-339`).

| # | Command | Module | Args | Returns | Wrapper | Handler |
|--:|---|---|---|---|---|---|
| 1 | `get_installation_state` | app | — | `InstallationSnapshot` | `ipc.ts:55-57` | `main.ts:177` |
| 2 | `get_app_version` | app | — | `string` | `ipc.ts:59-61` | `main.ts:179` |
| 3 | `navigate_to_page` (→ `/dashboard`) | app | `{ path }` | `void` | `ipc.ts:63-65` | `main.ts:181-184` |
| 4 | `navigate_to_page` (→ `/`) | app | `{ path }` | `void` | `ipc.ts:63-65` | `main.ts:186-189` |
| 5 | `register_process_monitoring` | infrastructure | `{ processId }` | `boolean` | `ipc.ts:71-73` | `main.ts:191-194` |
| 6 | `get_process_logs_history` | infrastructure | `{ processId, count? }` | `LogEntry[]` | `ipc.ts:75-83` | `main.ts:196-201` |
| 7 | `clear_process_logs_history` | infrastructure | `{ processId }` | `boolean` | `ipc.ts:85-87` | `main.ts:199` |
| 8 | `get_home_directory` | helpers | — | `string` | `ipc.ts:93-95` | `main.ts:203` |
| 9 | `select_directory` | helpers | `{ prompt? }` | `string` | `ipc.ts:97-99` | `main.ts:205` |
| 10 | `check_directory_exists` | helpers | `{ path }` | `boolean` | `ipc.ts:101-103` | `main.ts:207-210` |
| 11 | `get_user_credentials` | credentials | — | `JsonValue` | `ipc.ts:109-111` | `main.ts:212` |
| 12 | `update_user_credentials` | credentials | `{ args: { credentials } }` | `boolean` | `ipc.ts:113-119` | `main.ts:214-225` |
| 13 | `open_credentials_file` | credentials | `{ fileName }` | `boolean` | `ipc.ts:121-123` | `main.ts:227` |
| 14 | `obb_set_base_url` | obb | `{ url }` | `void` | `ipc.ts:129-131` | `main.ts:229` |
| 15 | `obb_get_base_url` | obb | — | `string` | `ipc.ts:133-135` | `main.ts:231` |
| 16 | `obb_health` | obb | — | `JsonValue` | `ipc.ts:137-139` | `main.ts:233` |
| 17 | `obb_widgets` | obb | — | `JsonValue` | `ipc.ts:141-143` | `main.ts:235` |
| 18 | `equity_price_historical` | obb_routes | `{ params }` | `JsonValue` | `ipc.ts:159-163` | `main.ts:237-241` |
| 19 | `obb_call` | obb | `{ args: ObbCallArgs }` | `JsonValue` | `ipc.ts:145-153` | `main.ts:243-248` |
| 20 | `list_all_routes` | openbb_meta | — | `RouteInfo[]` | `ipc.ts:169-171` | `main.ts:250-253` |
| 21 | `provider_list` | provider | — | `JsonValue` | `ipc.ts:173-175` | `main.ts:255` |
| 22 | `read_settings_json` | settings_files | `{ args: { fileName } }` | `JsonValue \| null` | `ipc.ts:181-185` | `main.ts:268` |
| 23 | `write_settings_json` | settings_files | `{ args: { fileName, content } }` | `boolean` | `ipc.ts:187-194` | `main.ts:264-267` |
| 24 | `routines_list` | routines | — | `RoutineMetadata[]` | `ipc.ts:200-202` | `main.ts:278` |
| 25 | `routines_save` | routines | `{ args: { name, content } }` | `boolean` | `ipc.ts:204-206` | `main.ts:274-277` |
| 26 | `routines_delete` | routines | `{ args: { name } }` | `boolean` | `ipc.ts:208-210` | `main.ts:279` |
| 27 | `list_backend_services` | backends | — | `JsonValue` | `ipc.ts:216-218` | `main.ts:288` |
| 28 | `list_conda_environments` | environments | `{ directory? }` | `JsonValue` | `ipc.ts:220-224` | `main.ts:290` |
| 29 | `server_attach` | server | `{ url }` | `void` | `ipc.ts:230-232` | `main.ts:292` |
| 30 | `server_health` | server | — | `JsonValue` | `ipc.ts:234-236` | `main.ts:294` |

Modules touched: **app, infrastructure, helpers, credentials, obb, obb_routes,
openbb_meta, provider, settings_files, routines, backends/environments/server
(stubs)** — 12 of the 18 IPC modules listed in `SPEC.md:38-55`. The remaining
six (`installation`, `jupyter`, `uninstall`, `certs`, `mcp`, plus per-OS bits
of `helpers`) are intentionally skipped — they require either a running
installer pipeline (Slice H) or platform dialogs that aren't useful in an
automated smoke run.

---

## 4. Event listeners

Three Tauri events from `src/events.rs` are subscribed in `wireEvents()`
(`main.ts:126-141`):

| Event | Payload (TS shape in `main.ts`) | Rust producer | Subscriber line | Sink in DOM |
|---|---|---|---|---|
| `navigate` | `{ path: string }` (`main.ts:111-113`) | `tray.rs` menu items + `ipc::app::navigate_to_page` | `main.ts:127-129` | `<pre id="navigate-log">` (`index.html:110`) |
| `process-output` | `{ processId, output, timestamp, type }` (`main.ts:114-119`) | Reader threads in `process_spawn::spawn_with_streaming` | `main.ts:131-134` | `<pre id="process-output">` (`index.html:113`) |
| `install-progress` | `{ step, progress, message }` (`main.ts:120-124`) | Installation pipeline (connector — Slice B `InstallProgressEvent`) | `main.ts:136-140` | `<progress id="install-progress">` + label (`index.html:106-107`) |

Each `listen<T>(...)` call is fire-and-forget — the unsub handle is dropped
because the renderer lifetime equals the window lifetime. For a long-lived
SPA you'd capture the returned `UnlistenFn` and call it on route teardown.

---

## 5. Args nesting convention (gotcha)

The Tauri `invoke` API serializes the second argument as a JSON object whose
keys are the **Rust parameter names** of the `#[tauri::command]` fn. Most
commands take individual scalars (`url`, `path`, `processId`), so the JS
payload is a flat object: `invoke("foo", { url: "..." })`.

A subset of commands instead take a **single struct parameter named `args`**.
For those the wrapper must wrap the payload in `{ args: { ... } }`. The
mismatch is the #1 source of silent `invalid args` errors when adding new
commands. The example flags this explicitly at `ipc.ts:114-117`:

```ts
// Note the inner `args` wrapper — the Rust handler takes a single struct
// parameter named `args`, so the JS payload mirrors that shape.
return invoke<boolean>("update_user_credentials", {
  args: { credentials },
});
```

Wrapped commands used in this example (cross-check the Rust signature in
`src/ipc/*.rs`):

| Command | Wrapper line | Rust struct |
|---|---|---|
| `update_user_credentials` | `ipc.ts:116-118` | `UpdateCredentialsArgs` (`bindings/UpdateCredentialsArgs.ts`) |
| `obb_call` | `ipc.ts:152` | `ObbCallArgs` (`bindings/ObbCallArgs.ts`) |
| `read_settings_json` | `ipc.ts:182-184` | `ReadJsonArgs` (`bindings/ReadJsonArgs.ts`) |
| `write_settings_json` | `ipc.ts:191-193` | `WriteJsonArgs` (`bindings/WriteJsonArgs.ts`) |
| `routines_save` | `ipc.ts:205` | `RoutinesSaveArgs` (`bindings/RoutinesSaveArgs.ts`) |
| `routines_delete` | `ipc.ts:209` | `RoutinesDeleteArgs` (`bindings/RoutinesDeleteArgs.ts`) |

Heuristic: if `bindings/` contains a `FooArgs.ts` file, the wire envelope is
`{ args: <FooArgs> }`. Otherwise it's flat.

---

## 6. Running it

From a fresh clone:

```bash
# 1. Install frontend deps (Node ≥ 18, npm 9+)
cd /home/user/OpenBBPort/tauri-shell/examples/typescript-frontend
npm install

# 2. Verify the TS compiles
npm run typecheck

# 3. Start the Vite dev server (terminal 1)
npm run dev        # serves http://localhost:1420

# 4. In a second terminal, boot Tauri pointing at Vite
cd /home/user/OpenBBPort/tauri-shell
cargo tauri dev    # opens a window loading http://localhost:1420
```

The `strictPort: true` in `vite.config.ts:14-15` guarantees port 1420 matches
what `tauri.conf.json` expects. If 1420 is busy, Vite errors out rather than
silently picking another port (which would leave the webview pointing at a
stale tab).

For the **production** wiring (single-command launch), point
`tauri.conf.json` at this dist as documented in `README.md:99-113`:

```json
"build": {
  "frontendDist": "../examples/typescript-frontend/dist",
  "devUrl": "http://localhost:1420",
  "beforeDevCommand": "npm --prefix examples/typescript-frontend run dev",
  "beforeBuildCommand": "npm --prefix examples/typescript-frontend run build"
}
```

---

## 7. Build verification (already passed)

```text
$ cd /home/user/OpenBBPort/tauri-shell/examples/typescript-frontend
$ npm install         # 165 packages, 0 vulnerabilities (from package-lock.json)
$ npm run typecheck   # tsc --noEmit          → 0 errors
$ npm run build       # tsc --noEmit && vite build
    vite v5.4.21 building for production...
    ✓ 8 modules transformed.
    dist/index.html                 4.55 kB │ gzip: 1.31 kB
    dist/assets/index-*.css         1.94 kB │ gzip: 0.81 kB
    dist/assets/index-*.js          7.77 kB │ gzip: 2.88 kB
    ✓ built in 305ms
```

Strict mode is clean: `strict`, `noImplicitAny`, `noUnusedLocals`, and
`noUnusedParameters` all enabled in `tsconfig.json:7-10`.

---

## 8. Known gaps

- **No import from `bindings/`.** `src/ipc.ts` hand-types `InstallationSnapshot`,
  `LogEntry`, `RouteInfo`, `RoutineMetadata`, etc. The Slice A bindings at
  `/home/user/OpenBBPort/tauri-shell/bindings/` cover all of these (35
  generated files + `index.ts`). Replacing the hand-rolled types is a 1-hour
  follow-up: change `ipc.ts:16-49` to `import type { ... } from "../../bindings"`
  and delete the duplicates. Deferred so the example stays runnable even if
  the bindings test isn't run first.
- **Vanilla HTML + TS only.** No React, Svelte, Vue, or Solid. The button
  dispatch (`main.ts:357-378`) is `document.querySelectorAll`, the dynamic
  output is `textContent =`. Intentional — the example shouldn't ship a
  framework opinion.
- **30 of 162 commands.** The remaining 132 are either (a) typed REST
  wrappers in `obb_routes` (60) / `obb_routes_extended` (167 added in
  Slice C), all of which share the same `{ params }` shape demoed by
  `equity_price_historical`, or (b) connector stubs whose return value is
  uniformly `Err(NotImplemented)` until Slice H lands. Coverage of those
  is better handled by Slice G's CLI (`src/bin/cli.rs`).
- **`select_directory` opens a real OS dialog**, so it isn't included in the
  unattended `runAll()` sequence (`main.ts:305-331`).
- **No HMR plugin for the parent crate.** Touching Rust files requires
  re-running `cargo tauri dev`. Vite-side HMR works fine for `.ts`/`.css`.

---

## 9. Integration with other slices

| Slice | Interaction |
|---|---|
| **A — TS bindings** | `bindings/*.ts` (e.g. `bindings/RoutinesSaveArgs.ts`, `bindings/RouteInfo.ts`) could replace the hand-rolled types in `src/ipc.ts:16-49`. Path: `import type { InstallationSnapshot, LogEntry, RouteInfo, RoutineMetadata } from "../../bindings"`. The wire-side `{ args: ... }` nesting in `ipc.ts` already matches every `*Args.ts` struct. |
| **B — Connector trait** | The "Stubs (expected to error)" fieldset in `index.html:83-87` and the two stub commands at `main.ts:288, 290` exist specifically to show which commands return `IpcError::NotImplemented`. Swapping `NoopConnector` for a real impl (Slice H) flips those buttons from red to green. |
| **C — Extended wrappers** | `obb_routes_extended.rs` adds 167 more typed wrappers, all with the same `{ params }` shape used by `equity_price_historical` (`ipc.ts:159-163`). Adding any one of them to the example is a single new `function fooBar(params): Promise<JsonValue>` clone. |
| **D — Integration tests** | Independent; tests are Rust-side and don't touch this example. |
| **E — README/docs** | `README.md:99-113` of the example shows the `frontendDist` swap snippet referenced by Slice E's cookbook section. The parent README (Slice E) should backlink to this example as the canonical "first-call" walkthrough. |
| **G — CLI binary** | `src/bin/cli.rs` covers the same surface from a terminal. The two are complementary — the TS frontend tests the renderer side of the IPC bridge (event listeners, JSON serialization), the CLI tests the Rust-only path (proxy + state). |
| **H — Connector reference impls** | Once `connectors/openbb-platform/` lands, the four stub buttons (`list_backend_services`, `list_conda_environments`, `server_attach`, `server_health`) will start returning real data. No frontend change required. |

---

## 10. Verification — exact commands

```bash
# From repo root
cd /home/user/OpenBBPort/tauri-shell/examples/typescript-frontend

# 1. Install (idempotent — uses package-lock.json)
npm install

# 2. Strict TypeScript check
npm run typecheck
# Expected output:
#   > tauri-shell-example-frontend@0.1.0 typecheck
#   > tsc --noEmit
# (exits 0 with no output)

# 3. Full production build
npm run build
# Expected output ends with:
#   ✓ built in <300-500> ms

# 4. (Optional) end-to-end launch — needs cargo + tauri-cli
npm run dev &                              # terminal 1
cd /home/user/OpenBBPort/tauri-shell
cargo tauri dev                            # terminal 2 — opens the webview
```

All three of `npm install`, `npm run typecheck`, `npm run build` were
re-verified in this session and pass with exit code 0. The build output is
written to `dist/` (gitignored). The `node_modules/` directory is also
gitignored.

---

## Confirmation

No commit made. All edits are scoped to
`/home/user/OpenBBPort/tauri-shell/handoffs/slice-f-ts-frontend.md`
(new file, ~250 lines). The example itself was not modified during this
handoff write-up; only verification commands were re-run.
