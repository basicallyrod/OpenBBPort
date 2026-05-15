# OpenBBPort → TypeScript Port: Handoff Index

This directory is the **shared workspace** for the multi-agent investigation and
port-planning effort. Every deep-dive agent writes its raw findings to
`raw-deep-dives/`. Wave 2 writer agents synthesize those into structured
feature docs in `20-features/`. Wave 3 produces architecture and port-strategy
docs in `10-architecture/` and `30-port/`.

**If you are an agent picking up work here — read this file first.**

## Status

Legend: ⬜ not started · 🟡 in progress · ✅ done · 🔁 needs revision

### Wave 1 — Deep-dive analysis (raw per-page traces)

| # | Deep-dive | File | Status | Owner |
|---|---|---|---|---|
| 1 | Setup + installation pages | `raw-deep-dives/installation.md` | ✅ | Wave 1 agent (Opus) |
| 2 | Environments page | `raw-deep-dives/environments.md` | ✅ | Wave 1 agent (Opus) |
| 3 | Backends page | `raw-deep-dives/backend-services.md` | ✅ | Wave 1 agent (Opus) |
| 4 | API Keys page | `raw-deep-dives/api-keys.md` | ✅ | Wave 1 agent (Opus) |
| 5 | Logs pages + process monitor | `raw-deep-dives/logs-streaming.md` | ✅ | Wave 1 agent (Opus) |
| 6 | Root layout + index + uninstall + tray | `raw-deep-dives/app-shell.md` | ✅ | Wave 1 agent (Opus) |
| 7 | Tauri ↔ Frontend bridge (IPC catalog) | `raw-deep-dives/ipc-bridge.md` | ✅ | Wave 1 agent (Opus) |
| 8 | Python openbb-api server | `raw-deep-dives/platform-rest-api.md` | ✅ | Wave 1 agent (Opus) |

### Wave 1B — Second-pass review (find-what-was-missed)

Each v2 agent reads its own raw v1 file PLUS adjacent ones to cross-check claims and
find what the isolated v1 view missed (events referenced by other features, file races,
state shared across features, conflicting claims, etc.). Output appended to v1 file as
a `## v2 Addendum` section, OR written to a separate `*.v2.md` file.

| # | v2 deep-dive | Reads (v1) | Status |
|---|---|---|---|
| 1 | Installation v2 | `installation.md` + `app-shell.md` + `environments.md` + `ipc-bridge.md` | ✅ → `installation.v2.md` |
| 2 | Environments v2 | `environments.md` + `installation.md` + `backend-services.md` + `logs-streaming.md` + `ipc-bridge.md` | ✅ → `environments.v2.md` |
| 3 | Backends v2 | `backend-services.md` + `logs-streaming.md` + `environments.md` + `platform-rest-api.md` + `ipc-bridge.md` | ✅ → `backend-services.v2.md` |
| 4 | API Keys v2 | `api-keys.md` + `platform-rest-api.md` + `installation.md` + `ipc-bridge.md` | ✅ → `api-keys.v2.md` |
| 5 | Logs v2 | `logs-streaming.md` + `backend-services.md` + `environments.md` + `ipc-bridge.md` | ✅ → `logs-streaming.v2.md` |
| 6 | App Shell v2 | `app-shell.md` + `installation.md` + `backend-services.md` + `ipc-bridge.md` | ✅ → `app-shell.v2.md` |
| 7 | IPC Bridge v2 | `ipc-bridge.md` + ALL feature deep-dives | ✅ → `ipc-bridge.v2.md` |
| 8 | Platform REST API v2 | `platform-rest-api.md` + `api-keys.md` + `backend-services.md` + `installation.md` | ✅ → `platform-rest-api.v2.md` |

### Wave 2 — Structured feature docs (queued; fires when Wave 1 completes)

| Feature doc | Reads | Status |
|---|---|---|
| `20-features/feature-installation.md` | `raw-deep-dives/installation.md` | ⬜ |
| `20-features/feature-environments.md` | `raw-deep-dives/environments.md` | ⬜ |
| `20-features/feature-extensions.md` | `raw-deep-dives/environments.md` + `installation.md` | ⬜ |
| `20-features/feature-backend-services.md` | `raw-deep-dives/backend-services.md` + `logs-streaming.md` | ⬜ |
| `20-features/feature-jupyter.md` | `raw-deep-dives/logs-streaming.md` + `environments.md` | ⬜ |
| `20-features/feature-api-keys.md` | `raw-deep-dives/api-keys.md` | ⬜ |
| `20-features/feature-logs-streaming.md` | `raw-deep-dives/logs-streaming.md` | ⬜ |
| `20-features/feature-tray-and-autostart.md` | `raw-deep-dives/app-shell.md` | ⬜ |
| `20-features/feature-uninstall.md` | `raw-deep-dives/app-shell.md` | ⬜ |
| `20-features/feature-platform-rest-api.md` | `raw-deep-dives/platform-rest-api.md` | ⬜ |
| `20-features/feature-cli-repl.md` | (new deep-dive needed) | ⬜ |

### Wave 3 — Architecture + port strategy (queued)

| Doc | Reads | Status |
|---|---|---|
| `10-architecture/architecture-overview.md` | all feature docs | ⬜ |
| `10-architecture/ipc-bridge.md` | `raw-deep-dives/ipc-bridge.md` | ⬜ |
| `10-architecture/process-lifecycle.md` | `logs-streaming.md`, `backend-services.md`, `app-shell.md` | ⬜ |
| `10-architecture/state-and-storage.md` | all | ⬜ |
| `30-port/port-strategy.md` | `platform-rest-api.md` + all feature docs | ⬜ |
| `30-port/port-tech-stack.md` | `ipc-bridge.md` + `app-shell.md` | ⬜ |
| `30-port/port-invoke-mapping.md` | every feature doc's translation table | ⬜ |
| `30-port/port-roadmap.md` | all | ⬜ |
| `30-port/port-design-process.md` | all | ⬜ |
| `00-overview.md` | all | ⬜ |

## Feature template (Wave 2 writers MUST follow)

```markdown
# Feature: <name>

## Purpose
One paragraph. What user problem.

## User flows
Numbered, golden path + edges.

## UI surface
Components, modals, forms, validation. file:line citations.

## Data flow
Sequence diagram (mermaid) UI → IPC → handler → external. Every hop.

## IPC contract
| Direction | Name | Payload | Returns | Used by |
|-----------|------|---------|---------|---------|

## State surfaces
- React state: ...
- Rust state: ...
- Disk files: ...

## Persistence
What gets written, where, format.

## Error handling
Failure modes + UX.

## ▸ Interfaces with
Explicit cross-refs. Format:
- depends-on: `feature-X.md` for ...
- depended-on-by: `feature-Y.md` for ...
- shares-state-with: `feature-Z.md` via file X

## TS port mapping
| Tauri call | TS equivalent | Notes |

## Known bugs and port-time fixes
List things current impl gets wrong. Each agent's deep-dive surfaces these.

## Open questions
Things the port should re-decide rather than copy.
```

## Cross-reference graph

```
installation ──► environments ──► extensions
                      │
                      ├──► jupyter ──┐
                      │              │
                      └──► backends ─┼──► logs-streaming
                                     │
              api-keys ──► platform-rest-api ◄──┘
                                     ▲
tray-and-autostart ──► (all backend services)
                                     │
cli-repl ──► platform-rest-api (in-process via SDK)
                                     │
uninstall ──► (stops + removes everything)
```

## Handoff rules for agents

1. **Read this file first.** Check status table; do not duplicate work.
2. **Write to your assigned file in `raw-deep-dives/`** (Wave 1) or `20-features/` (Wave 2).
3. **Use file:line citations** for every claim.
4. **At the bottom of every doc, include a "Cross-feature dependencies" section** listing what state, files, or processes this feature shares with others — this enables the port team to plan in isolation per feature.
5. **Mark known bugs** in the current implementation distinctly (`> ⚠️ BUG:` blockquotes). The port should not re-introduce them.
6. **Update this file's status table** after writing your doc.
