# Newsfeed Spec Sheet

A blueprint for recreating the OpenBB newsfeed feature from scratch. Distills how the existing implementation in this repo composes a domain extension, standard models, and provider fetchers into two commands: `news.world` and `news.company`.

---

## 1. Purpose and Surface

Two end-user commands exposed under the `/news` namespace:

| Command | Description | Standard Model |
|---|---|---|
| `news.world` | Global / topical financial news, not tied to a single ticker. | `WorldNews` |
| `news.company` | News articles about one or more companies (ticker-driven). | `CompanyNews` |

Each command is reachable via three surfaces with no per-surface code:

- Python SDK: `obb.news.world(provider="benzinga", limit=20)` / `obb.news.company(symbol="AAPL", provider="fmp")`
- REST API: `GET /api/v1/news/world?...` / `GET /api/v1/news/company?symbol=AAPL&...`
- CLI / generated package: same names, generated from the same router metadata.

A single user call resolves to one provider chosen at request time (default or `provider="..."`).

---

## 2. Architecture (3 Layers)

```
 ┌──────────────────────────────────────────────────────┐
 │ Layer 1 — News Extension (domain)                    │
 │   openbb_platform/extensions/news/                   │
 │   • news_router.py defines world() + company()       │
 │   • Each command binds to a model name (WorldNews,   │
 │     CompanyNews) — NOT to a specific provider.       │
 └──────────────────────────────────────────────────────┘
                          │ model name
                          ▼
 ┌──────────────────────────────────────────────────────┐
 │ Layer 2 — Standard Models (contract)                 │
 │   openbb_platform/core/.../standard_models/          │
 │   • world_news.py:   WorldNewsQueryParams /          │
 │                      WorldNewsData                   │
 │   • company_news.py: CompanyNewsQueryParams /        │
 │                      CompanyNewsData                 │
 │   Define the shared query shape + response shape     │
 │   every provider must conform to.                    │
 └──────────────────────────────────────────────────────┘
                          │ subclassed by
                          ▼
 ┌──────────────────────────────────────────────────────┐
 │ Layer 3 — Provider Fetchers (implementation)         │
 │   openbb_platform/providers/<provider>/models/       │
 │   • Subclass standard query + data with extras.      │
 │   • Implement Fetcher with three stages:             │
 │       transform_query → aextract_data → transform_data│
 │   Providers in this repo:                            │
 │     Benzinga, FMP, Tiingo, Intrinio (world+company)  │
 │     Biztoc                              (world only) │
 │     YFinance, TMX                       (company only)│
 └──────────────────────────────────────────────────────┘
```

The router never imports a provider. Provider classes are discovered from `openbb_provider_extension` entry points and merged into the request schema by `ProviderInterface`.

---

## 3. Data Flow for a Single Call

```
obb.news.world(provider="benzinga", limit=20)
   │
   ▼
news_router.world() — bound to model="WorldNews"
   │   returns: await OBBject.from_query(Query(**locals()))
   ▼
Query.filter_extra_params()
   • Strips params the chosen provider doesn't accept
   ▼
QueryExecutor.fetch_data("WorldNews", "benzinga", params, credentials)
   • Looks up BenzingaWorldNewsFetcher from the registry
   ▼
BenzingaWorldNewsFetcher.fetch_data()
   ├── transform_query(params)    → BenzingaWorldNewsQueryParams
   ├── aextract_data(query, creds) → list[dict] from Benzinga REST API
   └── transform_data(query, data) → list[BenzingaWorldNewsData]
   ▼
OBBject(results=[...], provider="benzinga", warnings, extra)
```

The same path applies to `company` — only the model name and fetcher change.

---

## 4. File Layout to Recreate

```
openbb_platform/
├── extensions/news/
│   ├── pyproject.toml                       # registers openbb_core_extension: news
│   ├── openbb_news/
│   │   ├── __init__.py
│   │   └── news_router.py                   # @router.command(model="WorldNews"|"CompanyNews")
│   ├── integration/
│   │   ├── test_news_api.py
│   │   └── test_news_python.py
│   └── tests/
│
├── core/openbb_core/provider/standard_models/
│   ├── world_news.py                        # WorldNewsQueryParams + WorldNewsData
│   └── company_news.py                      # CompanyNewsQueryParams + CompanyNewsData
│
└── providers/
    ├── benzinga/openbb_benzinga/models/{world_news.py, company_news.py}
    ├── fmp/openbb_fmp/models/{world_news.py, company_news.py}
    ├── tiingo/openbb_tiingo/models/{world_news.py, company_news.py}
    ├── intrinio/openbb_intrinio/models/{world_news.py, company_news.py}
    ├── biztoc/openbb_biztoc/models/world_news.py
    ├── yfinance/openbb_yfinance/models/company_news.py
    └── tmx/openbb_tmx/models/company_news.py
```

A frontend example also exists at `examples/streamlit/news.py` — not required for the backend feature, but a useful reference for what consumers do with the response.

---

## 5. Component Specs

### 5.1 News Router

File: `openbb_platform/extensions/news/openbb_news/news_router.py`

Responsibilities:
- Create a `Router(prefix="", description="Financial market news data.")`.
- Declare two async commands `world()` and `company()`.
- Each command takes only the framework-supplied `CommandContext`, `ProviderChoices`, `StandardParams`, `ExtraParams` and returns `await OBBject.from_query(Query(**locals()))`.
- Each command is decorated with `@router.command(model=<NAME>, examples=[APIEx(...)])` where `<NAME>` is `"WorldNews"` or `"CompanyNews"`.

The body never selects a provider, never hits a network, never normalizes data.

### 5.2 Extension Registration

File: `openbb_platform/extensions/news/pyproject.toml`

Must export the router as a core extension entry point:

```toml
[tool.poetry.plugins."openbb_core_extension"]
news = "openbb_news.news_router:router"
```

This is what makes `obb.news.*` exist.

### 5.3 Standard Models

#### `WorldNews` — `standard_models/world_news.py`

`WorldNewsQueryParams(QueryParams)`:
| Field | Type | Default | Notes |
|---|---|---|---|
| `start_date` | `date \| None` | `None` (validator → 2 weeks ago) | `@field_validator(..., mode="before")` populates default |
| `end_date` | `date \| None` | `None` (validator → today) | Same |
| `limit` | `NonNegativeInt \| None` | `None` | Number of articles to return |

`WorldNewsData(Data)`:
| Field | Type | Required | Description |
|---|---|---|---|
| `date` | `datetime` | yes | Publication datetime |
| `title` | `str` | yes | Headline |
| `author` | `str \| None` | no | |
| `excerpt` | `str \| None` | no | Teaser / preview |
| `body` | `str \| None` | no | Full text if available |
| `images` | `Any \| None` | no | Provider-shaped image payload |
| `url` | `str \| None` | no | Link to source |

#### `CompanyNews` — `standard_models/company_news.py`

`CompanyNewsQueryParams(QueryParams)`:
| Field | Type | Default | Notes |
|---|---|---|---|
| `symbol` | `str \| None` | `None` | Validator uppercases; comma-separated multi-symbol allowed by providers that support it |
| `start_date` | `date \| None` | `None` | |
| `end_date` | `date \| None` | `None` | |
| `limit` | `NonNegativeInt \| None` | `None` | |

`CompanyNewsData(Data)`:
Same fields as `WorldNewsData`, with two changes:
- `url: str` — required (not optional).
- `symbols: str | None` — comma-separated tickers associated with the article.

These two model pairs are the entire public contract. Anything beyond them is provider-specific extension.

### 5.4 Provider Fetcher Pattern

Every provider model file follows the same three-part recipe.

1. **Query class** — subclasses the standard query, adds provider-only fields, declares param renaming with `__alias_dict__` (e.g. Benzinga renames `start_date` → `dateFrom`, `limit` → `pageSize`).
2. **Data class** — subclasses the standard data, adds provider-only fields (e.g. Benzinga adds `channels`, `stocks`, `tags`, `updated`, `id`), declares response renaming with `__alias_dict__` (e.g. `date` ← `created`, `excerpt` ← `teaser`).
3. **Fetcher class** — `Fetcher[QueryClass, list[DataClass]]` with three static methods:
   - `transform_query(params: dict) -> QueryClass` — usually just `QueryClass(**params)`.
   - `aextract_data(query, credentials, **kwargs) -> list[dict]` — async; reads the API key from `credentials`, builds the URL with `get_querystring(...)`, paginates with `asyncio.gather([amake_request(url) for url in urls])`, raises `EmptyDataError` if nothing returns.
   - `transform_data(query, data, **kwargs) -> list[DataClass]` — validates each dict via `DataClass.model_validate(item)`, applies any post-processing (sorting, dedupe by URL, etc.).

The fetcher class is registered in the provider's `__init__.py`/`pyproject.toml` so the discovery layer picks it up.

### 5.5 Provider Coverage Matrix

| Provider | World | Company | Auth Credential | Notable Provider-Only Params | Notable Provider-Only Data |
|---|:-:|:-:|---|---|---|
| Benzinga | yes | yes | `benzinga_api_key` | `display` (headline/abstract/full), `topics`, `channels`, `authors`, `isin`, `cusip`, `sort`, `order`, `updated_since`, `published_since` | `channels`, `stocks`, `tags`, `updated`, `id`, `updated_id` |
| FMP | yes | yes | `fmp_api_key` | `topic` (fmp_articles/general/press_releases/stocks/forex/crypto), `page` | `source`, `symbols` |
| Tiingo | yes | yes | `tiingo_token` | `offset`, `source` (comma-separated domains) | `symbols`, `article_id`, `site`, `tags`, `crawl_date` |
| Intrinio | yes | yes | `intrinio_api_key` | `source`, `sentiment`, `language`, `topic`, `word_count_*`, `business_relevance_*`, `is_spam` | `summary`, `topics`, `word_count`, `business_relevance`, `sentiment` + confidence, `language`, `spam`, nested `company` / `security` |
| Biztoc | yes | — | `biztoc_api_key` (RapidAPI) | `term` (search), `source` | `images` (list of dicts), `tags`, `score` |
| YFinance | — | yes | none | (none — built on yfinance package) | (uses standard fields) |
| TMX | — | yes | none | `page` | (uses standard fields; symbol normalization strips `.TO`/`.TSX`) |

### 5.6 Pagination & Limit Strategy

A common pattern across providers — recreate it the same way:

```python
pages = math.ceil((query.limit if query.limit else <default>) / <page_size>)
urls  = [f"{base_url}?{querystring}&page={p}&token={token}" for p in range(pages)]
await asyncio.gather(*[get_one(u) for u in urls])
# then sort and slice to query.limit
```

Default fallback caps differ per provider (e.g. Benzinga 2500, Tiingo 1000). This is the only place where the "limit" semantic crosses into network behavior.

### 5.7 Error Handling

- Empty response → raise `openbb_core.provider.utils.errors.EmptyDataError`.
- Auth failure → raise `openbb_core.provider.utils.errors.UnauthorizedError` (or let the provider's `response_callback` translate HTTP 401 to it).
- Any other recoverable issue → raise `openbb_core.app.model.abstract.error.OpenBBError`.

The router/runner converts these to the right HTTP status or SDK exception — don't catch them in the fetcher.

---

## 6. Recreating It Step-by-Step

1. **Standard models.** Add `world_news.py` and `company_news.py` under `core/.../provider/standard_models/`. Define `*QueryParams` and `*Data` with the fields in §5.3. Add `start_date` / `end_date` validators that default to "2 weeks ago" → "today".
2. **Extension scaffold.** Create `extensions/news/` with `pyproject.toml`, `openbb_news/__init__.py`, and `openbb_news/news_router.py`. Register the entry point `news = "openbb_news.news_router:router"` under `openbb_core_extension`.
3. **Router.** In `news_router.py`, instantiate `router = Router(...)` and add `world()` and `company()` decorated with `@router.command(model="WorldNews"|"CompanyNews")`. Each body is one line: `return await OBBject.from_query(Query(**locals()))`. Attach `examples=[APIEx(...)]` for docs/tests.
4. **First provider (Benzinga or FMP).** Create `providers/<name>/openbb_<name>/models/{world_news.py, company_news.py}`. Subclass the standard query + data, add `__alias_dict__` mappings, implement the three-method Fetcher. Register the fetcher under `openbb_provider_extension` in that provider's `pyproject.toml`.
5. **End-to-end smoke test.** From a Python shell: `from openbb import obb; obb.news.world(provider="<name>", limit=5).results`. Confirm the response is a list of `WorldNewsData` instances with `date`, `title`, `url`.
6. **Add remaining providers.** Each one is the same recipe: query subclass + data subclass + fetcher subclass. They do not touch the router or each other.
7. **Integration tests.** Mirror `extensions/news/integration/test_news_api.py` and `test_news_python.py` — parametrize across providers, exercise date filters, limits, and provider-specific extras.
8. **Optional UI.** A Streamlit example like `examples/streamlit/news.py` is a useful consumer reference but is not required for the backend.

---

## 7. Invariants Worth Preserving

- **Router knows nothing about providers.** The only knob the router exposes is the model name. Adding a new provider must require zero changes to `news_router.py`.
- **Two models, not one.** World and company news intentionally have separate models because `CompanyNews` requires `url` and carries `symbols`. Don't collapse them.
- **Aliases live on the provider model, not the standard model.** Provider-side renames (`pageSize`, `dateFrom`, `created`, `teaser`) are isolated via `__alias_dict__` so the public contract stays clean.
- **Fetcher is the only place that talks to the network.** Routers, models, and the framework do not. This keeps the network surface auditable and mockable for tests.
- **Default dates are populated by validators on the standard model**, so every provider gets the same defaults without re-implementing them.

---

## 8. Reference Files in This Repo

| Concern | File |
|---|---|
| Router | `openbb_platform/extensions/news/openbb_news/news_router.py` |
| Extension registration | `openbb_platform/extensions/news/pyproject.toml` |
| World standard model | `openbb_platform/core/openbb_core/provider/standard_models/world_news.py` |
| Company standard model | `openbb_platform/core/openbb_core/provider/standard_models/company_news.py` |
| Provider example (richest) | `openbb_platform/providers/benzinga/openbb_benzinga/models/world_news.py` |
| Provider example (company) | `openbb_platform/providers/benzinga/openbb_benzinga/models/company_news.py` |
| Integration tests (API) | `openbb_platform/extensions/news/integration/test_news_api.py` |
| Integration tests (SDK) | `openbb_platform/extensions/news/integration/test_news_python.py` |
| Consumer UI reference | `examples/streamlit/news.py` |
