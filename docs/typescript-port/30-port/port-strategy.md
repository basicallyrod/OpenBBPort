# Port Strategy — Layer-by-Layer Decisions

> Wave 3 decision doc. Reads `raw-deep-dives/platform-rest-api.md{,.v2}`,
> `20-features/feature-platform-rest-api.md`, `20-features/feature-cli-repl.md`,
> and skim of all other feature docs. Drives `port-roadmap.md`,
> `port-tech-stack.md`, and `port-invoke-mapping.md`.
> Generated 2026-05-15.

This is the load-bearing decision for the entire TypeScript port. It commits — for
each of four layers — to one of: **(A) rewrite in TS**, **(B) wrap the existing
Python/Rust process and put TS in front of it**, or **(C) skip**. Every other Wave 3
doc inherits from these four choices.

## Executive summary

**Top recommendation: mixed strategy — B/B/B/A** with a documented migration path
toward A for selected layers later. Concretely:

| Layer | Strategy | One-line rationale |
|---|---|---|
| Desktop UI (Tauri shell + Rust backend) | **B** (keep Rust; port the renderer is already TS) | The Rust backend is 16k LOC of conda/process/network plumbing that Node can match but won't surpass for ~6 person-months. Renderer is already TS — there is no port to do there beyond bug fixes. |
| Platform REST server (`openbb-api`) | **B** (wrap subprocess + HTTP proxy) | The data plane is dozens of provider Fetchers + pandas + curl_cffi TLS impersonation. A rewrite trades one working server for a partial one. |
| CLI REPL (`openbb`) | **C** (skip; keep the existing Python `openbb` available via terminal spawn) | The desktop UI covers ~90% of CLI use cases. The remaining 10% (record/replay routines) is a desktop feature, not a TS-REPL feature. |
| Workspace integration (`/widgets.json`, `/apps.json`) | **A** via passthrough — emit from the TS layer when possible, proxy through Python otherwise | These endpoints are the lingua franca with OpenBB Workspace; we must own them. Their content can be derived from the Python server's OpenAPI doc at build time + runtime. |

Why mixed, not pure: a pure-A rewrite is blocked at Layer 2 by **yfinance, multpl,
seeking_alpha, tmx, stockgrid** (TLS impersonation, HTML/Excel scrapes) and by
**openbb-econometrics / -quantitative / -technical / -charting** (numpy/scipy/Plotly
Python). Skipping these is unacceptable for parity with the current product.
A pure-B leaves the TS port as decorative wrapping with no native surface that
justifies the cost. Mixed lets the TS layer **own credentials, settings,
codegen client types, and Workspace contract emission** — the layers that benefit
from TS — and lets Python keep doing what it does well.

The 8-15 second cold start of `openbb-api` (`platform-rest-api.md:G.1`) is the
worst part of Strategy B; the migration path mitigates it by allowing
selective rewrite of the **Easy bucket** of providers (FMP, Polygon, Intrinio,
Tiingo, AV, FRED, IMF, OECD, BLS — `platform-rest-api.v2.md:N.3`) into TS later
without touching the Hard bucket.

---

## The four layers, four decisions

### Layer 1: Desktop UI (Tauri + React + Rust)

**Current state.** Tauri 2.x app. Renderer is React + TS (`desktop/src/**`).
Backend is Rust:

| File | LOC | Role |
|---|---|---|
| `desktop/src-tauri/src/main.rs` | 858 | App bootstrap, tray, autostart, command registration |
| `desktop/src-tauri/src/tauri_handlers/startup.rs` | 1,883 | Install wizard, conda bootstrap, default seed |
| `desktop/src-tauri/src/tauri_handlers/environments.rs` | 3,888 | Conda env CRUD, smart retry, requirements parse |
| `desktop/src-tauri/src/tauri_handlers/backends.rs` | 2,000 | Backend services lifecycle, log scrape, URL discovery |
| `desktop/src-tauri/src/tauri_handlers/helpers.rs` | 2,806 | Conda spawn, atomic writes, file pickers |
| `desktop/src-tauri/src/tauri_handlers/jupyter.rs` | 743 | Jupyter Lab launch |
| `desktop/src-tauri/src/tauri_handlers/credentials.rs` | 533 | `user_settings.json` read/write, allow-listed editor open |
| `desktop/src-tauri/src/utils/process_monitor.rs` | 740 | Log buffer, ANSI strip, throttling |
| `desktop/src-tauri/src/utils/certs.rs` | 562 | Self-signed certs, trust-store install |
| `desktop/src-tauri/src/uninstall.rs` | 835 | Two-phase uninstall + helper executable |
| Other | ~1,500 | autostart, command sanitizer, app termination |
| **Total Rust** | **~16,400 LOC** | |

(Numbers verified via `find desktop/src-tauri -name '*.rs' | xargs wc -l`.)

> ⚠️ CONSTRAINT: The renderer is already TypeScript. "Port the desktop UI" only
> means: the **backend** (Rust → Node/TS) and the **shell** (Tauri 2.x → Electron
> or Tauri-with-Node-sidecar). The renderer itself is migrated as a tree-shake.

#### Options

| Option | Mechanism | Surface area to rewrite |
|---|---|---|
| **A. Port Rust → TS/Node** | Electron main process (Node) reimplements every Tauri command. Use `child_process`, `proper-lockfile`, `node-forge`, `dotenv`, `chokidar`, `electron-log`. | ~16k LOC Rust → ~12k LOC Node (denser); plus IPC channel re-mapping (Tauri `invoke` → Electron `ipcMain.handle` or HTTP). Every feature doc's `TS port mapping` row applies. |
| **B. Keep Rust; port only the renderer** | Continue Tauri 2.x. The renderer is already TS; "port" here is bug-fix + cleanup. | Renderer cleanup only (~zero net porting). Improvements: typed phase enums replacing substring matching; structured WS replacing window events; chosen-port reporting. |
| **C. Skip** | Ship the desktop app as-is. The "TS port" effort targets only the backend services layer. | No desktop work. |

#### Trade-offs

| Dimension | A (TS/Node main) | B (keep Rust) | C (skip) |
|---|---|---|---|
| Lines to write | ~12,000 | <500 (bug fixes) | 0 |
| Multi-platform binary size | ~120 MB (Electron) | ~30 MB (Tauri) | n/a |
| Memory baseline (idle) | ~250 MB | ~80 MB | n/a |
| Process kill / port-kill / lsof / netstat | npm wrappers ok, often shell-out anyway | already done in Rust | already done |
| Atomic file writes (`user_settings.json`) | `fs.rename` (POSIX atomic), Windows needs care | Rust already has `fs2`/`tempfile` patterns | already correct |
| Conda activation script generation | Must re-bake | Already correct | Already correct |
| Self-signed certs + trust-store install | `node-forge` + shell-out | `openssl` crate + shell-out | Already done |
| Process monitor (log buffer, throttle, ANSI strip) | `readline` + npm `strip-ansi` + custom | Already done | Already done |
| Crash recovery on `Mutex::lock().unwrap()` poisoning | n/a (no Rust mutex) | needs `parking_lot::Mutex` | unchanged |
| Hot-fix turnaround for bugs in `feature-backend-services.md:Known bugs` | Easier — JS deploys | Harder — Rust recompile per platform | n/a |
| Future Wave (workspace-server, browser deploy) | Wins — Node main = same code as server | Loses — Rust desktop = different code from a hypothetical TS server | n/a |

#### Recommendation

**B for v1. Migrate to A later if/when a non-desktop deployment (browser, hosted
agent, headless server) becomes a real product.**

Rationale: the Rust backend already works. The 16k LOC is **distributed across 11
files**, and every feature doc has documented its bugs and port-fixes. A pure-A
desktop port would consume 4-6 person-months and produce a less-stable v1
because the same bugs (atomic writes, race-conditions, port-kill filters)
must be re-discovered.

The strong argument for A is **future code-reuse with a server deployment**: if
the same Node code runs as Electron main AND as a hosted-server agent, the
ROI flips. But that's a Wave 4+ question. The roadmap should plan A as a
post-v1 milestone, not block v1 on it.

> ⚠️ CONSTRAINT: Rust binary `uninstall.rs:835` is a **separate executable** that
> outlives the main app (used during the two-phase uninstall to delete the install
> dir). Strategy A loses this — Node has no equivalent of a `.exe` that lives outside
> the Electron bundle and self-deletes its own parent dir. The current Rust pattern
> writes a small helper binary, spawns it, and quits. Replicate with a small Go or
> Rust shim, or accept "uninstall leaves a stub" (the macOS pattern).

---

### Layer 2: Platform REST server (`openbb-api`)

**Current state.** Python FastAPI app. Console-script registered at
`openbb_platform/extensions/platform_api/pyproject.toml:14`. Loads dozens of
provider extensions via Python entry-points
(`platform-rest-api.md:1d`, lines 64-81). Boots in 8-15 seconds on a full
install (`platform-rest-api.v2.md:G.1`). Default port 6900.

#### Options

| Option | Mechanism |
|---|---|
| **A. Rewrite in TS/Node** | Hono or Express + Zod schemas (generated at build time from Python's `/openapi.json`). Each Fetcher reimplemented in TS. Discriminated-union response models via `z.discriminatedUnion`. |
| **B. Wrap as subprocess** | TS UI spawns `openbb-api`, parses readiness from stdout (URL regex), proxies HTTP. Already documented in `feature-backend-services.md`. Already implemented in the current Rust backend. |
| **C. Skip** | Assume the user has Python installed; document `pip install openbb-platform-api`; run nothing. TS port becomes a thin client to a manually-managed server. |

#### What kills Strategy A

> ⚠️ CONSTRAINT: **yfinance cannot be ported to TS without TLS impersonation.**
> Source: `platform-rest-api.v2.md:N` (lines 826-870). `yfinance>=0.2.66` uses
> `curl_cffi` internally to mimic Chrome/Firefox TLS fingerprints and bypass Yahoo's
> bot detection. The Node ecosystem has no production-grade equivalent
> (`node-curl-impersonate` is experimental; Bun has TLS impersonation but is not yet
> a desktop runtime choice). yfinance is the default provider for `equity/price/*`.

> ⚠️ CONSTRAINT: **Multiple other providers depend on Python-native parsing.**
> From `platform-rest-api.v2.md:N.2`:
>
> | Provider | Constraint |
> |---|---|
> | `multpl` | HTML table scraper |
> | `federal_reserve` | `pandas.read_excel` for total factor productivity, HTML parsing for FOMC documents |
> | `eia` | `pandas.read_excel` for petroleum status report |
> | `tmx` | HTML scrapes |
> | `sec` | Custom HTML→Markdown, 13F-HR XML/HTML parsing, SIC code HTML scrape |
> | `fred` | Calendar HTML scrape |
> | `seeking_alpha`, `stockgrid` | TLS impersonation |

> ⚠️ CONSTRAINT: **`openbb-charting`, `openbb-econometrics`, `openbb-quantitative`,
> `openbb-technical`** are Python-only. They use numpy/scipy/statsmodels/Plotly
> Python. There is no plausible TS port without re-implementing these libraries.
> Note: `chart.content` is portable Plotly JSON
> (`feature-platform-rest-api.md:144`, `platform-rest-api.v2.md:E.1`,
> `platform-rest-api.v2.md:v2-correction-10`) so charts can still render in the
> renderer via `react-plotly.js` — but the **construction** of the figure is
> Python.

#### Trade-offs

| Dimension | A (rewrite) | B (wrap) | C (skip) |
|---|---|---|---|
| Lines to write | ~50k+ (~30 providers × ~5 fetchers × ~300 LOC + framework) | ~500 (spawn + proxy + readiness, already in Rust) | ~50 (docs) |
| Cold start | <500 ms (compiled Zod at build) | 8-15 s | depends on user |
| Memory baseline (server) | ~80 MB Node | ~150-250 MB Python | depends |
| Install size | ~10 MB extra in app bundle | ~150-200 MB Python runtime + deps | 0 (user supplies) |
| Provider coverage | Easy bucket only (~70% of data needs, per `platform-rest-api.v2.md:N.3`) | 100% (every Python provider Just Works) | depends |
| Charting | Drop server-side; render Plotly client-side from results | Native (`openbb-charting` runs in Python) | depends |
| Data-processing extensions (`econometrics`, `quantitative`, `technical`) | Drop or proxy | Native | depends |
| Workspace `/widgets.json` parity | Must regenerate from local route registry | Free (Python emits) | Must proxy or build |
| Breaking on upstream changes | TS port must track each Yahoo/SEC/etc. site change | Python community tracks it for us | depends |
| Debuggability | Single-language stack trace | Cross-process | depends |
| Security boundary | Same TS process — settings in memory | `127.0.0.1`-only via `check_port` (`utils/api.py:82-93`); auth via env (`OPENBB_API_AUTH=true`) | depends |
| User trust (no extra runtime) | Wins for laptop installs | Loses on disk space, gain on stability | Wins for power users only |

#### Recommendation

**B for v1. Strategy A is a multi-quarter follow-up for the Easy bucket only;
even then, the Python server must remain available for the Hard bucket
and the data-processing extensions.**

Rationale: every claimed advantage of A (smaller install, fast boot, single
language) is undone by the fact that **half the providers will fall back to
Python anyway**. Two sidecars (TS server for Easy providers, Python server for
Hard providers + charting + econometrics) is worse than one Python server.

The right time to revisit A is when:

1. A specific subset of providers (e.g. just FMP + Polygon + FRED) needs to ship
   in a no-Python-runtime distribution (browser, embedded, etc.).
2. The cold-start time becomes a critical UX problem and the easy mitigations
   (warm-keep on tray, lazy provider import) are exhausted.
3. The Easy bucket has been frozen by upstream (no new providers) so re-port
   maintenance is bounded.

Until then, the 8-15 s cold start is a real problem with realistic mitigations:

- Start `openbb-api` on app launch, **before** the renderer needs it (tray-and-autostart already does this).
- Show a deterministic "Loading providers…" progress UI tied to the stdout
  scan, not just a spinner (the current `feature-backend-services.md` log-scan
  already extracts the bound URL — extend it to extract `RegistryMap built`
  / `widgets.json built` / `ready` markers).
- Cache `widgets.json` between runs (the Python server rebuilds at every boot;
  desktop can short-circuit if the provider set hasn't changed).

---

### Layer 3: CLI REPL (`openbb`)

**Current state.** Python REPL using `prompt-toolkit` + Rich + argparse. Wraps
the `obb` SDK **in-process** (`feature-cli-repl.md:Data flow`). The desktop
spawns it via `execute_in_environment` in a native terminal
(`feature-cli-repl.md:124`).

#### Options

| Option | Mechanism |
|---|---|
| **A. Pure TS rewrite** | Re-implement `obb` SDK in TS, then drive a `commander`/`inquirer` REPL from it. |
| **B. REST-shim TS REPL** | TS REPL boots, fetches `/openapi.json` from a running `openbb-api`, builds command tree dynamically. Every command becomes an HTTP call. |
| **C. Skip** | Don't ship a TS REPL. Users who want one keep running the existing Python `openbb` command, spawned from the desktop's env-applications modal. |

#### Trade-offs

| Dimension | A (pure TS) | B (REST-shim) | C (skip) |
|---|---|---|---|
| Lines to write | ~50k+ (same as REST server A — blocked by same provider constraints) | ~3k (REPL shell + parser + completer + table renderer) | 0 |
| Depends on running REST server | No | **Yes** — every command pays HTTP round trip | No |
| Per-command latency | ~ms (in-process) | ~10-50 ms (localhost HTTP) | ~ms (in-process Python) |
| `OBBject` registry | In-process objects, full methods | JSON-only, no `.show()`/`.to_df()` (per `feature-cli-repl.md:288-293`) | Full Python parity |
| `.openbb` routine record/replay | Native | Possible via fetch trace | Native |
| Charts | Plotly.js via separate window | Plotly.js via separate window | Native PyWry pop-out |
| User base served | Power users + automation | Power users + automation | Power users (terminal users already have Python) |
| Maintenance burden | Forks the SDK | Tracks OpenAPI schema | Zero |
| Unique value over desktop UI | Routines + scripting | Routines + scripting | Routines + scripting |

> ⚠️ CONSTRAINT: **Strategy A is blocked by the same constraints as REST-Layer A.**
> No TS-native SDK exists (`feature-cli-repl.md:208`). Porting `obb` means porting
> every provider Fetcher. The CLI has a tiny user-base relative to the desktop UI;
> A is uneconomic.

> ⚠️ CONSTRAINT: **Strategy B's `OBBject` registry loses `.show()`, `.to_df()`,
> `.to_polars()`, `.to_llm()`** (`feature-cli-repl.md:288-293`,
> `platform-rest-api.v2.md:E`). These run in-process in Python; over HTTP only
> the serialized JSON survives. A TS CLI can re-tabulate from JSON, but
> client-side dataframe abstractions (`arquero`, `tinyframe`) aren't drop-in.

#### Recommendation

**C for v1. Consider Strategy B in Wave 4+ only if a non-desktop CLI is needed
(e.g. a "serverless" `npx openbb` for headless CI use).**

Rationale: the desktop UI already covers extension management, API keys, env
activation, and chart rendering — **all the CLI's UI-shaped affordances**. The
unique CLI value is:

1. **`.openbb` routines (record/replay).** Better implemented as a desktop
   feature ("Run Routine" button that replays REST calls). This moves a
   power-user feature into the GUI surface, where it gains observability and
   UI for parameter overrides.
2. **Scripting / pipelines.** Users who script already write Python; a TS REPL
   gives them nothing they don't have.
3. **Power-user terminal flow.** The desktop's `execute_in_environment` already
   spawns the **existing Python** `openbb` in a native terminal. This works
   today and costs the port nothing.

The risk in skipping: a small subset of users uses the CLI as their primary
interface. The mitigation: the Python CLI continues to work; we just don't
re-implement it in TS. The desktop binary is the unified front door.

---

### Layer 4: Workspace integration (`/widgets.json`, `/apps.json`, `/agents.json`)

**Current state.** Python launcher (`platform_api/main.py`) emits these three
endpoints. OpenBB Workspace polls them. Generated at boot from the OpenAPI
spec (`feature-platform-rest-api.md:Workspace integration`,
`platform-rest-api.v2.md:B`, `:C`, `:D`).

#### Options

| Option | Mechanism |
|---|---|
| **A. Emit from TS** | TS port owns `/widgets.json` and `/apps.json`. Either generated at TS build time from a captured `/openapi.json`, or at runtime by walking Python's OpenAPI and converting in TS. |
| **B. Proxy through Python** | TS UI exposes the endpoints by forwarding to the Python server. Zero translation. |
| **C. Skip** | No Workspace integration in v1. |

#### Trade-offs

| Dimension | A (emit from TS) | B (proxy from Python) | C (skip) |
|---|---|---|---|
| Lines to write | ~1,500 (port `utils/widgets.py:233-741`, ~500 LOC + `apps.json` merge logic ~150 LOC, plus tests) | ~0 (HTTP passthrough) | 0 |
| Owns `widget_id` naming convention | Yes — TS code is canonical | No — Python is canonical | n/a |
| Reacts to user-side customization | Yes — TS can layer overrides | Yes — Python supports `--widgets-json PATH` | n/a |
| Lifecycle dependency on Python | No (TS owns the endpoint, can run when Python is starting/down) | Yes (Python must be up) | n/a |
| Bug-fix turnaround for documented bugs (silent app drop on missing widget; auto-create-on-GET side effect; `merge_agents.py:49` typo) | Easy — fix in TS | Inherits Python bugs | n/a |
| Future ability to host Workspace **without** Python | Wins | Loses | n/a |

> ⚠️ CONSTRAINT: **`/apps.json` GET has a write side effect** —
> `~/OpenBBUserData/workspace_apps.json` is auto-created as `[]` if missing
> (`platform-rest-api.v2.md:C.4`, lines 327-330). Whichever layer emits this
> endpoint must preserve the side effect to avoid breaking Workspace startup.

> ⚠️ CONSTRAINT: **The widget catalogue is provider-discriminated.** One route
> with 5 providers yields **5 widget entries** with provider-specific params
> (`platform-rest-api.v2.md:B.3`). If TS emits, it must understand the full
> per-provider param shape. Easiest source: at install time, capture the
> Python server's `/openapi.json` and generate widget JSON from it once;
> regenerate when extensions change.

#### Recommendation

**A — but pragmatically a "TS emits, generated from Python's OpenAPI at install time" hybrid.**

Concretely:

1. At install (after `pip install openbb-platform-api` plus selected extensions),
   the desktop **once** starts `openbb-api`, captures `/openapi.json` and
   `/widgets.json`, and stores them next to the env metadata.
2. The TS layer serves `/widgets.json` and `/apps.json` from this captured
   data, applying user overrides (from
   `~/.openbb_platform/widget_settings.json` and
   `~/OpenBBUserData/workspace_apps.json`).
3. When extensions change (install / upgrade / remove), the TS layer
   regenerates the cache from Python.
4. The TS layer fixes the documented bugs at this boundary: silent app drop on
   missing widget → emit warnings, auto-create-on-GET → do at install time
   instead, `merge_agents.py:49` typo → corrected port.

Why not strict A (run-time TS implementation): the
`build_json(openapi, widget_exclude_filter)` algorithm at
`utils/widgets.py:233-741` is ~500 LOC of OpenAPI walking + parameter munging.
Porting it line-by-line is feasible but risky and gates v1 on subtleties (the
`TO_CAPS_STRINGS` list at `openapi.py:8-63`, the
provider-pretty-name map at `widgets.py:559-575`, the `multiple_items_allowed`
flag, etc.). The hybrid avoids reimplementing the algorithm while still
owning the endpoint and being able to fix its bugs at the seam.

Why not B (pure proxy): `/widgets.json` is the contract surface for OpenBB
Workspace. If we ever want to ship a TS-only deployment (no Python), or to
support multiple language backends behind one Workspace, we must own this
endpoint. A also lets us fix the silent-drop and side-effect-on-GET bugs.

---

## Mixed strategy recommendation

Final assignments:

| Layer | Strategy | Cost (eng-months v1) | Value (unlocked) |
|---|---|---|---|
| L1 Desktop UI / Rust backend | **B (keep Rust)** | ~0.5 (bug fixes) | Stable v1 |
| L2 Platform REST server | **B (wrap Python)** | ~0.5 (already in Rust; refine startup UX) | Full provider coverage |
| L3 CLI REPL | **C (skip)** | 0 | n/a — existing Python CLI works |
| L4 Workspace endpoints | **A hybrid (TS emits from Python-captured OpenAPI)** | ~1.5 | Endpoint ownership; bug fixes; future TS-only path |
| **Total** | | **~2.5** | |

### Cost × Value matrix

```
                  Low value                High value
              ┌────────────────────────┬───────────────────────┐
   High cost  │ L1-A (Rust→Node main)  │ L2-A (rewrite REST)   │
              │ — defer to Wave 4+     │ — blocked by yfinance │
              │                        │   et al.; partial A   │
              │                        │   later for Easy      │
              │                        │   bucket only         │
              ├────────────────────────┼───────────────────────┤
   Low cost   │ L3-A (TS CLI rewrite)  │ L4-A hybrid           │
              │ — blocked anyway       │ ◄── v1 work           │
              │                        │ L1-B, L2-B, L3-C      │
              │                        │ ◄── v1 work           │
              └────────────────────────┴───────────────────────┘
```

The v1 commitments cluster in the **low-cost / high-value** quadrant. The
high-cost quadrant is explicitly **out of scope** for v1, and the rationale is
documented so a future wave can revisit without re-litigating.

### Where the "TS" in "TS port" actually lives

Under this strategy, the TS code that gets written is:

1. **L4 Workspace emitter** — `/widgets.json`, `/apps.json`, `/agents.json`
   served by a small Hono/Express app embedded in the desktop (or hosted as
   a Node sidecar if Strategy L1-A is later chosen).
2. **OBBject client codegen** — `openapi-typescript` against the captured
   `/openapi.json` at build time, producing TS types the renderer imports
   when it does `fetch('/api/v1/...')`. No runtime cost; pure DX.
3. **Renderer cleanup** — typed phase enum for installation
   (`feature-installation.md:Open questions` and bugs list), structured
   WS frames replacing `window.event`, chosen-port reporting, etc. ~500 LOC.
4. **L4-side bug fixes** — silent-drop warning, mtime-based settings reload
   replacing per-request disk reads (if we ever move credentials proxying
   into the TS layer), etc.

This is the v1 contour. Layers 1 and 2 stay native (Rust + Python).

---

## What rewrite Strategy A would cost (per blocker)

If a future wave commits to Strategy A at Layers 1 and/or 2, here is the
realistic per-blocker scope. Sources are file:line citations from the
deep-dives.

| Blocker | Source | Scope estimate |
|---|---|---|
| **yfinance** (curl_cffi TLS impersonation against Yahoo, cookie+crumb dance) | `platform-rest-api.v2.md:N.1` (lines 831-851) | 2-4 weeks: Yahoo TLS lib + cookie-crumb scraper + per-symbol error mapping + cassette tests. Maintenance: ongoing — Yahoo breaks scrapers ~quarterly. |
| **Other `curl_cffi`-based providers** (`seeking_alpha`, `stockgrid`) | `platform-rest-api.v2.md:N.3` "Hard" bucket | 1-2 weeks each. Lower traffic so lower priority. |
| **`pandas.read_excel` providers** (`federal_reserve` total factor productivity, `eia` petroleum status) | `platform-rest-api.v2.md:N.2` table | 1 week per. Use `xlsx` npm or `exceljs`. |
| **HTML-table scrape providers** (`multpl`, `tmx`, `fred` calendar, `federal_reserve` FOMC) | `platform-rest-api.v2.md:N.2` table | 1 week per. `cheerio` npm. Fragile — site layout changes break port. |
| **SEC** (HTML→Markdown, 13F-HR XML/HTML, SIC scrape) | `platform-rest-api.v2.md:N.2` | 3-4 weeks. The custom `html2markdown.py` and `parse_13f.py` are substantial. |
| **`openbb-charting` (Plotly Python rendering)** | `feature-platform-rest-api.md:144`, `platform-rest-api.v2.md:E`, `:v2-corrections#10` | **Mostly free.** Wire format is Plotly JSON; render in renderer via `react-plotly.js`. The construction step (matplotlib/Plotly Python figure assembly) is Python-only — but it's not on the data path; only on `?chart=true` paths. Acceptable to drop server-side chart construction for v1, render results client-side. |
| **`pandas` DataFrame → TS equivalent** | `platform-rest-api.v2.md:E.1` | Not needed server-side (already serialized to records). Client-side `to_df` etc. are Python-only — TS users get raw `results: Array<Record>` and can use `arquero` / `tinyframe` / Arrow if they want. |
| **The `Fetcher` T-E-T pipeline per provider** | `platform-rest-api.md:2f`, `provider/abstract/fetcher.py:73-85` | ~30 providers × ~5 fetchers × ~300 LOC each = **~45k LOC**. Plus the standard models + Pydantic→Zod translations. **Estimate: 6-12 person-months for the Easy bucket only.** |
| **`ProviderInterface` + `RegistryMap`** (~700 LOC of Python metaclass-driven dataclass generation) | `platform-rest-api.md:1e`, `provider_interface.py:543-697` | 2-3 weeks to design TS-build-time equivalent (Zod codegen from a manifest). Much smaller than Python because the codegen runs once at build, not at every server boot. |
| **`openbb-econometrics`, `-quantitative`, `-technical`** | `feature-platform-rest-api.md:387` | **Blocked.** Numpy/scipy/statsmodels deep dependence. Either keep Python sidecar for these routes or drop. |

**Total Strategy A cost (Easy bucket only, no Hard providers, no
data-processing extensions): ~9-12 person-months.** And the result is a
PARTIAL port — Python still required for the Hard bucket and the
data-processing extensions. The realistic value/cost ratio tips Strategy A
into "not yet" territory.

---

## What Strategy B wrapping requires

Strategy B for Layer 2 (`openbb-api` subprocess) is what the current Rust
backend does. The TS port (Strategy B at Layer 2) inherits these requirements;
list is sourced from `feature-platform-rest-api.md` and
`feature-backend-services.md`.

| Requirement | Source | Notes |
|---|---|---|
| Bundle Python or detect existing | `feature-installation.md:TS port mapping`, `feature-environments.md` | Install wizard ships **Miniforge/Mambaforge** alongside the app and pip-installs `openbb-platform-api` + `openbb-mcp-server` into a `openbb` conda env. Total install size ~150-300 MB. |
| Install management (conda or uv-based) | `feature-environments.md` | Conda activation is non-trivial — `CONDA_DEFAULT_ENV`, `CONDA_PREFIX`, `CONDA_SHLVL` must be cleared and `CONDA_ROOT`, `CONDA_ENVS_PATH`, `CONDA_PKGS_DIRS`, `CONDARC` set (`helpers.rs:154-165`, also see "Why envs are the hardest thing to port" in `feature-environments.md`). |
| Subprocess lifecycle (spawn, monitor, stop, restart, cleanup) | `feature-backend-services.md`, `feature-platform-rest-api.md:298-329` | Current Rust does this; TS port (L1-B = stay Rust) inherits it. The MCP server is a **second** Python process (`platform-rest-api.v2.md:A`, lines 17-30) — both must be lifecycled. |
| HTTP proxy from TS UI | `feature-platform-rest-api.md:IPC contract` | Renderer does direct `fetch('http://127.0.0.1:6900/api/v1/...')`. CORS defaults to `*` (`platform-rest-api.md:rest_api.py:68-73`). No translation needed. |
| Settings file as shared contract (`~/.openbb_platform/user_settings.json`) | `feature-api-keys.md`, `feature-platform-rest-api.md:State surfaces` | **Both** TS and Python read this file. The Python server re-reads it on **every** request (`platform-rest-api.v2.md:L`); TS just needs atomic write semantics. ⚠️ The current Rust `std::fs::write` is non-atomic (`feature-api-keys.md:Known bugs`) — fix in port: `*.tmp` + `fs.rename` + `chmod 0o600`. |
| Cold-start UX (8-15 s) | `platform-rest-api.v2.md:G.1` | Spawn at app launch (tray autostart), show deterministic progress UI tied to stdout markers, NOT a generic spinner. |
| Cross-process credentials reload | `platform-rest-api.v2.md:L.1`, `feature-api-keys.md` | Python re-reads `user_settings.json` per request — **as long as the write is atomic**. Non-atomic write produces a transient-400 race window with Pydantic validation. ⚠️ Fix the atomicity. |
| `.env` change → server restart | `platform-rest-api.v2.md:L.2`, `feature-platform-rest-api.md:Known bugs` | `Env()` is a frozen snapshot at module import. Wire an mtime watcher on `.env` to surface a "restart required" prompt in the desktop. |
| MCP↔REST coupling | `platform-rest-api.v2.md:A.5`, `feature-platform-rest-api.md:298-329` | MCP server makes outbound HTTP back to the REST server. If REST is down or port drifted (auto-increment via `check_port`), every MCP tool 502s. Surface this dependency in the backends UI; consider auto-restarting MCP when REST restarts. |
| Two `ProviderInterface` singletons in two processes | `platform-rest-api.v2.md:A.1`, `:v2-corrections#7` | `openbb-api` and `openbb-mcp` are separate processes; each pays the 8-15 s import cost independently. Total memory: ~300-500 MB resident for both. Acceptable on a developer machine; documented constraint. |

---

## Migration path

Strategy B → Strategy A is **layer-by-layer feasible** because the architecture
is HTTP-fronted at every seam.

```
Today (Python):                After v1 (TS port, mixed):           Hypothetical Wave 4+ (TS-heavy):

┌─────────────────────────┐    ┌─────────────────────────┐          ┌─────────────────────────┐
│ Renderer (TS, React)    │    │ Renderer (TS, React)    │          │ Renderer (TS, React)    │
└───────────┬─────────────┘    └───────────┬─────────────┘          └───────────┬─────────────┘
            │ Tauri invoke                 │ Tauri invoke                       │ Tauri invoke (or web)
            ▼                              ▼                                    ▼
┌─────────────────────────┐    ┌─────────────────────────┐          ┌─────────────────────────┐
│ Rust backend            │    │ Rust backend            │          │ Node backend (Electron) │
│  - conda                │    │  - conda                │          │  - conda                │
│  - process lifecycle    │    │  - process lifecycle    │          │  - process lifecycle    │
│  - user_settings.json   │    │  - user_settings.json   │ ◄── L1-A │  - user_settings.json   │
└───────────┬─────────────┘    │  - widgets.json (TS!) ◄─┼──── L4-A │  - widgets.json (TS)    │
            │                  └───────────┬─────────────┘          │  - openapi proxy        │
            │ subprocess + HTTP            │ subprocess + HTTP      └───────────┬─────────────┘
            ▼                              ▼                                    │
┌─────────────────────────┐    ┌─────────────────────────┐                      ▼
│ Python openbb-api       │    │ Python openbb-api       │ ◄── L2-A  ┌─────────────────────────┐
│ Python openbb-mcp       │    │ Python openbb-mcp       │ partial   │ TS providers (Easy bucket)│
└─────────────────────────┘    └─────────────────────────┘           ├─────────────────────────┤
                                                                     │ Python sidecar           │
                                                                     │ (Hard bucket +           │
                                                                     │  econometrics, charting) │
                                                                     └─────────────────────────┘
```

Dependencies between migrations:

| Migration | Depends on | Why |
|---|---|---|
| **L4 (Workspace) A** | nothing | Free-standing endpoint. Already the v1 plan. |
| **L1 (Desktop backend) A** | L4 must already be in TS | Once `widgets.json` is owned by TS, the Rust↔TS HTTP boundary is well-defined and Rust can be swapped for Node without renderer changes. |
| **L2 (REST server) A, Easy bucket** | nothing on L1 or L4 | The TS REST server can run alongside the Python one. Renderer chooses based on which provider was requested. Practical: a thin "router" in front of both. |
| **L2 (REST server) A, Hard bucket** | Solving yfinance TLS impersonation (open research problem) | If `node-curl-impersonate` matures or Bun becomes a desktop runtime, this gets unblocked. Until then, **explicitly out of scope**. |
| **L3 (CLI) B** | L2 stable HTTP API | The REST-shim CLI is just a TS REPL on top of any L2 server (Python or TS). Could ship after L4 if there's user demand. |

The mixed strategy is therefore not a permanent compromise — it's a **starting
point** that **doesn't paint into a corner**.

---

## Open decisions for the user

These are the questions the user/team must answer before the port commits.
Each one materially changes the strategy.

1. **What is the v1 target user?**
   - "Existing OpenBB desktop user, Python already installed" → C-ish at every layer (focus on UI polish; Python stays).
   - "New user who must not install Python manually" → B at L2 (we bundle Python; not optional).
   - "Browser / hosted deployment" → A at all layers eventually; **block on yfinance**.

2. **Is `openbb-mcp` in scope for v1?**
   - If yes: two Python processes to lifecycle; the implicit MCP→REST HTTP dependency (`platform-rest-api.v2.md:A.5`) must be made explicit in the backends UI.
   - If no: drop the seed in `startup.rs:1461-1478` and recover ~150 MB resident memory. The user loses LLM tool-server functionality.

3. **Is the desktop ever going to host without Python?**
   - If "yes within 12 months": L4-A is required v1 work (so the TS layer can serve Workspace without Python). The Easy-bucket Strategy A becomes a near-term roadmap item.
   - If "no foreseeable need": L4-B (pure proxy) is acceptable, saving ~1.5 eng-months from v1. We recommend doing L4-A anyway for the bug fixes (silent drop, side-effect-on-GET).

4. **Do we re-implement the CLI in TS (B), or keep the Python CLI as a power-user feature only (C)?**
   - This is the cleanest decision. Recommendation: **C**. The CLI's unique value (record/replay routines) becomes a desktop feature, not a TS-REPL feature.

5. **What's the renderer's relationship to the REST server's wire format?**
   - Option: keep direct `fetch('/api/v1/...')` with codegen TS types from `/openapi.json` (current plan).
   - Option: proxy every request through Tauri/Electron IPC for security (renderer never makes direct HTTP). Adds a hop, gains uniform auth handling.
   - Recommendation: direct fetch with `OPENBB_API_AUTH=false` + `127.0.0.1` bind (current default); reconsider if multi-tenant becomes a target.

---

## Risks per strategy

| Strategy | Top risks | Mitigations |
|---|---|---|
| **L1-B (keep Rust)** | (1) Bus factor on Rust expertise; (2) Cross-platform Rust toolchain in CI; (3) `Mutex::lock().unwrap()` panic poisoning (`feature-installation.md:Known bugs #14`); (4) Bug-fix turnaround is per-platform recompile. | (1) Hire one Rust-fluent eng; (2) GitHub Actions matrix as today; (3) port to `parking_lot::Mutex` (no poisoning); (4) accept the cost; v1 lands faster with Rust than with a Node rewrite. |
| **L1-A (Rust→Node main)** | (1) ~12k LOC port; (2) Re-discovering port-kill / lsof / certutil corner cases per OS; (3) Conda activation script bake-in; (4) Electron memory baseline + binary size jump. | (1) Defer to Wave 4+; (2) port file-by-file with paired tests; (3) keep the Rust pattern of a generated shell wrapper; (4) accept the trade-off for code-reuse with hosted deployment. |
| **L2-B (Python subprocess)** | (1) 8-15 s cold start (`platform-rest-api.v2.md:G.1`); (2) Cross-process debugging; (3) Non-atomic `user_settings.json` write produces transient 400s (`platform-rest-api.v2.md:L.1`); (4) MCP↔REST implicit coupling. | (1) Spawn at app launch + deterministic progress UI; warm-keep on tray; (2) accept; document log paths; (3) fix the write to be `tmp+rename+chmod`; (4) make the dependency explicit in the backends UI; auto-restart MCP when REST restarts. |
| **L2-A (TS REST server)** | (1) yfinance TLS impersonation; (2) ~50k LOC fetcher porting; (3) Provider-vendor API drift; (4) No parity with charting/econometrics/quantitative/technical. | (1) Hard-block until upstream Node lib matures; (2) phase per-provider; (3) integration tests + cassettes; (4) keep Python sidecar for these routes (mixed forever). |
| **L3-C (skip CLI)** | (1) Power-user complaints; (2) `.openbb` routines stranded in Python. | (1) Existing Python `openbb` still spawnable from desktop env modal; (2) port `.openbb` semantics to a desktop "Run Routine" feature. |
| **L3-B (REST-shim CLI)** | (1) Every command pays HTTP round-trip; (2) `OBBject` registry methods lost (`.show()`, `.to_df()`); (3) Tracking OpenAPI changes per release; (4) Auth surface (`feature-cli-repl.md:Open questions`). | (1) Acceptable on localhost; (2) accept JSON-only; document; (3) regenerate command tree at install + on `pip install <extension>`; (4) reuse `OPENBB_API_AUTH` story. |
| **L4-A hybrid (TS emits via captured OpenAPI)** | (1) Capture-once requires the Python server to run during install/extension change; (2) `widget_id` schema drift if Python updates the format; (3) Provider-pretty-name map drift (`platform-rest-api.v2.md:B.5`). | (1) Accept; the Python server is already up at install completion (we just spawned it); (2) regenerate cache on `pip install`/upgrade; (3) snapshot the map per Python version. |
| **L4-B (proxy)** | (1) Workspace integration breaks when Python is down (vs L4-A which can serve cached); (2) inherits Python's silent-drop and side-effect-on-GET bugs; (3) blocks future TS-only Workspace deployment. | (1) Show "starting" placeholder when Python is unavailable; (2) accept the bugs in v1; (3) accept; revisit if needed. |

---

## What this doc decides — and what it doesn't

**Decided:**

- L1: B (keep Rust). L2: B (wrap `openbb-api` + `openbb-mcp`). L3: C (skip TS REPL). L4: A hybrid (TS emits from Python-captured OpenAPI).
- Total v1 effort estimate: ~2.5 eng-months for the **port-specific** TS work, on top of bug-fix work in the existing Rust+TS code.
- Strategy A at L1 and L2 is a deferred roadmap item, **not** a v1 commitment.
- The Easy bucket / Hard bucket split for L2 providers is the canonical phase-A scope when it gets revisited.

**Not decided (handed off to downstream Wave 3 docs):**

- Concrete tech-stack choices (Hono vs Express, etc.) → `port-tech-stack.md`.
- Every IPC → HTTP mapping → `port-invoke-mapping.md`.
- Release milestones / dependency order → `port-roadmap.md`.
- Cross-cutting design principles (immutability, error handling, logging) → `port-design-process.md`.
- Whether to switch the desktop shell from Tauri to Electron (this only matters under L1-A; v1 keeps Tauri).

The single most important downstream input from this doc is the answer to
**"what does the TS code actually do in v1?"** — the answer is **L4 + renderer
cleanup + codegen types + atomic-write fixes**, not a wholesale rewrite.
