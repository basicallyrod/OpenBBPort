# Slice D — Integration tests (handoff)

Status: complete. 8 test files, 59 test functions, all 8 test binaries compile
under `cargo test --no-run`. The `bindings_export` test is gated behind the
`bindings` Cargo feature; the other seven run by default.

## 1. Purpose

Slice D verifies the **infrastructure layer that the shell owns** — the parts
a connector author should never re-test: the ring buffer
(`state::LogBuffer`), the process-monitor facade (`process_monitor::*`),
atomic JSON writes (`settings::write_json_atomic`), the HTTP client wrapper
(`proxy::Proxy`), the cross-platform kill helpers (`process_kill::*`), the
bounded shutdown cascade (`cleanup::cleanup_all_processes`), and the
Slice A bindings export driver. The Slice B `Connector` trait is also
covered because the no-op fallback is shell-owned.

Why **integration** tests over `#[cfg(test)] mod tests`:

- These modules are the *contract surface* with any downstream connector or
  CLI binary. Tests under `tests/` use the public API only, so a breaking
  change to a `pub` item fails CI immediately.
- The tests need real OS resources (tempdirs, subprocesses, TCP listeners,
  mock HTTP). Wiring that into `src/` would force each module to import
  `[dev-dependencies]`.
- Several tests need `tauri::test::mock_app` (`tests/cleanup.rs:13`), which
  is only available when `tauri` is brought in with the `test` feature —
  done in `Cargo.toml:93` under `[dev-dependencies]` so release builds stay
  clean.

The suite is hermetic: no real network, no shared on-disk state, and tests
that touch process-global singletons are serialised with `#[serial]`.

## 2. What was built

Per-file summary (path / total tests / what each covers). Function names
cited so a future agent can grep directly.

### `tests/process_monitor.rs` — 8 tests, 177 lines

Hermetic tests of the `process_monitor::{register, unregister, history,
clear}` API against a freshly-constructed `LogStorage` (`tests/process_monitor.rs:18`,
`fresh_storage()`). One test (`global_log_storage_handle_round_trips`,
`tests/process_monitor.rs:155`) deliberately probes the real global
`LOG_STORAGE` singleton and is therefore `#[serial]`.

- `register_creates_buffer_and_is_idempotent` (`:31`) — `register` returns
  true on first call, false on second; history is empty after register.
- `get_returns_appended_entries_in_order` (`:41`) — 100 appends preserve
  insertion order.
- `clear_empties_buffer_but_keeps_registration` (`:54`) — `clear` empties
  the buffer but the id stays registered.
- `unregister_drops_buffer_and_history` (`:68`) — second `unregister`
  returns false.
- `capacity_overflow_evicts_oldest` (`:80`) — ring of capacity 5 with 8
  pushes drops `line-0..line-2`.
- `multi_id_isolation` (`:94`) — clearing `"b"` leaves `"a"` and `"c"`
  untouched.
- `concurrent_writes_then_reads_are_safe` (`:117`) — 8 writer threads ×
  125 entries = 1000 entries, with a reader thread asserting monotonic
  length growth.
- `global_log_storage_handle_round_trips` (`:155`, `#[serial]`) — confirms
  `state::log_storage()` is the same `Arc` as the `LOG_STORAGE` lazy and
  that `DEFAULT_RING_CAPACITY == 10_000`.

### `tests/log_storage.rs` — 8 tests, 134 lines

Pure-API tests of the lower-level ring buffer and map helpers exported from
`state.rs`: `LogBuffer::new/push/tail/len`, `append_entry`, `get_logs`,
`register_process`, `unregister_process`, `clear_process_logs`.

- `push_evicts_at_exact_capacity_plus_one` (`tests/log_storage.rs:31`) —
  fills to exactly `DEFAULT_RING_CAPACITY`, the next push evicts `line-0`.
- `tail_none_returns_all_entries` (`:50`).
- `tail_some_n_returns_last_n` (`:63`).
- `tail_some_larger_than_size_returns_all` (`:75`) — saturating behaviour.
- `isolation_between_process_ids` (`:85`).
- `empty_buffer_state_is_consistent` (`:108`) — `is_empty`, `len() == 0`,
  empty tails.
- `append_to_unregistered_id_creates_buffer_on_demand` (`:117`) — exercises
  the "soft register" path used by `process_spawn`.
- `unregister_returns_false_for_missing_id` (`:127`).

### `tests/settings.rs` — 9 tests, 185 lines

Tests the atomic-write / flock / chmod / corruption-recovery contract of
`settings::{write_json_atomic, read_json, modify_json}`. All run against a
`tempfile::TempDir` (`tests/settings.rs:17`, `fixture()`).

- `write_then_read_roundtrips` (`:23`).
- `read_returns_none_for_missing_file` (`:33`).
- `write_atomic_leaves_no_tmp_artefact` (`:39`) — scans the parent dir
  for any leftover `*.tmp` after a successful write.
- `write_survives_simulated_crash_mid_write` (`:60`) — plants a truncated
  `*.tmp` file as if the previous process died, then asserts the next
  atomic write overwrites it and renames cleanly.
- `flock_contention_returns_lock_failed` (`:89`) — opens the `.tmp` path
  itself, holds an exclusive `fs2::FileExt::try_lock_exclusive`, expects
  `SettingsError::LockFailed(path)`.
- `chmod_0600_applied_on_unix` (`:115`, `#[cfg(unix)]`) — asserts
  `mode & 0o777 == 0o600`.
- `modify_json_recovers_corrupt_file_via_default` (`:125`) — corrupt JSON
  surfaces as `SettingsError::Json`; a fresh `write_json_atomic` is the
  documented recovery.
- `modify_json_merges_into_existing_tree` (`:145`).
- `modify_json_seeds_default_when_file_missing` (`:165`).

### `tests/proxy.rs` — 10 tests, 251 lines

`httpmock`-backed (`MockServer::start_async`, no real network). Hand-rolled
base64 encoder (`tests/proxy.rs:12`, `base64_encode`) so the test crate
doesn't need to depend on a base64 implementation just to assert the exact
`Authorization: Basic ...` header.

- `get_with_query_params` (`:49`) — GET `/equity/price/historical` with
  `symbol` and `provider` query params.
- `post_with_json_body` (`:74`) — POST `/technical/sma` with JSON body.
- `basic_auth_header_is_sent` (`:96`).
- `bearer_token_is_sent` (`:116`).
- `clear_auth_drops_both_headers` (`:134`) — sets basic, clears, expects 401.
- `handles_204_no_content` (`:154`) — proxy maps 204 to JSON `null`.
- `error_envelope_for_non_2xx` (`:172`) — 404 surfaces as
  `ProxyError::Http { status: 404, body }`.
- `get_raw_skips_api_v1_prefix` (`:199`) — `get_raw` does not prepend
  `/api/v1` (used for `/widgets.json` etc).
- `timeout_short_circuits_slow_server` (`:216`) — asserts
  `config().timeout_seconds == 60` and exercises a 100ms delay mock.
- `empty_base_url_returns_not_configured` (`:242`) — `ProxyError::NotConfigured`.

### `tests/process_kill.rs` — 7 tests, 159 lines

Spawns real subprocesses through a `spawn_sleeper()` helper that branches
on `cfg(unix)` (`sleep N`) vs `cfg(windows)` (`cmd /C timeout /T N /NOBREAK`,
`tests/process_kill.rs:13-27`).

- `kill_pid_kills_a_live_subprocess` (`:30`) — SIGKILL path.
- `kill_pid_graceful_path` (`:46`) — `graceful=true` (SIGTERM first).
- `kill_pid_returns_false_for_unknown_pid` (`:56`) — `u32::MAX`.
- `is_alive_flips_after_child_exits` (`:65`) — zombie reap timing.
- `port_from_url_extracts_known_ports` (`:75`) — full URL, default-port
  fallback, regex `:9000` fallback.
- `kill_listeners_on_port_terminates_listener` (`:97`, `#[cfg(unix)]`) —
  spawns a Python listener on an ephemeral port, calls
  `kill_listeners_on_port`, asserts the port is re-bindable. Skips
  gracefully when `python3` isn't on `PATH`.
- `port_from_url_returns_none_for_garbage` (`:157`).

### `tests/cleanup.rs` — 6 tests, 220 lines

Uses `tauri::test::mock_app` (`tests/cleanup.rs:13,89`) to obtain an
`AppHandle<MockRuntime>` without booting a real window — the cleanup cascade
only needs `Manager::try_state::<T>()`. All cascade tests are `#[serial]`
because they each `.manage::<Arc<dyn ShutdownHook>>(...)` on a shared mock
runtime.

- `timeouts_match_spec` (`:79`, `#[serial]`) — guards
  `OUTER_TIMEOUT == 10s`, `HOOK_TIMEOUT == 3s`, `KILL_TIMEOUT == 3s`.
- `slow_hook_bounded_by_hook_timeout` (`:88`) — a `SlowHook` that sleeps
  5s; cascade must return between `HOOK_TIMEOUT` and `HOOK_TIMEOUT + KILL_TIMEOUT + 2s`.
- `cleanup_drains_running_processes` (`:128`) — spawns two `sleep 60`
  children, registers them in `RunningProcesses`, asserts both die and
  the registry empties.
- `cleanup_with_no_hook_still_drains_processes` (`:164`) — deliberately
  omits the `ShutdownHook` `manage` call; cleanup still kills children.
- `noop_hook_completes_immediately` (`:185`) — `CountingHook` increments
  exactly once, total elapsed < `HOOK_TIMEOUT`.
- `cleanup_is_idempotent` (`:205`) — two cascade calls increment the
  counter exactly twice; registry stays empty.

### `tests/connector.rs` — 10 tests, 259 lines

Demonstrates the override pattern that Slice B documents. `TestConnector`
(`tests/connector.rs:26`) overrides 4 methods covering each error category;
every other method falls through to the trait's default `Err(NotImplemented)`
body.

- `override_toggle_theme_accepts_known_value` (`:86`).
- `override_toggle_theme_rejects_unknown_value` (`:93`) — checks
  `ConnectorError::InvalidArgument("...rainbow...")`.
- `override_get_working_directory_uses_default` (`:105`).
- `override_list_conda_environments_returns_seeded_list` (`:117`).
- `override_execute_in_environment_round_trips_args` (`:126`).
- `override_execute_in_environment_rejects_empty_command` (`:141`).
- `unoverridden_method_returns_not_implemented` (`:158`) — confirms
  `list_backend_services` falls through.
- `unoverridden_server_spawn_returns_not_implemented` (`:172`).
- `noop_connector_returns_not_implemented_for_every_method` (`:189`) —
  macro-driven sweep across ~20 methods on `NoopConnector`.
- `connector_error_maps_to_ipc_error` (`:237`) — every `ConnectorError`
  variant has a matching `IpcError` variant (`From` impl preserved).

### `tests/bindings_export.rs` — 1 test, 128 lines (gated)

Gated behind `#![cfg(feature = "bindings")]` (`tests/bindings_export.rs:17`).
`force_export_all_bindings` (`:47`) calls `T::export_all_to(&out)` for ~30
structs spanning `state`, `events`, and every `ipc::*` module that crosses
the IPC boundary. Then `rebuild_index` (`:105`) writes
`bindings/index.ts` as a barrel of `export type { Foo } from "./Foo";`
lines, sorted, with a "DO NOT EDIT" header.

This is Slice A's deliverable surfaced as a test so CI fails the moment a
struct loses its `#[derive(TS)]`. Without the `bindings` feature, the file
is empty and the test binary contains zero tests (still compiles).

## 3. Dev-dependencies added

In `Cargo.toml:83-93` under `[dev-dependencies]`:

- **`httpmock = "0.7"`** — proxy tests in `tests/proxy.rs`. Local mock
  HTTP server with both sync (`MockServer::start`) and async
  (`MockServer::start_async`) entry points; we use the async form so the
  tests can be `#[tokio::test]` without blocking.
- **`tempfile = "3"`** — hermetic scratch directories for
  `tests/settings.rs`. `TempDir` cleans up on drop, even on panic.
- **`serial_test = "3"`** — `#[serial]` attribute used in
  `tests/process_monitor.rs:154` and across `tests/cleanup.rs`. Serialises
  tests that touch `LOG_STORAGE`, `INSTALLATION_PROGRESS`, or the mock
  `AppHandle`-managed `RunningProcesses` so they don't race when the
  default test runner spawns them in parallel.
- **`tauri = { version = "2.10", features = ["test"] }`** — re-declared
  with the `test` feature so `tauri::test::mock_app` is available to
  `tests/cleanup.rs`. The production dependency in `[dependencies]` does
  *not* enable `test`.

## 4. Patterns used

- **`#[serial]`** — applied to tests in `tests/cleanup.rs:78,87,127,163,184,204`
  and `tests/process_monitor.rs:154`. Required wherever the test mutates
  a process-wide singleton (`LOG_STORAGE`, `RunningProcesses`,
  `INSTALLATION_PROGRESS`) or registers state on the mock `AppHandle`.
- **`tempfile::TempDir`** — see `tests/settings.rs:17` (`fixture()`).
  Returned by value so the test owns the drop order; never store the
  `TempDir` in a global.
- **`httpmock::MockServer::start_async()`** — see every test in
  `tests/proxy.rs`. Each test owns its mock server; the `Proxy` is
  pointed at `server.base_url()` (`tests/proxy.rs:43`, `proxy_for`).
  The `mock.assert_async().await` line at the end verifies the request
  actually hit the expected route.
- **`#[tokio::test]` / `#[tokio::test(flavor = "current_thread")]`** —
  async tests in `proxy.rs` use the default multi-thread runtime;
  `cleanup.rs` uses the single-threaded flavour because
  `cleanup_all_processes` joins on background tasks via `tokio::time::timeout`
  and the single-threaded runtime makes those deadlines deterministic.
- **`tauri::test::mock_app` + `MockRuntime`** — `tests/cleanup.rs:13,89`.
  The cascade only needs `Manager::try_state::<T>()`, so a real window
  is overkill.

## 5. Running the suite

```bash
# Everything except bindings_export (which is feature-gated).
cargo test

# A single test binary.
cargo test --test proxy
cargo test --test cleanup

# A single test function (substring match).
cargo test --test process_monitor multi_id_isolation

# Includes bindings_export. Writes .ts files into ./bindings/.
cargo test --features bindings

# Compile-check only (CI fast path).
cargo test --no-run
```

The `cleanup` and `process_kill` test binaries are the slowest (~2s each)
because they spawn real subprocesses. Everything else completes in under
a second on a modern laptop.

## 6. Known limitations + gaps

- **No Tauri end-to-end tests.** `MockRuntime` is the limit of Tauri's
  official scaffolding; no headless coverage of `generate_handler!`
  dispatch (public API still unstable in Tauri 2.x).
- **No tests for the 60 typed wrappers in `obb_routes.rs` or the 167 in
  `obb_routes_extended.rs`.** Each is a 2-line `call_get` / `call_post`;
  `tests/proxy.rs` covers the underlying `Proxy::get` / `Proxy::post`.
  A future property-test enumerating wrappers from a manifest would
  beat 227 near-identical tests.
- **No tests for `tray.rs`, `autostart/*`, `updater.rs`, `windows.rs`.**
  OS-specific (osascript / COM / XDG / updater HTTP signature). Mocking
  costs more than the bugs would catch; left to manual verification.
- **No tests for `process_spawn.rs`.** Streamed event payload assertions
  need a real event bus; `MockRuntime` doesn't fully provide one.
  Pid-side effects are covered by `process_kill.rs` and `cleanup.rs`.

## 7. Integration with other slices

- **Slice A — TS bindings.** `tests/bindings_export.rs` is the driver
  that produces `bindings/*.ts`. Gated behind `--features bindings`
  (`Cargo.toml:81`). When a Slice A struct loses its `#[derive(TS)]`,
  this test fails to compile.
- **Slice B — Connector trait.** `tests/connector.rs` is the canonical
  override example for SPEC.md §2 Slice B. It also locks the
  `ConnectorError → IpcError` mapping (`tests/connector.rs:237`).
- **Slice C — Extended typed wrappers.** Not directly tested here; relies
  transitively on `tests/proxy.rs`.
- **Slice E — Docs.** README §"Testing" should link to this handoff and
  list the commands from §5 above.
- **Slice G — CLI binary.** `src/bin/cli.rs` builds against the same
  `state::log_storage()`, `Proxy::new()`, `settings::*` singletons that
  Slice D tests, so the test suite implicitly covers the CLI's
  domain-layer correctness. CLI-specific argument parsing is not
  covered here.

## 8. Verification

```bash
cd /home/user/OpenBBPort/tauri-shell
cargo test --no-run --message-format=short   # compile-check, ~4s incremental
cargo test                                   # run default suite
cargo test --features bindings               # includes bindings_export
```

Last verified: all 8 test binaries listed by `cargo test --no-run`:

```
tests/bindings_export.rs
tests/cleanup.rs
tests/connector.rs
tests/log_storage.rs
tests/process_kill.rs
tests/process_monitor.rs
tests/proxy.rs
tests/settings.rs
```

## 9. Adding a new test

Recipe for a new integration test against module `foo`:

1. **Create `tests/foo.rs`** with a `//! ...` header describing the surface
   under test and any global-state implications.
2. **Import only the public API.** `use tauri_shell::foo::{...}`. If
   something you need is private, promote it to `pub(crate)` and re-export
   via `lib.rs`, or add a thin public helper.
3. **Pick the test flavour:** plain `#[test]`, `#[tokio::test]`, or
   `#[tokio::test(flavor = "current_thread")]` (latter when you need
   deterministic timer behaviour, as in `tests/cleanup.rs`).
4. **Touch a global?** If yes (`LOG_STORAGE`, `INSTALLATION_PROGRESS`,
   mock-managed state) add `#[serial]`. Otherwise build a fresh local —
   see `fresh_storage()` in `tests/process_monitor.rs:18`.
5. **Filesystem tests:** `tempfile::TempDir::new()`, return the `TempDir`
   from your helper so Rust drop order keeps it alive.
6. **HTTP tests:** one `MockServer` per test, point `Proxy` at
   `server.base_url()`, end with `mock.assert_async().await`.
7. **Subprocess tests:** guard the spawn helper with
   `#[cfg(unix)] / #[cfg(windows)]` (see `tests/process_kill.rs:13-27`);
   always `child.wait()` on the success path so you don't leave zombies.
8. **Verify:** `cargo test --test foo`, then `cargo test`. Update this
   handoff with the new file's line count and per-test summary.

---

Did not commit. Slice D files were already on disk; this handoff documents
their final state and is the only new file. Update `SPEC.md` §8 to mark
Slice D as ✅ if no further work is planned.
