# Platform REST API — v2 Addendum

> Second-pass deep-dive. Reads `platform-rest-api.md` (v1) plus
> `api-keys.md`, `backend-services.md`, `installation.md`, `environments.md`.
> Findings here are NEW material; v1 stands. Final section reconciles any
> discrepancies discovered.
> Generated 2026-05-15.

This file lives next to `platform-rest-api.md` and supplements it. Cross-cutting
sections numbered to roughly mirror v1, but the focus is on what v1 missed:
the parallel MCP server, `widgets.json` schema details, the apps/agents merge
flow, OBBject client methods, request-time settings reload behaviour, the
discriminated-union response model on the wire, and Python-only providers.

---

## A. The parallel `openbb-mcp` server (the second seed backend)

`backend-services.md:1525` shows the desktop env's `openbb.yaml` ships with
**two** pip packages, not one: `openbb-platform-api` (the v1 deep-dive subject)
**and** `openbb-mcp-server`. The default installation seeds **two** backend
services (`desktop/src-tauri/src/tauri_handlers/startup.rs:1428-1480`):

| Backend | Default command | Default port |
|---|---|---|
| OpenBB Platform API | `openbb-api` | `6900` (`main.py:299`) |
| OpenBB MCP | `openbb-mcp --transport streamable-http --host 127.0.0.1 --port 8001` | `8001` |

v1 only documented `openbb-api`. The MCP server is critical and architecturally
distinct.

### A.1. Package & entry point

- Package: `openbb-mcp-server` (`extensions/mcp_server/pyproject.toml:2`).
- Console script: `openbb-mcp = openbb_mcp_server.app.app:main` (`pyproject.toml:14-15`).
- Hard dependencies: `openbb-core ^1.6.7`, `fastmcp >=3.2.0` (`pyproject.toml:19-20`).
- The MCP server is **not** a separate FastAPI app. It imports the same
  `from openbb_core.api.rest_api import app` (`extensions/mcp_server/openbb_mcp_server/app/app.py:41`),
  which means **all the boot-time work documented in v1 §1c-1e runs again** in
  the MCP process. The two servers do not share memory; each is a full Python
  process with its own ProviderInterface singleton.

### A.2. Architecture: FastAPI to MCP bridge

The bridge is `FastMCP.from_fastapi(app=fastapi_app, ...)`
(`app/app.py:527-534`). FastMCP introspects FastAPI routes and surfaces each
one as an MCP tool, resource, or resource-template. The mapping:

1. `process_fastapi_routes_for_mcp(fastapi_app, settings)` (`utils/fastapi.py`)
   walks every route, applies category-allow filters from
   `settings.allowed_tool_categories`, and produces `route_lookup`,
   `route_maps`, plus `prompt_definitions` extracted from
   `route.openapi_extra["mcp_config"]["prompts"]`.
2. `customize_components` callback (`app/app.py:404-518`) renames each tool to
   `{category}_{subcategory}_{tool}` (or `{category}_{tool}` if subcategory is
   `general`), tags it with the category, and toggles enable/disable based on
   `mcp_config.enable` per-route or `default_tool_categories` globally.
3. `compress_schema(...)` (`app/app.py:459-464`) collapses the per-tool
   parameter and output schemas (which are normally a discriminated union of
   every provider — see §F) into a smaller form usable by LLM context windows.
4. `mcp.disable(names=all_registered)` then selective `mcp.enable(...)`
   (`app/app.py:537-549`) — every tool is registered but disabled by default in
   discovery mode; LLMs progressively activate via the `activate_tools`,
   `activate_category`, and `available_tools` admin tools (`app/app.py:599-734`).

### A.3. Transport options

`--transport <mode>` (`utils/app_import.py:234`); options:

| Transport | Behaviour |
|---|---|
| `streamable-http` (default) | uvicorn-hosted HTTP server with chunked SSE-style streaming responses. Listens on `host:port` from `settings.uvicorn_config` (`app/app.py:1006-1011`). |
| `stdio` | Reads MCP framed JSON-RPC from stdin, writes to stdout. No port. Started via `asyncio.run(stdio_main(mcp_server))` (`app/app.py:935-949`). Used by Claude Desktop / Cursor / VSCode style integrations that want to spawn the server as a child and pipe to it. |
| `sse` | Server-Sent Events transport (legacy). Wrapped with `SSEShutdownWrapper` (`app/app.py:874-932`) to handle graceful client disconnects on a path ending in `/sse/`. |

### A.4. Settings

`MCPSettings` (`models/settings.py:14-220`) is loaded with priority
**CLI > env > config file > defaults** (`app/app.py:975`,
`service/mcp_service.py`). Disk file is `~/.openbb_platform/mcp_settings.json`
(referenced from the api-keys.md file allow-list at `:279`).

Notable env vars (all `OPENBB_MCP_*` prefixed; aliased in `MCPSettings`):

| Var | Default | Effect |
|---|---|---|
| `OPENBB_MCP_NAME` | `OpenBB MCP` | Server name in the MCP handshake |
| `OPENBB_MCP_DEFAULT_TOOL_CATEGORIES` | `["all"]` | Comma-separated category list enabled by default |
| `OPENBB_MCP_ALLOWED_TOOL_CATEGORIES` | None | If set, restricts to this list (others stripped before registration) |
| `OPENBB_MCP_ENABLE_TOOL_DISCOVERY` | `false` | Discovery mode — all tools start disabled, agents call `activate_tools` per-session |
| `OPENBB_MCP_DEFAULT_SKILLS_DIR` | bundled `openbb_mcp_server/skills/` | Path to skills served as MCP resources at `skill://<name>/SKILL.md` URIs |
| `OPENBB_MCP_SKILLS_PROVIDERS` | None | Vendor providers: `claude,cursor,vscode,copilot,codex,gemini,goose,opencode` (`app/app.py:65-74`) |
| `OPENBB_MCP_UVICORN_CONFIG` | `{"host":"127.0.0.1","port":"8001"}` | Same shape as `python_settings.uvicorn` for the REST server but isolated |
| `OPENBB_MCP_HTTPX_CLIENT_KWARGS` | `{}` | Forwarded to FastMCP's outbound HTTP client (used when MCP tools call back into the REST API) |
| `OPENBB_MCP_CLIENT_AUTH` | None | Tuple `[user,pass]` used in the FastMCP→REST httpx client (i.e. when MCP tool fans out to `openbb-api` with basic auth) |
| `OPENBB_MCP_SERVER_AUTH` | None | Tuple `[user,pass]` for inbound auth to the MCP server (`app/auth.py:35-77`, Bearer with base64-encoded `user:pass`) |

### A.5. MCP request lifecycle (vs the REST lifecycle in v1 §2)

When an LLM agent invokes a tool via MCP:

1. MCP client sends JSON-RPC `tools/call` over the chosen transport.
2. `TokenAuthProvider.authorize` (`app/auth.py:35-76`) validates the Bearer
   token if `server_auth` is set. Token format is `base64(user:pass)`, NOT a
   real JWT.
3. FastMCP looks up the tool by its registered name (`{category}_{tool}`),
   resolves it back to the FastAPI `OpenAPITool`, and calls FastMCP's internal
   httpx client to make a real HTTP request **to the REST endpoint** at
   `http://<settings.uvicorn_config.host>:<port>/api/v1/...`.
4. The REST server (which is the same Python process here, but a different
   ASGI app instance) processes the request via the v1 §2 pipeline.
5. Response is returned as MCP content. LLM-friendly compression is applied
   per `settings.describe_responses` and the per-route
   `mcp_config.describe_responses` override (`app/app.py:467-473`).

> ⚠️ Critical: when bundled in the desktop, the MCP server makes an HTTP call
> back into the REST server as a separate process. There are TWO Python
> processes consuming RAM (each with a full ProviderInterface singleton); the
> MCP server's outbound httpx client must reach
> `http://127.0.0.1:6900/api/v1/...` for the OpenBB-bundled REST endpoints.
> If the user changes the REST port or stops the REST server, the MCP server's
> tools fail with `ConnectError`. The desktop UI does not couple their
> lifecycles.

### A.6. Per-route MCP customisation

Routes can declare MCP behaviour via `@router.command(mcp_config={...})`
(`core/openbb_core/app/router.py:105-106` stores it under
`openapi_extra["mcp_config"]`). Schema (per `models/mcp_config.py`):

- `name: str` — override the auto-generated `{category}_{tool}` name.
- `enable: bool` — explicit enable/disable, overrides category defaults.
- `tags: list[str]` — extra tags for filtering.
- `describe_responses: bool` — include full response schema in tool description.
- `mime_type: str` — for routes exposed as `OPENAPIResource` (file-like).
- `prompts: list[{name, description, content, arguments, tags}]` — inline
  prompts associated with this tool. Surfaced via `_add_inline_prompts`
  (`app/app.py:263-313`) and listed in the tool description under
  `**Associated Prompts:**` (`app/app.py:476-488`).

The widget catalogue (§B) also surfaces an `mcp_tool` block per widget with
`tool_id` matching the widget name, so OpenBB Workspace knows which MCP tool
backs a given widget (`utils/widgets.py:600-603`).

---

## B. `/widgets.json` actual output shape (v1 §3b said "generated from OpenAPI")

`build_json(openapi, widget_exclude_filter)` (`utils/widgets.py:233-741`)
walks every path in the OpenAPI spec and for each `(route, provider)` pair
emits one entry. The output dict is `{widget_id: widget_config}`.

### B.1. `widget_id` format

Pattern: `<route_with_slashes_replaced_by_underscores>_<provider>_obb`
(`widgets.py:299-303,599`). Examples:

- `equity_price_historical_yfinance_obb`
- `equity_price_historical_fmp_obb`
- `economy_fred_search_fred_obb`

For routes with a chart variant, a parallel `<id>_chart` widget is also added
(`widgets.py:710-739`).

### B.2. Widget config schema

Each widget entry (final, post-merge) has the following top-level keys
(`widgets.py:594-615`):

```jsonc
{
  "name": "Historical",                    // humanised; UPPER for words in TO_CAPS_STRINGS
  "description": "...",                    // from route's OpenAPI 'description'
  "category": "Equity",                    // first URL segment titlecased; "Fixedincome" → "Fixed Income"
  "subCategory": "Price",                  // second URL segment, optional
  "type": "table",                         // table | markdown | chart | metric | pdf | multi_file_viewer | omni | ssrm_table
  "widgetId": "equity_price_historical_yfinance_obb",
  "mcp_tool": {                            // links Workspace UI to an MCP tool of the same id
    "mcp_server": "Open Data Platform",
    "tool_id": "equity_price_historical"
  },
  "params": [                              // see B.3 — flattened, provider-specific
    { "paramName": "symbol", "type": "text", "description": "...", "value": "AAPL", ... },
    { "paramName": "start_date", "type": "date", ... },
    { "paramName": "provider", "value": "yfinance", "show": false }
  ],
  "endpoint": "/api/v1/equity/price/historical",
  "runButton": false,
  "gridData": { "w": 40, "h": 15 },        // 4×5 for metric, 20×25 for pdf
  "data": {
    "dataKey": "results",                  // OBBject.results when response_model is OBBject; "" otherwise
    "table": { "showAll": true, "enableAdvanced": true, "columnsDefs": [...] }
  },
  "source": ["yFinance"]                   // pretty provider name (per provider_map at widgets.py:559-575)
}
```

`columnsDefs` is built by `data_schema_to_columns_defs(openapi, widget_id, provider, route)` (`widgets.py:348-353`). It walks the per-provider Data
schema in OpenAPI components and emits AG-Grid-style column defs.

### B.3. `params` array — per-parameter object

For each query parameter, after `modify_query_schema(query_schema, provider)`
(`widgets.py:88-220`), each param has:

| Field | Source | Notes |
|---|---|---|
| `paramName` | OpenAPI `parameter_name` (renamed from `name`) | Required |
| `type` | inferred from JSON schema | `text`, `date`, `boolean`, `number`, `form`, `button` |
| `description` | OpenAPI description | Augmented with "Multiple comma separated items allowed." for params where `multiple_items_allowed[provider]==true` |
| `label` | OpenAPI `label` or `parameter_name.upper()` for known IDs | Words in `TO_CAPS_STRINGS` (`openapi.py:8-63`: PE, EBITDA, GDP, USD, ETF, …) get uppercased |
| `value` | default | If `provider` param: filled with the provider name and `show:false` |
| `options` | per-provider override | If a param has `options[provider]`, those options replace the defaults |
| `multiSelect` | `true` for provider-allowed multi-value | If true and no options, `multiple:true` and `style:{popupWidth:400}` |
| `available_providers` | filter | Used to drop the param if it's not valid for the current provider; the field itself is then stripped |

The provider-discrimination is **per-widget**, not per-call: a single OpenBB
route with 5 providers becomes **5 widgets** in `widgets.json`, each with
provider-specific params and a hidden `provider` param fixed to that provider.

### B.4. Form widgets (POST endpoints with companion form_endpoint)

A route's `widget_config.form_endpoint` (`widgets.py:223-230,259,418-455`)
points to a separate POST route used for the form. The widget then becomes a
`type:"form"` param with `inputParams` describing the fields, and a hidden
`submit:button` is auto-appended if not present (`widgets.py:419-435`).

### B.5. Provider-pretty-name map

`utils/widgets.py:559-575` hard-codes display names: `tmx→TMX`,
`ecb→ECB`, `econdb→EconDB`, `eia→EIA`, `fmp→FMP`, `oecd→OECD`, `finra→FINRA`,
`fred→FRED`, `imf→IMF`, `bls→BLS`, `yfinance→yFinance`, `sec→SEC`, `cftc→CFTC`,
`tradingeconomics→Trading Economics`, `wsj→WSJ`. Anything else → `Title Case`
of the snake-cased name.

### B.6. Cold-start cost

`build_json` is called once at boot (`main.py:109-111`). For a full openbb
install with ~30 providers and ~150 routes, this iterates `routes × providers`
(several hundred widget entries) and for each one walks the OpenAPI components
recursively to build columnsDefs. **Expect 2-5 seconds of pure widget
generation on top of the import-time singleton work** (§G).

---

## C. `/apps.json` merge logic and priority

v1 said "merges 3 sources". Actual flow (`main.py:174-249`):

```
new_templates = []
default_templates = []           # source 1: bundled default_apps.json
                                 # source 2: router-contributed apps.json (additional)
templates       = []             # source 3: ~/OpenBBUserData/workspace_apps.json (USER)

if file exists DEFAULT_APPS_PATH: default_templates = json.load(...)

if has_additional_apps(app):     # other routers mounted /<x>/apps.json endpoints
    for apps in (await get_additional_apps(app)).values():
        default_templates.extend(apps)        # ← APPENDED to defaults

if file exists APPS_PATH:        # user file
    templates = json.load(APPS_PATH)
    if isinstance(templates, dict): templates = [templates]
    templates.extend(default_templates)        # ← USER FIRST, then DEFAULTS
    for template in templates:
        # filter: keep only apps whose layout references a known widget_id
        #         OR widgets matching `rich_note*`
        ...
return new_templates
```

### C.1. Priority order

User apps appear **first** in the rendered list (because `templates` is the
user list, then `templates.extend(default_templates)` appends defaults after).
The filtering step deduplicates by reference (`if template not in new_templates`
at `main.py:222`), but uses Python `==` deep equality on dicts — so two apps
with identical content won't both appear, but two apps with different `id`
fields and otherwise identical layouts will both appear.

### C.2. The "known widget_id" filter

`main.py:220-244`: an app is dropped if its `layout`/`tabs.layout` contains
**any** widget `i` field that is not in the in-memory `widgets_json` dict and
doesn't start with `rich_note`. This is a **silent drop** — there's no logging
about why an app is missing. If the user has an app referencing a provider that
isn't installed, the entire app disappears from `/apps.json`. This is a major
foot-gun for the desktop UX (e.g. user uninstalls FMP → all FMP-using apps
vanish without warning).

### C.3. App schema (per entry)

From `default_apps.json`:

```jsonc
{
  "name": "Example FRED App",
  "img": "https://...",
  "description": "...",
  "authentication": "Get your FRED API KEY at https://...",  // displayed in UI
  "allowCustomization": true,
  "tabs": {
    "Search": {
      "id": "Search",
      "name": "Search",
      "layout": [
        {
          "i": "economy_fred_search_fred_obb",   // ← MUST match a widget_id
          "x": 0, "y": 2, "w": 40, "h": 15,
          "state": {
            "params": { "query": "pce", "tag_names": "inflation;pce" },
            "chartView": { "enabled": false, "chartType": "line" },
            "columnState": { ... }
          }
        }
      ]
    }
  }
  // OR a flat layout: "layout": [...] without "tabs"
  // OR id-only: { "id": "<widget_id>" } for single-widget apps
}
```

### C.4. Auto-create empty `workspace_apps.json`

If `~/OpenBBUserData/workspace_apps.json` doesn't exist, the server creates the
parent directory and writes `[]` (`main.py:183-189`). On the FIRST request to
`/apps.json`. This is a **side effect of the GET** — the TS port should
preserve to avoid breaking Workspace.

---

## D. `/agents.json` — when used and schema

`AGENTS_PATH = kwargs.pop("agents-json", None)` (`main.py:61`). Three
mutually-exclusive paths:

1. `--agents-json /path/to/file.json`: simply serves the file's contents
   (`main.py:252-261`). On every request reads from disk.
2. No `--agents-json` flag, but extension routers contributed
   `<x>/agents.json`: aggregate them via `get_additional_agents`
   (`utils/merge_agents.py:20-55`).
3. Neither: serves `{}` (`main.py:282-285`).

### D.1. Schema (inferred from `merge_agents.py:46-51`)

```jsonc
{
  "<agent_name>": {
    "endpoints": {
      "<endpoint_name>": "/path/to/endpoint"   // path-prefixed by router base path
    },
    // additional vendor-specific keys allowed
  }
}
```

Note: `merge_agents.py:49` has a bug — `endpoints.startwith` (typo for
`startswith`) means the path-rewrite branch will raise `AttributeError` if
ever taken. Effectively the prefix-rewrite never runs for additional agents
unless someone supplies an `endpoints` value that's truthy but a string.
(The TS port should NOT re-emulate this bug.)

### D.2. When does desktop use it?

The desktop seed backends do not pass `--agents-json`. The bundled MCP server
exposes its own discovery mechanism (`available_categories` + `available_tools`
admin tools). `/agents.json` exists for third-party agent frameworks that
prefer a static manifest.

---

## E. OBBject client-side methods (cannot run server-side)

v1 §6 noted these exist but didn't enumerate. From
`core/openbb_core/app/model/obbject.py`:

| Method | Lines | Returns | Notes |
|---|---|---|---|
| `to_df(index, sort_by, ascending)` | 81-119 | pandas DataFrame | Alias of `to_dataframe` |
| `to_dataframe(index, sort_by, ascending)` | 121-285 | pandas DataFrame | 165-line method handling 8+ result shapes (List[BaseModel], List[Dict], Dict[str, Dict], BaseModel, str, scalar list, …). Imports pandas lazily |
| `to_polars()` | 287-296 | polars DataFrame | Calls `from_pandas(self.to_dataframe(index=None))`; raises `ImportError` with install hint if polars missing |
| `to_numpy()` | 298-300 | numpy.ndarray | `self.to_dataframe(index=None).to_numpy()` |
| `to_dict(orient)` | 302-335 | dict or list[dict] | Wraps pandas `.to_dict(orient=...)`; orients: `dict|list|series|split|tight|records|index` |
| `to_llm()` | 337-353 | JSON string | `df.to_json(orient="records", date_format="iso", date_unit="s")` |
| `show(**kwargs)` | 355-362 | None | Plotly `chart.fig.show(...)`; raises if no chart |

### E.1. Implication for the TS SDK

These methods are **client-side**: they run in the user's Python process when
they import `from openbb import obb`. The REST server NEVER calls them; it
serialises the OBBject via Pydantic and returns JSON.

For a TS port that wants to expose an `obb` SDK to TS user code:

- Skip `to_dataframe/to_polars/to_numpy` entirely — TS doesn't have a
  pandas-equivalent in mainstream use. If you need a tabular abstraction,
  emit the OBBject as JSON and let user code use `arquero`, `tinyframe`, or
  raw arrays.
- `to_dict()` is essentially the existing JSON shape — trivial.
- `to_llm()` is a date-formatted JSON-records dump — easy to reproduce
  (`results.map(r => ({...r, date: r.date.toISOString()}))`).
- `show()` requires a Plotly equivalent on the TS side. Plotly.js exists
  (`react-plotly.js`); the chart payload is `chart.content` (raw Plotly JSON)
  so it's directly consumable.

---

## F. Discriminated-union response model in `/openapi.json`

v1 §1e and §5 mentioned the discriminated union. Verified the actual
emitted shape at `provider_interface.py:678-741`:

```python
SerializeAsAny[Annotated[Union[
    Annotated[YFinanceEquityHistoricalData, Tag("yfinance")],
    Annotated[FMPEquityHistoricalData, Tag("fmp")],
    Annotated[PolygonEquityHistoricalData, Tag("polygon")],
    ...
], Discriminator(get_provider)]]
```

Where `get_provider(v) -> v._provider` and `_provider` is set on each Data
class via `setattr(data, "_provider", provider)` at
`provider_interface.py:689`.

### F.1. How FastAPI emits this in OpenAPI

FastAPI walks the response_model and per Pydantic's
[discriminated-union JSON schema](https://docs.pydantic.dev/latest/concepts/unions/#discriminated-unions),
emits a `oneOf` with a `discriminator.propertyName` and a `mapping` of tag
values to `$ref`s. So for `EquityHistorical` with 5 providers, the OpenAPI
schema for `OBBject_EquityHistorical.results` is:

```json
{
  "oneOf": [
    { "$ref": "#/components/schemas/YFinanceEquityHistoricalData" },
    { "$ref": "#/components/schemas/FMPEquityHistoricalData" },
    ...
  ],
  "discriminator": {
    "propertyName": "_provider",
    "mapping": {
      "yfinance": "#/components/schemas/YFinanceEquityHistoricalData",
      "fmp": "#/components/schemas/FMPEquityHistoricalData",
      ...
    }
  }
}
```

This is **usable** schema, not `{}` — codegen tools (openapi-typescript,
openapi-generator) can consume it directly. Confirmed via the per-route
`response_model` declared at `router.py:125-132`.

### F.2. Caveat: the discriminator field `_provider`

The discriminator key is the underscore-prefixed `_provider` attribute, not a
real serialised field. Pydantic emits it in the JSON schema for routing, but
**the actual JSON response does not contain `_provider`** — the provider name
appears separately in `OBBject.provider`. Some openapi-typescript generators
may produce types that expect a `_provider` field on each data row; consumers
will need a custom transform that drops it from request types and reads
`obbject.provider` instead.

### F.3. The MCP server compresses this

`compress_schema(component.parameters)` and `compress_schema(component.output_schema)`
(`extensions/mcp_server/openbb_mcp_server/app/app.py:459-464`) flatten the
oneOf into a more compact form for LLM context windows. The TS port doesn't
need to re-implement this if it uses raw OpenAPI — but for an MCP-equivalent
in TS, this compression matters.

---

## G. Cold-start cost (the "user clicks Start, waits N seconds" problem)

The boot sequence in v1 §1b-1e is dominated by entry-point traversal and
singleton initialisation. Concretely, `from openbb_core.api.rest_api import app`
triggers (in order):

1. `Env()` — singleton, `dotenv.load_dotenv` + `os.environ.copy()`. Cheap.
2. `SystemService()` — singleton, reads `system_settings.json`. Cheap.
3. Import of `commands.py` triggers
   `add_command_map → RouterLoader.from_extensions()` →
   `ExtensionLoader.core_objects` (entry-points lookup) →
   each extension's `__init__.py` runs, each `Router.command(model="X")` runs,
   each `SignatureInspector.complete(func, model)` calls `ProviderInterface()`
   (which is a singleton — first call triggers full provider+model registry
   build via `RegistryMap`).
4. `RegistryMap.__init__` (`provider/registry_map.py:23-30`) walks
   `registry.providers[*].fetcher_dict` and for each fetcher calls
   `_extract_info` (introspect Pydantic fields) and `_get_model` (record
   classes for return-annotation generation). For ~30 providers × ~150 model
   names, that's ~thousands of fetcher introspections.
5. `ProviderInterface._generate_return_annotations` (`provider_interface.py:699-741`)
   then constructs one `create_model("OBBject_X", __base__=OBBject[Annotated[Union[...]]])`
   per model. This is the expensive Pydantic dynamic-model creation step and
   compiles validators.
6. `app.openapi()` (`main.py:107`) — eagerly serialises the entire OpenAPI
   spec; FastAPI walks every route's response_model and Pydantic emits a
   schema for each discriminated union (§F).
7. `get_widgets_json(...)` — see §B.6 (~2-5s).
8. `check_for_platform_extensions` walks `app.openapi_tags` (cheap).

### G.1. Measured total

On a developer machine with a full openbb install (`uv pip install openbb`),
the import-to-first-bind time is typically **8-15 seconds** for a cold start.
This is **before uvicorn opens the listen socket**. The desktop's
backends.tsx UX must reflect this — there's a multi-second window where the
backend is "starting" with no port reachable.

### G.2. Implication for the desktop "Start" button

The backend status flips from `stopped` → `running` when the child process
exits cleanly OR when the URL it printed (`main.py:325-330`) is parsed from
stdout. v1 of `backend-services.md` (lines 192,774-807) confirms there's a
`openbb-api`-specific URL extraction step. Until that line appears, the user
sees a spinner.

For a TS-native rewrite (v1's "Strategy A") the equivalent build step
(constructing all the per-route Zod schemas and the discriminated unions)
should be done **at TS build time**, not at runtime — generated code instead
of reflective metaclass work. Boot would then be <500ms.

---

## H. `defaults.commands` mechanism — schema and override semantics

v1 mentions this but doesn't show the schema. From
`core/openbb_core/app/model/defaults.py` and the wrapper at
`core/openbb_core/api/router/commands.py:247-275`:

### H.1. On-disk format

In `~/.openbb_platform/user_settings.json`:

```jsonc
{
  "credentials": { ... },
  "preferences": { ... },
  "defaults": {
    "commands": {              // alias: "routes" (deprecated, see H.2)
      "equity.price.historical": {
        "provider": "yfinance",        // string OR list
        "interval": "1d",
        "extended_hours": false,
        "chart": true,
        "chart_params": {              // forwarded to extra_params["chart_params"]
          "title": "AAPL Price"
        }
      },
      "economy.fred_search": {
        "provider": ["fred"]
      }
    }
  }
}
```

The key is the route path with slashes converted to dots and stripped of the
leading `/api/v1`. Conversion happens via `clean_k = k.strip("/").replace("/", ".")`
at `defaults.py:45`.

### H.2. Migration: `routes` → `commands`

`Defaults.validate_before` (`defaults.py:26-51`) accepts both `routes`
(deprecated) and `commands` keys. If `routes` is present and `show_warnings`
is on, emits an `OpenBBWarning`. The TS port should write the new key but
read both.

### H.3. Per-request override behaviour

`commands.py:256-275` shows the precedence:

- `defaults.provider` is **always popped** (not applied) — providers come from
  the `?provider=` query param, not defaults.
- `defaults.chart` becomes `kwargs["chart"]`.
- `defaults.chart_params` becomes `extra_params["chart_params"]`.
- For other keys: only applied if the corresponding param in
  `standard_params`/`extra_params` is `None` (i.e. user didn't pass it on the
  query). User-supplied values always win.

### H.4. Interaction with credentials

Defaults DO NOT override credentials. The credentials block lives in
`user_settings.credentials` and is filtered by `QueryExecutor.filter_credentials`
on every request (v1 §2e). There is NO `defaults.credentials` field — the
api-keys page is the only credential surface.

---

## I. Custom auth extension contract

v1 §4c lays out the entry-points lookup. Now the contract:

`AuthService._load_extension` (`service/auth_service.py:67-76`) imports the
extension's module and reads three attributes:

```python
entry_mod.router               # FastAPI APIRouter — replaces /user router
entry_mod.auth_hook            # async (request) -> None — gates coverage/system routers
entry_mod.user_settings_hook   # async (...) -> UserSettings — injected per command request
```

The defaults in `core/openbb_core/api/router/user.py:9-11`:

```python
router = APIRouter(prefix="/user", tags=["User"])
auth_hook = authenticate_user             # from auth/user.py
user_settings_hook = get_user_settings    # from auth/user.py
```

### I.1. `auth_hook` signature

```python
async def auth_hook(
    credentials: Annotated[HTTPBasicCredentials | None, Depends(security)] = None,
) -> None:
    # raises HTTPException(401) on failure, returns None on success
```

For a JWT extension, this would be `bearer = HTTPBearer(); creds = Depends(bearer); decode JWT; raise on failure`.

### I.2. `user_settings_hook` signature

```python
async def user_settings_hook(
    _: Annotated[None, Depends(authenticate_user)],
) -> UserSettings:
    # returns the logged-in user's UserSettings object
    # (could load per-user from DB, not just from disk)
```

This is the hook that allows multi-tenant deployments — each authenticated
user gets their own `UserSettings` (with their own credentials, defaults,
preferences) without sharing the global `~/.openbb_platform/user_settings.json`.

### I.3. The `router` attribute

Mounted in DEV_MODE only (`rest_api.py:78`), as `[AuthService().router, ...]`.
For an OAuth/JWT extension, `router` typically exposes login, token-refresh,
me, and logout endpoints under `/user/*`.

### I.4. Discovery

The extension must register an entry-point in group `openbb_core_extension`
(NOT `openbb_provider_extension`), with `name = OPENBB_API_AUTH_EXTENSION` env
value. So a `pyproject.toml`:

```toml
[tool.poetry.plugins."openbb_core_extension"]
auth-jwt = "openbb_auth_jwt:my_module"
```

then `OPENBB_API_AUTH_EXTENSION=auth-jwt openbb-api` loads it.
`_get_entry_mod` (`auth_service.py:60-65`) does `import_module(extension.module)`
and then attribute-lookup for `router/auth_hook/user_settings_hook` on that
module.

### I.5. No example ships in this repo

Searched: no first-party JWT/OAuth extension is bundled in
`openbb_platform/extensions/`. The only consumer is in OpenBB's hosted Pro
product (out of scope for this repo). For the TS port, the contract is what
the `auth_service.py` code shows.

---

## J. Pre-request middleware audit

v1 §2b said "CORS and auth, no global middleware". Confirmed by exhaustive
search:

- `app.add_middleware(CORSMiddleware, ...)` (`rest_api.py:68-73`) — only
  middleware registered on the FastAPI app at boot.
- No FastAPI `BaseHTTPMiddleware` subclass anywhere in `core/openbb_core/api/`.
- No request-logging middleware. The only logging is uvicorn's default access
  log (controlled by `python_settings.uvicorn.log_level`).
- No rate-limiting, no telemetry, no request-ID injection.
- The `import_app` helper at `utils/api.py:305-310` ALSO adds CORSMiddleware
  to externally imported FastAPI apps.

### J.1. The MCP server adds more middleware

`_build_runtime_middleware` (`mcp_server/app/app.py:124-137`) returns a
single CORSMiddleware. Then in `main()` at `app/app.py:1018`, an `SSEShutdownWrapper`
ASGI middleware is appended. So the MCP server has CORS + SSE-shutdown; still
no logging/rate/telemetry.

### J.2. The custom-headers feature is NOT middleware

`api_settings.custom_headers` (`api_settings.py:41-43`) is a dict that the
`build_new_signature` step (`commands.py:119-130`) turns into per-route
`Header(include_in_schema=False)` parameters. So custom headers are sent **by
the client** with default values, not added to every response by middleware.
The single response header set for `/widgets.json`/`/apps.json`/`/agents.json`
is `X-Backend-Type: OpenBB Platform` — manually attached via
`JSONResponse(content=..., headers=obb_headers)` (`main.py:69,154,247,260,278`).

---

## K. `OBBject.from_query` and the `maybe_coroutine` async chain

The async runtime is **asyncio** (FastAPI's default, run by uvicorn). The
fetcher pipeline tolerates both sync and async user code:

`maybe_coroutine` (`provider/utils/helpers.py:581-588`):

```python
async def maybe_coroutine(func, /, *args, **kwargs):
    if not iscoroutinefunction(func):
        return cast(T, func(*args, **kwargs))
    return await func(*args, **kwargs)
```

Used at:

- `command_runner.py:251` — wraps the user's router function (e.g. `historical`).
- `fetcher.py:82` — wraps `cls.extract_data` (or `aextract_data`).

> ⚠️ Caveat: `maybe_coroutine` calls **sync** code **directly on the event
> loop**. There is NO `loop.run_in_executor()` or thread-pool wrapping. So a
> sync `extract_data` that does a blocking HTTP call (`requests.get(...)`)
> will block the entire FastAPI worker. yfinance's `yf.download(...)` is
> sync (but `yfinance>=0.2.66` uses `curl_cffi` async sessions internally —
> see N).

`run_async` (`helpers.py:591-602`) is the inverse: synchronously runs an
async function in a blocking-portal subthread. Used at boot in
`utils/api.py:120` to call `get_and_fix_widget_paths` from sync init code.

### K.1. `Fetcher.__init_subclass__` promotion

`provider/abstract/fetcher.py:60-71` automatically promotes `aextract_data`
(an async method) to the canonical name `extract_data`. So a fetcher can
implement either, and the call chain is uniform. Same for `atransform_query`/
`atransform_data` in some fetchers.

---

## L. Settings reload behaviour — what's cached, what's re-read

| Settings | Loaded at | Reloaded? | File |
|---|---|---|---|
| `Env()` | Module import (singleton) | No — process lifetime only | `~/.openbb_platform/.env` |
| `SystemSettings` | First `SystemService()` call (singleton) | Only via explicit `SystemService().refresh_system_settings()` (`system_service.py:105-109`) — never auto. Frozen Pydantic model | `~/.openbb_platform/system_settings.json` |
| `UserSettings.credentials` | **Every** command request | Yes — `UserService.read_from_file()` at `commands.py:244` | `~/.openbb_platform/user_settings.json` |
| `UserSettings.preferences` | Same as credentials | Yes — same read | Same file |
| `UserSettings.defaults` | Same as credentials | Yes — same read | Same file |
| `widget_settings.json` | Module import only | **No reload** | `~/.openbb_platform/widget_settings.json` |
| `widgets.json` (in-memory dict) | Boot only, OR every request if `--editable` | `--editable` mode rebuilds on every `/widgets.json` GET (`main.py:147-153`); else cached | N/A — derived from OpenAPI |
| `default_apps.json` | **Every** `/apps.json` request (`main.py:191-193`) | Yes | bundled |
| User `workspace_apps.json` | **Every** `/apps.json` request | Yes | `~/OpenBBUserData/workspace_apps.json` |
| `agents.json` (`--agents-json`) | **Every** `/agents.json` request | Yes | user-supplied |

### L.1. The credentials race

Because `UserSettings` is re-read on every request, the desktop UI can update
credentials and the next request will see them — **as long as the file write
is atomic**. The api-keys.md v1 already flags that the Rust write isn't
atomic (`api-keys.md:111`, `:301`). If the user updates credentials mid-request
to the REST server, they could see a torn read producing a Pydantic
`ValidationError` (caught by `command_runner.py:_execute_func` and returned as
500 with a corrupt-JSON error).

### L.2. The `Env()` non-reload

Setting `OPENBB_API_AUTH=true` in the `.env` requires a **server restart** —
the singleton's `_environ` is a frozen snapshot of `os.environ.copy()` at
init. The desktop UI must restart the backend after any `.env` edit. The
api-keys file allow-list at `:279` includes `.env` so users CAN edit it via
the UI's "Open File" action — but no server restart is triggered.

### L.3. The `widget_settings.json` non-reload

`main.py:75-84` reads it once at boot. Adding a widget to the exclude list
requires a server restart. Same for `--exclude` CLI flag.

---

## M. Tagged mixin — purpose

Three classes extend `Tagged` (which carries a single field `id: str = uuid7str`):

1. `OBBject` (`obbject.py:36`) — every response object gets a unique id.
2. `UserSettings` (`user_settings.py:15`) — id stored in
   `~/.openbb_platform/user_settings.json` (silently overwritten if user
   manually deletes the field; auto-regenerated on next load).
3. `SystemSettings` (`system_settings.py:22`) — id present but never
   serialised (read-only frozen model).

### M.1. Why UUID v7?

`uuid_extensions.uuid7str` (`pyproject.toml:20`: `uuid7 = "^0.1.0"`) generates
[UUID v7](https://datatracker.ietf.org/doc/html/draft-peabody-dispatch-new-uuid-format)
which is **time-ordered** (the first 48 bits are Unix milliseconds). Two
practical consequences:

1. **Lexicographic sort = chronological sort.** Logs/IDs sort naturally by
   creation time without needing a separate timestamp.
2. **Reduced entropy in the leading bytes.** Don't use it as a primary key
   shard input — the prefix is non-uniform.

### M.2. Implications for the TS port's correlation/logging

If the TS port wants compatible IDs (so that a log line on the TS side and a
log line on the Python side can be cross-referenced), use a UUID v7 library:
`uuidv7` npm package or `crypto.randomUUID()` IS NOT v7 (it's v4).

For desktop logs streaming (per `logs-streaming.md`), prefixing each log line
with the OBBject id gives free time-ordering across the renderer→IPC→Rust→file
hops.

### M.3. The id field is NOT for cache keys

`OBBject.id` is regenerated per response. It's not idempotent — calling the
same endpoint twice returns two different IDs. So clients should NOT use it
as a cache key. Use `(route, sorted_query_params)` instead.

---

## N. yfinance HTTP constraints — verified, plus other Python-only providers

v1 §port-strategy claims yfinance "can't be ported to TS". Verified by reading
the actual usage:

- `providers/yfinance/openbb_yfinance/utils/helpers.py:524`:
  > "yfinance>=0.2.66 manages its own curl_cffi sessions internally."
- `curl_cffi` is the Python binding to `curl-impersonate`, which mimics
  Chrome/Firefox TLS fingerprints to bypass Yahoo's bot detection. Yahoo
  Finance's anonymous endpoints additionally require a **cookie + crumb pair**
  obtained by scraping a Yahoo page first.
- The repo's recorded yfinance test cassettes confirm:
  `providers/yfinance/tests/test_yfinance_fetchers.py:78` mocks the `crumb`
  and `cookie` headers.

### N.1. TS-port path for yfinance

To port yfinance to TS one would need:
- A TLS impersonation library (Bun has one experimentally; Node would need
  `node-curl-impersonate` or run `curl` as a subprocess).
- The Yahoo cookie/crumb dance reimplemented (HTTP GET to
  `finance.yahoo.com/quote/AAPL`, scrape `crumb` from the response, save the
  `B` cookie, then call `query1.finance.yahoo.com/v8/finance/chart/AAPL?crumb=...`).
- Per-symbol error handling that the Python lib provides.

This is feasible but a substantial maintenance burden — Yahoo deliberately
breaks scrapers periodically.

### N.2. Other providers with non-API-only constraints

Searching the providers tree for `BeautifulSoup`/`read_html`/`read_excel`:

| Provider | Constraint |
|---|---|
| `multpl` | Scrapes HTML tables from multpl.com (`models/sp500_multiples.py`) |
| `federal_reserve` | `read_excel` for total factor productivity, HTML for FOMC documents (`utils/fomc_documents.py`) |
| `eia` | `read_excel` for `petroleum_status_report` |
| `tmx` | HTML scrapes from money.tmx.com (`utils/helpers.py`) |
| `sec` | Custom HTML→Markdown (`utils/html2markdown.py`), 13F-HR XML/HTML parsing (`utils/parse_13f.py`), SIC-code HTML scrape |
| `fred` | Calendar HTML scrape in `models/economic_calendar.py` |

Each requires a non-trivial parser. Pure-API providers (FMP, Polygon,
Intrinio, Tiingo, Alpha Vantage, Benzinga, FRED-data-only, …) port cleanly.

### N.3. Provider portability ranking (rough)

- **Easy** (REST + JSON, no auth tricks): `fmp`, `polygon`, `intrinio`,
  `tiingo`, `alpha_vantage`, `benzinga`, `nasdaq`, `tradier`, `imf`, `oecd`,
  `bls`, `fred` (data API only), `tradingeconomics`.
- **Medium** (REST + auth dance OR small HTML scrape): `sec` (most fetchers
  are EDGAR-API-based), `cboe`, `finra`, `cftc`, `wsj`.
- **Hard** (TLS impersonation OR major HTML/Excel scrape): `yfinance`,
  `multpl`, `tmx`, `seeking_alpha`, `stockgrid`.

Recommendation for Strategy A: port the Easy bucket only (already covers
~70% of OpenBB Workspace's data needs) and proxy the Hard bucket through a
keep-Python sidecar.

---

## O. `openbb-build` — what it does, where it lives

`openbb-build = openbb_core.build:main` (`core/pyproject.toml:31`).

### O.1. Behaviour

`core/openbb_core/build.py:18-83`:
1. Spawns a subprocess: `python -c "import openbb"`.
2. If the import emits `Building ...` lines on stdout (the static SDK
   generator at import time), assumes the build already happened — exit 0.
3. Otherwise, imports `openbb` directly in this process and calls
   `openbb.build()` to trigger `PackageBuilder.build()` at
   `app/static/package_builder.py:171-230`.
4. `PackageBuilder.build` takes a file lock at `~/.openbb_platform/.build.lock`
   (`package_builder.py:178-189`), runs `_clean → _save_modules →
   _save_reference_file → _save_package` then `_run_linters`.

### O.2. Where the static SDK lives

The "static SDK" is the `openbb` Python package itself — generated `.py` files
under `<site-packages>/openbb/package/`. They're auto-generated thin wrappers
for every command (e.g. `obb.equity.price.historical(symbol="AAPL")` →
HTTP-less direct call). `_save_package` writes one .py per top-level extension.

### O.3. When is it run?

- During `pip install openbb-platform-api` if `OPENBB_AUTO_BUILD=true` (default),
  the package's import-time hook builds the SDK on first import. The desktop
  install flow at `installation.md` runs `pip install openbb-platform-api`
  in the conda env, which pulls in `openbb-core`, which auto-builds.
- Manually by a user via `openbb-build` after installing a new provider/extension
  (e.g. `pip install openbb-charting && openbb-build`).
- The desktop's "Add Extension" UI (`InstallComponents.tsx:139` references
  `openbb-mcp-server` as one of the installable extensions) should
  trigger `openbb-build` after pip install — but this requires verification
  in `environments.md`.

### O.4. Relevance to the REST server

**None directly.** `openbb-api` does not call `openbb.build()` at startup —
it only imports `openbb_core`, not `openbb`. The static SDK is for **Python
SDK consumers** (`from openbb import obb`); the REST server reaches the
fetchers via `RegistryLoader` (entry-points), not via the generated wrappers.

### O.5. Implication for the TS port

`openbb-build` is part of the install/update flow, not the runtime serving
flow. A TS port doesn't need an equivalent. But: if the TS port wants to
call `openbb-api` programmatically without HTTP (rejected; the Python SDK
takes ~10s to import vs <100ms for HTTP), it'd need the static SDK — adding
back complexity.

---

## P. AnnotatedResult flow — when a fetcher returns one

`AnnotatedResult` (`provider/abstract/annotated_result.py:10-21`):

```python
class AnnotatedResult(BaseModel, Generic[T]):
    result: T | None
    metadata: dict | None
```

A `Fetcher.transform_data` MAY return `AnnotatedResult(result=..., metadata=...)`
instead of a plain list. `OBBject.from_query` (`obbject.py:364-383`):

```python
results = await query.execute()
if isinstance(results, AnnotatedResult):
    return cls(results=results.result, extra={"results_metadata": results.metadata})
return cls(results=results)
```

So the on-the-wire effect: `obbject.results` is the unwrapped list, and
`obbject.extra.results_metadata` contains the metadata dict. This is
distinct from `obbject.extra.metadata` (which is the per-call timing/route
metadata controlled by `preferences.metadata`).

### P.1. When do fetchers use it?

Searching providers reveals it's used by paginated/cursor-based fetchers
(e.g. SEC EDGAR endpoints that need to surface a `next_cursor` so the client
can keep fetching). Also by FRED endpoints for series metadata
(units, frequency, last_updated) that the table view wants but isn't part of
the row schema.

### P.2. Distinction from `chart`

`Chart` (`obbject.chart`) is a separate field (`obbject.py:55-58`) for
visualisation payload. Metadata via `AnnotatedResult` is for tabular row
context that doesn't fit in a row.

### P.3. Implication for the TS port

A consuming client should check both `obbject.extra.results_metadata`
(per-fetcher) and `obbject.extra.metadata` (per-call). They're independent
and may both be present.

---

## Q. UserSettings model duplication concern

The user asked about "TWO `InstallationState` types confusion". For the
Python side, the analogous question is whether `UserSettings` and `Credentials`
have duplicate definitions. The answer:

- `UserSettings` is defined ONCE: `core/openbb_core/app/model/user_settings.py:15-41`.
- `Credentials` is **dynamically generated**. At
  `core/openbb_core/app/model/credentials.py:172`,
  `_Credentials = CredentialsLoader().load()` runs at module import. The
  generated class has one Optional[OBBSecretStr] field per
  `<provider>_<credential>` pair declared by every installed provider's
  `Provider(credentials=[...])` (`abstract/provider.py:46-51`, which prefixes
  each cred name with `<provider_name>_`).
- `Credentials` (`credentials.py:175-208`) then subclasses `_Credentials`
  to add `model_post_init` env-var fallback and `__repr__`.

So there's no duplication — but the dynamic generation means **the model's
field set differs across installations**. The TS port can't ship a static
type for credentials; it must generate them at runtime from
`/coverage/providers` (DEV_MODE) or by a manual scan of the `widgets.json`
`source` arrays. Easier path: read the JSON file as `Record<string, string>`
in TS and skip type generation.

---

## R. Custom headers schema (mentioned in v1 §1c, undocumented schema)

`api_settings.custom_headers` (`api_settings.py:41-43`) is `dict[str, str] | None`
with the comment "Custom headers and respective default value". In `system_settings.json`:

```jsonc
{
  "api_settings": {
    "custom_headers": {
      "X-Tenant-Id": "default-tenant",
      "X-Request-Source": "openbb-workspace"
    }
  }
}
```

The wrapper at `commands.py:119-130` injects each as a hidden `Header(...)`
parameter on every command route. The header name is dash-replaced-with-
underscore as the Python identifier (`name.replace("-", "_")`). The user
sends them via HTTP request; the default kicks in if the client omits them.

### R.1. Impact on the TS port

If a deployment uses custom headers (e.g. for tenant routing in front of the
REST server), the desktop's invoke code must set them on every fetch — they
aren't sent by default. A TS-side fetch wrapper could read
`system_settings.api_settings.custom_headers` once at startup and inject
them.

---

## v2 → v1 corrections

After verifying everything in v1 against source:

1. **v1 §1b item 9** says `--agents-json` adds `GET /agents.json` "optionally".
   Correction: regardless of whether `--agents-json` is supplied, the server
   ALWAYS mounts `/agents.json`. Without the flag, it returns `{}` (`main.py:280-285`)
   OR aggregates additional router-mounted agents endpoints
   (`main.py:267-278`). So the endpoint always exists; the flag changes the
   response source.

2. **v1 §3b** says `/apps.json` filters "to only apps whose layout references
   known widget IDs." Confirmed and adds: it ALSO accepts widgets prefixed
   with `rich_note` regardless of presence in `widgets_json`
   (`main.py:228,239`). Useful for static-content app cards.

3. **v1 §4d** says credentials are filtered by `provider.credentials` (the
   list the provider declares). Correction: provider-declared credential
   names are **prefixed** with `<provider_name>_` at provider construction
   time (`abstract/provider.py:46-51`). So FMP's declared `["api_key"]`
   becomes the credential key `fmp_api_key` in `user_settings.credentials`,
   not `api_key`. The api-keys.md page schema reflects this correctly via
   the dynamic Credentials model.

4. **v1 §6** describes OBBject wire shape. Adds: `extra.results_metadata` is
   a separate field from `extra.metadata` — populated only when the fetcher
   returned an `AnnotatedResult`, not gated by `preferences.metadata`.
   Both can coexist on a single response.

5. **v1 §8a table** lists `OPENBB_AUTO_BUILD`. Clarification: this controls
   whether `import openbb` triggers the static SDK rebuild. **It is read
   only by the `openbb` package's `__init__.py`**, not by `openbb-core`,
   `openbb-platform-api`, or `openbb-mcp-server`. So setting
   `OPENBB_AUTO_BUILD=false` has zero effect on either REST or MCP server
   boot — they don't import `openbb`, only `openbb_core`.

6. **v1 §8b** says `~/.openbb_platform/widget_settings.json` is read by
   `main.py:75-84`. Adds: it's read **once at module import**, never reloaded.
   The on-disk file changes only take effect on server restart.

7. **v1 port-strategy** says "Strategy B" reuses the Python servers without
   modification. Adds: **two** Python servers must be lifecycled (`openbb-api`
   AND `openbb-mcp`), not just one. The default seed at
   `desktop/src-tauri/src/tauri_handlers/startup.rs:1461-1478` makes both
   mandatory for parity. The desktop's backends.tsx UI doesn't couple them,
   so the user can stop one without affecting the other — but the MCP server
   making outbound HTTP calls to the REST server creates an implicit
   dependency that the TS port should surface (or harden by making MCP
   process direct in-process calls when possible).

8. **v1 §1e** mentions `RegistryMap` walks `fetcher_dict`. Adds: the same
   `RegistryMap` instance is re-used by `ProviderInterface` (a SingletonMeta
   class), so importing both `openbb_platform_api.main` and
   `openbb_mcp_server.app.app` in the SAME process would NOT double the
   work. But the desktop runs them in separate processes — so the work IS
   doubled.

9. **v1 §2c** describes the `wrapper`. Adds: the
   `__authenticated_user_settings` default value is `UserSettings()` (which
   itself reloads from disk) — meaning even when auth is disabled, every
   request calls `UserSettings.__init__` which does
   `json.load(open(USER_SETTINGS_PATH))`. This is a **disk read per
   request** even with no real authentication. For a hot path, the TS port
   should cache the parsed settings with a `mtime` check.

10. **v1 §port-strategy A** says `openbb-charting` is Python-only. Adds:
    the wire payload `chart.content` is raw Plotly JSON — Plotly.js
    (`react-plotly.js`) can render it directly. So the **rendering** is
    portable; only the **chart construction** (calling matplotlib / Plotly
    Python to assemble the figure) is Python-only. For Strategy A, the TS
    port could expose the OBBject results directly to a TS-side Plotly.js
    component — no charting extension needed.
