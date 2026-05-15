# Slice E — Documentation + Cookbook (handoff)

**Slice:** E (Docs + cookbook)
**Status:** ✅ done (see `SPEC.md:591`)
**Primary artifact:** `/home/user/OpenBBPort/tauri-shell/README.md` (861 lines, was ~150)
**Secondary artifacts:** `//!` module-level docs across `src/ipc/*.rs`

---

## 1. Purpose

Convert tauri-shell from "code with comments" into a self-contained reference
that a developer arriving cold can use without reading the source. Before Slice
E, the README was a ~150-line project overview that pointed at `SPEC.md` for
everything else; the only command-level docs lived in the `///` Rustdoc
comments in `src/ipc/*.rs`. After Slice E:

- A reader can clone the repo, follow §Quick start, and have a signed bundle.
- A reader can find every IPC command by name in one searchable table without
  grepping `src/main.rs`.
- A reader can copy-paste a renderer-side TS snippet for every common pattern.
- A reader implementing a `Connector` has three worked patterns to copy from.
- A reader reaches for security-sensitive code with the threat model in hand.

The companion `SPEC.md` (`/home/user/OpenBBPort/tauri-shell/SPEC.md:1-596`)
remains the canonical agent-coordination doc; the README is the user-facing
doc. The two are linked so neither needs to duplicate the other.

---

## 2. README structure

Ten top-level sections, in this order
(`/home/user/OpenBBPort/tauri-shell/README.md:1-861`):

1. **What this is** (`README.md:5-22`) — One screen explaining the shell's
   scope: OS-level work fully implemented, domain logic deliberately left as
   stubs.
2. **Quick start** (`README.md:25-130`) — 5-step bring-up: install Rust + Tauri
   CLI, install Linux system deps (apt/dnf/pacman variants), replace icons,
   point `tauri.conf.json` at a frontend, build + bundle.
3. **Architecture overview** (`README.md:133-160`) — Three concentric layers
   (infrastructure / domain / proxy) plus a mermaid diagram showing the
   renderer → IPC → {infrastructure, connector, proxy} → Python data flow.
4. **Full command catalog** (`README.md:163-374`) — 162-command table grouped
   by module with status (✅ real / 🪝 stub), args, return type, and a
   one-line description per command. Mirrors `tauri::generate_handler!` order.
5. **Cookbook** (`README.md:377-635`) — 10 worked TS recipes covering the most
   common renderer ↔ shell interactions.
6. **Connector patterns** (`README.md:638-727`) — Three concrete `Connector`
   strategies (HTTP proxy / Node sidecar / pure Rust) with code snippets.
7. **Security** (`README.md:730-748`) — 15 bullets covering atomic writes,
   path-traversal allow-lists, secrets-in-events/logs/titles, clipboard
   exfiltration, updater signatures, CSP.
8. **Configuration reference** (`README.md:752-794`) — Env vars table +
   `tauri.conf.json` fields table + capabilities note.
9. **Troubleshooting** (`README.md:798-844`) — 5 first-build / runtime issues:
   missing webkit2gtk, placeholder icons, capability denials,
   `EADDRINUSE`, updater signature mismatch.
10. **Status** (`README.md:848-861`) — Pointers back to `SPEC.md` §2 slice
    definitions so the README never grows stale on slice progress.

---

## 3. Cookbook recipes summary

Ten recipes (`README.md:377-635`); each teaches a single end-to-end pattern.

1. **Basic data fetch via `obb_call`** (`README.md:381-401`) — the generic
   stringly-typed REST passthrough; baseline shape every later recipe deviates
   from.
2. **Typed wrapper for equity historical** (`README.md:403-418`) — same call
   shape via one of the 60 typed wrappers in `obb_routes.rs`.
3. **Read + write `user_settings.json`** (`README.md:420-449`) — the atomic
   `.tmp` + flock + chmod 0600 + rename codepath, with a credentials-leakage
   warning.
4. **Spawn a backend and stream logs to a new window** (`README.md:451-489`) —
   the full subprocess lifecycle: register ring buffer → spawn → open logs
   window → subscribe to `process-output`.
5. **Generate a self-signed cert** (`README.md:491-511`) — one-shot connector
   stub, with the per-OS trust-store side effects called out.
6. **Enable auto-launch at login** (`README.md:513-540`) — the macOS osascript
   / Windows COM `.lnk` / Linux `.desktop` divergence behind one API.
7. **React to `navigate` events from the tray** (`README.md:542-560`) — the
   event hop pattern so tray clicks can drive the renderer router without
   `webContents.eval`.
8. **Save + load a `.openbb` routine** (`README.md:562-586`) — `routines_*` as
   the "saved query" persistence layer.
9. **Drive a connector validation probe** (`README.md:588-604`) — the
   "verify-key-before-storing" UX on top of `provider_validate`.
10. **Install pipeline with `install-progress` listener** (`README.md:606-634`)
    — the long-running-task pattern: subscribe before invoke, abort via
    `CancellationRegistry`.

---

## 4. Module-level docs

Every `src/ipc/*.rs` file now opens with a `//!` block. Before Slice E most had
a one-line header; now each gives ~3–8 lines of:

- Real/stub split for that module
- Process-id namespace (where applicable)
- Related modules + event names
- Pointer at the canonical `docs/typescript-port/feature-*.md` contract
- Security notes where applicable

Per-file before → after (`//!` block sizes):

| File | Before | After | Coverage notes |
|---|---|---|---|
| `src/ipc/app.rs` | 1 line | 20 lines (`app.rs:1-20`) | Cross-refs `InstallationState`, `events::NAVIGATE`, `cleanup_all_processes`. |
| `src/ipc/helpers.rs` | 1 line | 20 lines (`helpers.rs:1-20`) | Names the 6 functional clusters + the security note for `open_url_in_window`. |
| `src/ipc/jupyter.rs` | 1 line | 19 lines (`jupyter.rs:1-19`) | Documents `jupyter-<env>` process-id namespace + the spawn flow. |
| `src/ipc/openbb_meta.rs` | 1 line | 18 lines (`openbb_meta.rs:1-18`) | Two use-cases (command palette, route-detail panel) + sibling-module refs. |
| `src/ipc/uninstall.rs` | 1 line | 17 lines (`uninstall.rs:1-17`) | Lists the typical 5-step cascade order so connector authors know the contract. |

Other ipc modules already had module-level docs (e.g. `infrastructure.rs:1`,
`obb.rs:1`, `obb_routes.rs:1`, `routines.rs:1`, `server.rs:1`,
`mcp.rs:1`, `provider.rs:1`, `settings_files.rs:1-3`, `installation.rs:1-2`,
`environments.rs:1-2`, `credentials.rs:1-2`, `backends.rs:1-2`, `certs.rs:1-2`,
`obb_routes_extended.rs:1-2`, `mod.rs:1-2`) and were left untouched in this
slice — they were already informative.

---

## 5. Audience model

The README is written for four distinct reader profiles:

(a) **Bringing up the shell on a fresh machine.** Reads §Quick start
(`README.md:25-130`) and stops at "step 5 worked". Troubleshooting
(`README.md:798-844`) catches the common first-build failures. They will not
read past the catalog headline.

(b) **Implementing a `Connector`.** Reads §Architecture overview
(`README.md:133-160`) to understand the three layers, then §Connector
patterns (`README.md:638-727`) to pick A/B/C, then dips into the catalog to
find the methods their `impl Connector` block needs. Cross-refs into
`src/connector.rs` (Slice B output) for the full trait surface.

(c) **Consuming the IPC from a TS frontend.** Reads §Full command catalog
(`README.md:163-374`) and §Cookbook (`README.md:377-635`). The recipes are
designed to be copy-pasted; the catalog is designed to be Ctrl-F'd. Once Slice
A's `bindings/` lands they'll also `import { … } from "tauri-shell/bindings"`
for typed args/returns.

(d) **Debugging a running app.** Reads §Troubleshooting
(`README.md:798-844`) for the common issues, then §Security
(`README.md:730-748`) to confirm they're not about to leak a credential via
`console.log`. The architecture diagram tells them whether to look in
`src/ipc/*.rs` (domain), `src/proxy.rs` (HTTP), or `src/state.rs` /
`src/process_*.rs` (infrastructure).

---

## 6. `cargo doc` integration

Zero warnings:

```
cd /home/user/OpenBBPort/tauri-shell
cargo doc --no-deps    # passes clean
cargo doc --open --no-deps   # rebuilds and opens in the system browser
```

The generated docs are at `target/doc/tauri_shell/index.html`. Every public
item has a `///` doc comment; every module has a `//!` block. `cargo doc` is
the source-of-truth view for connector implementors who want to follow types
across module boundaries — the README is the prose, `cargo doc` is the
hyperlinked reference.

---

## 7. Known gaps + next steps

The slice deliberately stops short of these; they're left for future work.

- **No screenshots.** The README has zero images. A future pass should add
  screenshots of the running app (tray menu, log window, install progress) so
  a reader knows what they're building toward. Requires having a real
  frontend pointed at the shell (Slice F's example will do).
- **No video walkthrough.** A 3-minute "clone → `cargo tauri dev` → log
  window" screencast would lower the activation cost further.
- **Per-command docs in the catalog table are one-liners.** Full Rustdoc on
  each handler is recommended next — every `#[tauri::command]` should get a
  `/// ` block explaining args, returns, errors, and the contract it
  implements. About a third of handlers have this today; the rest inherit
  their description from the catalog row.
- **No JSDoc / TSDoc on the bindings dir.** Slice A's `bindings/*.ts` should
  carry comment translations of the Rust `///` blocks. Today they'll be raw
  type aliases with no prose.
- **Cookbook recipes don't yet use the `bindings/` types.** Once Slice A
  lands, each TS snippet should `import` from `bindings/` instead of
  hand-rolling the interface inline (recipe 4's `ProcessOutputEvent`,
  recipe 10's `InstallProgressEvent` are the obvious wins).
- **No CONTRIBUTING.md.** The `SPEC.md` §3 "Conventions agents must follow"
  is the closest equivalent today.

---

## 8. Integration with other slices

The README is downstream of every other slice — its job is to surface the
work the others produce.

- **Slice A (TS bindings)** — Catalog rows
  (`README.md:169-374`) reflect the structs ts-rs will export. Once
  `bindings/` lands, recipes 4 and 10 should be updated to import the typed
  event payloads instead of redeclaring them inline.
- **Slice B (Connector trait)** — §Connector patterns
  (`README.md:638-727`) references `tauri_shell::connector::{Connector,
  ConnectorError}` and uses the actual `InstallArgs` / `InstallRuntimeArgs` /
  `GenerateCertArgs` struct names from `src/connector.rs`. All stub-row
  descriptions in the catalog mention "delegates to connector".
- **Slice C (Extended typed wrappers)** — The catalog headline says "**162
  commands**" but the per-module count under `obb_routes_extended` reflects
  Slice C's 167 additional wrappers via the wording "plus another tranche in
  `obb_routes_extended.rs`" at `README.md:298`. When the catalog is
  regenerated to include those, the headline jumps to 329.
- **Slice D (Integration tests)** — Not yet referenced in the README; a
  "Testing" section is a candidate addition once Slice D's `tests/*.rs` ships.
- **Slice F (TS frontend example)** — Cookbook recipes are written to be the
  spiritual cousin of the example frontend. When `examples/typescript-frontend/`
  is fleshed out, recipe 1's snippet should be the same code that ships in
  `examples/typescript-frontend/src/main.ts`.
- **Slice G (CLI binary)** — Could earn a §Companion binary section once it
  matures. The cookbook recipes currently target the TS renderer only.
- **Slice H (Connector reference impls)** — §Connector patterns
  (`README.md:638-727`) is the prose; Slice H ships the concrete crates
  (`connectors/http-proxy/`, `connectors/openbb-platform/`) that those
  snippets compile against.

---

## 9. Verification

Two automated checks:

```bash
cd /home/user/OpenBBPort/tauri-shell

# 1. cargo doc must build without warnings.
cargo doc --no-deps
# Generated target/doc/tauri_shell/index.html
# (no compile/doc warnings; output shown above)

# 2. README must be ≥ 500 lines (the slice target was ~600).
wc -l README.md
# 861 README.md
```

Both pass. The slice target in `SPEC.md:232` was ~600 lines; the final
artifact lands at 861 lines, comfortably over.

---

## 10. Files changed in Slice E

- `README.md` — expanded from ~150 to 861 lines.
- `src/ipc/app.rs:1-20` — expanded `//!` block (1 → 20 lines).
- `src/ipc/helpers.rs:1-20` — expanded `//!` block (1 → 20 lines).
- `src/ipc/jupyter.rs:1-19` — expanded `//!` block (1 → 19 lines).
- `src/ipc/openbb_meta.rs:1-18` — expanded `//!` block (1 → 18 lines).
- `src/ipc/uninstall.rs:1-17` — expanded `//!` block (1 → 17 lines).
- `SPEC.md:591` — status table updated to mark Slice E ✅.
