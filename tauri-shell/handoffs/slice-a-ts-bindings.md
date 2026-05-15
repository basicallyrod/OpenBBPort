# Slice A — TypeScript type bindings

Status: shipped. The slice's brief is in `tauri-shell/SPEC.md:67-87` (§2 Slice A).

## 1. Purpose

The Tauri IPC boundary is the hottest "stringly-typed" surface in this
project. Every `invoke<T>("name", args)` from the renderer relies on the
renderer spelling the command name correctly, hand-typing the `args`
object with the right keys/casing, and writing a `T` that matches what
Rust actually returns — a contract invisible to the TypeScript compiler,
so any field rename in Rust silently rots the renderer at runtime. Slice
A solves this by auto-generating a `.ts` file per Rust struct that
crosses the IPC boundary, plus a barrel `index.ts` that re-exports them,
so the TS frontend imports `BackendService`, `ProcessOutputEvent`, etc.
by name and the compiler refuses to build if a field is renamed, retyped,
or removed in Rust. Bindings are produced by `ts-rs` v9 behind a Cargo
feature flag (`--features bindings`): `ts-rs` is an *optional* dependency
(`tauri-shell/Cargo.toml:54`), and every annotation is
`#[cfg_attr(feature = "bindings", ...)]`, so production builds are
unaffected.

## 2. What was built

### 2.1 Annotated source structs (36 types, 18 files touched)

Every struct that crosses the IPC boundary gained two attributes:

```rust
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "../bindings/"))]
```

(camelCase structs add a third `rename_all = "camelCase"` arg.)

| # | Type | Source | Category |
|---|---|---|---|
| 1 | `LogEntry` | `tauri-shell/src/state.rs:22` | State |
| 2 | `InstallationSnapshot` | `tauri-shell/src/state.rs:245` | State |
| 3 | `InstallationProgress` | `tauri-shell/src/state.rs:277` | State |
| 4 | `ProcessOutputEvent` | `tauri-shell/src/events.rs:47` | Event |
| 5 | `InstallProgressEvent` | `tauri-shell/src/events.rs:91` | Event |
| 6 | `BackendUrlEvent` | `tauri-shell/src/events.rs:105` | Event |
| 7 | `JupyterStatusEvent` | `tauri-shell/src/events.rs:115` | Event |
| 8 | `NavigateEvent` | `tauri-shell/src/events.rs:124` | Event |
| 9 | `BackendService` | `tauri-shell/src/ipc/backends.rs:25` | IPC payload |
| 10 | `GenerateCertArgs` | `tauri-shell/src/ipc/certs.rs:19` | IPC args |
| 11 | `UpdateCredentialsArgs` | `tauri-shell/src/ipc/credentials.rs:42` | IPC args |
| 12 | `CondaEnvironment` | `tauri-shell/src/ipc/environments.rs:23` | IPC payload |
| 13 | `Extension` | `tauri-shell/src/ipc/environments.rs:32` | IPC payload |
| 14 | `ExecResult` | `tauri-shell/src/ipc/environments.rs:192` | IPC payload |
| 15 | `JupyterStatus` | `tauri-shell/src/ipc/jupyter.rs:32` | IPC payload |
| 16 | `McpSpec` | `tauri-shell/src/ipc/mcp.rs:21` | IPC args |
| 17 | `McpStatus` | `tauri-shell/src/ipc/mcp.rs:46` | IPC payload |
| 18 | `ObbCallArgs` | `tauri-shell/src/ipc/obb.rs:20` | IPC args |
| 19 | `RouteInfo` | `tauri-shell/src/ipc/openbb_meta.rs:33` | IPC payload |
| 20 | `RouteSearchArgs` | `tauri-shell/src/ipc/openbb_meta.rs:110` | IPC args |
| 21 | `RouteParamsArgs` | `tauri-shell/src/ipc/openbb_meta.rs:134` | IPC args |
| 22 | `ProviderSummary` | `tauri-shell/src/ipc/provider.rs:17` | IPC payload |
| 23 | `ProviderValidateArgs` | `tauri-shell/src/ipc/provider.rs:93` | IPC args |
| 24 | `RoutineMetadata` | `tauri-shell/src/ipc/routines.rs:52` | IPC payload |
| 25 | `RoutinesReadArgs` | `tauri-shell/src/ipc/routines.rs:92` | IPC args |
| 26 | `RoutinesSaveArgs` | `tauri-shell/src/ipc/routines.rs:108` | IPC args |
| 27 | `RoutinesDeleteArgs` | `tauri-shell/src/ipc/routines.rs:126` | IPC args |
| 28 | `RoutinesRenameArgs` | `tauri-shell/src/ipc/routines.rs:144` | IPC args |
| 29 | `ServerSpec` | `tauri-shell/src/ipc/server.rs:25` | IPC args |
| 30 | `ServerStatus` | `tauri-shell/src/ipc/server.rs:49` | IPC payload |
| 31 | `ReadJsonArgs` | `tauri-shell/src/ipc/settings_files.rs:39` | IPC args |
| 32 | `WriteJsonArgs` | `tauri-shell/src/ipc/settings_files.rs:52` | IPC args |
| 33 | `ReadTextArgs` | `tauri-shell/src/ipc/settings_files.rs:68` | IPC args |
| 34 | `WriteTextArgs` | `tauri-shell/src/ipc/settings_files.rs:85` | IPC args |
| 35 | `IpcError` | `tauri-shell/src/ipc/mod.rs:37` | Error envelope |
| 36 | `JsonValue` | `serde_json::Value` (transitive) | Bridge type |

`JsonValue` (#36) is not declared in our source; it comes from the
`serde-json-impl` feature of `ts-rs` and lands at
`tauri-shell/bindings/serde_json/JsonValue.ts`. It is the TS analogue of
`serde_json::Value` and is what every `Value`-typed field references.

### 2.2 Test harness

`tauri-shell/tests/bindings_export.rs` (128 lines). Gated on
`#![cfg(feature = "bindings")]` so it is a no-op under the default feature
set. The single test `force_export_all_bindings` calls
`T::export_all_to(&out)` for every type. Two roles:
1. Hard compile-time guarantee that every binding-relevant type still
   implements `TS`. If a struct loses its `derive(ts_rs::TS)` cfg_attr,
   the test stops compiling.
2. Side-effect: each `export_all_to` writes the `.ts` file. The
   ts-rs-auto-emitted per-type `#[test]` would also do this, but the
   explicit call list doubles as the canonical inventory.

The same test also runs `rebuild_index` (`tauri-shell/tests/bindings_export.rs:105`)
which scans the output dir and writes `index.ts` as a barrel.

### 2.3 Bindings directory

`tauri-shell/bindings/` (37 files total, 254 lines):

- One `.ts` file per exported type — 35 top-level + 1 nested.
- `index.ts` — barrel re-exporting all 35 (`tauri-shell/bindings/index.ts:5-39`).
- `serde_json/JsonValue.ts` — nested module emitted by `ts-rs` for
  `serde_json::Value` field references.
- `README.md` — consumer-facing doc (`tauri-shell/bindings/README.md`).
- `.gitkeep` — placeholder so the directory survives a `rm -rf` then
  `git checkout`.

### 2.4 Cargo plumbing

- `tauri-shell/Cargo.toml:54` — optional `ts-rs` dep with the
  `serde-json-impl` + `chrono-impl` features. The `chrono-impl` feature is
  there because `LogEntry` and event payloads use `chrono::Utc` timestamps
  internally (the wire format is `i64` ms-since-epoch, but bringing it in
  guards against future `DateTime<Utc>` fields).
- `tauri-shell/Cargo.toml:75-81` — feature definitions; `bindings =
  ["dep:ts-rs"]` is the only entry that touches `ts-rs`, so default
  builds neither compile nor link it.

## 3. How to use

### 3.1 "I just want to regenerate bindings"

```bash
cd tauri-shell
cargo test --features bindings --test bindings_export
```

The test creates `tauri-shell/bindings/<TypeName>.ts` for each annotated
struct and regenerates `tauri-shell/bindings/index.ts`. Idempotent —
re-running overwrites cleanly. Existing files for types that were
removed must be deleted manually (the test does not prune).

### 3.2 "I added a new IPC struct, how do I make it bind?"

1. Add the two `cfg_attr` lines on your struct. Copy the pattern from any
   existing annotated type, e.g. `tauri-shell/src/ipc/routines.rs:48-52`:

   ```rust
   #[derive(Debug, Clone, Serialize, Deserialize)]
   #[serde(rename_all = "camelCase")]
   #[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
   #[cfg_attr(feature = "bindings", ts(export, export_to = "../bindings/", rename_all = "camelCase"))]
   pub struct MyNewArgs {
       pub foo: String,
       pub maybe_bar: Option<i32>,
   }
   ```

   If your struct is *not* camelCase on the wire, drop the
   `rename_all = "camelCase"` from both `serde` and `ts` attrs (the
   binding inherits Rust's snake_case).

2. Add a use-import + a `MyNewArgs::export_all_to(&out)` line to
   `tauri-shell/tests/bindings_export.rs`. Pick a sensible spot in the
   `-- ipc payloads --` block (`tauri-shell/tests/bindings_export.rs:64-91`).
   This is what enforces "the binding cannot silently disappear".

3. Re-run the regenerate command from §3.1.

4. Commit the new `tauri-shell/bindings/MyNewArgs.ts` alongside the Rust
   change. The diff to `tauri-shell/bindings/index.ts` is regenerated by
   the test — also commit it.

### 3.3 "I want to consume bindings from my TS frontend"

Three common shapes:

a) **Symlinked into a Vite/Webpack frontend (simplest):**

```ts
// frontend/src/lib/shell.ts
import { invoke } from "@tauri-apps/api/core";
import type { BackendService, ServerSpec, ServerStatus } from "@/bindings";

export async function listBackends(): Promise<BackendService[]> {
    return invoke<BackendService[]>("list_backend_services");
}
```

Point your bundler at `tauri-shell/bindings/` via a path alias (Vite
`resolve.alias["@/bindings"] = "../tauri-shell/bindings"`).

b) **As a workspace package in a monorepo:**

```jsonc
// frontend/package.json
{
  "dependencies": {
    "@tauri-shell/bindings": "file:../tauri-shell/bindings"
  }
}
```

Then `import type { ... } from "@tauri-shell/bindings"`.

c) **Bulk namespace import** (useful in event listeners):

```ts
import type * as Shell from "@tauri-shell/bindings";
import { listen } from "@tauri-apps/api/event";

listen<Shell.ProcessOutputEvent>("process-output", (e) => {
    console.log(e.payload.processId, e.payload.type, e.payload.output);
});
```

Slice F (`tauri-shell/examples/typescript-frontend/`) shows pattern (a) in
practice — that is the recommended setup.

## 4. Public API surface

All 36 types, with their full export path on disk. Every entry maps
`Rust type @ source` → `bindings file`.

| TS export | Rust source | `.ts` artifact |
|---|---|---|
| `LogEntry` | `tauri-shell/src/state.rs:22` | `bindings/LogEntry.ts` |
| `InstallationSnapshot` | `tauri-shell/src/state.rs:245` | `bindings/InstallationSnapshot.ts` |
| `InstallationProgress` | `tauri-shell/src/state.rs:277` | `bindings/InstallationProgress.ts` |
| `ProcessOutputEvent` | `tauri-shell/src/events.rs:47` | `bindings/ProcessOutputEvent.ts` |
| `InstallProgressEvent` | `tauri-shell/src/events.rs:91` | `bindings/InstallProgressEvent.ts` |
| `BackendUrlEvent` | `tauri-shell/src/events.rs:105` | `bindings/BackendUrlEvent.ts` |
| `JupyterStatusEvent` | `tauri-shell/src/events.rs:115` | `bindings/JupyterStatusEvent.ts` |
| `NavigateEvent` | `tauri-shell/src/events.rs:124` | `bindings/NavigateEvent.ts` |
| `BackendService` | `tauri-shell/src/ipc/backends.rs:25` | `bindings/BackendService.ts` |
| `GenerateCertArgs` | `tauri-shell/src/ipc/certs.rs:19` | `bindings/GenerateCertArgs.ts` |
| `UpdateCredentialsArgs` | `tauri-shell/src/ipc/credentials.rs:42` | `bindings/UpdateCredentialsArgs.ts` |
| `CondaEnvironment` | `tauri-shell/src/ipc/environments.rs:23` | `bindings/CondaEnvironment.ts` |
| `Extension` | `tauri-shell/src/ipc/environments.rs:32` | `bindings/Extension.ts` |
| `ExecResult` | `tauri-shell/src/ipc/environments.rs:192` | `bindings/ExecResult.ts` |
| `JupyterStatus` | `tauri-shell/src/ipc/jupyter.rs:32` | `bindings/JupyterStatus.ts` |
| `McpSpec` | `tauri-shell/src/ipc/mcp.rs:21` | `bindings/McpSpec.ts` |
| `McpStatus` | `tauri-shell/src/ipc/mcp.rs:46` | `bindings/McpStatus.ts` |
| `ObbCallArgs` | `tauri-shell/src/ipc/obb.rs:20` | `bindings/ObbCallArgs.ts` |
| `RouteInfo` | `tauri-shell/src/ipc/openbb_meta.rs:33` | `bindings/RouteInfo.ts` |
| `RouteSearchArgs` | `tauri-shell/src/ipc/openbb_meta.rs:110` | `bindings/RouteSearchArgs.ts` |
| `RouteParamsArgs` | `tauri-shell/src/ipc/openbb_meta.rs:134` | `bindings/RouteParamsArgs.ts` |
| `ProviderSummary` | `tauri-shell/src/ipc/provider.rs:17` | `bindings/ProviderSummary.ts` |
| `ProviderValidateArgs` | `tauri-shell/src/ipc/provider.rs:93` | `bindings/ProviderValidateArgs.ts` |
| `RoutineMetadata` | `tauri-shell/src/ipc/routines.rs:52` | `bindings/RoutineMetadata.ts` |
| `RoutinesReadArgs` | `tauri-shell/src/ipc/routines.rs:92` | `bindings/RoutinesReadArgs.ts` |
| `RoutinesSaveArgs` | `tauri-shell/src/ipc/routines.rs:108` | `bindings/RoutinesSaveArgs.ts` |
| `RoutinesDeleteArgs` | `tauri-shell/src/ipc/routines.rs:126` | `bindings/RoutinesDeleteArgs.ts` |
| `RoutinesRenameArgs` | `tauri-shell/src/ipc/routines.rs:144` | `bindings/RoutinesRenameArgs.ts` |
| `ServerSpec` | `tauri-shell/src/ipc/server.rs:25` | `bindings/ServerSpec.ts` |
| `ServerStatus` | `tauri-shell/src/ipc/server.rs:49` | `bindings/ServerStatus.ts` |
| `ReadJsonArgs` | `tauri-shell/src/ipc/settings_files.rs:39` | `bindings/ReadJsonArgs.ts` |
| `WriteJsonArgs` | `tauri-shell/src/ipc/settings_files.rs:52` | `bindings/WriteJsonArgs.ts` |
| `ReadTextArgs` | `tauri-shell/src/ipc/settings_files.rs:68` | `bindings/ReadTextArgs.ts` |
| `WriteTextArgs` | `tauri-shell/src/ipc/settings_files.rs:85` | `bindings/WriteTextArgs.ts` |
| `IpcError` | `tauri-shell/src/ipc/mod.rs:37` | `bindings/IpcError.ts` |
| `JsonValue` | (from `ts-rs`'s `serde-json-impl`) | `bindings/serde_json/JsonValue.ts` |

## 5. Conventions

The five rules every annotated struct in this slice follows:

1. **camelCase on the wire.** Structs that cross IPC carry
   `#[serde(rename_all = "camelCase")]` AND, when ts-rs is on,
   `ts(rename_all = "camelCase")`. Both attributes are needed because
   `ts-rs` doesn't read `serde` attrs in its own macro expansion. Example:
   `tauri-shell/src/state.rs:241-244`.

2. **Optional/dev-only annotations.** Every binding line is
   `#[cfg_attr(feature = "bindings", ...)]`. Default `cargo build` /
   `cargo check` does not see `ts_rs`, does not load it, and does not
   produce a `.ts` file. The annotations are zero-cost in release.

3. **`serde_json::Value` → `JsonValue`.** Any `Value` field in a bound
   struct emits `JsonValue` on the TS side, imported from
   `./serde_json/JsonValue`. See `tauri-shell/bindings/ObbCallArgs.ts:2`
   for the import shape. This is what the `serde-json-impl` feature on
   `ts-rs` does.

4. **Optional fields → `T | null`.** `Option<i32>` becomes `number | null`,
   `Option<String>` becomes `string | null`. Note: *not* `T | undefined`
   — `ts-rs` chose `null` because serde_json emits `null`, not omission.
   See `tauri-shell/bindings/BackendService.ts:3` for many examples.

5. **`i64` / `u64` → `bigint`.** `ts-rs` is conservative about 64-bit
   integers because JS `number` is f64 and loses precision past 2^53.
   `ProcessOutputEvent.timestamp` (Rust `i64`) lands as `bigint`
   (`tauri-shell/bindings/ProcessOutputEvent.ts:10`). Renderer code that
   passes these into `JSON.stringify` must either use the value as
   `bigint` or coerce with `Number(...)` accepting precision loss for
   ms timestamps (fine for ~285,616-year span).

## 6. Known gaps + next steps

The slice closed every annotated struct on the IPC boundary, but the
generated TypeScript is not 100% expressive for every Rust shape that
travels over the wire. Five known gaps:

### 6.1 `IpcError` variants collapse to `& string`

`tauri-shell/src/ipc/mod.rs:37` is a single-tuple-variant enum
(`NotImplemented(String)`, `Io(String)`, …). `ts-rs` emits each variant
as `{ "kind": "not-implemented" } & string` (see
`tauri-shell/bindings/IpcError.ts:7`). That is syntactically valid TS but
semantically lossy — the renderer can discriminate on `kind` but the
inner message is typed as the whole intersection rather than as a
separate `message` field. Two follow-up options:

- Change the Rust enum to use named fields: `NotImplemented { message: String }` …
  this would emit `{ "kind": "not-implemented", "message": string }` cleanly.
- Live with it; treat `IpcError` as opaque on the renderer side and
  only inspect `(err as { kind: string }).kind`.

### 6.2 `obb_routes.rs` / `obb_routes_extended.rs` params are unbound

The 227 typed wrappers in `tauri-shell/src/ipc/obb_routes.rs:19` and
`tauri-shell/src/ipc/obb_routes_extended.rs:21` take
`type Params = Option<Map<String, Value>>` — a free-form JSON map. They
intentionally are *not* bound because they punt on schema typing (the
module docstring at `tauri-shell/src/ipc/obb_routes.rs:7-10` explains
why: typing those would duplicate `/openapi.json`). Recommendation:
generate per-route TS types from `/openapi.json` with
`openapi-typescript` and import those alongside our `.ts` bindings.

### 6.3 No CI guard against forgetting to regen

The export test only runs under `--features bindings`. A regular
`cargo test` in CI will skip it, so a struct rename in Rust can land
without the corresponding `.ts` update. Suggested follow-ups:

- Add a CI job that runs `cargo test --features bindings --test bindings_export`
  and then `git diff --exit-code tauri-shell/bindings/` to fail the
  build if the regenerated bindings differ from what was committed.
- Or: pre-commit hook with the same `git diff --exit-code` check.

### 6.4 `JsonValue` recursion does not include `null` or `boolean`

`tauri-shell/bindings/serde_json/JsonValue.ts:3` reads
`number | string | Array<JsonValue> | { [key: string]: JsonValue }`.
That is what `ts-rs` v9 emits. JSON `null` and `boolean` are missing.
Workaround: type the fields you care about with a stricter local type,
or upgrade `ts-rs` when the upstream fix lands and re-run the export.

### 6.5 `index.ts` is positional `export type` only

The barrel does not re-export runtime values (none of the types are
runtime values anyway since `ts-rs` emits type-only output), but it also
doesn't re-export the nested `serde_json/JsonValue`. A consumer who
imports `JsonValue` from `./bindings` will not find it — they have to
import from `./bindings/serde_json/JsonValue`. Fix: extend
`rebuild_index` (`tauri-shell/tests/bindings_export.rs:105`) to also walk
the `serde_json/` subdir.

### 6.6 No `tsc --noEmit` check

§2 of SPEC.md asked for `tsc --noEmit bindings/index.ts` as a
verification step. The repo does not ship a `tsconfig.json` at
`tauri-shell/bindings/`, so this remains a manual step. Two paths:

- Drop a minimal `tsconfig.json` next to `index.ts` and document the
  check in `bindings/README.md`.
- Defer to Slice F's TS frontend, which has a `tsconfig.json` of its
  own and will indirectly compile-check the bindings when it imports
  them.

## 7. Verification commands

Run from the repo root:

```bash
# 1. Default build must not see ts-rs.
cd tauri-shell && cargo check
# Expected: passes; ts-rs absent from Cargo.lock's resolved deps for this profile.

# 2. Regenerate bindings.
cd tauri-shell && cargo test --features bindings --test bindings_export
# Expected: test "force_export_all_bindings" passes;
# tauri-shell/bindings/*.ts is rewritten in place.

# 3. Inventory check.
ls tauri-shell/bindings/*.ts | wc -l
# Expected: 36 (35 type files + index.ts).

ls tauri-shell/bindings/serde_json/*.ts | wc -l
# Expected: 1 (JsonValue.ts).

# 4. Barrel sanity.
grep -c '^export type' tauri-shell/bindings/index.ts
# Expected: 35.

# 5. (Optional) Type-check from a TS toolchain.
cd tauri-shell/bindings && npx -y typescript@5 tsc --noEmit \
    --strict --target es2022 --module nodenext --moduleResolution nodenext index.ts
# Expected: 0 errors. (Skip if Node not installed.)
```

A 1-line drift sanity check for CI:

```bash
cd tauri-shell && cargo test --features bindings --test bindings_export \
  && git diff --exit-code bindings/
```

## 8. Integration with other slices

### Slice B — Connector trait (`tauri-shell/src/connector.rs`)

Slice B's `Connector` trait returns the same domain types that Slice A
binds. For example: `Connector::list_conda_environments` returns
`Vec<CondaEnvironment>` (`tauri-shell/src/ipc/environments.rs:23`), and
the TS renderer reads that as `CondaEnvironment[]` from
`tauri-shell/bindings/CondaEnvironment.ts`. When Slice B grew its arg
structs (`InstallArgs`, `SetupEnvArgs`, etc. — see `SPEC.md:104-109`),
those structs should likewise gain the two `cfg_attr` lines and a row in
`tests/bindings_export.rs`. Audit suggestion: walk
`tauri-shell/src/connector.rs` for any `pub struct` that lacks the
binding annotations and add them in a follow-up PR.

### Slice F — Example TS frontend (`tauri-shell/examples/typescript-frontend/`)

This is the canonical consumer. It imports directly from
`tauri-shell/bindings/index.ts` (pattern §3.3 above). The frontend's
`tsconfig.json` is the closest thing this repo has to a `tsc` smoke
test of the bindings; if the example builds, the bindings are
syntactically valid TypeScript.

### Slice G — `tauri-shell-cli` companion binary (`tauri-shell/src/bin/cli.rs`)

The CLI invokes the same Rust functions the Tauri commands wrap, so it
emits identical JSON shapes — meaning any external consumer (e.g. a
shell script piping the CLI through `jq`) can rely on the exact same
camelCase schema documented in `tauri-shell/bindings/`. The bindings are
in effect the JSON-schema documentation for both the Tauri IPC and the
CLI's stdout.

### Slice C — Extended typed wrappers (`tauri-shell/src/ipc/obb_routes_extended.rs`)

Deliberately *not* in this slice's surface — see §6.2. Slice C's 167
wrappers all take `Option<Map<String, Value>>` and return `Value`, so
their TS shape is `Record<string, JsonValue> | null` → `JsonValue`, which
is uninformative. The follow-up here is to bolt on `openapi-typescript`
on top of `/openapi.json` rather than to expand Slice A.

### Slice D — Integration tests

Independent. Slice A's bindings test (`tests/bindings_export.rs`) lives
in the same `tests/` directory but does not interact with Slice D's
fixtures; it only runs under `--features bindings`, while Slice D's
tests run by default.
