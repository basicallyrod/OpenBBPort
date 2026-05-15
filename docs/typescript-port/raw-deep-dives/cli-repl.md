# Deep-dive: OpenBB CLI REPL

**Scope:** `/home/user/OpenBBPort/cli/openbb_cli/` — the Python package that registers the `openbb` console script. This is a **menu-driven shell** (bash-style with `/`-separated paths) that wraps `openbb-platform`'s `obb` SDK in-process. It is independent of the desktop app and of the `openbb-api` REST server, though the desktop app can launch it.

**Package metadata:** `openbb-cli` v1.4.1, AGPL-3.0-only, requires `openbb ^4.7.1` with `[all]` extras, plus `prompt-toolkit ^3.0.50`, `rich ^14.0.0`, `python-dotenv ^1.0.1`, `openpyxl ^3.1.5`, `pywry >=2.0.0rc7` (`cli/pyproject.toml:1-28`).

---

## 1. Boot sequence: `openbb` → `main` → `bootstrap()` → `launch()` → CLI menu

The Poetry script entry `openbb = 'openbb_cli.cli:main'` (`cli/pyproject.toml:14`) makes `openbb` a console script.

### 1.1 `cli.py:main()` — first-touch entry

`cli/openbb_cli/cli.py:9-22`:

```python
def main():
    print("Loading...\n")
    from openbb_cli.config.setup import bootstrap
    from openbb_cli.controllers.cli_controller import launch

    bootstrap()
    dev = "--dev" in sys.argv[1:]
    debug = "--debug" in sys.argv[1:]
    launch(dev, debug)
```

Notable: imports are **deferred** inside the function so the `print("Loading...")` shows immediately (`cli.py:11`), since pulling in `openbb` and the platform SDK can take several seconds.

The `if __name__ == "__main__"` block at `cli.py:25-32` wraps `main()` between `change_logging_sub_app()` / `reset_logging_sub_app()` calls. Those mutate `~/.openbb_platform/system_settings.json` to set `logging_sub_app = "cli"` so the platform's logger tags entries as coming from the CLI (`utils/utils.py:11-34`).

> Note: when launched via the Poetry-installed `openbb` console script entry, Python doesn't run the `if __name__ == "__main__"` branch — the script stub calls `openbb_cli.cli:main` directly, so the `change_logging_sub_app`/`reset_logging_sub_app` wrappers are **only active when running `python cli.py` directly**, not via the installed `openbb` command. This is an apparent oversight.

### 1.2 `bootstrap()` — make config dir

`cli/openbb_cli/config/setup.py:8-11`:

```python
def bootstrap():
    SETTINGS_DIRECTORY.mkdir(parents=True, exist_ok=True)
    Path(ENV_FILE_SETTINGS).touch(exist_ok=True)
```

Where `SETTINGS_DIRECTORY = HOME_DIRECTORY / ".openbb_platform"` and `ENV_FILE_SETTINGS = SETTINGS_DIRECTORY / ".cli.env"` (`config/constants.py:9, 12`). The same `~/.openbb_platform/` directory is used by the platform itself for `user_settings.json` and `system_settings.json` — so the CLI shares state with the SDK and indirectly with the REST server and desktop app.

### 1.3 `launch(debug, dev)` and `parse_args_and_run()`

`controllers/cli_controller.py:905-916`:

```python
def launch(debug: bool = False, dev: bool = False, queue: list[str] | None = None) -> None:
    if queue:
        main(debug, dev, queue, module="")
    else:
        parse_args_and_run()
```

`parse_args_and_run()` at lines 811-902 sets up an `argparse.ArgumentParser(prog="cli")` with `-d/--debug`, `--dev`, `--file`, `-i/--input`, `-t/--test`, plus three SUPPRESS-help-text arguments (`-m`, `-f`, `--HistoryManager.hist_file`) used by Jupyter/papermill (lines 859-881). If the first positional arg has no leading `-`, it's silently rewritten as `--file <arg>` (lines 882-883). Unknown args either trigger `sys.exit(-1)` or just print in debug mode (lines 886-892).

`main(debug, dev, path_list, routines_args, **kwargs)` at lines 763-808:
- `--debug` sets `session.settings.DEBUG_MODE = True` (line 791).
- `--dev` flips the Hub URLs to `payments.openbb.dev` / `my.openbb.dev` (lines 793-796).
- If the path ends with `.openbb`, calls `run_routine(file=…)` (lines 798-802).
- Otherwise, joins the remaining args into a `/`-prefixed command queue and calls `run_cli(argv_cmds)` (lines 803-806).
- With no args, plain `run_cli()` (line 808).

### 1.4 `run_cli(jobs_cmds, test_mode)` — top-level REPL

`controllers/cli_controller.py:498-608`:
1. Instantiate `t_controller = CLIController(jobs_cmds)` (line 501) — this is what triggers the heavy SDK introspection.
2. `bootup()` (line 506) — reconfigures stdin/stdout to utf-8 on Windows and enables VT100 escapes (`controllers/utils.py:80-93`).
3. If no preloaded jobs: `welcome_message()`, check `first_time_user()` (opens `https://docs.openbb.co/cli` in webbrowser if true) and `t_controller.print_help()` (lines 507-514).
4. Enter the infinite loop (`while ret_code:` line 516):
   - If `t_controller.queue` is non-empty: pop the head, echo it to console if it's a known command (lines 518-530).
   - Else: call `session.prompt_session.prompt(f"{flair} / $ ", completer=t_controller.completer, search_ignore_case=True, bottom_toolbar=HTML(...))` (lines 538-555). The toolbar text shows `[h]` `[q]` `[e]` `[cmd -h]` keybind hints.
   - Pass the line to `t_controller.switch(an_input)` which returns the new queue (line 573).
   - Recognize `q/quit/..`, `e/exit`, `r/reset` for control flow (lines 575-582).
   - `SystemExit` exception is caught and rewritten via `difflib.get_close_matches(…, cutoff=0.7)` for typo suggestions ("Replacing by 'x'.") (lines 584-608).

`KeyboardInterrupt`/`EOFError` calls `print_goodbye()` and breaks (lines 567-569).

---

## 2. Singleton `Session` — global mutable state

`cli/openbb_cli/session.py:21-94`. Uses `openbb_core.app.model.abstract.singleton.SingletonMeta` as a metaclass, so every `Session()` call returns the same instance. (Imported at line 9.)

Held state, all created lazily in `__init__`:

| Field | Type | Source line |
|---|---|---|
| `_obb` | `from openbb import obb` (the runtime SDK accessor) | `session.py:27` |
| `_settings` | `Settings()` (pydantic model from `models/settings.py`) | `session.py:28` |
| `_style` | `Style(style=settings.RICH_STYLE, directory=obb.user.preferences.user_styles_directory)` | `session.py:29-32` |
| `_console` | `Console(settings=_settings, style=_style.console_style)` — Rich wrapper | `session.py:33-35` |
| `_prompt_session` | `PromptSession(history=CustomFileHistory(HIST_FILE_PROMPT))` if stdin.isatty() else `None` | `session.py:36, 76-88` |
| `_obbject_registry` | `Registry()` (LIFO stack of `OBBject`s) | `session.py:37` |
| `_backend` | `openbb_charting.core.backend.Backend(ChartingSettings(...))` (PyWry-based) | `session.py:39-44` |

The `user` property at lines 47-49 returns `obb.user` — that's the **same** `UserSettings` object the platform SDK uses, loaded from `~/.openbb_platform/user_settings.json`. The CLI therefore inherits credentials, preferences, defaults, and routine paths from the platform without any duplication.

Why a singleton: every controller, every utility, every command imports `from openbb_cli.session import Session` and calls `Session()` at module top — `controllers/cli_controller.py:61`, `controllers/base_controller.py:35`, `controllers/base_platform_controller.py:19`, etc. They all share `session.console`, `session.settings`, `session.obbject_registry`, `session.prompt_session`, `session._backend`. State changes (e.g., flipping `USE_INTERACTIVE_DF`, registering a new `OBBject`, updating the completer) propagate to every other controller via the shared object.

`max_obbjects_exceeded()` at lines 90-94 is a guard used by command callers to evict before pushing.

---

## 3. The Controller pattern — menu navigation, queues, path parsing

### 3.1 `BaseController` — abstract REPL base

`controllers/base_controller.py:47-951`. Every menu (root, settings, every dynamically-generated `equity/`, `equity/price/`, `crypto/`, …) is a subclass.

Class-level constants:
- `CHOICES_COMMON` (lines 50-65): `cls`, `home`, `h`, `?`, `help`, `q`, `quit`, `..`, `e`, `exit`, `r`, `reset`, `stop`, `results` — universal in every menu.
- `CHOICES_COMMANDS` / `CHOICES_MENUS` — populated by subclasses.
- `COMMAND_SEPARATOR = "/"` (line 70).
- `PATH: str = ""` — overridden, e.g. `"/"` for `CLIController`, `"/settings/"` for `SettingsController`, `"/equity/"` for the equity router. Validated at lines 124-134 (must start+end with `/`, only lowercase + `/`).
- `CHOICES_GENERATION = False` — set to `True` in subclasses that want auto-built completer choice trees via `controllers/choices.py:build_controller_choice_map` (line 80-86).

`__init__(queue)` at lines 87-117:
- Parses the leftover queue with `parse_input` (line 99) — but only if `PATH != "/"`, since root takes raw `jobs_cmds`.
- `controller_choices = CHOICES_COMMANDS + CHOICES_MENUS + CHOICES_COMMON` (lines 104-108).
- Creates a private `argparse.ArgumentParser(add_help=False, prog=self.path[-1])` with one positional `cmd` argument whose `choices = self.controller_choices` (lines 112-117). This is the dispatch parser.

### 3.2 The queue / path / `/` semantics

User types a string like `equity/price/historical --symbol AAPL`. `parse_input` (lines 163-171) splits on `/` (using a regex that excludes split points inside quoted strings), producing `["equity", "price", "historical --symbol AAPL"]`. These become **separate items** in the queue.

`switch(an_input)` (lines 173-239) handles three cases:
1. Empty → no-op.
2. Multiple actions (slash navigation): if `actions[0] == ""` (absolute path starting with `/`), substitute `"home"` (line 194). Then prepend all actions to the queue in reverse (lines 196-199).
3. Single command: parse with the dispatch parser (line 204), map shortcut aliases (`..`/`q` → `quit`, `e` → `exit`, `?`/`h` → `help`, `r` → `reset`) at lines 215-222, and call `self.call_{cmd}(other_args)` (lines 224-228). Falls back to `lambda _: "Command not recognized!"` if missing.

A separate `RECORD_SESSION` global (line 39) and `SESSION_RECORDED: list` (line 40) capture every line typed when recording is active (lines 210-211).

`menu()` at lines 838-951 is the **per-controller** REPL identical in structure to `run_cli` but parameterized by `self.PATH`. When entering a sub-menu, the caller does `self.queue = self.load_class(SubController, ...)` which calls `SubController(...).menu()` (lines 136-145). The previous menu's `queue` is passed through, and the inner `menu()` consumes commands until it hits a `quit/q/..`, at which point it returns the remaining queue back up the stack (lines 846-858).

Controllers are cached in a module-level `controllers: dict[str, Any] = {}` (line 34) keyed by `PATH`. On re-entry to a menu, the old instance is reused so its completer, state, and recorded session continue (lines 141-145).

### 3.3 Navigation traversal

- `call_home` (lines 245-251): inserts as many `quit` ops in the queue as the current path depth, unwinds back to root. Prints help on the way out if `ENABLE_EXIT_AUTO_HELP`.
- `call_quit` (lines 257-260): single `quit`, exits one level.
- `call_exit` (lines 262-267): `path.count("/")` `quit`s — exits the whole CLI.
- `call_reset` (lines 269-284): saves class, queues up `["quit", "quit", …, "reset", … original path]` — the outer `run_cli` sees `reset`, calls `controllers.utils.reset(queue)` which clears all `openbb_cli` modules from `sys.modules` and re-imports + re-runs `main()` with the saved queue (`controllers/utils.py:106-131`).

`parse_and_split_input` (`controllers/utils.py:166-228`) is the wrapper used by `run_cli` for jobs_cmds: it temporarily replaces UNIX-path-like substrings (`-f some/path.csv`) with placeholders so they're not split by `/`, then restores them after splitting on `/`. The `timezone` setting is also kept on a single line as a special case (line 219).

### 3.4 Universal commands: `cls`, `record`, `stop`, `results`, `help`

`call_cls` → `os.system("cls||clear")` (line 241-243 + utils:865-867).

`call_record(other_args)` (lines 286-424): parses `-n/--name`, `-d/--description`, `--tag1/2/3`, `-p/--public`. On success, sets module globals `RECORD_SESSION = True`, `SESSION_RECORDED_NAME/_DESCRIPTION/_TAGS/_PUBLIC`. From that point on, every `switch()` call appends `an_input` to `SESSION_RECORDED` (lines 210-211).

`call_stop(other_args)` (lines 426-507): requires ≥5 commands recorded. Writes `<routines>/<name>.openbb` with a header (`# OpenBB Platform CLI - Routine`, `# Title:`, `# Tags:`, `# Description:`) followed by each recorded line. Routine directory is `user.preferences.export_directory/routines` (line 456) — same path the platform uses.

`call_results(other_args)` (lines 509-591): inspects `session.obbject_registry`. Supports `--index N`, `--key K`, `--chart`, `--export csv,json,xlsx,png,jpg`, `--sheet-name`. Without index/key, prints a Rich table of `registry.all`.

---

## 4. `PlatformControllerFactory` — synthesizing controllers from the SDK

`controllers/platform_controller_factory.py:9-56`. This is the magic that lets `obb.equity.price.historical(...)` show up as the menu command `equity/price/historical`.

### 4.1 What it consumes

At module import time, `cli_controller.py:53-57` builds:

```python
PLATFORM_ROUTERS = {
    d: "menu" if not isinstance(getattr(obb, d), BaseModel) else "command"
    for d in dir(obb)
    if "_" not in d
}
```

i.e., walk every public attribute on `obb`. If it's a pydantic `BaseModel` (like `obb.user`, `obb.system`, `obb.coverage`, `obb.reference`), treat it as a **leaf command** that just dumps the model. Otherwise it's a router (a `Container` subclass like `obb.equity`, `obb.crypto`, …) — those become menus.

`NON_DATA_ROUTERS = ["coverage", "reference", "system", "user"]` and `DATA_PROCESSING_ROUTERS = ["technical", "quantitative", "econometrics"]` (`cli_controller.py:58-59`) — these get special placement in the help text but otherwise go through the same factory.

### 4.2 `_generate_platform_commands()` in `CLIController`

`cli_controller.py:103-151`. For each `(router_name, kind)` in `PLATFORM_ROUTERS`:
- If `kind == "menu"`: instantiate `PlatformControllerFactory(target=getattr(obb, router), reference=obb.reference["paths"])` and `pcf.create()` to get a `DynamicController` class. Bind a `method_call_class` closure as `call_<router>` that calls `self.load_class(controller=DynamicController, name=router, ...)` (lines 124-143).
- If `kind == "command"`: bind `method_call_command` that just `pd.DataFrame.from_dict(getattr(obb, router).model_dump()).T` and prints it (lines 144-149).

The binding uses `MethodType` + `functools.partial` + `update_wrapper` so the closure's name still reports the original wrapped method name in stack traces.

### 4.3 `PlatformControllerFactory.create()`

`platform_controller_factory.py:27-56`. Steps:

1. Run `ArgparseClassProcessor(target_class=platform_router, reference=…)` on the router (line 15-17). This recursively walks the router's Container hierarchy and produces:
   - `translators: dict[str, ArgparseTranslator]` — one per leaf method, keyed like `equity_price_historical`.
   - `paths: dict[str, str]` — keyed by sub-container name, value is either `"path"` (top-level), `"subpath"`, `"subsubpath"`, … encoding depth.
2. Derive `router_name` from `str(type(platform_router))` stripping `ROUTER_` prefix and lowercasing (lines 18-24). For `obb.equity`, this yields `equity`.
3. Walk `paths.items()` — each non-root entry becomes a `CHOICES_MENUS` entry (e.g., `price`, `fundamental`, `discovery`).
4. Walk `translators.items()` — if the translator name doesn't contain any sub-path prefix, it becomes a `CHOICES_COMMANDS` (e.g., `equity_search → search`).
5. Use `type(ClassName, (PlatformController,), Attributes)` to **synthesize a new class** dynamically (line 54).

The newly-minted class inherits `PlatformController` from `controllers/base_platform_controller.py` and gets `CHOICES_MENUS`, `CHOICES_COMMANDS`, `CHOICES_GENERATION=True`.

### 4.4 `PlatformController.__init__` (`base_platform_controller.py:31-71`)

When the dynamic class is instantiated:
1. Build `PATH = f"/{parent_path}/{name}/"`.
2. Re-run `ArgparseClassProcessor` to get translators + paths for this specific target — yes, it's done twice in the worst case (once in factory, once at construction). The factory rebuilds because it wants the dynamic class, the runtime instance wants the actual `argparse.ArgumentParser` objects.
3. Call `_link_obbject_to_data_processing_commands()` to inject `OBB0..OBBN` and registry-key choices into any `--data` argument (lines 73-89).
4. Call `_generate_commands()` (lines 143-149) — for each translator, bind a `method` closure as `call_<command>` (`_generate_command_call`, lines 151-282). The closure parses with `self.parse_known_args_and_warn(parser, other_args, export_allowed="raw_data_and_figures")`, executes via `translator.execute_func(parsed_args=ns_parser)` (line 171), and handles the result.
5. Call `_generate_sub_controllers()` (lines 106-141) — for each path entry that's not `"path"`, partition the translators into a sub-set whose name starts with `f"{self._name}_{path}"`, create a `SubController = type(f"{Name}{Path}Controller", (PlatformController,), {...})`, and bind a `call_<path>` closure that loads the sub-controller. This is where the recursive nesting actually happens.
6. `update_completer(self.choices_default)` — wires up `NestedCompleter` for prompt-toolkit autocompletion (line 71, also `base_controller.py:119-122`).

### 4.5 What command execution does (`_generate_command_call`)

`base_platform_controller.py:151-282`:

```python
def method(self, other_args, translator=translator):
    parser = translator.parser  # deep-copied each call
    if ns_parser := self.parse_known_args_and_warn(parser, other_args, export_allowed="raw_data_and_figures"):
        ns_parser = self._intersect_data_processing_commands(ns_parser)  # rewrite OBB0/key → actual results
        ...
        obbject = translator.execute_func(parsed_args=ns_parser)  # the real SDK call
        ...
        # Register in obbject_registry, set obbject.extra["command"]/extra["register_key"]
        # Display: chart (via OpenBBFigure.show()), interactive table (via charting.table()), or print_rich_table
```

This is the only place where the **actual SDK is invoked** — `translator.execute_func` (`argparse_translator.py:472-505`) eventually calls `self.func(**kwargs)`, where `self.func` is bound to e.g. `obb.equity.price.historical` (a `Container` method that internally calls the platform's `CommandRunner`).

> Note: `parse_known_args_and_warn` (`base_controller.py:642-836`) adds two pseudo-arguments to every parser: `--register_obbject` (store_false, default True) and `--register_key KEY` (lines 733-746). These never reach the SDK — they're stripped from `kwargs` in `_intersect_data_processing_commands` (lines 91-104) and the storage logic in `_generate_command_call` (lines 166-229).

### 4.6 Universal `parse_known_args_and_warn` behaviors

Beyond `--register_obbject`/`--register_key`, the function also:
- Adds `--export {csv,json,xlsx,png,jpg}` with smart `check_file_type_saved` validator (lines 677-700).
- Adds `--sheet-name` for xlsx exports (lines 702-712).
- Adds `--raw` and `-l/--limit` if requested (lines 715-731).
- Performs a quirky comma-splitting pass on `other_args` (lines 775-810) so that `--symbol AAPL,MSFT` is preserved as a single token (because providers may accept it), but unquoted positional values get split. The complexity in `no_split_indices` accounts for `--flag=value`, multi-value flags (nargs `+`/`*`/int), and the `-i routine_args` case.

### 4.7 Linking results to data-processing commands

`_link_obbject_to_data_processing_commands` (`base_platform_controller.py:73-89`) iterates every translator and finds parser actions where `action.dest == "data"`. It rewrites `action.choices` to the list of currently-registered OBBject identifiers (`OBB0`, `OBB1`, …) plus any user-supplied `register_key` strings. So when you do e.g. `technical/sma --data OBB0`, the choice list autocompletes from the registry.

After every successful command that stored a new OBBject, this is re-run (line 219) plus `self.update_completer(self.choices_default)` (line 221), so newly-cached results become instantly typeable.

---

## 5. `ArgparseClassProcessor` — SDK → argparse adapter

`argparse_translator/argparse_class_processor.py:15-147`.

### 5.1 Class walking (`_process_class`)

For each public member of the target class (`name` not starting with `_`, lines 91-92):
- If it's a method → wrap it in `ArgparseTranslator(func=member, custom_argument_groups=…)` keyed as `f"{class_name}_{name}"` (lines 94-103).
- If it's a `Container` (a sub-router like `obb.equity.price`) → recurse (lines 103-109).

`_get_class_name(target)` (lines 113-121) returns the class name with the `ROUTER_` prefix stripped and lowercased, used as the key prefix.

### 5.2 Reference enrichment

`_custom_groups_from_reference` (lines 74-81) looks up the call's reference entry (`obb.reference["paths"]`) by route string (`f"/{class_name.replace('_', '/')}/{function_name}"`), and runs `ReferenceToArgumentsProcessor` to build per-provider argument groups (`argparse_translator/reference_processor.py:12-122`). For each provider (e.g., `fmp`, `yfinance`, `polygon`), the provider's extra parameters become a custom argparse group that's grafted onto the parser. If a parameter exists in multiple providers, the choices are unioned and the help text is rewritten to include `(provider: fmp, yfinance)` (lines 99-148 of `argparse_translator.py`).

### 5.3 `ArgparseTranslator` — per-method machinery

`argparse_translator/argparse_translator.py:38-529`. Constructor (lines 41-75):
- `func = method`, `signature = inspect.signature(func)`, `type_hints = get_type_hints(func)`.
- Build an `argparse.ArgumentParser(prog=func.__name__, description=_build_description(func.__doc__), formatter_class=RawTextHelpFormatter)`.
- Add a `required arguments` group (line 65).
- Iterate parameters via `_generate_argparse_arguments` (lines 359-439):
  - Skip `kwargs`.
  - For each, get `(param_type, choices)` via `_get_type_and_choices` (lines 241-308). This unwraps `Optional`, `Union`, `Annotated`, `Literal`, `List` recursively. `Literal` types yield `choices`; `bool` → `action="store_true"`; lists → `nargs="+"`.
  - If the param is itself a pydantic `BaseModel`, **flatten it** into individual args prefixed with `<paramname>__<field>` (the `SEP = "__"` constant) — lines 367-405. This is the key piece that handles `DateRange`, `Filters`, etc. The `_unflatten_args` static method at lines 441-456 reverses the flattening at execute time.
  - Custom annotations: `Annotated[..., OpenBBField(description=..., choices=...)]` provides help text and override choices (`_get_argument_custom_help`, `_get_argument_custom_choices` at lines 324-342).

`_escape_help` (lines 156-166) doubles `%` to `%%` because Python 3.14+ validates `%(default)s`-style format strings in argparse help.

`_build_description` (lines 168-214) strips the numpy-docstring Parameters/Returns/Examples/Raises sections, cleans up `Optional[T]`/`Annotated[T, …]`/`Union[A, B]`/`A | B` annotations.

`execute_func` (lines 472-505) is invoked at command time: `kwargs = _unflatten_args(vars(parsed_args))`, then `_update_with_custom_types` reconstructs pydantic instances from the flat dicts (lines 458-470). The provider's extra args are filtered against `self.provider_parameters[provider]` (lines 488-504). Finally `self.func(**kwargs)` — i.e., the actual SDK call.

---

## 6. OBBject registry — LIFO stack of cached results

`argparse_translator/obbject_registry.py:8-128`. Each entry is an `OBBject` (the platform's universal response wrapper from `openbb_core.app.model.obbject`).

`register(obbject)` (lines 20-29):
- Deduplicates by `obbject.id` (a uuid set when the SDK creates it).
- Skips if `obbject.results` is falsy.
- Appends to `self._obbjects` (a plain list).

The list works as a stack: `_get_by_index(idx)` (lines 47-57) reverses the list so the **most recent** result is `OBB0`. `_get_by_key(key)` (lines 40-45) looks up by `obbject.extra.get("register_key")`. `remove(idx=-1)` defaults to popping the oldest (lines 59-65).

`Settings.N_TO_KEEP_OBBJECT_REGISTRY = 10` (default) bounds the registry (`models/settings.py:139-146`). When a command tries to add #11, `_generate_command_call` calls `session.obbject_registry.remove()` to evict the oldest (`base_platform_controller.py:181-189`).

`all` (lines 67-114) builds a display dict for the help / `results` command: each obbject becomes `{route, provider, standard_params(json), data, command, key}`.

The command-time storage flow (`base_platform_controller.py:166-229`):
1. `obbject.extra["command"] = f"{self.PATH}{translator.func.__name__} {' '.join(other_args)}"` — the literal command becomes the audit log.
2. If `--register_key K` was given, store as `obbject.extra["register_key"] = str(K)` — but only if the key isn't already taken (lines 197-209).
3. If `--register_obbject` wasn't suppressed, `session.obbject_registry.register(obbject)`. Then re-link `--data` choices and refresh completer.

The `--data` argument linking (covered in §4.7) is what makes `technical/sma --data OBB0` and `quantitative/normality --data my_register_key` work — the parser action's `choices` gets rewritten before each command, and `_intersect_data_processing_commands` (lines 91-104) translates the string back to the actual `obbject.results` payload before invocation.

---

## 7. Settings — pydantic + dotenv

`models/settings.py:22-202`. Pydantic `BaseModel(validate_assignment=True)`.

### Fields (json_schema_extra carries `command` and `group` metadata)

**Development flags** (no group, internal): `TEST_MODE`, `DEBUG_MODE`, `DEV_BACKEND` (lines 28-31).

**Hub-related**: `HUB_URL = "https://my.openbb.co"`, `BASE_URL = "https://payments.openbb.co"` (lines 34-35). Flipped by `--dev` (`cli_controller.py:793-796`).

**Feature flags** (group `feature_flag`, toggled by setting command of the same name):
| Field | Default | Command name |
|---|---|---|
| `FILE_OVERWRITE` | False | `overwrite` |
| `SHOW_VERSION` | True | `version` |
| `USE_INTERACTIVE_DF` | True | `interactive` |
| `USE_CLEAR_AFTER_CMD` | False | `cls` |
| `USE_DATETIME` | True | `datetime` |
| `USE_PROMPT_TOOLKIT` | True | `promptkit` |
| `ENABLE_EXIT_AUTO_HELP` | True | `exithelp` |
| `ENABLE_RICH_PANEL` | True | `richpanel` |
| `TOOLBAR_HINT` | True | `tbhint` |
| `SHOW_MSG_OBBJECT_REGISTRY` | False | `obbject_msg` |

**Preferences** (group `preference`, set with `<cmd> -v VALUE`):
| Field | Default | Command |
|---|---|---|
| `TIMEZONE` | `America/New_York` | `timezone` |
| `FLAIR` | `:openbb` (🦋) | `flair` |
| `N_TO_KEEP_OBBJECT_REGISTRY` | 10 | `obbject_res` |
| `N_TO_DISPLAY_OBBJECT_REGISTRY` | 5 | `obbject_display` |
| `RICH_STYLE` | `dark` | `console_style` |
| `ALLOWED_NUMBER_OF_ROWS` | 20 | `n_rows` |
| `ALLOWED_NUMBER_OF_COLUMNS` | 5 | `n_cols` |

### Persistence

`from_env` validator (lines 188-196) reads `~/.openbb_platform/.cli.env` via `python-dotenv`, strips the `OPENBB_` prefix, and merges with constructor values (env file wins lower precedence than explicit values).

`set_item(key, value)` (lines 198-201): both updates `setattr(self, key, value)` (subject to pydantic validation) and persists with `set_key(ENV_FILE_SETTINGS, "OPENBB_" + key, str(value))`. This is what the settings menu commands call.

`SettingsController` (`controllers/settings_controller.py`) auto-generates one `call_<command>` per setting field at construction time (lines 46-52, 75-148):
- Feature flags get a `_toggle` that flips the bool (lines 80-93).
- Preferences get a `_set` that adds `-v/--value` (with `choices=Literal.__args__` or `session.style.available_styles` for `console_style`) and assigns it (lines 95-135).

---

## 8. Console — Rich-based output

`config/console.py:16-94`. Wraps `rich.console.Console(theme=Theme(style), highlight=False, soft_wrap=True)`.

`print(*args, **kwargs)`:
- If `text=...` and `menu=...` keyword args are given, wrap in a `rich.panel.Panel(text, title=menu, subtitle="OpenBB Platform CLI vX.Y …")` provided `ENABLE_RICH_PANEL` is true (lines 62-79). This is what every `print_help()` call uses.
- Otherwise, plain `self._console.print(*args, **kwargs)` (line 86).
- In `TEST_MODE`, rich tags are stripped and plain `print()` is used (lines 83-84, 87-88).

`input(*args, **kwargs)` (lines 90-93) just calls `print(..., end="")` then `input()`.

`Style` (`config/style.py:15-106`) discovers `.richstyle.json` files in `cli/openbb_cli/assets/styles/{default,user}/` and any user-configured `user.preferences.user_styles_directory`. `apply(style)` swaps `self.console_style` (a dict) used by `Console`. Defaults to `dark`.

Custom Rich tags recognized everywhere: `[menu]`, `[cmds]`, `[info]`, `[param]`, `[src]`, `[help]`, plus standard `[green]`, `[red]`, `[yellow]` (`config/menu_text.py:12-25`).

The `MenuText` builder (`config/menu_text.py:28-165`) is what every `print_help` calls. It produces aligned three-column rows (`name (23) + description (65) + providers`) using string padding and rich color tags. Long command names truncate with a warning recorded in `mt.warnings` (lines 62-87). Providers are looked up via `obb.reference["paths"][f"{menu_path}{cmd}"]["parameters"]` (lines 42-60) and displayed like `[fmp, yfinance, polygon]`.

---

## 9. Charting — Plotly via OpenBBFigure / PyWry

The CLI never directly imports Plotly. Charting is delegated to `openbb_charting` (already a transitive dep through `openbb[all]`).

- `Session._backend = Backend(ChartingSettings(system_settings=obb.system, user_settings=obb.user))` (`session.py:39-44`). This `Backend` is from `openbb_charting.core.backend` and is built on **PyWry** (`pywry >=2.0.0rc7` in `pyproject.toml:27`). PyWry is a Rust-backed webview wrapper that opens Plotly figures and tables in a popped-out native window.
- `controllers/utils.py:print_rich_table` (lines 264-465): if `session.settings.USE_INTERACTIVE_DF`, it calls `session._backend.send_table(df_table=df_outgoing, title=…, theme=session.user.preferences.table_style)` (line 360). That ships the DataFrame to the PyWry window for interactive display.
- For chart rendering, `obbject.show()` and `obbject.charting.to_chart()` (`base_platform_controller.py:234-237`, `controllers/utils.py:944-955`) hand off to the same `_backend`. `OpenBBFigure` is the platform's wrapper around `plotly.graph_objects.Figure`.

Export to image: when `--export png` or `jpg` is in `ns_parser.export`, the code calls `figure.show(export_image=saved_path, margin=True)` (`controllers/utils.py:847-851`).

In `TEST_MODE` and when stdin isn't a TTY, the interactive backend isn't actually used (PyWry windows would block headless runs).

---

## 10. Scripting / `.openbb` routines — record, stop, exe

### 10.1 Recording (covered in §3.4)

`call_record` → globals `RECORD_SESSION = True`, then every `BaseController.switch(an_input)` appends to `SESSION_RECORDED` (`base_controller.py:210-211`). `call_stop` writes the file. The history file is also kept in `~/.openbb_platform/.cli.his` via `CustomFileHistory(HIST_FILE_PROMPT)` (`session.py:81`, `config/completer.py:400-420`), which sanitizes `--password`/`--email`/`--pat` arguments before storing.

### 10.2 `.openbb` syntax

Example at `cli/openbb_cli/assets/routines/routine_example.openbb`:

```
# Go into the equity context
equity
profile --symbol aapl
fundamental
balance --symbol aapl
../price
historical --symbol aapl
historical --symbol aapl --chart --export csv,jpg
```

Supports:
- `#` line comments.
- Variable substitution: `$VARNAME` is replaced from `ROUTINE_VARS`. `$ARGV[i]` from `-i/--input GME,AMC,BTC-USD` CLI args.
- Variable declaration: `$DATE = 2024-01-01`.
- Slicing: `$VAR[0]`, `$VAR[1:3]`.
- `foreach $$VAR in a,b,c … end` loops — body lines containing `$$VAR` are duplicated per element.
- Date keywords: `$1MONTHAGO`, `$3YEARSFROMNOW`, `$LASTFRIDAY`, `$NEXTTUESDAY`, `$LASTJANUARY`, … resolved by `match_and_return_openbb_keyword_date` (`controllers/script_parser.py:60-143`).
- Bare `reset`/`r` lines are stripped before parsing (line 175, `is_reset` at lines 42-57).

### 10.3 `exe` command

`CLIController.call_exe(other_args)` (`cli_controller.py:291-463`):
- `--file PATH` (or `-f`): local routine (looked up via `ROUTINE_FILES`, `ROUTINE_DEFAULT_FILES`, `ROUTINE_PERSONAL_FILES` populated from `<export_dir>/routines/{,/hub/default,/hub/personal}`).
- `--url URL`: download a routine from `my.openbb.co` (uses `requests.get(f"{url}?raw=true")`, expects JSON `{"script": "..."}`, saves locally as `<username>_<script_name>.openbb` — lines 353-377).
- `-i/--input`: `script_inputs` for `$ARGV` substitution.
- `-e/--example`: runs the bundled `assets/routines/routine_example.openbb`.

If the first argument starts with `my.` or `http`, auto-prepends `--url` (lines 340-344).

After parsing, the routine is read, passed through `parse_openbb_script(raw_lines, script_inputs)` (`controllers/script_parser.py:146-472`), and the **resulting `/`-joined string** becomes the new `self.queue`. From that point, the main REPL loop just consumes the queue like a typed sequence of commands. If the first line is `export <folder>`, the folder is created/announced and stripped (lines 435-457).

### 10.4 `run_scripts` (test mode)

`cli_controller.py:620-715` is a parallel path used by `--test` and `--file foo.openbb`: it pre-builds the `simulate_argv` string (`/equity/load gme/.../exit`) and passes it as `jobs_cmds` to `run_cli(..., test_mode=True)`. With `output=True`, stdout is captured into `<repo>/integration_test_output/<timestamp>_<cmd>_output.txt`.

---

## 11. Hub login / Pro / cloud sync

**The CLI does *not* implement its own auth flow.** Everything Hub-related routes through the platform SDK or the local routine files.

Specifics:
- The settings model holds `HUB_URL` (`https://my.openbb.co` or `https://my.openbb.dev` with `--dev`) and `BASE_URL` (payments endpoint) but **no auth code uses them** in the CLI source. The CLI doesn't have a `login` command — credentials live entirely in the platform SDK's `user_settings.json`.
- Hub routines (`<export_dir>/routines/hub/default/*.openbb` and `<export_dir>/routines/hub/personal/*.openbb`) are read at startup (`cli_controller.py:164-175`). They're populated by **whoever pulls them from the Hub** — apparently the platform itself or an external sync tool, not the CLI.
- The `exe --url my.openbb.co/...` flow (lines 353-377) fetches a single routine HTTP without auth headers. It expects the route to be public.
- `controllers/cli_controller.py:18` imports `requests` — but the only HTTP call in the CLI is that routine fetch.
- Sensitive arg sanitization in history: `--password`, `--email`, `--pat` values are masked to `********` before storage (`config/completer.py:404-415`). This suggests these flags exist somewhere in the platform's argument set (likely via SDK methods like `obb.account.login`), but they're consumed transparently by `_generate_command_call` — the CLI just sanitizes the history.

> Note: A previous version of the CLI (pre-Platform, pre-v3) had explicit `account/login` controllers. Those are gone in v1.4.1.

---

## 12. Cross-cutting connections to the rest of the system

### 12.1 CLI vs `openbb-api` REST server

**The CLI does NOT talk to the REST server.** It calls `obb.equity.price.historical(...)` **in-process**, which in turn calls `openbb_core.app.command_runner.CommandRunner.run(...)` directly (see `platform-rest-api.md:137-160` for the same call flow from the REST side).

The REST server (`openbb_platform_api`) and the CLI are two **parallel front-ends** to the same `obb` SDK. They share:
- The credentials store (`~/.openbb_platform/user_settings.json`).
- The system settings store (`~/.openbb_platform/system_settings.json`).
- The provider implementations (`openbb_yfinance`, `openbb_fmp`, etc.).
- The `OBBject` response model.

They do NOT share:
- Process state. Each CLI session has its own in-memory `obbject_registry`, while the REST server doesn't have one at all (it just returns serialized `OBBject`s).
- The argparse-based command interface — the REST side uses FastAPI's generated route dispatch.

This is documented in `HANDOFFS.md:138`: `cli-repl ──► platform-rest-api (in-process via SDK)`.

### 12.2 CLI vs the desktop Tauri app

The desktop app launches `openbb` as a **subprocess inside an environment terminal**. Code at `desktop/src/routes/environments.tsx:867-924`:

```ts
const startCliSession = useCallback(
  async (envName: string) => {
    if (!envName || !installDir) return;
    const { isWindows, isMac } = getPlatformInfo();
    const condaDir = `${installDir}/conda`;
    const workDir = currentWorkingDir || installDir;
    try {
      if (isWindows) {
        await invoke("execute_in_environment", {
          command: `start cmd.exe /k "cd /d "${workDir}" && "${condaDir}\\Scripts\\activate.bat" "${envName}" && openbb && exit"`,
          environment: "base",
          directory: installDir,
        });
      } else if (isMac) {
        const appleScript = `tell application "iTerm" … write text "cd ${workDir} && source ${condaDir}/bin/activate ${envName} && openbb && exit" …`;
        await invoke("execute_in_environment", { command: `osascript -e "${appleScript}"`, ... });
      } else {
        await invoke("execute_in_environment", {
          command: `x-terminal-emulator -e "cd ${workDir} && source ${condaDir}/bin/activate ${envName} && exec openbb && exit"`,
          ...
        });
      }
    } catch (err) { console.error("Failed to start CLI Session:", err); }
  },
  [installDir, getPlatformInfo, currentWorkingDir],
);
```

The detection uses `hasCliSupport(envName)` which checks if the environment has `openbb-cli` in its package list (`environments.tsx:1265-1295`). The "OpenBB CLI" action button is in the environment's Applications modal (`components/EnvironmentActions.tsx:94-101`).

Rust side: `execute_in_environment_impl` in `desktop/src-tauri/src/tauri_handlers/environments.rs:3044-3232` writes a bash/batch script that activates conda + sources conda activate, then `exec`s the command. The `cmd.exe /k` (Windows) and iTerm/Terminal AppleScript (mac) variants open a **new terminal window**, so the user sees the CLI's interactive prompt.

So: the desktop app **does not** embed a terminal view. It just spawns a native terminal window with `openbb` running inside the right conda env. The CLI then runs as a normal interactive REPL within that terminal.

### 12.3 CLI vs Jupyter notebooks

The `obb` object referenced everywhere in the CLI (`from openbb import obb`) is **the same** singleton-ish object that gets imported in any Python interpreter, including Jupyter. So if a user does:

```python
from openbb import obb
result = obb.equity.price.historical(symbol="AAPL")
```

in Jupyter, they hit the same `CommandRunner.run` pipeline that the CLI's `equity/price/historical --symbol AAPL` hits. The differences:
- The CLI tracks history in `obbject_registry`.
- The CLI emits Rich-styled output and interactive PyWry tables.
- The CLI's argparse layer rewrites datetime-keyword variables (`$1MONTHAGO`).

Both share `user_settings.json` for credentials and preferences.

### 12.4 Shared files inventory

| File | Owners |
|---|---|
| `~/.openbb_platform/.cli.env` | CLI only (its settings) |
| `~/.openbb_platform/.cli.his` | CLI only (prompt-toolkit history) |
| `~/.openbb_platform/user_settings.json` | Platform SDK; read by CLI as `obb.user`; written by REST server (`api-keys.md`); written by desktop app for API keys |
| `~/.openbb_platform/system_settings.json` | Platform SDK; CLI temporarily mutates `logging_sub_app="cli"` (`utils/utils.py:11-23`); the REST server also mutates this to `"api"` |
| `<user.preferences.export_directory>/routines/*.openbb` | CLI (record/stop) + Hub sync |
| `<user.preferences.user_styles_directory>/*.richstyle.json` | CLI styles (user-supplied) |
| `<repo>/integration_test_output/*.txt` | CLI test mode |

---

## 13. TS port translation

### 13.1 What would a pure TS rewrite look like?

The CLI has three layers, each with a clean TS equivalent:

| Python layer | TS equivalent | Effort |
|---|---|---|
| `argparse` parsing | `commander` or `yargs` (preferred: `commander` for declarative subcommand trees) | Mechanical, but every menu generates parsers dynamically — see "Hard parts" below |
| `prompt-toolkit` (autocompletion, history, bottom toolbar) | `inquirer` (high-level), `enquirer`, or directly `readline` + a custom completer; for bottom toolbar, `blessed` or `terminal-kit` | Medium — `NestedCompleter` semantics are non-trivial |
| `rich` (Panel, Table, Theme, styled text) | `chalk` (styling), `cli-table3` (tables), `boxen` (panels) | Mechanical |
| `pydantic` Settings | `zod` for validation, manual env-file persistence with `dotenv` (npm) | Easy |
| OBBject registry | Plain JS array + map; identical semantics | Trivial |
| `.openbb` script parser | Direct port of `script_parser.py` regex logic | Medium (~400 lines) |
| `PlatformControllerFactory` introspecting `obb` | **Doesn't exist in TS** — the SDK isn't ported | **Blocker** |
| Plotly via PyWry charts | Would need a separate web-based renderer (Electron window? browser tab?) | High |

**Hard parts:**

1. **There is no TS SDK.** The CLI's reason for existing is to call `obb.equity.price.historical(...)` etc. A TS port would have to either:
   - Call the REST server (`openbb-api`) for every command — turning the CLI into a thin REST client.
   - Spawn Python in a subprocess to do each call — same trade-off as the REST server's Strategy B port.
   - Wait for an upstream TS-native port of `openbb-platform` and all its provider extensions — currently nonexistent and unrealistic.

2. **Dynamic introspection of the SDK** (`ArgparseClassProcessor` walking method signatures via `inspect.signature`, `get_type_hints`, etc.) has no parallel in JS — TypeScript types are erased at runtime. The equivalent would be to consume a **schema** (e.g., the platform's OpenAPI spec or `obb.reference["paths"]`) instead of live reflection. This is in fact what the platform already provides via `/api/v1/openapi.json` (see `platform-rest-api.md`).

3. **PyWry-backed interactive tables and charts.** A TS port could shell out to a webview (Electron-style) or open a browser tab, or just emit ASCII tables and write Plotly HTML/PNG via the platform's own renderer.

### 13.2 Realistic port options ranked

**Option A: REST-shim CLI (recommended).** A TS REPL that, on startup, hits `GET /openapi.json` against a running `openbb-api` server (managed by the desktop app or user-spawned). The OpenAPI document gives every command's path, parameters with types, descriptions, and provider variants. The CLI builds its menu tree from that JSON. When a command executes, it POSTs to `/api/v1/{path}?provider=…` with the parsed args, displays the JSON response. Charts come back as `OpenBBFigure.to_html()` or `OpenBBFigure.to_image()` from the server (existing endpoints support this). **Pros:** decoupled from Python install, can run anywhere with network access, doesn't require a TS port of the SDK. **Cons:** dependent on a running REST server (one-extra-process), latency on every command.

**Option B: Subprocess wrapper.** A thin TS REPL that spawns `python -m openbb_cli` and pipes stdin/stdout through. The TS layer adds nothing except a possibly-richer UI shell. **Pros:** trivial to implement, identical fidelity. **Cons:** no behavioral changes possible, awkward to render interactive completion in the wrapper, defeats the purpose of porting.

**Option C: Skip the CLI entirely.** The desktop app already exposes nearly every CLI feature (extension management, API key setup, environment activation, chart rendering) through its UI. The CLI is mostly a power-user tool for scripted automation (`.openbb` routines, `record`/`stop`). The TS port could:
   - Add a "Run Routine" button to the desktop UI that imports `.openbb` files and replays them by calling the REST API.
   - Add a "Recording" mode that captures REST API calls and serializes them to a `.openbb`-like format.
   - Let users continue using the Python `openbb` command if they want the bash-style REPL — the desktop app already launches it via `startCliSession`. **Recommended for v1 port.**

**Option D: Full rewrite (not recommended).** Without a TS-native SDK, this is months of work for a feature that has a tiny fraction of the desktop app's user base.

### 13.3 What a TS REPL skeleton would look like (Option A sketch)

```ts
import { program } from "commander";
import inquirer from "inquirer";
import chalk from "chalk";

interface CliSession {
  obbjectRegistry: OBBject[];
  settings: Settings;
  currentPath: string[];        // ["equity", "price"]
  openapi: OpenAPIDoc;          // fetched at boot
}

async function bootstrap() {
  await ensureSettingsDir();
  const settings = loadSettings();
  const openapi = await fetch(`${settings.BASE_URL}/openapi.json`).then(r => r.json());
  return { obbjectRegistry: [], settings, currentPath: [], openapi };
}

async function repl(session: CliSession) {
  while (true) {
    const prompt = `${flair(session)} /${session.currentPath.join("/")} $ `;
    const { input } = await inquirer.prompt([{
      type: "input", name: "input", message: prompt,
      // custom completer using openapi paths
    }]);
    await switchCommand(session, input);
  }
}
```

The OBBject registry, settings persistence, and routine script parser all port cleanly. The execution layer becomes `await fetch(buildUrl(path, args), { method: "POST" }).then(r => r.json())`.

---

## 14. Cross-feature dependencies

### State / files shared

- **`~/.openbb_platform/user_settings.json`** — read at boot via `obb.user`. Carries credentials, preferences, defaults, export/styles paths. **Shared with**: `feature-api-keys.md`, `feature-platform-rest-api.md`, all platform SDK calls. Any change to credentials via the desktop API-keys panel becomes visible to a freshly-launched CLI (and vice versa, if the CLI ever wrote to it — it currently only reads).
- **`~/.openbb_platform/system_settings.json`** — temporarily mutated by `utils/utils.py:11-34` to flip `logging_sub_app` to `"cli"`. **Shared with**: REST server (which also mutates the same field). Race: if a CLI and REST server start at the same time, the file write is non-atomic.
- **`~/.openbb_platform/.cli.env`** — CLI-exclusive but lives in the platform's shared directory. **Risk:** uninstaller (`feature-uninstall.md`) presumably wipes `~/.openbb_platform/` — and would remove the CLI's settings too.
- **`<user.preferences.export_directory>/routines/`** — `.openbb` routine files. **Shared with**: Hub sync (populates `hub/default/` and `hub/personal/`); desktop app could expose a routines browser.

### Processes / launching

- **The desktop app spawns `openbb` as a subprocess** via `invoke("execute_in_environment", ...)` (`environments.tsx:867-924`). **Depends on**: `feature-environments.md` (conda env activation), `ipc-bridge.md` (the `execute_in_environment` IPC). The CLI subprocess is **not** managed/monitored by the desktop app — once launched, it runs in a separate terminal window.
- **The CLI calls the SDK in-process** — no IPC with the REST server, no shared HTTP socket.

### Conceptual overlap

- **`feature-platform-rest-api.md`** and the CLI are sibling front-ends to the same `CommandRunner`. Any behavioral difference (e.g., auth handling, default merging) between the two is a discoverable bug surface. The CLI bypasses auth entirely because it's in-process.
- **`feature-jupyter.md`** is the third sibling — Jupyter sessions import `obb` directly. The desktop's Applications modal exposes all three (Python session, IPython, CLI session) for each environment.
- **`feature-environments.md`** owns the conda activation logic; the CLI only runs inside an activated env and inherits its packages (`openbb-cli` plus the platform plus all installed provider extensions).

### Bugs / risks

> ⚠️ BUG: `cli.py:25-32` wraps `main()` in `change_logging_sub_app`/`reset_logging_sub_app` only when run as `__main__`. The Poetry console script entry calls `openbb_cli.cli:main` directly, so the `logging_sub_app` is never set to `"cli"` for the actual `openbb` command users invoke. Platform logs from CLI sessions will be tagged with whatever was previously in `system_settings.json` (likely `""` or `"api"`).

> ⚠️ BUG: Two parallel CLI sessions in two terminals will both fight over `system_settings.json["logging_sub_app"]` if either of them did use the wrapper, and over `.cli.env` (every `set_item` is a `set_key` write). No file-locking.

> ⚠️ BUG: `controllers/base_controller.py:34` has `controllers: dict[str, Any] = {}` as a **module-level** singleton. Once entered, sub-controllers are cached forever in this dict (`save_class`/`load_class`, lines 136-149). After `reset` (which deletes all `openbb_cli` modules from `sys.modules` and re-imports), the dict is empty again — but a `reset` is the *only* way to clear it. Long-running sessions accumulate stale controller instances whose `queue` and other state may be inconsistent with the current SDK state if user credentials were rotated mid-session.

> ⚠️ BUG: `PlatformControllerFactory` runs `ArgparseClassProcessor` once in `__init__` (`platform_controller_factory.py:15-17`), and the `PlatformController.__init__` runs it again with the same target on instantiation (`base_platform_controller.py:52-59`). Wasted work on every menu enter; the parser objects are deep-copied per command (`argparse_translator.py:152-154`).

> ⚠️ BUG: `CHOICES_GENERATION = True` on dynamically-generated controllers triggers a `unittest.mock.patch` + `inspect`-based traversal in `controllers/choices.py` that monkey-patches `parse_known_args_and_warn` to extract the choice tree. This pattern is fragile — any change to the parser signature breaks the introspector.

> ⚠️ BUG: `cli_controller.py:53-57` builds `PLATFORM_ROUTERS` at module top-level (i.e., during `from openbb_cli.controllers.cli_controller import launch`). That means `from openbb import obb` is forced at import time — boot cost is paid even if the user typed `openbb --help`.

> ⚠️ Race / edge: `_link_obbject_to_data_processing_commands` (`base_platform_controller.py:73-89`) sets `action.choices = […]` on every call, but the parser is deep-copied on use (`argparse_translator.py:152-154`), so the choice mutation on the *cached* parser doesn't survive — the linking has to happen via a wrapper somewhere. In practice, the link is set up at controller construction and again after each successful command, so it works, but the design is brittle.

### Cross-reference graph entry

```
cli-repl ──► platform-rest-api (sibling front-end; same CommandRunner in-process)
cli-repl ──► environments (spawned inside activated conda env)
cli-repl ──► api-keys (reads user_settings.json for credentials)
cli-repl ──► ipc-bridge (only at launch via execute_in_environment)
cli-repl ──► uninstall (loses .cli.env, .cli.his if ~/.openbb_platform is wiped)
desktop-app ──► cli-repl (launches via startCliSession in environments.tsx:867)
hub-sync ──► cli-repl.routines (populates <export_dir>/routines/hub/{default,personal})
```

The CLI is **leaf-ish** in the dependency graph: it consumes lots (the SDK, the user settings, the conda env) but nothing else in the system depends on the CLI's runtime state.
