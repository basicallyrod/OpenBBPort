# Slice G — `tauri-shell-cli` Companion Binary

Status: done. Owner: opus. Output: `src/bin/cli.rs` (~810 lines) + `[[bin]]` target
in `Cargo.toml` + `clap` dependency.

## 1. Purpose

`tauri-shell-cli` is a terminal companion that exercises the **same IPC surface**
as the Tauri app, without booting the Tauri runtime. It is intended for:

- **Smoke-testing connector implementations.** Before wiring a custom `Connector`
  trait into the desktop binary, point the CLI at a running Python REST server
  and confirm individual routes return what you expect.
- **Scripting batch operations.** Reading credentials, saving routines, tailing
  process logs, listing route catalogs — useful in shell pipelines.
- **CI integration tests.** A subprocess in CI can hit `/health`, validate a
  provider's credential keys, or write a settings file without a display server.
- **Debugging.** When the GUI's Proxy auth / base-URL configuration looks wrong,
  the CLI lets you reproduce the same `Proxy::new()` + setter pattern in
  isolation with verbose JSON output.

This binary is **independent of every other slice** — it does not depend on the
Connector trait (Slice B), the TS bindings (Slice A), or the TS frontend
(Slice F).

## 2. Architecture

The CLI bypasses `#[tauri::command]` injection entirely. Tauri commands rely on
`State<'_, T>` extractors that are only populated inside `tauri::Builder::manage`.
Since the CLI never builds a Tauri app, it instantiates the same state singletons
**directly** and calls into the underlying logic.

```
┌──────────────────────────────────────────────────────────────────┐
│ src/bin/cli.rs                                                   │
│                                                                  │
│   #[tokio::main] async fn main() -> ExitCode                     │
│           │                                                      │
│           ▼                                                      │
│   Cli::parse()  (clap, derive API)                               │
│           │                                                      │
│           ▼                                                      │
│   match TopCmd → run_obb / run_settings / run_credentials /      │
│                  run_logs / run_routines / run_provider /        │
│                  run_paths                                       │
│           │                                                      │
│           ▼                                                      │
│   build_proxy(&cli)  ──►  Proxy::new() + set_base_url +          │
│                            set_bearer / set_basic_auth           │
│                                                                  │
│   path_utils::{home_dir, settings_dir, settings_file, …}         │
│   settings::{read_json, write_json_atomic, modify_json}          │
│   state::{LOG_STORAGE, register_process, get_logs, …}            │
│                                                                  │
│           ▼                                                      │
│   serde_json::Value → emit() → stdout                            │
└──────────────────────────────────────────────────────────────────┘
```

The same library crate (`tauri_shell::*`) backs both `src/main.rs` and
`src/bin/cli.rs`; see `src/bin/cli.rs:24-27` for the imports — `path_utils`,
`Proxy`, `settings`, and `state` (with `LogEntry`, `LOG_STORAGE`).

Every leaf subcommand returns a `serde_json::Value`. Errors are encoded as
`json!({"error": "..."})` (`src/bin/cli.rs:281-283`); the process exits with
status 1 when the top-level `Value` has an `"error"` key
(`src/bin/cli.rs:316-322`).

## 3. Subcommand tree

Seven top-level groups (`src/bin/cli.rs:76-112`), 25 leaf subcommands:

| Group | Leaves | Source |
|---|---|---|
| `obb` | `call`, `set-url`, `widgets`, `apps`, `agents`, `openapi`, `health`, `routes {list, search, params}` | `src/bin/cli.rs:117-158` |
| `settings` | `list`, `read`, `write` | `src/bin/cli.rs:169-180` |
| `credentials` | `get`, `set` | `src/bin/cli.rs:185-190` |
| `logs` | `register`, `tail`, `clear` | `src/bin/cli.rs:195-206` |
| `routines` | `list`, `read`, `save`, `delete` | `src/bin/cli.rs:211-224` |
| `provider` | `list`, `routes`, `creds` | `src/bin/cli.rs:229-236` |
| `paths` | `home`, `settings` | `src/bin/cli.rs:241-246` |

Total leaves: 25 (count `obb.call`, `obb.set-url`, `obb.widgets`, `obb.apps`,
`obb.agents`, `obb.openapi`, `obb.health`, plus the 3 under `obb.routes`, then
3+2+3+4+3+2 across the other six groups).

Each group mirrors one or two `ipc::*` modules:
- `obb` → `ipc::obb` + `ipc::openbb_meta`
- `settings` → `ipc::settings_files`
- `credentials` → `ipc::credentials`
- `logs` → `ipc::infrastructure`
- `routines` → `ipc::routines`
- `provider` → `ipc::provider`
- `paths` → `ipc::helpers` (the two path-resolution commands)

## 4. Global flags

Defined on the top-level `Cli` struct (`src/bin/cli.rs:40-73`):

| Flag | Env var | Default | Purpose |
|---|---|---|---|
| `--base-url <URL>` | `TAURI_SHELL_BASE_URL` | `http://127.0.0.1:6900` | Override Python REST base URL |
| `--username <USER>` | `TAURI_SHELL_USERNAME` | — | HTTP Basic auth username |
| `--password <PASS>` | `TAURI_SHELL_PASSWORD` | — | HTTP Basic auth password |
| `--bearer <TOKEN>` | `TAURI_SHELL_BEARER` | — | Bearer token (mutually exclusive with basic) |
| `--raw` | — | off | Single-line compact JSON output |
| `--json` | — | off | Alias for `--raw` (kept for discoverability) |

`build_proxy()` (`src/bin/cli.rs:259-268`) prefers `--bearer` over basic auth when
both are provided. All flags are `global = true`, so they can appear before or
after the subcommand path.

## 5. Output formatting

`emit()` (`src/bin/cli.rs:270-279`) handles all stdout writes:

- **Default** — `serde_json::to_string_pretty` (multi-line, indented).
- **`--raw`** — `serde_json::to_string` (single line, no whitespace).
- **`--json`** — identical to `--raw`; both set the same internal flag
  (`src/bin/cli.rs:304`).

The "already-JSON, just no pretty" semantics mean every command emits valid
JSON regardless of mode — pipe-friendly under `--raw` / `--json`, eyeball-friendly
by default.

Errors **never** print as JSON to stderr; they print to stdout as
`{"error": "..."}` and trigger a non-zero exit. This is a known gap (see §10).

## 6. Building

Cargo target declared in `Cargo.toml:102-104`:

```toml
[[bin]]
name = "tauri-shell-cli"
path = "src/bin/cli.rs"
```

```bash
# Debug build (fast, ~7s incremental)
cd tauri-shell && cargo build --bin tauri-shell-cli
# → target/debug/tauri-shell-cli

# Release build (small, optimized via lto = true in Cargo.toml:107-110)
cd tauri-shell && cargo build --release --bin tauri-shell-cli
# → target/release/tauri-shell-cli
```

The `clap` dependency (`Cargo.toml:49`) carries the `derive` + `env` features —
the latter is what enables the `env = "TAURI_SHELL_*"` annotations on each flag.

## 7. Example invocations

All examples assume `tauri-shell-cli` is on `$PATH` (or substitute
`./target/debug/tauri-shell-cli`).

```bash
# 1. Resolve the user's home dir (no network needed).
tauri-shell-cli paths home

# 2. Dump current credentials tree (reads ~/.openbb_platform/user_settings.json).
tauri-shell-cli credentials get

# 3. Insert a single API key (atomic write + flock + chmod 0600 under the hood).
tauri-shell-cli credentials set fmp_api_key sk-test-123

# 4. Hit the running Python REST server's health endpoint.
tauri-shell-cli --base-url http://127.0.0.1:6900 obb health

# 5. Call any route by path (params parsed as JSON literal first, else string).
tauri-shell-cli obb call /equity/price/historical \
    --param symbol=AAPL --param provider=yfinance

# 6. List every route the server advertises in /openapi.json.
tauri-shell-cli obb routes list

# 7. Free-text search over path/model/summary.
tauri-shell-cli obb routes search equity

# 8. Create a log ring buffer for a process id.
tauri-shell-cli logs register backend-test

# 9. Tail the most recent N entries from a buffer.
tauri-shell-cli logs tail backend-test --count 50

# 10. Save a .openbb routine file (atomic write via *.tmp + rename).
tauri-shell-cli routines save smoke --content "fetch AAPL"
```

Note example 5: `parse_kv` (`src/bin/cli.rs:252-257`) splits on the first `=`,
then `params_to_map` (`src/bin/cli.rs:285-295`) tries `serde_json::from_str`
on the value so `--param limit=50` is sent as a number, but `--param symbol=AAPL`
is sent as a string.

## 8. Use cases

| Scenario | Why the CLI helps |
|---|---|
| Smoke-test a custom Connector | Run the CLI against `http://127.0.0.1:<your-port>` before wiring `Connector::server_spawn` into the Tauri binary. If `obb health` and `obb routes list` both work, your Python side is healthy independently of any Rust/Tauri plumbing. |
| Scripted batch ops | `for sym in AAPL MSFT GOOG; do tauri-shell-cli obb call /equity/price/historical --param symbol=$sym --raw > $sym.json; done` |
| Debug Proxy auth/URL | Reproduce `obb_set_basic_auth` + `obb_set_base_url` + `obb_call` in three command-line invocations without launching the GUI. |
| CI integration tests | A GitHub Actions workflow can call `tauri-shell-cli obb health` after starting a Python server, exit-status-check, and skip the heavyweight Tauri build entirely. |

## 9. Tokio runtime

`main()` is decorated with `#[tokio::main]` (`src/bin/cli.rs:301`) because
`Proxy::get`, `Proxy::post`, and `Proxy::get_raw` are all `async fn`. The
multi-threaded scheduler is overkill for a CLI but matches the runtime that the
Tauri app uses, so identical async code paths execute. Sync subcommands
(`run_settings`, `run_credentials`, `run_logs`, `run_routines`, `run_paths`)
are still called from inside the runtime but do no `.await`.

## 10. Known gaps + next steps

- **No subcommand for typed wrappers.** The CLI does not expose the 200+ typed
  Tauri commands in `ipc::obb_routes` / `ipc::obb_routes_extended` individually.
  This is **deliberate**: those wrappers all forward to `proxy.get/post(route,
  params)`, so `tauri-shell-cli obb call <route> --param k=v` is the equivalent
  one-shot. If a user wants discoverability, `obb routes list` enumerates every
  route the server advertises.
- **No `server spawn` or `backends start` subcommand.** Those commands depend on
  a `Connector` implementation (Slice B / Slice H). The CLI is connector-agnostic
  by design: it uses only state and proxy logic that exists today in
  `NoopConnector` form.
- **No shell tab completion.** `clap` can emit Bash / Zsh / Fish completion
  scripts via `clap_complete`. Left as a follow-up; a `tauri-shell-cli completions
  <shell>` subcommand is the standard pattern.
- **No JSON-formatted stderr.** Errors print to stdout as `{"error": "..."}`
  and the process exits 1. Splitting success-JSON to stdout and error-JSON to
  stderr would be more pipeline-friendly. See `emit()` at
  `src/bin/cli.rs:270-279` and the exit logic at `src/bin/cli.rs:316-322`.
- **No `--config` file.** Every option must be set via flag or env var. A future
  iteration could read `~/.config/tauri-shell-cli/config.toml` for default
  base-url + auth.

## 11. Integration with other slices

- **Slice A (TS bindings)** — Not used. The CLI works in `serde_json::Value`
  throughout; no `ts-rs`-annotated struct ever crosses the boundary because
  there is no boundary.
- **Slice B (Connector trait)** — Connector-independent. The CLI exercises only
  the parts of the IPC surface that are **real** in `NoopConnector` form
  (proxy / settings / credentials / logs / routines / provider / paths). It
  never calls a stub handler.
- **Slice C (Extended typed wrappers)** — Not used directly. The 167 extended
  wrappers all delegate to `Proxy::get` under the hood, which `obb call`
  exposes generically.
- **Slice E (Docs)** — `README.md` (post-Slice-E) references this CLI in the
  cookbook section as the "no-GUI smoke test" path. Cross-link both ways.
- **Slice F (TS frontend example)** — Parallel surface. The TS example hits
  Tauri commands via `invoke()` IPC; the CLI hits the same Rust functions
  directly. Two ways to test the same shell, useful for narrowing down
  whether a bug is in the IPC layer or the underlying logic.
- **Slice H (Connector reference impls)** — Future: once `connectors/openbb-platform/`
  exists, the CLI could grow a `--connector <impl>` flag that selects which
  `Connector` to instantiate, making `server spawn` and friends reachable.

## 12. Verification

```bash
cd /home/user/OpenBBPort/tauri-shell

# 1. Compiles cleanly.
cargo build --bin tauri-shell-cli

# 2. Help renders the full subcommand tree.
./target/debug/tauri-shell-cli --help
./target/debug/tauri-shell-cli obb --help
./target/debug/tauri-shell-cli obb routes --help

# 3. Sanity checks that need no network.
./target/debug/tauri-shell-cli paths home
./target/debug/tauri-shell-cli paths settings
./target/debug/tauri-shell-cli settings list
./target/debug/tauri-shell-cli logs register cli-smoke
./target/debug/tauri-shell-cli logs tail cli-smoke

# 4. Pretty vs raw vs json.
./target/debug/tauri-shell-cli settings list           # multi-line
./target/debug/tauri-shell-cli --raw  settings list    # one line
./target/debug/tauri-shell-cli --json settings list    # one line (alias)

# 5. With a running Python REST server on :6900.
./target/debug/tauri-shell-cli obb health
./target/debug/tauri-shell-cli obb routes list | head
./target/debug/tauri-shell-cli obb call /equity/price/historical \
    --param symbol=AAPL --param provider=yfinance --raw
```

Files touched by this slice:

- `/home/user/OpenBBPort/tauri-shell/src/bin/cli.rs` — entire CLI implementation.
- `/home/user/OpenBBPort/tauri-shell/Cargo.toml` — `clap` dependency at line 49,
  `[[bin]] tauri-shell-cli` target at lines 102-104.
- `/home/user/OpenBBPort/tauri-shell/SPEC.md` — status table row updated to ✅.
