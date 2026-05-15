# Deep-Dive: Python `openbb-api` REST Server

> Raw findings from Wave 1 agent. Source of truth for `20-features/feature-platform-rest-api.md`
> and `30-port/port-strategy.md`. The "Strategy A vs B" comparison at the end drives the
> entire port roadmap.
> Generated 2026-05-15.

This is what the desktop app's "Backend services" actually spawn when it runs `openbb-api`. Everything below is the Python server side; the TS port either reimplements or proxies it.

---

## 1. Boot sequence — `openbb-api` to first request

### 1a. Console script registration

The `openbb-api` command is **NOT** in `openbb-core`. It lives in a separate package:

- `/home/user/OpenBBPort/openbb_platform/extensions/platform_api/pyproject.toml:14`
  ```toml
  [tool.poetry.scripts]
  openbb-api = "openbb_platform_api.main:main"
  ```
- `openbb-core`'s only console script is `openbb-build` (`core/pyproject.toml:31`), used to regenerate the static Python SDK; it does NOT start a server.

So `openbb-api` -> `openbb_platform_api.main:main` -> `launch_api(**kwargs)` -> `uvicorn.run("openbb_platform_api.main:app", host=…, port=…, **_kwargs)` (`extensions/platform_api/openbb_platform_api/main.py:288-336`).

### 1b. Module import side effects (happen before `main()`)

Importing `openbb_platform_api.main` executes a large amount of work at module-top level:

1. `Env()` initialised at `main.py:39` — `core/openbb_core/env.py:11-19` reads `~/.openbb_platform/.env` via `dotenv.load_dotenv(...)` and copies `os.environ`. All env-var properties (`API_AUTH`, `API_USERNAME`, `API_PASSWORD`, `API_AUTH_EXTENSION`, `AUTO_BUILD`, `DEBUG_MODE`, `DEV_MODE`, `ALLOW_MUTABLE_EXTENSIONS`, `ALLOW_ON_COMMAND_OUTPUT`) are read on access with `OPENBB_` prefix.
2. `from openbb_core.api.rest_api import app` (`main.py:14`). That module is the FastAPI factory and it has its own module-level execution (see 1c).
3. `parse_args()` (`utils/api.py:317`) — converts `sys.argv` flags into a kwargs dict: `--app`, `--name`, `--factory`, `--editable`, `--build`, `--no-build`, `--exclude`, `--widgets-json`, `--apps-json`, `--agents-json`, plus passthrough uvicorn flags (`--host`, `--port`, `--ssl-keyfile`, `--ssl-certfile`, `--ssl-keyfile-password`, `--ssl-version`, `--ssl-cert-reqs`, `--ssl-ca-certs`, `--ssl-ciphers`, `--use-colors`/`--no-use-colors`).
4. `uvicorn_settings = SystemService().system_settings.python_settings.model_dump().get("uvicorn", {})` (`main.py:66-73`) — pulls a dict of uvicorn defaults from `~/.openbb_platform/system_settings.json` -> `python_settings.uvicorn`. CLI flags win (`if key not in kwargs and key != "app"`).
5. `widget_exclude_filter` is augmented from `~/.openbb_platform/widget_settings.json` (`main.py:75-84`).
6. `check_for_platform_extensions(app, ...)` (`main.py:87-103`) auto-excludes data-processing routes (`econometrics`, `quantitative`, `technical`) from `/widgets.json`.
7. `openapi = app.openapi()` (`main.py:107`) — eagerly builds the OpenAPI schema once at boot.
8. `widgets_json = get_widgets_json(...)` (`main.py:109-111`) — generates the widget catalogue used by `/widgets.json` (and later by `/apps.json`). Source: `utils/widgets.py` + `utils/openapi.py`.
9. The launcher mounts a few extra non-OpenBB-router endpoints onto `app`: `GET /` landing HTML (`main.py:124-130`), `GET /widgets.json` (`main.py:139-168`), `GET /apps.json` (`main.py:174-249`), optionally `GET /agents.json` (`main.py:252-285`).

### 1c. `openbb_core.api.rest_api` module evaluation

This is where the actual FastAPI app is built (`core/openbb_core/api/rest_api.py:1-89`):

1. `system = SystemService().system_settings` (line 18) — singleton, reads `~/.openbb_platform/system_settings.json` via `SystemService._read_from_file()` (`service/system_service.py:50-76`). Allowed keys are whitelisted to `SYSTEM_SETTINGS_ALLOWED_FIELD_SET` (`system_service.py:16-26`), including `api_settings`, `python_settings`, `debug_mode`, etc.
2. `FastAPI(...)` is constructed (line 45) with title/description/version/contact/license/servers from `system.api_settings` (`model/api_settings.py:25-49`). The API prefix is computed: `prefix = "/api/v" + version` (`api_settings.py:46-49`), so default is `/api/v1`.
3. `app.add_middleware(CORSMiddleware, ...)` with `allow_origins/methods/headers` from `api_settings.cors` — defaults to `["*"]` (`api_settings.py:6-13`).
4. **Route mounting** (line 74-86):
   ```python
   AppLoader.add_routers(app, routers=(
       [AuthService().router, router_system, router_coverage, router_commands]
       if Env().DEV_MODE
       else (
           [router_commands, router_coverage]
           if router_commands.routes
           else [router_commands]
       )
   ), prefix=system.api_settings.prefix)
   ```
   So **the auth/user and system routers are only mounted when `OPENBB_DEV_MODE=true`**. In a normal install, only `router_commands` and `router_coverage` are registered under `/api/v1`. This is critical for the TS port: the `/user/me` and `/system` endpoints are NOT exposed by default.
5. `AppLoader.add_openapi_tags(app)` (`api/app_loader.py:23-33`) — calls `RouterLoader.from_extensions()` and synthesises one OpenAPI tag per top-level extension router (`equity`, `news`, `crypto`, etc.) with the description from each router's docstring/`Router(prefix=..., description=...)`.
6. `AppLoader.add_exception_handlers(app)` (lines 36-43) — wires `Exception`, `ValidationError`, `ResponseValidationError`, `OpenBBError`, `EmptyDataError`, `UnauthorizedError` to handlers in `api/exception_handlers.py`.

### 1d. Router auto-loading from extensions

`api/router/commands.py` is imported as part of route mounting:

1. Bottom of file (lines 354-356):
   ```python
   system_settings = SystemService(logging_sub_app="api").system_settings
   command_runner_instance = CommandRunner(system_settings=system_settings)
   add_command_map(command_runner=command_runner_instance, api_router=router)
   ```
2. `add_command_map` (lines 345-351) calls `RouterLoader.from_extensions()` (`app/router.py:518-537`):
   ```python
   for name, entry in ExtensionLoader().core_objects.items():
       router.include_router(router=entry, prefix=f"/{name}")
   ```
   This iterates Python entry-points group `openbb_core_extension` (`app/extension_loader.py:17-31`) and mounts each `Router` returned by every installed extension package (`equity`, `news`, etc.) under its name as the URL prefix.
3. Each extension's `router.command(model="EquityHistorical")` call (e.g. `extensions/equity/openbb_equity/price/price_router.py:46-60`) was already executed at extension import time. The `command` decorator (`app/router.py:87-171`) uses `SignatureInspector.complete()` to inject FastAPI dependencies (`provider_choices`, `standard_params`, `extra_params`) generated from `ProviderInterface()` (`app/router.py:230-296`).
4. Back in `commands.py:349-350`, every route gets its `endpoint` swapped for an `async wrapper` produced by `build_api_wrapper(command_runner, route)`. This wrapper is what FastAPI actually calls — the original handler from the extension is invisible to FastAPI.

### 1e. Provider registry build (lazy, but typically eager via `ProviderInterface`)

- `app/extension_loader.py:180-203`: `ExtensionLoader.provider_objects` reads entry-points group `openbb_provider_extension`. Each module's loaded value must be a `Provider` instance (e.g. `openbb_yfinance.provider:yfinance_provider`).
- `provider/registry.py:34-55`: `RegistryLoader.from_extensions()` is `@lru_cache`d; iterates the loaded provider modules and calls `registry.include_provider(provider)`. Result: `registry.providers: dict[str, Provider]`, keyed by lowercase provider name.
- `provider/registry_map.py:23-106`: `RegistryMap.__init__` walks `registry.providers[*].fetcher_dict` to build:
  - `standard_extra: MapType` — per-model mapping `{model_name: {"openbb": {QueryParams, Data}, "<provider>": {QueryParams, Data}}}` (used by `ProviderInterface` to synthesise per-route param dataclasses).
  - `original_models` — references to the actual fetcher input/output classes (used to compute `return_annotations` -> `OBBject[...]` schema).
  - `available_providers`, `credentials`.
- `app/provider_interface.py:98-118`: `ProviderInterface.__init__` (a singleton via `SingletonMeta`) consumes the `RegistryMap` and generates:
  - `model_providers[model]` — a dataclass with `provider: Literal["fmp", "yfinance", ...]`. (`provider_interface.py:543-574`).
  - `params[model]["standard"]` / `params[model]["extra"]` — dataclasses used as FastAPI dependencies (`@dataclass` field defaults become `fastapi.Query` instances).
  - `return_annotations[model]` — concrete `OBBject[Annotated[Union[YFinanceData|FMPData|...], Discriminator]]` types so the OpenAPI schema enumerates every provider's data shape.

### 1f. uvicorn launch

`launch_api()` (`main.py:288-331`):
- `host = _kwargs.pop("host", os.getenv("OPENBB_API_HOST", "127.0.0.1"))`.
- `port = _kwargs.pop("port", os.getenv("OPENBB_API_PORT", "6900"))`. Validates >1024 else falls back to 6900.
- `check_port(host, port)` (`utils/api.py:82-93`) probes the port; if busy, increments until free.

> ⚠️ Port-collision behavior: `check_port` will silently increment past the requested port. The desktop app's `backends.json` records the *requested* port; the actually-bound port will only show up in the URL extracted from logs. The TS port should preserve this (or surface it as a warning).

- Final call: `uvicorn.run("openbb_platform_api.main:app", host=host, port=port, **_kwargs)` — uvicorn imports the module string, so all the boot-time work above runs in the worker process. `--reload` defaults to True only when launched directly via `python -m openbb_core.api.rest_api` (`rest_api.py:97-105`); through `openbb-api` it follows whatever's in `python_settings.uvicorn`.

---

## 2. Request lifecycle — `GET /api/v1/equity/price/historical?symbol=AAPL&provider=yfinance`

### 2a. HTTP arrival -> FastAPI route resolution

- uvicorn ASGI -> Starlette/FastAPI dispatcher -> matches the route registered for `/api/v1/equity/price/historical`. The endpoint object on this APIRoute is the `wrapper` returned by `build_api_wrapper` (`api/router/commands.py:212-342`); the original `historical()` from `price_router.py:53` was replaced.
- The route's `__signature__` was rewritten by `build_new_signature` (`commands.py:53-149`). FastAPI inspects this signature to know what query/path/dependency parameters to extract:
  - `provider_choices: ProviderChoices` (a generated dataclass) -> dependency-injected via `provider_interface.model_providers["EquityHistorical"]`. Provides the `provider` query param as `Literal["fmp","intrinio","polygon","tiingo","yfinance",...]`.
  - `standard_params: StandardParams` -> generated from `EquityHistoricalQueryParams` (`provider/standard_models/equity_historical.py`): `symbol: str` (required), `start_date: date | None`, `end_date: date | None`. Each becomes a `Query(...)` parameter.
  - `extra_params: ExtraParams` -> per-provider extras, e.g. `interval`, `extended_hours`, `include_actions`, `adjustment` from `YFinanceEquityHistoricalQueryParams` (`providers/yfinance/openbb_yfinance/models/equity_historical.py:23-87`).
  - The `cc: CommandContext` and `kwargs` parameters are stripped (`commands.py:62-72`).
  - If `OPENBB_API_AUTH=true`, an extra `__authenticated_user_settings: Annotated[UserSettings, Depends(AuthService().user_settings_hook)]` is inserted (`commands.py:132-144`).

### 2b. Middleware & auth

- `CORSMiddleware` runs first (added at `rest_api.py:68-73`). With defaults `["*"]` it short-circuits preflights.
- Auth is enforced **only via the dependency** injected at `commands.py:132-144`. There is no global auth middleware. `AuthService().user_settings_hook` defaults to `get_user_settings` from `api/router/user.py:6,11`, which depends on `authenticate_user` from `api/auth/user.py:15-43`. With `OPENBB_API_AUTH=false` (default), `security = lambda: None` (`auth/user.py:12`) so the dependency returns immediately and `kwargs["__authenticated_user_settings"]` is the default `UserSettings()` (which itself reloads from `~/.openbb_platform/user_settings.json`, see `model/user_settings.py:22-41`).

### 2c. Wrapper entry & param flattening

`wrapper(**kwargs)` (`commands.py:238-342`):
1. `user_settings = UserSettings.model_validate(kwargs.pop("__authenticated_user_settings", UserService.read_from_file()))` (lines 241-246).
2. `defaults = user_settings.defaults.commands.get("equity.price.historical", {})` (lines 247-252) — applies user-saved defaults to standard/extra params.
3. `kwargs["standard_params"]` and `kwargs["extra_params"]` are popped, merged with defaults, and reinserted as plain dicts (lines 253-278).
4. Custom-header parameters (added at lines 119-130) and dependency injections (lines 290-302) are flattened into `kwargs["kwargs"]`.
5. `execute = partial(command_runner.run, path, user_settings)` then `output = await execute(*args, **kwargs)` (lines 304-306).

### 2d. CommandRunner -> ExecutionContext

`CommandRunner.run(route, user_settings, *args, **kwargs)` (`app/command_runner.py:690-710`) builds an `ExecutionContext` and forwards to `StaticCommandRunner.run`.

`StaticCommandRunner.run` (`command_runner.py:430-535`):
1. `func = command_map.get_command("/equity/price/historical")` — that's the **original** `historical()` from the extension (`CommandMap` was built in `RouterLoader.from_extensions` and stored in `CommandMap._map`, see `app/router.py:423-430`). So the wrapper was the FastAPI-facing layer; the actual user command code is invoked here.
2. Calls `_execute_func(...)` (lines 308-427).

`_execute_func`:
1. `with catch_warnings(record=True)` so per-request warnings can be attached to the `OBBject` later (lines 323, 394-410).
2. `kwargs = ParametersBuilder.build(...)` (`command_runner.py:209-236`):
   - `get_polished_func` strips `__authenticated_user_settings` (lines 70-86).
   - `merge_args_and_kwargs` aligns positional/keyword args to the function's polished signature (lines 88-124).
   - `update_command_context` constructs `CommandContext(user_settings, system_settings)` and assigns it to `kwargs["cc"]` if the function has a `cc` parameter (lines 126-144). The historical handler has `cc: CommandContext`, so it's filled.
   - `validate_kwargs` builds an ad-hoc Pydantic model from the function signature and runs `ValidationModel(**kwargs)` to coerce types (lines 180-206).
3. `cls._command(func, kwargs)` (lines 243-258) calls `await maybe_coroutine(func, **kwargs)` -> the user's `async def historical(...)` runs, which simply does `return await OBBject.from_query(Query(**locals()))`.

### 2e. Router function -> Query.execute -> QueryExecutor

`Query.__init__` (`app/query.py:17-34`) caches `cc`, `provider`, `standard_params`, `extra_params`, `name = "EquityHistorical"`, `provider_interface = ProviderInterface()`.

`Query.execute` (`query.py:66-80`):
1. `standard_dict = asdict(self.standard_params)` -> `{"symbol": "AAPL", "start_date": None, "end_date": None}`.
2. `extra_dict = self.filter_extra_params(extra_params, "yfinance")` — drops anything not titled to the chosen provider, warns otherwise (`query.py:36-64`). For yfinance with default extras, gives e.g. `{"interval": "1d", "extended_hours": False, ...}`.
3. `query_executor = self.provider_interface.create_executor()` -> `QueryExecutor(registry)`.
4. `await query_executor.execute(provider_name="yfinance", model_name="EquityHistorical", params={**standard_dict, **extra_dict}, credentials=cc.user_settings.credentials.model_dump(), preferences=cc.user_settings.preferences.model_dump())`.

`QueryExecutor.execute` (`provider/query_executor.py:65-97`):
1. `provider = self.get_provider("yfinance")` (lines 19-26) — the lower-cased lookup against `registry.providers`.
2. `fetcher = self.get_fetcher(provider, "EquityHistorical")` -> `YFinanceEquityHistoricalFetcher` (registered in the yfinance Provider object's `fetcher_dict`).
3. `filter_credentials(credentials, provider, fetcher.require_credentials)` (lines 36-63) — picks only the keys the provider actually declares (`provider.credentials`); raises `OpenBBError("Missing credential 'X'.")` if a required one is absent. yfinance has none, so this returns `{}`.
4. `return await fetcher.fetch_data(params, filtered_credentials, **kwargs)`.

### 2f. Fetcher pipeline

`Fetcher.fetch_data` (`provider/abstract/fetcher.py:73-85`):
```python
query = cls.transform_query(params=params)
data = await maybe_coroutine(cls.extract_data, query=query, credentials=credentials, **kwargs)
return cls.transform_data(query=query, data=data, **kwargs)
```

For `YFinanceEquityHistoricalFetcher` (`providers/yfinance/openbb_yfinance/models/equity_historical.py:108-193`):
1. `transform_query` (lines 116-131) — defaults `start_date` to `today - 1y`, `end_date` to `today`, then `YFinanceEquityHistoricalQueryParams(**params)` runs Pydantic validation (uppercases symbol via the standard model's `field_validator`).
2. `extract_data` (lines 133-167) — calls `yf_download(...)` (a wrapper around `yfinance.download`). Returns a pandas DataFrame. Raises `EmptyDataError()` (`provider/utils/errors.py:6-14`) if empty.
3. `transform_data` (lines 169-193) — drops `capital_gains` if `include_actions=False`, warns for any missing symbol when multiple, then returns `[YFinanceEquityHistoricalData.model_validate(row) for row in df.to_dict("records")]`.

`Fetcher.__init_subclass__` (`fetcher.py:60-71`) — automatically promotes `aextract_data` to `extract_data` if defined; raises if neither is implemented.

### 2g. OBBject construction

Back in `OBBject.from_query` (`app/model/obbject.py:364-383`):
```python
results = await query.execute()        # list[YFinanceEquityHistoricalData]
return cls(results=results)             # OBBject[list[YFinanceEquityHistoricalData]]
```
(If the result is an `AnnotatedResult`, the `metadata` is moved into `extra["results_metadata"]`.)

`StaticCommandRunner._execute_func` then attaches private attrs: `obbject._route = "/equity/price/historical"`, `_standard_params`, `_extra_params` (`command_runner.py:366-376`). Sets `obbject.provider = "yfinance"` (`command_runner.py:251-258`).

After the `with catch_warnings`, recorded warnings are converted via `cast_warning(w)` (`model/abstract/warning.py:15-20`) into `Warning_(category, message)` and appended to `obbject.warnings` (`command_runner.py:394-410`).

`StaticCommandRunner.run` then:
- Adds `obbject.extra["metadata"] = Metadata(arguments=kwargs, duration=ns, route=..., timestamp=...)` if `user_settings.preferences.metadata` is true (lines 458-471).
- Strips dependency-injection callables out of `extra.metadata.arguments` (lines 472-485, 519-533).
- Triggers obbject extension callbacks (`_trigger_command_output_callbacks`, lines 537-639) — these are extensions registered in entry-points group `openbb_obbject_extension` that mutate the OBBject (e.g. add accessors).

### 2h. Response serialization

Back in `wrapper` (`commands.py:308-340`):
- If `output` is an `OBBject` and the path is unmodified by extensions and has no `no_validate` flag: returns the OBBject directly. FastAPI then runs response-model validation against the auto-generated `OBBject[Annotated[Union[<all provider data types>], Discriminator]]` from `provider_interface.return_annotations[model]` (set up at `app/router.py:279-282`). The decorator declared `response_model_exclude_unset=True` (`router.py:112`), so unset fields are dropped.
- If an extension mutated the OBBject (`_extension_modified=True`) or `no_validate=True`: bypasses response_model and emits a raw `JSONResponse(jsonable_encoder(output))`.
- If a `_results_only` extension is active: emits ONLY `output.results`, not the wrapping OBBject.

Final wire format (typical, with `OBBject` validation): a JSON object with keys `id`, `results`, `provider`, `warnings`, `chart`, `extra`. See section 6 for the exact shape.

---

## 3. Coverage / Apps endpoints — what OpenBB Workspace polls

### 3a. `/api/v1/coverage/...` (only when DEV_MODE)

`api/router/coverage.py` — three endpoints, all under prefix `/coverage`, all hidden from widgets via `openapi_extra={"widget_config": {"exclude": True}}`:

- `GET /coverage/command_model` (lines 14-89): Returns the per-route, per-provider `QueryParams`/`Data` field schema for every model. Built by walking `command_map.commands_model` -> `provider_interface.map[model]` -> serialised field attributes. Heavily used for codegen / SDK generation, **not** by Workspace day-to-day.
- `GET /coverage/providers` (lines 92-97): `command_map.provider_coverage` — `{ "fmp": ["/equity/price/historical", ...], ... }`.
- `GET /coverage/commands` (lines 100-105): `command_map.command_coverage` — `{ "/equity/price/historical": ["fmp","yfinance",...], ... }`.

These come from `CommandMap` (`app/router.py:377-508`):
- `get_provider_coverage` walks the FastAPI route table; for each route with `openapi_extra["model"]`, it pulls `ProviderInterface().map[model].keys()` minus `"openbb"`.
- `get_command_coverage` is the inverted view.
- `get_commands_model` returns `{route: model_name}`.

These routes only mount when `OPENBB_DEV_MODE=true` (`rest_api.py:78-83`). In a default desktop install they're absent.

### 3b. `/widgets.json` and `/apps.json` (always present, mounted by `openbb-api`)

The "Test successful — X apps found" banner that OpenBB Workspace shows comes from these endpoints, NOT `/coverage`:

- `GET /widgets.json` (`extensions/platform_api/openbb_platform_api/main.py:139-168`): returns the dict generated by `get_widgets_json(...)` at boot. It walks the OpenAPI spec, converts each path into a "widget" with parameter metadata (`utils/widgets.py`, `utils/openapi.py`). Workspace uses this catalogue to render its widget palette.
- `GET /apps.json` (`main.py:174-249`): merges the bundled `default_apps.json` with `~/OpenBBUserData/workspace_apps.json` and any apps contributed by router extensions. Filters to only apps whose layout references known widget IDs.
- `GET /agents.json` (`main.py:252-285`): optionally serves an agents config when `--agents-json PATH` is passed.

Custom response header on all three: `X-Backend-Type: OpenBB Platform` (`main.py:69`).

---

## 4. Auth model

### 4a. Default (`OPENBB_API_AUTH=false`)

- `api/auth/user.py:12`: `security = HTTPBasic() if Env().API_AUTH else lambda: None`.
- `commands.py:132-144`: the `__authenticated_user_settings` dependency is **not** added to the route signature.
- `coverage.py` and `system.py` routers depend on `AuthService().auth_hook` which is `authenticate_user`, but with `security = lambda: None`, `credentials` is None and the function returns immediately (`auth/user.py:18-19`).
- `UserSettings()` re-reads `~/.openbb_platform/user_settings.json` on every request (`model/user_settings.py:22-41`) — fine for local desktop, awful for multi-tenant.

### 4b. Built-in HTTP Basic (`OPENBB_API_AUTH=true`, no extension)

- `auth/user.py:15-43`: `authenticate_user` checks `credentials.username/password` against `Env().API_USERNAME / API_PASSWORD` (env vars `OPENBB_API_USERNAME` / `OPENBB_API_PASSWORD`) using `secrets.compare_digest`. Raises `HTTPException(401, headers={"WWW-Authenticate": "Basic"})` on mismatch.
- `get_user_settings` (`auth/user.py:51-56`) depends on `authenticate_user` and then loads `UserSettings` from disk via `UserService`.
- `commands.py:132-144` injects this as a hidden dependency on every command route, so every endpoint becomes 401-protected.

### 4c. Custom auth extension (`OPENBB_API_AUTH_EXTENSION=<name>`)

- `service/auth_service.py:31-76`: looks up an entry-point in `openbb_core_extension` with the given name, expects it to expose `router`, `auth_hook`, and `user_settings_hook`. Replaces all three. This is how OpenBB ships JWT/OAuth (e.g. an `openbb-platform-api-jwt` extension) without modifying core. No bearer/JWT logic is in core — the hook is what an extension implements.
- `dependency/coverage.py:11-22` and `system.py` use `AuthService().auth_hook` so an extension also gates the diagnostic endpoints.

### 4d. Credential storage

- API keys for **upstream providers** (FMP, Polygon, Intrinio…) live in `~/.openbb_platform/user_settings.json` under `credentials`. `Credentials` is a Pydantic model whose fields are dynamically declared by each provider's `Provider(credentials=[...])`.
- Read each request via `cc.user_settings.credentials.model_dump()` (`app/query.py:78`), filtered by `QueryExecutor.filter_credentials` (`provider/query_executor.py:36-63`) — only keys the provider declared are forwarded; missing required keys raise `OpenBBError`.

---

## 5. OpenAPI generation

- Generated by FastAPI's normal `app.openapi()` (called eagerly in `main.py:107`) plus a few customisations:
  - Tags: `AppLoader.add_openapi_tags` (`api/app_loader.py:23-33`) sets `app.openapi_tags` from `RouterLoader.from_extensions().routers` keyed by extension name (one per top-level subrouter, e.g. `equity`, `news`).
  - Per-route `openapi_extra`: `app/router.py:99-167` stores `model`, `examples`, `widget_config`, `mcp_config`, `no_validate`, plus standard `responses` for 204/400/404/500/502 keyed to `OpenBBErrorResponse(detail, error_kind)`.
  - Response models are the **provider-discriminated union** generated by `provider_interface._get_annotated_union` (`provider_interface.py:678-697`) — the OpenAPI schema therefore lists every provider's data model for each endpoint.
- The `/api/v1` prefix is computed (`api_settings.py:46-49`) and applied at `AppLoader.add_routers(prefix=...)` (`app_loader.py:16-20`), so all paths in the OpenAPI doc are `/api/v1/{extension}/{...}`.
- Consumers:
  - **OpenBB Workspace** consumes `/widgets.json` (built from the OpenAPI by `get_widgets_json` at boot) — Workspace doesn't read `/openapi.json` directly for routing; it gets the curated widget catalogue.
  - **CLI/SDK codegen** (`openbb-build`) uses `RegistryMap` directly, not OpenAPI.
  - **Third-party MCP/agents** can use either the OpenAPI doc or `/widgets.json`.

---

## 6. OBBject on the wire

The default success body for any `OBBject`-returning command, after FastAPI's `response_model_exclude_unset=True` strip:

```json
{
  "id": "01963f7a-7f8c-7c84-bff2-...",
  "results": [
    { "date": "2024-01-02", "open": 187.15, "high": 188.44, "low": 183.89,
      "close": 185.64, "volume": 82488700, "vwap": null,
      "split_ratio": null, "dividend": null }
  ],
  "provider": "yfinance",
  "warnings": [
    { "category": "OpenBBWarning", "message": "Parameter 'foo' not found." }
  ],
  "chart": null,
  "extra": {
    "metadata": {
      "arguments": { "standard_params": {...}, "extra_params": {...}, "provider_choices": {"provider": "yfinance"} },
      "duration": 482103120,
      "route": "/equity/price/historical",
      "timestamp": "2026-05-15T..."
    }
  }
}
```

Notes:
- `id` comes from `Tagged.id = Field(default_factory=uuid7str)` (`model/abstract/tagged.py:7-10`).
- `results` is **always a list of dicts (or a single dict), never a DataFrame**. The Fetcher already converted via `data.to_dict("records")` and instantiated Pydantic models (`yfinance/.../equity_historical.py:190-193`). Pandas-conversion methods (`to_dataframe`, `to_polars`, `to_dict`, `to_llm`) live on the **client-side** OBBject, never run server-side.
- `warnings` -> serialisation of `Warning_(category, message)` (`model/abstract/warning.py:8-12`), populated by `_execute_func` from `catch_warnings` (`command_runner.py:394-401`).
- `chart`: `Chart(content, format, fig)` (`model/charts/chart.py`). The `fig` field has `json_schema_extra={"exclude_from_api": True}` and `validate_output` (`commands.py:152-209`) deletes any field marked this way before serialisation. So over the wire `chart.content` (raw plotly/matplotlib JSON) and `chart.format` are sent, never the live figure object.
- `extra.metadata`: only present when `user_settings.preferences.metadata=true`. After stripping callables and unencodable values (`command_runner.py:519-533`).
- Empty result: when a Fetcher raises `EmptyDataError`, the response is a bare `204 No Content` with no body (`exception_handlers.py:122-125`).

If the `no_validate=True` path is taken (an extension on this command output mutated the OBBject, or it was decorated `@router.command(no_validate=True)`), the body is the raw `jsonable_encoder(output)` with `output.results` already stringified by `model_dump(exclude_unset=True, exclude_none=True)` (`commands.py:322-331`).

---

## 7. Error responses

All wired in `AppLoader.add_exception_handlers` (`app_loader.py:36-43`) -> `api/exception_handlers.py`:

| Exception | HTTP | Body | Source |
|---|---|---|---|
| `EmptyDataError` | **204** | _empty_ | `exception_handlers.py:122-125` |
| `OpenBBError` | **400** | `{"detail": str(error.original)}` | `exception_handlers.py:113-120` |
| `ValidationError` (Pydantic) | **422** | `{"detail": [{"type":..., "loc":["query","symbol"], "msg":..., "input":...}]}` | `exception_handlers.py:64-111` |
| `ResponseValidationError` (FastAPI) | **422** | same shape, location prefixed with `query` | `exception_handlers.py:74-86` |
| `UnauthorizedError` (provider 401/403 upstream) | **502** | `{"detail": str(error.original)}` | `exception_handlers.py:127-134` |
| Bare `ValueError` from inside handler | **422** | `{"detail": error.args}` | `exception_handlers.py:42-47` |
| Anything else | **500** | `{"detail": "Unexpected Error -> ClassName -> message"}` | `exception_handlers.py:57-61` |

If `OPENBB_DEBUG_MODE=true` (`Env().DEBUG_MODE`, `env.py:47-49`), `_handle` re-raises instead of returning JSON (`exception_handlers.py:25-27`) — uvicorn then logs the full traceback. The route decorator also pre-declares OpenAPI `responses` for 204/400/404/500/502 with `OpenBBErrorResponse(detail, error_kind)` (`app/router.py:137-156`), although `error_kind` is never actually populated by the handlers above — it's documented but unused.

`HTTPException(401)` from `authenticate_user` (`auth/user.py:39-43`) bypasses these handlers and is processed by FastAPI's built-in handler — so 401 bodies look like `{"detail": "Incorrect email or password"}` with `WWW-Authenticate: Basic` header.

---

## 8. Settings & env vars

### 8a. Environment variables (read in `core/openbb_core/env.py`)

All read from `os.environ` after `dotenv.load_dotenv("~/.openbb_platform/.env")`:

| Var | Default | Effect |
|---|---|---|
| `OPENBB_API_AUTH` | `false` | Toggles HTTP Basic / extension auth dependency injection on every command route (`commands.py:132-144`). |
| `OPENBB_API_USERNAME` | None | Basic-auth username (`auth/user.py:20`). |
| `OPENBB_API_PASSWORD` | None | Basic-auth password (`auth/user.py:21`). |
| `OPENBB_API_AUTH_EXTENSION` | None | Replace the auth router/hooks with an extension (`service/auth_service.py:19,31`). |
| `OPENBB_AUTO_BUILD` | `true` | Triggers static SDK rebuild on import — irrelevant for the API server but read by core. |
| `OPENBB_DEBUG_MODE` | `false` | Re-raise instead of catching exceptions in handler (`exception_handlers.py:26`); also surfaces extension-load tracebacks. |
| `OPENBB_DEV_MODE` | `false` | Mounts `auth/user`, `system`, and `coverage` routers (`rest_api.py:78`). |
| `OPENBB_ALLOW_MUTABLE_EXTENSIONS` | `false` | Allows obbject extensions to mutate output (downstream of `_trigger_command_output_callbacks`). |
| `OPENBB_ALLOW_ON_COMMAND_OUTPUT` | `false` | Same family — gates the on-command-output extensions. |

Read in the launcher (`extensions/platform_api/openbb_platform_api/main.py:290-310`) but not via `Env()`:

| Var | Default | Effect |
|---|---|---|
| `OPENBB_API_HOST` | `127.0.0.1` | uvicorn bind host. |
| `OPENBB_API_PORT` | `6900` | uvicorn bind port (auto-incremented if busy via `check_port`). |
| `HOME` / `USERPROFILE` | _required_ | Used to locate `~/.openbb_platform/user_settings.json`, `widget_settings.json`, `OpenBBUserData/workspace_apps.json` (`main.py:40-48`). |

### 8b. Disk-backed settings

- `~/.openbb_platform/system_settings.json` -> `SystemSettings` (`service/system_service.py:50-76`). Drives `api_settings` (CORS, prefix, custom_headers) and `python_settings.uvicorn` (extra uvicorn kwargs).
- `~/.openbb_platform/user_settings.json` -> `UserSettings` (`model/user_settings.py:22-41`). Holds `credentials`, `preferences`, `defaults` per route.
- `~/.openbb_platform/widget_settings.json` -> `{"exclude": [...]}` (`main.py:75-84`).
- `~/OpenBBUserData/workspace_apps.json` -> user-saved Workspace apps (`main.py:112-121`).
- `~/.openbb_platform/.env` -> all `OPENBB_*` vars (loaded by `Env()`, `env.py:18`).

### 8c. CLI flags vs env vs uvicorn settings precedence

`extensions/platform_api/openbb_platform_api/main.py:71-73`:
```python
for key, value in uvicorn_settings.items():
    if key not in kwargs and key != "app" and value is not None:
        kwargs[key] = value
```
Order (highest to lowest): CLI flag (`--port 6901`) -> env var (`OPENBB_API_PORT`) -> `system_settings.python_settings.uvicorn[port]` -> hard-coded default `6900`.

---

## Port strategy comparison

### Strategy A — Reimplement in TS/Node

What you must rebuild:
- **A FastAPI-equivalent HTTP shell** — trivial (Express/Hono). Replicate the path scheme `/api/v1/{extension}/{...}` exactly, plus `/widgets.json`, `/apps.json`, optionally `/coverage/*` and `/user/*`.
- **The `Router` + `command(model="...")` registration system** — also trivial conceptually (decorator -> registry of handlers).
- **`ProviderInterface` -> `RegistryMap` -> per-route param/data dataclass synthesis** (`provider_interface.py:543-697`, ~700 LOC of metaclass-driven Pydantic model generation). In TS you'd build Zod schemas at build time instead of at import time — not 1:1 but workable.
- **Response-model-as-discriminated-union** of every provider's data shape — Zod `discriminatedUnion` works.
- **Every Fetcher** — this is the killer. There are dozens of providers (`providers/yfinance`, `fmp`, `polygon`, `intrinio`, `tiingo`, `alpha_vantage`, `nasdaq`, `sec`, `fred`, `tradingeconomics`, `benzinga`, `biztoc`, `cboe`, `ecb`, `eia`, `federal_reserve`, `finra`, `government_us`, `imf`, `oecd`, `seeking_alpha`, `stockgrid`, `tmx`, `tradier`, `wsj`, ...), each with multiple Fetchers. Each Fetcher is `transform_query` / `extract_data` / `transform_data` against bespoke vendor endpoints (yfinance even calls a Python lib, not a REST API — `yf_download` at `providers/yfinance/.../equity_historical.py:141`). **You cannot port yfinance at all without an HTTP-only re-engineering** because the Python library does cookie/CSRF handshake against the Yahoo site. Same for some SEC/FINRA fetchers that lean on `pandas` heavily.
- **Pandas `to_df` / `to_dict` / `to_llm`** — needs DataFrame-equivalent, but these run client-side, so the *server* doesn't need them; they only matter if the desktop app's TS runtime exposes a SDK to user code.
- **OBBject / Charting** — `Chart.fig` is a Plotly figure; the charting extension is Python-only (`openbb_charting`). Either drop charts or proxy to Python.

Pros: single language, single process, no Python toolchain on user machines, smaller install, faster cold start, native to the TS desktop app.
Cons: enormous and ongoing surface area (every upstream API change must be re-ported), no parity with `openbb-charting`, no parity with `openbb-econometrics`/`-quantitative`/`-technical` (data-processing extensions that are entirely Python/numpy/scipy).

### Strategy B — Keep Python, TS as process manager + UI

What the TS port must do:
1. Bundle Python (or detect a system Python ≥ 3.10) and `pip install openbb-platform-api` (which transitively installs `openbb-core` plus selected provider extensions).
2. Spawn `openbb-api --host 127.0.0.1 --port <free port>`. Capture stdout for the "X apps found" banner and the URL the launcher logs (`main.py:325-330`).
3. Track child process lifecycle, restart on crash, surface logs in the desktop app UI.
4. Forward all data calls from the renderer to `http://127.0.0.1:<port>/api/v1/...` — essentially a thin CORS-friendly fetch proxy. The Python server already sets CORS to `*` by default (`api_settings.py:11-13`).
5. Manage credentials by writing to `~/.openbb_platform/user_settings.json` (the file the Python server already reads).
6. For Workspace integration: just expose the Python server's `/widgets.json` and `/apps.json` to the renderer as-is.

Pros: zero porting risk for the data layer, you get every provider, every charting extension, every data-processing extension for free, and stay drop-in compatible with `OpenBB Workspace` (which already speaks this exact protocol). Auth, env vars, settings storage all work unchanged.
Cons: ships a Python runtime (50-200MB depending on packaging), boot time is slow (singleton `RegistryMap` walks every fetcher at import to build dataclasses; on a full install this is several seconds), debugging crosses a process boundary, security boundary is `127.0.0.1` only (which the launcher enforces — see `main.py:290-297`).

### Mixed (most realistic)
- B for everything provider-shaped (`/api/v1/*`).
- A for the things the TS UI actually owns: settings persistence UI, credential vault UI, theme, layout, `widgets.json` overrides via `--widgets-json`, the agent/MCP layer, anything that's pure UI state.
- A thin TS wrapper around `OBBject` JSON for type-safe consumption in the renderer (codegen from the Python `/openapi.json` at build time, not runtime).

The architecture supports B cleanly because (a) every command flows through the same `wrapper` -> `CommandRunner` -> `Fetcher` pipeline, (b) the response shape is uniform `OBBject` JSON, and (c) settings are file-backed — the TS side can read/write them directly without an IPC channel to Python.

---

## Cross-feature dependencies

- **depends-on** `feature-installation.md` — the Python server can only start after install seeds the `openbb` env via Steps 2 + 3 of the wizard.
- **depends-on** `feature-environments.md` — must be invoked from a conda env that has `openbb-platform-api` installed (the install flow explicitly puts this in the openbb.yaml).
- **depended-on-by** `feature-backend-services.md` — the entire backends UI exists primarily to start/stop this server; default seed services are `openbb-api` and `openbb-mcp`.
- **depended-on-by** `feature-api-keys.md` — the credentials this UI manages are read by THIS server at request time. UI changes don't propagate without a server restart (see ⚠️ note in 4d).
- **shares-state-with** `feature-api-keys.md` via `~/.openbb_platform/user_settings.json` — but the Python server never writes to it; only reads.
- **independent-of** `feature-jupyter.md` and `feature-logs-streaming.md` (those are about *how* the TS app runs subprocesses, not what the subprocesses do).

### Key file reference (absolute paths)

- `/home/user/OpenBBPort/openbb_platform/extensions/platform_api/openbb_platform_api/main.py` — `openbb-api` entry, mounts `/widgets.json` and `/apps.json`, runs uvicorn.
- `/home/user/OpenBBPort/openbb_platform/extensions/platform_api/openbb_platform_api/utils/api.py` — CLI argparse, app importer, port checker.
- `/home/user/OpenBBPort/openbb_platform/extensions/platform_api/pyproject.toml` — registers `openbb-api` console script.
- `/home/user/OpenBBPort/openbb_platform/core/openbb_core/api/rest_api.py` — FastAPI app construction, route mounting, lifespan banner.
- `/home/user/OpenBBPort/openbb_platform/core/openbb_core/api/app_loader.py` — `add_routers`, `add_openapi_tags`, `add_exception_handlers`.
- `/home/user/OpenBBPort/openbb_platform/core/openbb_core/api/router/commands.py` — wraps every extension command with the FastAPI-facing async wrapper.
- `/home/user/OpenBBPort/openbb_platform/core/openbb_core/api/router/coverage.py` — `/coverage/{command_model,providers,commands}` endpoints.
- `/home/user/OpenBBPort/openbb_platform/core/openbb_core/api/router/system.py` — `/system` endpoint (DEV_MODE only).
- `/home/user/OpenBBPort/openbb_platform/core/openbb_core/api/router/user.py` — `/user/me` and the default `auth_hook` / `user_settings_hook`.
- `/home/user/OpenBBPort/openbb_platform/core/openbb_core/api/auth/user.py` — HTTP Basic auth implementation.
- `/home/user/OpenBBPort/openbb_platform/core/openbb_core/api/exception_handlers.py` — exception -> HTTP status mapping.
- `/home/user/OpenBBPort/openbb_platform/core/openbb_core/app/command_runner.py` — `ParametersBuilder`, `StaticCommandRunner`, `CommandRunner`.
- `/home/user/OpenBBPort/openbb_platform/core/openbb_core/app/router.py` — `Router`, `command` decorator, `SignatureInspector`, `CommandMap`, `RouterLoader`.
- `/home/user/OpenBBPort/openbb_platform/core/openbb_core/app/query.py` — `Query.execute` — the bridge from router function to QueryExecutor.
- `/home/user/OpenBBPort/openbb_platform/core/openbb_core/app/provider_interface.py` — synthesises per-route Pydantic param dataclasses and discriminated-union response models.
- `/home/user/OpenBBPort/openbb_platform/core/openbb_core/app/extension_loader.py` — entry-points discovery (`openbb_core_extension`, `openbb_provider_extension`, `openbb_obbject_extension`).
- `/home/user/OpenBBPort/openbb_platform/core/openbb_core/app/service/auth_service.py` — auth extension loader.
- `/home/user/OpenBBPort/openbb_platform/core/openbb_core/app/service/system_service.py` — `system_settings.json` loader.
- `/home/user/OpenBBPort/openbb_platform/core/openbb_core/app/model/obbject.py` — the response container.
- `/home/user/OpenBBPort/openbb_platform/core/openbb_core/app/model/api_settings.py` — `/api/v1` prefix, CORS, custom_headers.
- `/home/user/OpenBBPort/openbb_platform/core/openbb_core/app/model/python_settings.py` — `uvicorn` kwargs passthrough.
- `/home/user/OpenBBPort/openbb_platform/core/openbb_core/app/model/user_settings.py` — `credentials/preferences/defaults`.
- `/home/user/OpenBBPort/openbb_platform/core/openbb_core/env.py` — all `OPENBB_*` env-var properties.
- `/home/user/OpenBBPort/openbb_platform/core/openbb_core/provider/registry.py` — `Registry`, `RegistryLoader`.
- `/home/user/OpenBBPort/openbb_platform/core/openbb_core/provider/registry_map.py` — `RegistryMap`.
- `/home/user/OpenBBPort/openbb_platform/core/openbb_core/provider/query_executor.py` — `QueryExecutor.execute`, credential filtering.
- `/home/user/OpenBBPort/openbb_platform/core/openbb_core/provider/abstract/fetcher.py` — `Fetcher.fetch_data` TET pipeline.
- `/home/user/OpenBBPort/openbb_platform/core/openbb_core/provider/utils/errors.py` — `EmptyDataError`, `UnauthorizedError`.
- `/home/user/OpenBBPort/openbb_platform/extensions/equity/openbb_equity/price/price_router.py` — example router with the `historical` command.
- `/home/user/OpenBBPort/openbb_platform/providers/yfinance/openbb_yfinance/models/equity_historical.py` — example fetcher.
- `/home/user/OpenBBPort/openbb_platform/core/openbb_core/provider/standard_models/equity_historical.py` — standard model both router and fetcher reference.
