# tauri-shell — minimal TypeScript frontend example

A small Vite + TypeScript single-page app that exercises ~30 representative
IPC commands from the parent `tauri-shell` crate. It doubles as documentation
(every command call has a typed wrapper in `src/ipc.ts`) and as a smoke test
(click *Run all* to fire every command sequentially and see which connector
stubs need wiring).

## What's in here

```
examples/typescript-frontend/
├── index.html        — UI: one button per command family
├── package.json      — Vite + TS + @tauri-apps/api
├── tsconfig.json     — strict mode, ES2020, ESNext modules
├── vite.config.ts    — dev server on :1420 (matches tauri.conf.json)
└── src/
    ├── ipc.ts        — typed `invoke<T>(...)` wrappers
    ├── main.ts       — button wiring + event listeners
    └── style.css     — minimal styling
```

## Prerequisites

- Node ≥ 18 with `npm`
- Rust toolchain
- [Tauri CLI v2](https://v2.tauri.app/start/prerequisites/) — `cargo install
  tauri-cli --version "^2.0" --locked`

## Run it

From this directory, install the frontend deps once:

```bash
npm install
```

Then, from `tauri-shell/` (the parent crate), boot Tauri in dev mode. Tauri
will spawn `vite` itself via the `beforeDevCommand` if you point it at this
example. The simplest workflow is to run Vite manually in one terminal and
Tauri in another:

```bash
# terminal 1 — frontend
cd examples/typescript-frontend
npm run dev
# Vite is now serving http://localhost:1420

# terminal 2 — backend (Rust + Tauri webview)
cd ../..        # back to tauri-shell/
cargo tauri dev
```

The Tauri window will open and load the page from `http://localhost:1420`.

> **Note**: `tauri.conf.json` in the parent crate has `"frontendDist": "dist"`
> with a placeholder. To make this example the primary frontend, point
> `frontendDist` to `examples/typescript-frontend/dist` and re-run.

## What every button does

| Section | Command(s) exercised |
|---|---|
| App / version | `get_installation_state`, `get_app_version`, `navigate_to_page` (×2) |
| Infrastructure | `register_process_monitoring`, `get_process_logs_history`, `clear_process_logs_history` |
| Helpers | `get_home_directory`, `select_directory`, `check_directory_exists` |
| Credentials | `get_user_credentials`, `update_user_credentials`, `open_credentials_file` |
| OpenBB proxy | `obb_set_base_url`, `obb_get_base_url`, `obb_health`, `obb_widgets` |
| Data routes | `equity_price_historical`, `obb_call` (generic) |
| Introspection | `list_all_routes`, `provider_list` |
| Settings files | `read_settings_json` + `write_settings_json` round-trip |
| Routines | `routines_save` + `routines_list` + `routines_delete` |
| Stubs | `list_backend_services`, `list_conda_environments` (both expected to return `NotImplemented` until a connector is wired) |
| Server attach | `server_attach`, `server_health` |
| Run all | every command above, sequentially, ignoring per-call errors |

Total: **30 distinct commands** across 12 IPC modules.

## Event channels

The right-hand pane subscribes to three Tauri events from `tauri-shell`:

- `navigate` — fires when tray-menu items request a route change. The
  *Navigate to page* buttons also trigger this through the Rust handler.
- `process-output` — fan-in for any spawned subprocess. Producers call
  `process_spawn::spawn_with_streaming` to feed it.
- `install-progress` — structured installation pipeline events; the
  progress bar reflects them.

## TypeScript verification

```bash
npm run typecheck   # tsc --noEmit
npm run build       # tsc --noEmit && vite build
```

Both must pass before commit. The example is `strict: true` clean.

## Wiring it as the production frontend

Once the example is "real" for your app, point the parent
`tauri-shell/tauri.conf.json` at it:

```json
"build": {
  "frontendDist": "../examples/typescript-frontend/dist",
  "devUrl": "http://localhost:1420",
  "beforeDevCommand": "npm --prefix examples/typescript-frontend run dev",
  "beforeBuildCommand": "npm --prefix examples/typescript-frontend run build"
}
```

That makes `cargo tauri dev` a single-command launch.
