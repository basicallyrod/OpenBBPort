# 01 — Architecture Map

## Executive summary

OpenBBPort is a Python monorepo centered on the OpenBB Platform. The architecture is not a conventional static SDK. It is a runtime-composed platform where:

1. Core runtime code provides application bootstrapping, command execution, provider discovery, settings, logging, and API exposure.
2. Domain extensions define user-facing command routers such as `equity`, `crypto`, `economy`, and related nested routers.
3. Provider extensions register concrete fetchers that know how to transform inputs, call external data sources, and normalize provider-specific responses into standard OpenBB data models.
4. The static SDK surface is generated from installed routers and providers.
5. REST endpoints are generated from the same router/command map, then wrapped to inject user settings, default command values, custom headers, auth, charting, and output validation.

The core design is powerful because one command can support multiple providers without duplicating route code. The cost is high runtime indirection: command signatures, provider choices, schemas, SDK methods, and REST behavior are all assembled from metadata and entry points.

## Repository-level topology

The inspected source indicates these major project regions:

```text
openbb_platform/
  pyproject.toml
  dev_install.py
  core/
    pyproject.toml
    openbb/
      __init__.py
    openbb_core/
      api/
      app/
      provider/
  extensions/
    equity/
    platform_api/
    ... other router/domain extensions
  providers/
    yfinance/
    ... other provider integrations
```

The root package `openbb_platform/pyproject.toml` declares the public `openbb` package and aggregates core runtime packages, router/domain extensions, official data providers, optional community providers, and optional output/analysis extensions.

The core package `openbb_platform/core/pyproject.toml` declares `openbb-core`, which owns the core runtime and depends on FastAPI, Pydantic, pandas, requests, aiohttp, importlib metadata, Uvicorn, and related infrastructure.

## Primary architectural layers

### 1. Public SDK entrypoint

Path: `openbb_platform/core/openbb/__init__.py`

This file exposes the user-facing import path:

```python
from openbb import obb
```

Key responsibilities:

- Defines `build(...)` for regenerating static SDK modules.
- Runs `PackageBuilder(...).auto_build()` during import.
- Loads reference metadata through `ReferenceLoader`.
- Tries to import generated extension classes from `openbb.package.__extensions__`.
- Creates the final SDK app object through `create_app(...)`.
- Falls back to a base app when generated extensions are unavailable.

Architectural implication: importing `openbb` is not passive. It can trigger build/reference behavior and dynamically construct the SDK object from generated package artifacts.

### 2. Application shell

Path: `openbb_platform/core/openbb_core/app/static/app_factory.py`

Core objects:

- `BaseApp`
- `create_app(...)`

`BaseApp` wires the user-facing runtime object to:

- `CommandRunner`
- `UserSettings`
- `SystemSettings`
- coverage metadata
- reference metadata

`create_app(...)` dynamically creates a class that inherits from `BaseApp` and optionally from generated extension classes. This creates the final `obb` object.

Architectural implication: the SDK surface is mixed into the app dynamically rather than being imported as a fixed hand-written SDK class.

### 3. Router and command declaration layer

Path: `openbb_platform/core/openbb_core/app/router.py`

Core objects:

- `Router`
- `SignatureInspector`
- `CommandMap`
- `RouterLoader`

Domain extensions define `Router` instances and decorate command functions with `@router.command(model=...)`. The router:

- wraps FastAPI `APIRouter`
- stores nested routers
- assigns route metadata
- associates route functions with provider-backed model names
- injects provider, standard param, and extra param dependencies when a command references a model
- constructs OpenAPI response models and operation IDs

`RouterLoader.from_extensions()` discovers core extension entry points and includes each extension router under a top-level prefix.

Architectural implication: routes are not centralized. They are registered by installed extension packages through entry points, then unified into one runtime router.

### 4. Extension discovery

Path: `openbb_platform/core/openbb_core/app/extension_loader.py`

Core objects:

- `OpenBBGroups`
- `ExtensionLoader`

The platform recognizes three entry-point groups:

- `openbb_core_extension`
- `openbb_provider_extension`
- `openbb_obbject_extension`

The loader resolves:

- domain/router extension objects
- provider extension objects
- OBBject accessor/output extensions
- command-output callback extensions

Architectural implication: installing packages changes the platform's available commands and providers without editing central source files.

### 5. Provider abstraction and registry

Paths:

- `openbb_platform/core/openbb_core/provider/abstract/provider.py`
- `openbb_platform/core/openbb_core/provider/abstract/fetcher.py`
- `openbb_platform/core/openbb_core/provider/registry.py`
- `openbb_platform/core/openbb_core/provider/registry_map.py`
- `openbb_platform/core/openbb_core/provider/query_executor.py`

Core objects:

- `Provider`
- `Fetcher`
- `Registry`
- `RegistryLoader`
- `RegistryMap`
- `QueryExecutor`

A provider package contributes a `Provider` instance containing provider metadata and a `fetcher_dict` mapping model names to `Fetcher` classes.

A fetcher implements the provider contract:

1. `transform_query(params)`
2. `extract_data(...)` or `aextract_data(...)`
3. `transform_data(query, data, **kwargs)`

The query executor chooses the provider, locates the fetcher for the requested standard model, filters credentials, and executes `fetch_data`.

Architectural implication: provider integrations are intentionally normalized behind a transform/extract/transform pipeline.

### 6. Provider interface and dynamic schema composition

Path: `openbb_platform/core/openbb_core/app/provider_interface.py`

Core object:

- `ProviderInterface`

This is one of the most central pieces of the system. It builds runtime artifacts from provider registry metadata:

- provider choices per model
- standard parameter dataclasses per model
- extra parameter dataclasses per model
- standard data models
- extra data models
- merged return schemas
- annotated `OBBject[...]` return models

It also merges provider-specific descriptions, choices, `Literal` values, schema metadata, and fields across providers.

Architectural implication: a single command model like `EquityHistorical` can support multiple provider-specific parameter sets while presenting one command/API surface.

### 7. Query object

Path: `openbb_platform/core/openbb_core/app/query.py`

Core object:

- `Query`

The command router functions are intentionally thin. A typical command returns:

```python
return await OBBject.from_query(Query(**locals()))
```

`Query` extracts:

- command context
- selected provider
- standard parameters
- provider-specific extra parameters
- credentials and preferences from user settings

It filters unsupported extra parameters by provider and delegates execution to `QueryExecutor`.

Architectural implication: route functions are declarative. The real execution logic sits behind `Query`, `ProviderInterface`, and `QueryExecutor`.

### 8. Command execution and result handling

Path: `openbb_platform/core/openbb_core/app/command_runner.py`

Core objects:

- `ExecutionContext`
- `ParametersBuilder`
- `StaticCommandRunner`
- `CommandRunner`

This layer handles:

- merging positional and keyword args
- injecting command context
- Pydantic validation/coercion
- charting invocation
- warning capture
- logging
- metadata injection
- callback execution for OBBject extensions
- conversion between sync and async execution

Architectural implication: command functions are not called directly by the SDK or API. They pass through a validation, metadata, warning, logging, and output-extension pipeline.

### 9. Result object

Path: `openbb_platform/core/openbb_core/app/model/obbject.py`

Core object:

- `OBBject[T]`

`OBBject` is the platform's canonical result wrapper. It contains:

- `results`
- `provider`
- `warnings`
- `chart`
- `extra`
- private route/parameter metadata

It also provides conversion utilities:

- `to_df(...)`
- `to_dataframe(...)`
- `to_dict(...)`
- `to_llm(...)`
- `to_polars(...)`
- `to_numpy(...)`
- `show(...)`

Architectural implication: commands return a transportable object that is useful in Python notebooks, REST responses, charting contexts, and LLM-oriented consumers.

### 10. REST API surface

Paths:

- `openbb_platform/core/openbb_core/api/rest_api.py`
- `openbb_platform/core/openbb_core/api/router/commands.py`

The REST API creates a FastAPI app, adds CORS, includes command/coverage/system/auth routers depending on mode, and builds command endpoints from extension routes.

`api/router/commands.py` wraps each route endpoint to:

- rebuild endpoint signatures for API usage
- inject charting switches when available
- inject configured custom headers
- inject authenticated user settings when auth is enabled
- apply command defaults from user settings
- pass dependencies into commands where appropriate
- validate or serialize `OBBject` output

Architectural implication: the REST API is generated from the same command/router metadata as the SDK, but it has API-specific signature, auth, defaulting, and serialization behavior.

### 11. Static SDK package generation

Path: `openbb_platform/core/openbb_core/app/static/package_builder.py`

Core objects:

- `PackageBuilder`
- `ModuleBuilder`
- `ImportDefinition`
- `ClassDefinition`
- `MethodDefinition`

This module generates importable SDK modules and reference metadata from the router map. It handles:

- build locking
- cleanup of generated assets/package directories
- extension map recording
- module generation per route path
- function signatures
- type imports
- parameter formatting
- provider argument handling
- docstring generation
- dependency injection considerations
- chart parameter insertion
- deprecation wrappers
- filtering inputs before `_run(...)`

Architectural implication: this is a critical complexity hotspot. It maps dynamic FastAPI/router/provider metadata back into static-looking Python methods.

## Representative domain extension: equity

Paths:

- `openbb_platform/extensions/equity/pyproject.toml`
- `openbb_platform/extensions/equity/openbb_equity/equity_router.py`
- `openbb_platform/extensions/equity/openbb_equity/price/price_router.py`

The equity extension registers as an `openbb_core_extension` entry point named `equity`. The main equity router includes nested routers such as calendar, compare, darkpool, discovery, estimates, fundamental, ownership, price, and shorts.

The price router shows the core command pattern:

- commands are small functions
- commands declare a model name such as `EquityHistorical`
- provider-aware dependencies are injected by the core router machinery
- command execution delegates to `Query` and `OBBject.from_query(...)`

## Representative provider extension: yfinance

Paths:

- `openbb_platform/providers/yfinance/pyproject.toml`
- `openbb_platform/providers/yfinance/openbb_yfinance/__init__.py`
- `openbb_platform/providers/yfinance/openbb_yfinance/models/equity_historical.py`

The yfinance package registers as an `openbb_provider_extension` entry point named `yfinance`. Its provider instance maps many standard model names to fetchers, including `EquityHistorical`.

The `YFinanceEquityHistoricalFetcher` demonstrates the provider pipeline:

1. Extend a standard query model with yfinance-specific fields.
2. Extend a standard data model with provider-specific output fields.
3. Set default dates and provider-specific intervals in `transform_query`.
4. Call the provider library in `extract_data`.
5. Validate each normalized record into `YFinanceEquityHistoricalData` in `transform_data`.

## Core architectural pattern

The effective runtime chain is:

```text
Installed package entry points
  -> ExtensionLoader
  -> RouterLoader / RegistryLoader
  -> Router + ProviderInterface
  -> CommandMap
  -> SDK-generated methods or REST endpoints
  -> CommandRunner
  -> Query
  -> QueryExecutor
  -> Provider Fetcher
  -> OBBject
```

## Architectural strengths

1. **Extensible by installation**: providers and command domains can be added as packages.
2. **Provider normalization**: standard models create a consistent command surface across heterogeneous data sources.
3. **Single command plane**: Python SDK and REST API share route metadata.
4. **Strong user-facing wrapper**: `OBBject` provides data conversion, warning storage, metadata, chart display, and provider attribution.
5. **Generated SDK ergonomics**: dynamic provider metadata becomes discoverable Python methods.

## Architectural costs

1. **High dynamic complexity**: much of the runtime behavior is discovered or generated, not obvious from static imports.
2. **Schema merge risk**: provider-specific fields, choices, aliases, and descriptions are merged dynamically and can collide.
3. **Import-time behavior**: importing `openbb` can trigger auto-build/reference loading behavior.
4. **Large generation module**: `package_builder.py` carries many responsibilities and is difficult to reason about locally.
5. **Multiple execution surfaces**: SDK, REST, charting, callbacks, auth, defaults, warnings, and metadata can diverge without broad integration tests.

## Practical mental model

Treat OpenBBPort as a plugin-composed financial data operating layer:

- **Routers** describe what users can ask for.
- **Standard models** define the common contract.
- **Providers** define where the data comes from.
- **Fetchers** implement provider-specific data acquisition and normalization.
- **ProviderInterface** reconciles every provider's capabilities into runtime schemas.
- **CommandRunner** executes commands safely and attaches metadata.
- **PackageBuilder** turns dynamic routes into a Python SDK surface.
- **REST wrappers** turn the same routes into HTTP endpoints.

The key engineering principle is to protect the provider/model/router contract. Most regressions are likely to appear when a change seems local but actually affects generated signatures, API schemas, provider parameter filtering, result serialization, or callback/chart behavior.