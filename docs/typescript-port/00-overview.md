# OpenBBPort → TypeScript Port: Documentation Overview

## Welcome

This `docs/typescript-port/` tree is the planning artifact for migrating the
OpenBBPort desktop app from its current Tauri + Rust + React shape onto a
TypeScript-first stack. It is not a tutorial and not a changelog — it is the
**reference corpus** a port team needs in front of them while making
architectural decisions, writing code, and reviewing pull requests. Every claim
is backed by `file:line` citations into the live source tree so that any reader
can verify the description against the implementation.

The corpus was produced by a structured multi-agent investigation in May 2026.
Wave 1 ran nine isolated per-page deep-dives. Wave 1B sent eight cross-cutting
reviewers to catch what the isolated agents missed. Wave 2 synthesized those raw
notes into eleven structured feature specs. Wave 3 produced the cross-cutting
architecture digests, the layer-by-layer port strategy, and the phased roadmap.
Every agent was Claude Opus. The methodology — prompt structure, handoff file,
v1/v2 review pattern — is preserved in [`HANDOFFS.md`](./HANDOFFS.md) and
[`30-port/port-design-process.md`](./30-port/port-design-process.md).

## What this project is

**OpenBBPort** is a cross-platform desktop application that installs and
operates a self-contained Python data-platform on the end user's machine. It is
three independently-runnable layers glued together by a settings tree at
`~/.openbb_platform/`: (1) the **Tauri desktop** (Rust backend + React frontend
in a system WebView) that hosts the UI and orchestrates everything, (2) the
**Python `openbb-api` REST server** (the "Platform") which is the actual data
plane that providers and the desktop UI both consume, and (3) the **Python
`openbb` CLI REPL** which is an in-process menu-driven shell over the same
Python SDK. The desktop owns conda env CRUD, extension installs, credential
storage, subprocess supervision, system-tray and autostart, an updater, and a
clean uninstall cascade. None of this exists in a vacuum — full reading of
`architecture-overview.md` is recommended before any port code is written.

## How to use this docs/ tree

### If you are deciding whether/how to port

Read in this order:

1. [`00-overview.md`](./00-overview.md) (this file)
2. [`10-architecture/architecture-overview.md`](./10-architecture/architecture-overview.md) — the three-layer mental model
3. [`30-port/port-strategy.md`](./30-port/port-strategy.md) — rewrite vs wrap vs skip, decided per layer
4. [`30-port/port-tech-stack.md`](./30-port/port-tech-stack.md) — concrete dependency choices and their trade-offs
5. [`30-port/port-roadmap.md`](./30-port/port-roadmap.md) — phased plan, 8 phases, 18-27 weeks
6. [`30-port/port-design-process.md`](./30-port/port-design-process.md) — working methodology, ADRs, review gates

### If you are implementing a specific feature

Read in this order:

1. [`00-overview.md`](./00-overview.md) and [`10-architecture/architecture-overview.md`](./10-architecture/architecture-overview.md)
2. [`20-features/feature-<your-feature>.md`](./20-features/) — the synthesized spec following the Wave 2 template
3. [`30-port/port-invoke-mapping.md`](./30-port/port-invoke-mapping.md) — every Tauri invoke and event mapped to a TS equivalent
4. [`raw-deep-dives/<your-feature>.md`](./raw-deep-dives/) plus the `.v2.md` second-pass — the citation layer with `file:line`
5. [`10-architecture/state-and-storage.md`](./10-architecture/state-and-storage.md) — shared state surfaces, lock contracts, race windows

### If you are reviewing security

Read in this order:

1. [`00-overview.md`](./00-overview.md)
2. [`20-features/feature-api-keys.md`](./20-features/feature-api-keys.md) — credential vault surfaces, plaintext disk, no-lock writer race
3. [`10-architecture/ipc-bridge.md`](./10-architecture/ipc-bridge.md) — capability scoping and the `window.eval` allow-all gap
4. [`raw-deep-dives/ipc-bridge.v2.md`](./raw-deep-dives/ipc-bridge.v2.md) — the `open_url_in_window` capability-inheritance gap
5. [`raw-deep-dives/logs-streaming.v2.md`](./raw-deep-dives/logs-streaming.v2.md) — `dangerouslySetInnerHTML` XSS surface in the log viewer

### If you are debugging a flaky behavior in the current Tauri app

Read in this order:

1. [`00-overview.md`](./00-overview.md)
2. The relevant [`20-features/feature-<X>.md`](./20-features/) "Known bugs" section
3. The matching [`raw-deep-dives/<X>.v2.md`](./raw-deep-dives/) — corrections, additions, and disagreements with v1
4. [`10-architecture/process-lifecycle.md`](./10-architecture/process-lifecycle.md) if the symptom is process-related (zombie PIDs, kill-by-port, conda-run wrappers)

## Feature dependency graph

This is the cross-reference graph between feature docs, lifted from
[`HANDOFFS.md`](./HANDOFFS.md). It is the cheapest mental model of "what
breaks if I change X":

```
installation ──► environments ──► extensions
                      │
                      ├──► jupyter ───────────────┐
                      │                           │
                      └──► backends ──────────────┼──► logs-streaming
                                                  │
              api-keys ──► platform-rest-api ◄────┘
                                  ▲
tray-and-autostart ──► (all backend services)
                                  │
cli-repl ──► platform-rest-api (in-process via SDK)
                                  │
uninstall ──► (stops + removes everything)
```

`installation` is the root: nothing else works until it succeeds. `logs-streaming`
is shared infrastructure: every subprocess-spawning feature funnels through
`process-output`. `platform-rest-api` is the data plane that the UI, the CLI,
and any AI agent (via `openbb-mcp`) all consume. `uninstall` is the terminal
cascade — it reaches every other feature's state and tears it down.

## Glossary

- **OBBject** — the canonical response container produced by every Platform endpoint; carries `results`, `provider`, `warnings`, and metadata.
- **Conda env** — a Python environment under `<install_dir>/envs/`, managed by `conda`/`mamba`/`micromamba` and described by a YAML at `~/.openbb_platform/environments/<env>.yaml`.
- **`openbb-api`** — the Python FastAPI REST server that exposes the OpenBB Platform; the data plane.
- **`openbb-mcp`** — the parallel Model Context Protocol server, same provider surface for AI agents.
- **`openbb` (CLI)** — the Python REPL shell; an in-process consumer of the same SDK, separate from the REST server.
- **Tauri** — the current Rust + WebView desktop framework hosting the React UI and all Rust-side handlers.
- **`process-output`** — the single multiplexed Tauri event channel onto which every spawned subprocess's stdout/stderr is forwarded as `{id, kind, line}` payloads.
- **`LogStorage`** — the per-process ring-buffer singleton in Rust; bounded history per process ID so a UI can re-attach without losing context.
- **`RunningProcesses`** — the `Map<id, Child>` of currently-tracked subprocesses in Rust state.
- **`ACTIVE_JUPYTER_SERVERS`** — a separate `static` map keyed by env name that tracks running JupyterLab servers (the only producer with its own store).
- **Wrapper PID vs server PID** — `conda run` spawns a Python process as a grandchild; the wrapper is what `RunningProcesses` holds, the server is what listens on the port. Killing the wrapper does not always kill the server — see `process-lifecycle.md`.
- **Process ID (namespace)** — string keys with feature-prefixed conventions: `jupyter-<env>`, `backend-<uuid>`, `installation`, `extension-install-<env>`.
- **`~/.openbb_platform/`** — the shared settings + extensions + envs directory; the only persistence surface that survives uninstall when "keep settings" is checked.
- **Strategy A / B / C** — rewrite / wrap / skip, applied per layer in `port-strategy.md`.
- **Wave 1 / 1B / 2 / 3** — the four investigation phases (see "Investigation methodology" below).
- **ADR** — Architecture Decision Record, the format used to log irreversible choices (`port-design-process.md`).

## File map

```
docs/typescript-port/
├── 00-overview.md                          — This file. Entry point and reading order.
├── HANDOFFS.md                             — Multi-agent workflow status table + Wave 2 doc template.
│
├── 10-architecture/                        — Cross-cutting digests (Wave 3, port-team-facing).
│   ├── architecture-overview.md            — 3-layer system summary; read this first.
│   ├── ipc-bridge.md                       — Tauri IPC patterns; 57 commands and 8 events catalogued.
│   ├── process-lifecycle.md                — Subprocess spawn / monitor / kill / cleanup model.
│   └── state-and-storage.md                — Every byte of state: files, memory, browser, contracts.
│
├── 20-features/                            — Synthesized per-feature specs (Wave 2, structured template).
│   ├── feature-installation.md             — Setup wizard + Miniforge install pipeline.
│   ├── feature-environments.md             — Conda env CRUD; YAML lifecycle.
│   ├── feature-extensions.md               — Python package install/remove inside an existing env.
│   ├── feature-backend-services.md         — Long-lived HTTP servers (openbb-api, openbb-mcp, custom).
│   ├── feature-jupyter.md                  — Per-env JupyterLab launch and stop.
│   ├── feature-api-keys.md                 — Credential vault and bulk-import surface.
│   ├── feature-logs-streaming.md           — Shared subprocess-output plumbing.
│   ├── feature-platform-rest-api.md        — The openbb-api server itself (data plane).
│   ├── feature-cli-repl.md                 — The openbb Python REPL shell.
│   ├── feature-tray-and-autostart.md       — Tray menu, updater, autostart, close-to-tray.
│   └── feature-uninstall.md                — Full removal cascade with optional data preservation.
│
├── 30-port/                                — Port decisions, dependency choices, plan (Wave 3).
│   ├── port-strategy.md                    — Layer-by-layer A/B/C decision with rationale.
│   ├── port-tech-stack.md                  — Every major dependency choice and trade-off.
│   ├── port-invoke-mapping.md              — Tauri command/event → TS equivalent checklist.
│   ├── port-roadmap.md                     — 8 phases, 18-27 weeks, shippable artifact per phase.
│   └── port-design-process.md              — Working methodology, ADRs, review gates.
│
└── raw-deep-dives/                         — Citation layer; per-page traces with file:line evidence.
    ├── installation.md / .v2.md            — Setup wizard pipeline.
    ├── environments.md / .v2.md            — Conda env handlers and YAML races.
    ├── backend-services.md / .v2.md        — Backend HTTP servers, port resolution.
    ├── api-keys.md / .v2.md                — Credential surfaces and plaintext disk.
    ├── logs-streaming.md / .v2.md          — process-output channel and LogStorage.
    ├── app-shell.md / .v2.md               — Root layout, tray, uninstall, updater.
    ├── ipc-bridge.md / .v2.md              — Full IPC catalog and capability gaps.
    ├── platform-rest-api.md / .v2.md       — Python openbb-api server internals.
    └── cli-repl.md                         — Python REPL menu shell (no v2; single-pass).
```

## Statistics

A few numbers that orient the scale of the system and the corpus:

- Documentation corpus: **~16,000 lines** across **37 files** (1 overview, 1 handoff, 4 architecture, 11 feature, 5 port, 9 raw + 8 v2 deep-dives).
- IPC surface: **57 invoke commands**, **8 events** (one of which, `process-output`, multiplexes all subprocess output and accounts for the majority of runtime IPC traffic).
- Known bugs catalogued: **~70** explicitly tagged `⚠️ BUG:` across feature and raw docs; the port is expected to fix or consciously inherit each.
- Subprocess producers: **5** (Miniforge installer, env create/update, extension install, Jupyter launch, generic backend spawn) — every one routes through the same `process-output` channel.
- PID-tracking stores in Rust state: **3** (`RunningProcesses`, `ACTIVE_JUPYTER_SERVERS`, plus ad-hoc one-shots inside specific handlers).
- Persistence files under `~/.openbb_platform/`: **9** distinct documented paths plus the `environments/` and `user_data/` subtrees.
- Python REST surface: dozens of endpoints exposed by `openbb-api`, all returning `OBBject`; full inventory in `feature-platform-rest-api.md`.
- Cold start: **8-15 seconds** from `openbb-api` invocation to listening socket on a full install — load-bearing for any "is the server up?" polling logic.
- Estimated port effort: **18-27 weeks** for full parity per `port-roadmap.md`, with shippable artifacts at each phase boundary.

## Investigation methodology

The corpus was produced in four waves, each fully completing before the next began. The wave numbers appear in `HANDOFFS.md` status tables and are referenced throughout the docs as shorthand for "what evidence backs this claim":

- **Wave 1** — Nine deep-dive agents ran in parallel, each one isolated to a single page or surface (installation, environments, backends, API keys, logs, app shell, IPC, REST API, CLI). Each agent worked from the source tree only, with no knowledge of the others' findings. Output: `raw-deep-dives/<topic>.md` with exhaustive `file:line` citations.
- **Wave 1B** — Eight second-pass agents. Each one was given its own v1 plus the v1s of adjacent features and asked specifically to surface what the isolated view missed: events referenced by another feature, races on shared files, conflicting claims, capability gaps that only show up when two surfaces interact, and "the v1 said X but the IPC catalog actually says Y" disagreements. Output: `<topic>.v2.md`. This pass produced most of the high-value bug findings.
- **Wave 2** — Eleven feature writers consumed v1+v2 and produced structured specs against a fixed template (Purpose, User flows, UI surface, Data flow, IPC contract, State surfaces, Persistence, Error handling, Interfaces with, TS port mapping, Known bugs, Open questions). The template is reproduced verbatim in `HANDOFFS.md`. Output: `20-features/feature-*.md`.
- **Wave 3** — Cross-cutting writers. Four architecture digests synthesized over all feature docs (architecture overview, IPC bridge, process lifecycle, state and storage); five port docs decided rewrite vs wrap per layer, picked dependencies, mapped every IPC call, and produced the phased roadmap; this overview ties them together.

The pattern — isolated raw pass, cross-cutting review pass, structured synthesis, executive digest — is reusable for any complex codebase investigation; see `30-port/port-design-process.md` for prompts and checklists.

## Open questions for the user

Before the port begins, five high-level decisions need a human answer. The docs surface the trade-offs but cannot decide for you:

1. **Per-layer Strategy.** Which of A (rewrite), B (wrap the Python), or C (skip) for each of the four layers? `port-strategy.md` recommends a mixed B/B/C/B stance for v1 but lays out the cost/value for every cell.
2. **Top-level framework.** Tauri 2 stays, or switch to Electron, or Wails, or something else? `port-tech-stack.md` Layer 1 lists the candidates and their cascading effects on the rest of the stack.
3. **Provider subset.** The Platform supports many upstream providers; some are not portable (`yfinance` has no equivalent JS client of comparable quality). Which subset must the port support on day one? Affects Strategy choices for Layer 2.
4. **Workspace integration scope.** The current desktop integrates with the OpenBB Workspace (`widgets.json`, `apps.json`, `agents.json`). Is that surface in scope for v1, or deferred? `port-strategy.md` Layer 4 frames this as a parallel discussion.
5. **Time budget.** A full parity port through Phase 0-8 is **18-27 weeks** per `port-roadmap.md`. Is that budget acceptable, or must scope be cut? Each phase ships a usable artifact, so partial completion is viable.

## Acknowledgments

This documentation was produced by a multi-agent investigation in May 2026 using
Claude Opus. The methodology, prompts, and structure are themselves an artifact
worth preserving — see [`HANDOFFS.md`](./HANDOFFS.md) for the orchestration
record and [`30-port/port-design-process.md`](./30-port/port-design-process.md)
for how to apply the same approach to the port itself. Every claim in this
corpus is backed by a `file:line` citation into the OpenBBPort source tree at
the May 2026 commit; if the source has moved since, the raw deep-dives are the
fastest way to re-anchor.
