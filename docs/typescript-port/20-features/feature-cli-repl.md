# Feature: OpenBB CLI REPL (Python menu shell)

## Purpose

`openbb` is a bash-style, menu-driven Python REPL that wraps the `openbb-platform`
SDK **in-process**. Users navigate a `/`-separated tree (`/equity/price/historical
--symbol AAPL`), commands are translated to SDK calls via `argparse`, and results
(`OBBject`s) are pushed onto a LIFO registry so they can be referenced by later
data-processing commands (`technical/sma --data OBB0`). It is a parallel front-end
to the `openbb-api` REST server — both consume the same `obb` SDK — and the
desktop app launches it as a subprocess inside an activated conda environment.

## User flows

1. **Golden path — interactive REPL inside a desktop-spawned terminal.** User
   clicks "OpenBB CLI" in the env's Applications modal → `execute_in_environment`
   IPC opens a native terminal, activates the conda env, runs `openbb` →
   `cli.py:main` prints `Loading...`, defers heavy imports, calls `bootstrap()`
   (creates `~/.openbb_platform/.cli.env`) then `launch()`. User sees prompt
   `🦋 / $ `, types `equity/price/historical --symbol AAPL --provider yfinance`,
   sees a Rich-styled table or PyWry-rendered chart, repeats. Quits with `e`/`exit`.
2. **Scripted routine.** User runs `openbb --file my_routine.openbb` (or
   `exe -f my_routine.openbb` from the REPL). `script_parser.py` resolves
   `$VARNAME` substitutions, date keywords (`$1MONTHAGO`, `$LASTFRIDAY`),
   `foreach $$X in a,b,c … end` loops; result is a `/`-joined command queue that
   the main loop consumes line by line.
3. **Record-and-replay.** From any menu: `record -n daily_report -d "..."` →
   every subsequent input is appended to a buffer → `stop` writes
   `<export_dir>/routines/daily_report.openbb` (requires ≥5 commands).
4. **One-shot invocation.** `openbb /equity/price/historical --symbol AAPL` (with
   leading `/`) runs that one command and drops into the REPL (or exits if
   `--test` is given via `run_scripts`).
5. **Reset.** Typing `r`/`reset` saves the current path, clears every
   `openbb_cli.*` module from `sys.modules`, re-imports everything, and replays
   the path — used to pick up new credentials mid-session.

## UI surface

A terminal UI, not a GUI. Implemented with `prompt-toolkit` + Rich; described per
component.

- **Prompt** (`controllers/cli_controller.py:543-555`): `f"{flair} {path} $ "`
  where `flair` defaults to the butterfly emoji `🦋` (`models/settings.py:127`,
  command `flair`) and `path` is the current `/`-separated menu (`/`,
  `/equity/`, `/equity/price/`, …).
- **Completer** (`config/completer.py` + `controllers/choices.py`):
  `NestedCompleter` built from `CHOICES_COMMANDS + CHOICES_MENUS + CHOICES_COMMON`
  per controller, plus per-argument `choices=[...]` from the argparse spec
  (providers, `OBB0..OBBN`, Literal types). Tab-cycles through subcommands,
  flags, and registry keys.
- **Bottom toolbar** (`cli_controller.py:548-552`): HTML hints `[h] help`,
  `[q] quit`, `[e] exit`, `[cmd -h] command help`, only shown if
  `Settings.TOOLBAR_HINT` is true.
- **Help panel** (`config/console.py:62-79`, `config/menu_text.py:28-165`):
  every `print_help()` produces a Rich `Panel(title=menu, subtitle="OpenBB
  Platform CLI vX.Y …")` with three-column rows
  `name (23) + description (65) + providers` — providers come from
  `obb.reference["paths"]`.
- **Key bindings**: standard `prompt-toolkit` (`Ctrl+A/E`, `Ctrl+R` reverse-i-search
  via `CustomFileHistory(~/.openbb_platform/.cli.his)` at `session.py:81`).
  `Ctrl+C`/`Ctrl+D` → `KeyboardInterrupt`/`EOFError` → `print_goodbye()` exits
  (`cli_controller.py:567-569`).
- **Universal commands** in every menu (`base_controller.py:50-65`): `cls`,
  `home`, `h`/`?`/`help`, `q`/`..`/`quit`, `e`/`exit`, `r`/`reset`, `stop`,
  `results`, plus contextual `record`/`exe` at the root.
- **Output**: Rich tables for `print_rich_table` (or PyWry interactive tables
  if `USE_INTERACTIVE_DF=True`); PyWry pop-out windows for charts via
  `openbb_charting.core.backend.Backend.send_table` /
  `OpenBBFigure.show()` (`controllers/utils.py:264-465`, `session.py:39-44`).

## Data flow

One command — `/equity/price/historical --symbol AAPL --provider yfinance`:

```mermaid
sequenceDiagram
    participant U as User (terminal)
    participant P as prompt_toolkit
    participant BC as BaseController.menu()
    participant SW as switch() / dispatch parser
    participant DC as DynamicController.call_historical
    participant AT as ArgparseTranslator.execute_func
    participant SDK as obb.equity.price.historical
    participant CR as CommandRunner (in-process)
    participant OBB as OBBject (uuid, results, extra)
    participant REG as session.obbject_registry
    participant CON as Rich/PyWry console

    U->>P: types "/equity/price/historical --symbol AAPL --provider yfinance"
    P->>BC: prompt() returns line
    BC->>SW: switch("/equity/price/historical --symbol AAPL --provider yfinance")
    SW->>SW: parse_input → ["", "equity", "price", "historical --symbol AAPL --provider yfinance"]
    SW->>BC: prepend to queue, navigate; menu enters /equity, then /equity/price
    BC->>DC: call_historical("--symbol AAPL --provider yfinance".split())
    DC->>DC: parse_known_args_and_warn() — adds --export/--register_obbject/--register_key
    DC->>AT: translator.execute_func(parsed_args=ns_parser)
    AT->>AT: _unflatten_args + _update_with_custom_types (rebuild pydantic models)
    AT->>SDK: obb.equity.price.historical(symbol="AAPL", provider="yfinance", ...)
    SDK->>CR: CommandRunner.run(...)
    CR->>OBB: returns OBBject(results=[...], extra={})
    OBB-->>AT: OBBject
    AT-->>DC: OBBject
    DC->>OBB: obbject.extra["command"] = "/equity/price/historical --symbol AAPL --provider yfinance"
    DC->>REG: register(obbject) (LIFO push; evict oldest if >N_TO_KEEP_OBBJECT_REGISTRY)
    DC->>DC: _link_obbject_to_data_processing_commands (refresh OBB0.. choices)
    DC->>DC: update_completer(choices_default)
    DC->>CON: print_rich_table OR session._backend.send_table (PyWry) OR figure.show()
    CON-->>U: rendered output
    BC->>P: next prompt() iteration
```

The critical move is `AT→SDK` at the middle: `translator.func` is bound to the
real `obb.equity.price.historical` `Container` method
(`argparse_translator/argparse_translator.py:472-505`). **No HTTP. No subprocess.
No IPC.** The CLI process *is* the SDK process.

## IPC contract

**None internal to the CLI.** The CLI is a self-contained Python process. The
only "IPC" is with the desktop app at launch:

| Direction | Name | Payload | Returns | Used by |
|-----------|------|---------|---------|---------|
| Renderer→Tauri | `execute_in_environment` | `{ command: "... openbb && exit", environment, directory }` | `Result<(), String>` | `desktop/src/routes/environments.tsx:867-924` `startCliSession` — spawns a native terminal window running `openbb` inside the activated conda env. The CLI then has no further IPC with the host. |

Once spawned, the CLI subprocess is **not** monitored, sidecar-tracked, or piped
through Tauri. It owns its own terminal window until the user types `exit`.

## State surfaces

- **Process singleton `Session()`** (`session.py:21-94`,
  `SingletonMeta` from `openbb_core`): holds `_obb` (the SDK accessor),
  `_settings` (pydantic), `_style`, `_console` (Rich), `_prompt_session`
  (prompt-toolkit), `_obbject_registry` (LIFO stack), `_backend` (PyWry).
  Every controller imports `Session()` at module top — shared across the
  entire process.
- **Module-level `controllers: dict[str, Any]`** (`base_controller.py:34`):
  cache of instantiated sub-controller objects keyed by `PATH`. Survives
  menu re-entry; only cleared by `reset`.
- **Module-level `PLATFORM_ROUTERS: dict[str, str]`** (`cli_controller.py:53-57`):
  built at import time by walking `dir(obb)`; used by `_generate_platform_commands`.
- **Module-level recording globals** (`base_controller.py:39-46`):
  `RECORD_SESSION`, `SESSION_RECORDED: list[str]`, `SESSION_RECORDED_NAME/_DESCRIPTION/_TAGS/_PUBLIC`.
- **Disk files** (see Persistence).

## Persistence

| File | Owner | Format | Written by |
|---|---|---|---|
| `~/.openbb_platform/.cli.env` | CLI-only settings overlay | `OPENBB_<KEY>=<value>` dotenv | `Settings.set_item` → `dotenv.set_key` (`models/settings.py:198-201`) |
| `~/.openbb_platform/.cli.his` | prompt-toolkit history (sanitized) | line-oriented (`prompt_toolkit.FileHistory` format) | every accepted prompt; `--password`/`--email`/`--pat` values masked to `********` (`config/completer.py:404-415`) |
| `~/.openbb_platform/user_settings.json` | platform SDK (credentials, preferences) | JSON | **Read** by CLI as `obb.user`. **Not written** by CLI. Written by `feature-api-keys.md` + REST server. |
| `~/.openbb_platform/system_settings.json` | platform SDK system config | JSON | Temporarily mutated by `utils/utils.py:11-34` to set `logging_sub_app="cli"` (but see Known Bugs). |
| `<user.preferences.export_directory>/routines/*.openbb` | CLI record/stop + Hub sync | plain text with comments + variables | `call_stop` (`base_controller.py:426-507`); Hub sync populates `hub/default/` and `hub/personal/`. |
| `<user.preferences.user_styles_directory>/*.richstyle.json` | user-defined Rich themes | JSON | external; CLI discovers and applies. |

## Error handling

- **Unknown command** at any menu: `BaseController.switch` catches `SystemExit`
  from the dispatch parser and runs `difflib.get_close_matches(cutoff=0.7)` to
  suggest a typo correction; if exactly one match, it re-runs as that command
  (`base_controller.py:584-608`).
- **SDK exception** during `translator.execute_func`: `_generate_command_call`
  prints `[red]…[/red]` via `session.console`; if `Settings.DEBUG_MODE`, the
  traceback is also shown. Registry push is skipped.
- **Missing argument**: argparse prints its usage; control returns to the
  prompt with no registry push.
- **Registry overflow**: `max_obbjects_exceeded()` (`session.py:90-94`) +
  pre-push `registry.remove()` (oldest) at
  `base_platform_controller.py:181-189`.
- **Routine HTTP fetch** (`exe --url`): `requests.get` with no auth headers;
  HTTP errors are caught and printed (`cli_controller.py:353-377`).
- **KeyboardInterrupt / EOF**: graceful `print_goodbye()` exit.

## ▸ Interfaces with

- **depends-on**: `feature-environments.md` for the conda env that hosts the
  Python interpreter, the `openbb-cli` extension, and every provider package.
- **depends-on**: `feature-api-keys.md` for credentials in
  `~/.openbb_platform/user_settings.json` — the CLI reads but never writes
  that file.
- **depends-on**: `feature-installation.md` — `openbb-cli` ships as an
  optional extension that must be installed into the env (via the
  Extensions modal or `pip install openbb-cli`).
- **sibling-of**: `feature-platform-rest-api.md` — both wrap the **same**
  `obb` SDK. Identical credentials + provider behavior; different transport
  (in-process for CLI, HTTP for the REST server). Neither calls the other.
- **spawned-by**: `feature-app-shell.md` / `feature-environments.md` via
  `invoke("execute_in_environment", ...)` (`environments.tsx:867-924`,
  `startCliSession`). The desktop app does **not** embed a terminal — it
  shells out to `cmd.exe`/`iTerm`/`x-terminal-emulator`.
- **shares-state-with**: `feature-platform-rest-api.md` via
  `system_settings.json["logging_sub_app"]` (both temporarily mutate it;
  see bug below) and `user_settings.json` (both read).
- **at-risk-from**: `feature-uninstall.md` — wiping `~/.openbb_platform/`
  removes `.cli.env` + `.cli.his` along with platform state.
- **conceptual-overlap**: `feature-jupyter.md` is a third front-end to the
  same `obb` SDK; same credentials, same `CommandRunner`, different shell.

## TS port mapping

The CLI's reason for existing is "call `obb.equity.price.historical(...)`
in-process". A TypeScript port has no in-process SDK to wrap — so the
translation question is structural, not mechanical. Three viable strategies:

| Strategy | Mechanism | Verdict |
|---|---|---|
| **A. Pure TS rewrite (in-process SDK)** | Re-implement `obb` + every provider extension in TS, then drive a `commander`/`inquirer` REPL from it. | **Blocked.** No TS-native SDK exists; porting `openbb-platform` + 50+ provider extensions is months of work for a feature with a tiny user-base relative to the desktop UI. |
| **B. REST-shim TS REPL** | Boot fetches `/openapi.json` from a running `openbb-api`; the OpenAPI doc supplies command discovery (paths, params, types, provider variants, descriptions). Each command becomes `POST /api/v1/{path}?provider=…`. Menu shell uses `commander` for parsing + `prompt-sync`/`inquirer` for the REPL; `cli-table3`/`chalk` for output; `commander`'s subcommand tree mirrors the `/equity/price/historical` hierarchy. | **Viable.** Decouples CLI from Python; reuses everything the REST server already produces. **Cost:** depends on a running REST server, every command pays HTTP round-trip latency, `OBBject` registry has to live entirely client-side (results stored as JSON only, no rebound charting objects). |
| **C. Skip entirely** | Don't ship a TS REPL. Users who want one keep running the existing Python `openbb` command — the desktop app already spawns it via `startCliSession`. The desktop UI covers almost every CLI feature (extension management, API-key setup, env activation, chart rendering); the only thing missing is scripted `.openbb` routines, which can be added as a "Run Routine" desktop action that replays REST API calls. | **Recommended for v1 port.** Lowest cost; defers the question; the desktop app + Python CLI continue to coexist exactly as today. |

Per-call mapping if Strategy B is chosen:

| Python piece | TS equivalent | Notes |
|---|---|---|
| `argparse` per-command parsers | `commander` Command tree built from `/openapi.json` parameter schemas | Each OpenAPI path → one `Command`; query params → `--flags` |
| `prompt-toolkit` REPL + completer | `prompt-sync` + custom completer over the command tree | `NestedCompleter` semantics ≈ commander's built-in `.helpInformation()` |
| `rich` Panel/Table/Theme | `chalk` + `cli-table3` + `boxen` | Mechanical |
| `pydantic` Settings | `zod` + `dotenv` (npm) for `.cli.env` persistence | Easy |
| OBBject LIFO registry | Plain JS array of `{ id, results, extra, command, register_key }` | Trivial; same eviction semantics. But results are JSON-only — no `.show()`/`.to_chart()` because there's no charting backend. |
| `.openbb` routine parser | Direct port of `script_parser.py` (~400 lines of regex) | Medium; date keywords + `foreach`/variable substitution all port cleanly |
| PyWry charts | Open browser tab with server-rendered Plotly HTML, or skip | High effort; defer to Workspace embedded view |
| Dynamic `PlatformControllerFactory` | Build command tree from `/openapi.json` once at boot, cache | The whole metaprogramming pattern collapses into a JSON walk |

## Known bugs and port-time fixes

> ⚠️ BUG: `change_logging_sub_app("cli")` only runs when `cli.py` is executed as
> `__main__` (`cli.py:25-32`). The Poetry-installed `openbb` console script
> calls `openbb_cli.cli:main` directly, bypassing that wrapper. Result: the
> platform logger never tags CLI sessions as `"cli"` for real users — it keeps
> whatever value `system_settings.json["logging_sub_app"]` previously held
> (likely `""` or `"api"`). **Port-time fix:** move the
> `change_logging_sub_app` call inside `main()` itself.

> ⚠️ BUG: Two parallel CLI sessions race on
> `~/.openbb_platform/.cli.env` — every `set_item` is a non-atomic `set_key`
> write; the loser's edits are lost. Same risk on
> `system_settings.json["logging_sub_app"]` between any two of {CLI, REST
> server, second CLI}. **Port-time fix:** use a lock file or move per-process
> state out of the shared platform dir.

> ⚠️ BUG: Module-level `controllers: dict[str, Any] = {}` at
> `base_controller.py:34` caches sub-controller instances forever in the
> process. Long sessions accumulate stale state (e.g., old credentials are
> baked into a cached controller's translators); `reset` is the only way to
> clear it. **Port-time fix:** scope the cache to the controller instance, or
> invalidate on settings change.

> ⚠️ BUG: `ArgparseClassProcessor` runs **twice** per menu enter — once in
> `PlatformControllerFactory.__init__` (`platform_controller_factory.py:15-17`)
> and once in `PlatformController.__init__`
> (`base_platform_controller.py:52-59`). The parser objects are then
> deep-copied per command (`argparse_translator.py:152-154`). Wasted boot time
> on every traversal. **Port-time fix:** factory hands the processed result to
> the runtime instance.

> ⚠️ BUG: `PLATFORM_ROUTERS` is built at module import time
> (`cli_controller.py:53-57`), forcing `from openbb import obb` even for
> `openbb --help`. Boot cost (several seconds) is paid before the help text
> renders. **Port-time fix:** lazy-build inside `launch()`; cache to disk.

> ⚠️ BUG: `CHOICES_GENERATION=True` triggers `controllers/choices.py`, which
> uses `unittest.mock.patch` + `inspect` to monkey-patch
> `parse_known_args_and_warn` and harvest the choice tree. Any change to the
> parser signature silently breaks the introspector. **Port-time fix:** make
> the choice tree a first-class output of the factory, not an introspection
> side effect.

> ⚠️ Race: `_link_obbject_to_data_processing_commands`
> (`base_platform_controller.py:73-89`) mutates `action.choices` on a parser
> object that is then deep-copied in `argparse_translator.py:152-154` — the
> mutation on the cached parser doesn't survive the copy. In practice this
> works because re-linking happens on every controller construction *and*
> after every successful command, but the design is brittle.

## Open questions

- **Should the port ship a REPL at all?** The desktop UI already covers
  extension management, API keys, env activation, and chart rendering. The CLI's
  unique value is `.openbb` routines (record/replay automation) and being
  spawnable inside a conda terminal for power users. If the desktop adds a
  "Run Routine" action that replays REST API calls, the CLI loses ~70% of its
  unique value. **Decision needed before any TS REPL work.**
- **If yes: Strategy B (REST-shim) or keep using the Python CLI?** Strategy B
  introduces a new TS surface that has to track every change to `openbb-platform`
  via OpenAPI; keeping the Python CLI is zero work but forks the codebase across
  languages.
- **How does the OBBject registry survive without an in-process SDK?**
  Strategy B's TS CLI would have to deserialize each command's response JSON
  into an in-memory object, but `OBBject.results` is a `List[pydantic.BaseModel]`
  serialized to JSON — no methods. `obbject.show()`/`obbject.charting.to_chart()`
  semantics are lost. Acceptable for `--data OBB0` linking (just pass the JSON
  back as `--data` body)? Or do we need a server-side registry?
- **Should `.openbb` routine semantics be standardized?** Today they're
  CLI-internal: `script_parser.py` is the only reader. If the desktop's "Run
  Routine" feature also reads them, the parser must be moved out of `openbb-cli`
  (probably into `openbb-platform` itself), which is upstream work.
- **What happens to `record`/`stop`?** Without an interactive REPL, recording
  has to happen by intercepting REST calls — a different mechanism entirely.
  The Tauri layer could trace `fetch` calls to `openbb-api` and serialize them
  to a `.openbb` file.
- **Auth surface.** The Python CLI inherits credentials from
  `user_settings.json` with no auth code of its own; a REST-shim TS CLI would
  have to authenticate against `openbb-api` (which is currently localhost-only
  but has `OPENBB_API_AUTH` for hub-token mode — see `feature-platform-rest-api.md`).
