# Port Tech Stack Decisions

Every major dependency choice for the TypeScript port, with trade-offs and a recommendation. Choices are conditional where Layer-1 (top-level framework) decides them; the dependency table at the end resolves both branches.

Strategy labels referenced below (decided in `port-strategy.md`):
- **Strategy A** — Tauri stays, only the renderer is rewritten.
- **Strategy B** — Replace Rust backend with Node (Electron, or Node agent + UI). The Python `openbb-api` server may be reused as a sub-process.

---

## Top-level framework

| Option | Binary | Subprocess parity | Risk | When to pick |
|---|---|---|---|---|
| **Tauri 2** (incumbent) | ~10–15 MB | Excellent (Rust stdlib) | Lowest | Strategy A; UI-only rewrite |
| **Electron + Node** | ~150 MB | Good (`child_process`, `execa`) | Medium | Strategy B; team has Electron skills |
| **Wails 2 (Go)** | ~10 MB | Good (Go stdlib) | High | Want small binary, willing to maintain Go |
| **Pure web + Node agent** | Browser + ~80 MB agent | OK | Medium-high | Want UI to also run in browser |
| **Neutralino / Photino / Verso** | varies | Poor | Very high | Not viable; too immature |

**Verdict:** Strategy A → keep **Tauri 2**, rewrite only `src/`. Strategy B → **Electron + Node** (v1 section 8 recommendation; Wails costs a Go dep no one is asking for). Strategy C (hybrid) → keep Tauri shell but begin moving handlers behind a Node sidecar. Recommend **Tauri 2 (Strategy A)** as the default unless the team has an explicit mandate to eliminate Rust.

---

## Frontend framework

| Option | Notes |
|---|---|
| **React 18.3** (incumbent) | All `@openbb/ui-pro` components are React/Radix; switching costs the entire `src/` rewrite. |
| Solid / Svelte / Vue | No reuse of `ui-pro`; each `invoke()`/`listen()` call site still works (framework-agnostic). |

**Verdict:** stay on **React 18** (consider React 19 once `ui-pro` supports it). Reuse `@openbb/ui-pro` — it is already in deps (`package.json:20`) and provides the Radix primitives the rebuild needs.

---

## Routing

| Option | Notes |
|---|---|
| **TanStack Router** (incumbent, file-based) | Type-safe routes, devtools, generates `routeTree.gen.ts`. |
| React Router v6/v7 | Mature, no codegen step, lower type-safety. |
| Wouter | Tiny, hooks-based; no nested layouts to match current `__root.tsx` shape. |

**Verdict:** keep **TanStack Router**. The file-based layout (`src/routes/*.tsx`) maps cleanly to the existing pages and the type-safe `Link`/`useNavigate` calls are pulling weight today.

---

## State management

| Option | Best for |
|---|---|
| **React state + Context** (incumbent — `EnvironmentCreationContext`) | UI-local state, small fan-out. |
| **Zustand** | Cross-page mutable stores (e.g. `runningProcesses` mirror). Trivial migration from Context. |
| Jotai | Atom-graph; overkill for this app. |
| Redux Toolkit | Heaviest; not justified. |
| **XState** | The install wizard's substring-matched phase transitions (`installation.md`) are a textbook FSM. |

**Verdict:** **React state + Zustand** for ambient mirrors of Rust/Node state (process map, env list, install status). **XState** for the install wizard specifically — current code does `output.includes("Pkgs downloaded")` to advance phase; an explicit machine eliminates the fragility. Keep `useContext` for narrow form-scoped state like `EnvironmentCreationContext`.

---

## Forms + validation

| Option | Notes |
|---|---|
| **react-hook-form + Zod** (incumbent, `@hookform/resolvers` at `package.json:19`) | Uncontrolled, integrates with Radix, schemas double as TS types. |

**Verdict:** **keep**. Every form in the rebuild stays on this stack.

---

## Styling

| Option | Notes |
|---|---|
| **Tailwind 3 JIT** (incumbent) | Works; `ui-pro` ships its own preset. |
| Tailwind 4 | Native CSS-cascade-layers, faster builds, but `ui-pro` must publish a v4-compatible preset first. |
| vanilla-extract / CSS Modules | Type-safe but loses utility-first speed and `ui-pro` compatibility. |

**Verdict:** **stay on Tailwind 3** for the port; revisit Tailwind 4 once `@openbb/ui-pro` ships a v4 preset. No migration during the port itself.

---

## IPC layer

| Option | Strategy fit | Notes |
|---|---|---|
| **Tauri `invoke`/`listen`** (incumbent) | A only | Stays untouched; remove unused `taurpc` (`package.json:43`). |
| **Electron `ipcRenderer.invoke` + `ipcMain.handle`** | B | Maps 1:1 onto Tauri semantics; v1 §8.2 shows the preload shim. |
| **tRPC over IPC** (`trpc-electron`) | B | Closes the type-safety gap (57 stringly-typed commands today); higher build complexity. |
| Local HTTP/WS, gRPC, socket.io | — | Rejected (v1 §8.1) — adds an internal port surface for no IPC benefit. |

**Verdict:** Strategy A — **Tauri IPC**, drop `taurpc`. Strategy B — **Electron IPC behind a `tauriCompat` preload shim** plus a thin **zod-validated wrapper** so the 57 invokes get runtime-checked args. Consider `trpc-electron` only if the team will write new commands; for the port itself the shim keeps ~120 call sites unchanged.

---

## Subprocess management (Strategy B only)

| Option | Notes |
|---|---|
| `child_process.spawn` (stdlib) | Lowest dep, manual signal/timeout handling. |
| **`execa`** | Better defaults: `kill('SIGTERM', {forceKillAfterTimeout})`, `cleanup`, stdout `Readable`. |
| **`node-pty`** | Required only for the CLI REPL where Python `prompt_toolkit` needs a real TTY. |

**Verdict:** **`execa`** for `start_backend_service`, `start_jupyter_server`, `install_conda`, and every other current `Command::new` site. **`node-pty`** *only* for the CLI integration (`feature-cli-repl.md`) — it needs a PTY to render menus correctly. Do not mix: PTY processes don't pipe cleanly into the log ring buffer.

---

## Process cleanup / port→PID→kill

| Option | Notes |
|---|---|
| **`fkill`** npm | Cross-platform; takes `:port` syntax; tree-kills under the hood. |
| `tree-kill` | Kills by PID tree only; doesn't resolve from port. |
| Shell-out `lsof`/`netstat`/`taskkill` | Matches current Rust impl (`backends.rs`), most control, most code. |

**Verdict:** **`fkill`** for the `port → PID → kill` path (Jupyter on 8888, backend on 6900). Fall back to shell-out (`lsof -ti :PORT`) only on the macOS bug case `fkill` misses. Avoid mixing `tree-kill` in — `fkill` already handles the tree.

---

## File locking (`user_settings.json`, `backends.json`)

| Option | Notes |
|---|---|
| **`proper-lockfile`** | Cross-platform; staleness detection; matches `feature-backend-services.md:358` recommendation. |
| `lockfile` | Older, callback API, no stale lock retry. |
| Manual `O_EXCL` sentinel | Has to handle stale locks by hand. |

**Verdict:** **`proper-lockfile`**. Critical for `backends.json` (current Rust uses `fs2::FileExt::try_lock_exclusive`; the port must preserve that) and recommended for `user_settings.json` to close the read-modify-write race called out in `feature-api-keys.md:179`.

---

## Atomic file writes

| Option | Notes |
|---|---|
| **`write-file-atomic`** | `tmp + fsync + rename` in one call; cross-platform Windows quirks handled. |
| Manual `fs.writeFile` + `fs.rename` | Three lines, no Windows rename-over-existing handling. |

**Verdict:** **`write-file-atomic`**. Fixes the non-atomic `std::fs::write` bug noted at `feature-api-keys.md:223` and applies to every JSON write (settings, backends, env metadata).

---

## JSON schema + IPC payload validation

| Option | Notes |
|---|---|
| **Zod** (incumbent at `package.json:46`) | Runtime check + inferred TS types; pairs with react-hook-form resolver already in use. |
| TypeBox | JSON-Schema-first; better for OpenAPI alignment but a second mental model. |
| valibot | Smaller bundle; less ecosystem. |

**Verdict:** **Zod**. The single highest-ROI improvement during the port is wrapping every IPC handler with `schema.parse(args)` — current 57 invokes are stringly typed end-to-end (v1 §8.3 item 8). One `schemas/ipc.ts` module is the source of truth on both sides.

---

## OpenAPI codegen (Strategy B only; for `openbb-api` reuse)

| Option | Output | Notes |
|---|---|---|
| **`openapi-typescript`** | Types only | Lightweight; pairs with `fetch`/`ky` of your choice. |
| **`openapi-fetch`** | Types + 6 KB client | Fully typed `client.GET('/api/v1/...')`. |
| `orval` / `kubb` | Full SDK incl. hooks | Heavy generators; overkill since most pages don't call the Platform API directly. |

**Verdict:** **`openapi-fetch`** for the few endpoints the renderer talks to (API-keys verification, Platform health). Generate from `http://127.0.0.1:6900/openapi.json` at build time and commit the output.

---

## Charts

| Option | Notes |
|---|---|
| **Plotly.js** + thin React wrapper | Server emits `chart.content` as raw Plotly JSON (`feature-platform-rest-api.md:142-144`); Plotly renders directly. |
| Apache ECharts, recharts, visx | Would require translating Plotly JSON → other format per chart type. |

**Verdict:** **Plotly.js** (`plotly.js-dist-min` + a small `<PlotlyChart>` wrapper). Parity with the Python server's wire format is the constraint; switching libraries loses fidelity for negligible bundle savings.

---

## Logging

| Option | Strategy fit |
|---|---|
| **`tauri-plugin-log`** (incumbent) | A |
| **`electron-log`** | B (Electron) — file rotation, IPC bridge, console mirror out of box |
| `pino` / `winston` | B (Node agent) — structured JSON, faster than console.log |

**Verdict:** A → keep `tauri-plugin-log`. B with Electron → **`electron-log`** (rotates to `userData/logs/main.log` and mirrors renderer console). B with a Node agent → **`pino`** with `pino-pretty` in dev. Keep log channel names identical to the Rust `tauri-plugin-log` targets so existing diagnostic recipes still work.

---

## Updater

| Option | Strategy fit |
|---|---|
| **`tauri-plugin-updater`** (incumbent) | A — already wired to the GitHub-release flow at `main.rs:108`. |
| **`electron-updater`** | B (Electron) — same GitHub-release-feed shape; signature-verifying. |
| `velopack` | Newer; cross-framework; less proven on macOS notarization. |

**Verdict:** A → keep `tauri-plugin-updater`. B → **`electron-updater`** with `provider: github`. Velopack is interesting but unproven; revisit later.

---

## Single instance

| Option | Strategy fit |
|---|---|
| **`tauri-plugin-single-instance`** (incumbent) | A |
| **`app.requestSingleInstanceLock()` + `second-instance`** | B (Electron) — built-in, identical semantics |

**Verdict:** A → keep plugin. B → **`requestSingleInstanceLock`**. Both surface a `second-instance` callback to focus the existing window.

---

## Autostart (start at login)

| Option | Notes |
|---|---|
| **`app.setLoginItemSettings`** (Electron) | macOS + Windows in one call. |
| Manual `.desktop` for Linux | Required regardless of framework; write to `~/.config/autostart/openbb.desktop`. |
| Per-OS shell-outs (current Rust) | Most code, most edge cases. |

**Verdict:** B → **`setLoginItemSettings`** on macOS/Windows + manual `.desktop` writer on Linux. A → keep current Rust impl (`feature-tray-and-autostart.md`). Either way, store the user's preference in `user_settings.json` so it survives reinstalls.

---

## Self-signed cert generation

| Option | Notes |
|---|---|
| **`node-forge`** | Pure JS; emits PKCS#12; no system openssl dependency. |
| `node:crypto` X509 helpers | Native, no extra dep, but PKCS#12 export is awkward pre-Node 20. |
| Shell out to `openssl` | Matches current Rust openssl crate behavior but needs openssl in PATH. |

**Verdict:** B → **`node-forge`**. It eliminates the openssl-in-PATH dependency (currently mitigated by `src-tauri/scripts/copy_openssl.cjs` — `package.json:12`). Adding the cert to the OS trust store remains platform-specific.

---

## ANSI / log cleanup

| Option | Notes |
|---|---|
| **`strip-ansi`** | One function; tested against every terminal escape. |
| Manual regex | Misses 8-bit CSI and OSC sequences. |

**Verdict:** **`strip-ansi`** in the log ring buffer's ingest path. Subprocess output from conda/pip is heavily ANSI-coloured; the current Rust impl strips with a regex that misses some sequences.

---

## Testing

| Layer | Tool | Notes |
|---|---|---|
| Renderer unit | **Vitest** (incumbent) + `@testing-library/react` | Already at `package.json:72`. |
| E2E | **Playwright** | Works against Electron *and* Tauri (via `tauri-driver`). |
| IPC mocking | Vitest module-mock of the `tauriCompat` (or `@tauri-apps/api/core`) wrapper | Centralised shim means one mock file. |

**Verdict:** **Vitest + Playwright**. Add a `src/test/ipc-mock.ts` that exposes `mockInvoke('command_name', returnValue)` so tests don't reach into Tauri/Electron internals.

---

## Build

| Option | Notes |
|---|---|
| **Vite 7** (incumbent at `package.json:70`) | Fast HMR; `@vitejs/plugin-react`; works under both Tauri and Electron renderer. |

**Verdict:** **keep Vite**. Under Electron, add `electron-vite` (or `vite-plugin-electron`) only if you want the same Vite pipeline for the main process; otherwise `tsc` is enough for `main.ts` since it doesn't need HMR.

---

## Bundling Python (Strategy B only)

| Option | Notes |
|---|---|
| **conda** (incumbent) | Already supported end-to-end; users get the same Anaconda Cloud channels. |
| **`uv` / `rye`** | Order-of-magnitude faster solves; replaces conda entirely. Different env layout breaks the migration tool (`feature-environments.md`). |
| PyInstaller / Briefcase | Freezes a single bundled Python; breaks user-installable extensions. |

**Verdict:** **keep conda** as the default for Strategy B. Add an *optional* `uv`-based code path behind a feature flag for new installs (uv-managed envs would not be visible to the existing `conda env list` flow, so the env list UI needs a backend abstraction first). Do not bundle Python — the app's extension installer requires a real package manager.

---

## Final recommended stack

| Layer | Strategy A (Tauri stays) | Strategy B (Node backend) |
|---|---|---|
| Framework | **Tauri 2** | **Electron 32+** |
| UI library | React 18 + `@openbb/ui-pro` | React 18 + `@openbb/ui-pro` |
| Router | TanStack Router | TanStack Router |
| State | React + Zustand; **XState** for install wizard | React + Zustand; **XState** for install wizard |
| Forms | react-hook-form + Zod | react-hook-form + Zod |
| Styling | Tailwind 3 | Tailwind 3 |
| IPC | Tauri `invoke`/`listen` (drop `taurpc`) | Electron IPC + `tauriCompat` preload shim + Zod validation |
| Subprocess | Rust `std::process` (unchanged) | **`execa`** (general) + **`node-pty`** (CLI only) |
| Port kill | Rust (unchanged) | **`fkill`** |
| File lock | `fs2` (Rust) | **`proper-lockfile`** |
| Atomic write | manual Rust | **`write-file-atomic`** |
| Schema/types | **Zod** for IPC payloads (NEW in both) | **Zod** for IPC payloads (NEW in both) |
| OpenAPI client | n/a | **`openapi-fetch`** (only if renderer hits `:6900` directly) |
| Charts | **Plotly.js** | **Plotly.js** |
| Logging | `tauri-plugin-log` | **`electron-log`** (Electron) / **`pino`** (Node agent) |
| Updater | `tauri-plugin-updater` | **`electron-updater`** |
| Single instance | `tauri-plugin-single-instance` | `app.requestSingleInstanceLock` |
| Autostart | current Rust impl | `setLoginItemSettings` + `.desktop` writer |
| Self-signed cert | Rust openssl | **`node-forge`** |
| ANSI strip | manual regex (replace) | **`strip-ansi`** |
| Testing | Vitest + Playwright | Vitest + Playwright |
| Build | Vite 7 | Vite 7 (+ `electron-vite` if desired) |
| Python runtime | conda | **conda** (keep) |

**Drop in both strategies:** `taurpc`, `@tauri-apps/plugin-app`, `@tauri-apps/plugin-http`, `@tauri-apps/plugin-process`, `@tauri-apps/plugin-window`, `@tauri-apps/plugin-shell` — all currently unused (v1 §8.3 item 7).

**Add in both strategies:** `zod` for IPC validation (already a dep — extend its scope), `write-file-atomic`, `proper-lockfile`, `strip-ansi`, **XState** for the install wizard.
