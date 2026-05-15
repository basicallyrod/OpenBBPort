# 03 — Extension and Provider Model

## Purpose

This file explains the architecture that lets OpenBBPort compose domain commands and data providers dynamically. The core idea is that command domains and providers are independent packages connected by shared standard model names.

## Core concept

OpenBBPort separates **what the user asks for** from **where the data comes from**.

```text
Domain extension
  declares command route and model name

Standard model
  declares shared query/data contract

Provider extension
  declares provider-specific fetcher for that model

Core runtime
  merges provider capabilities and executes selected fetcher
```

Example:

```text
Command: /equity/price/historical
Model: EquityHistorical
Provider: yfinance
Fetcher: YFinanceEquityHistoricalFetcher
```

The command route does not call yfinance directly. It declares `model="EquityHistorical"`; the runtime resolves which provider implementation should satisfy that model.

## Entry-point groups

Path: `openbb_platform/core/openbb_core/app/extension_loader.py`

OpenBBPort recognizes these package entry-point groups:

| Group | Purpose |
|---|---|
| `openbb_core_extension` | Domain routers and command trees. |
| `openbb_provider_extension` | Provider packages and fetcher registries. |
| `openbb_obbject_extension` | Output/accessor extensions for `OBBject`. |

This means installation state directly affects runtime capabilities. Adding a provider package can add supported providers for existing commands. Adding a domain extension can add new command routes.

## Domain extension structure

Representative package: `openbb_platform/extensions/equity`

The equity package declares itself as a core extension in its `pyproject.toml`:

```text
[tool.poetry.plugins."openbb_core_extension"]
equity = "openbb_equity.equity_router:router"
```

The main equity router:

- creates `Router(prefix="", description="Equity market data.")`
- includes nested routers such as `calendar`, `compare`, `darkpool`, `discovery`, `estimates`, `fundamental`, `ownership`, `price`, and `shorts`
- declares model-bound command functions

Nested route example: `openbb_platform/extensions/equity/openbb_equity/price/price_router.py`

```text
router = Router(prefix="/price")
```

Commands include:

- `quote` with model `EquityQuote`
- `nbbo` with model `EquityNBBO`
- `historical` with model `EquityHistorical`
- `performance` with model `PricePerformance`

Each model-bound command uses the same parameter pattern:

```text
cc: CommandContext
provider_choices: ProviderChoices
standard_params: StandardParams
extra_params: ExtraParams
```

The body delegates to:

```text
OBBject.from_query(Query(**locals()))
```

## Router registration model

Path: `openbb_platform/core/openbb_core/app/router.py`

When a command is decorated with `@router.command(model="...")`, `SignatureInspector.complete(...)` performs provider-aware enhancement:

1. verifies the model exists in `ProviderInterface().models`
2. validates that the command signature includes expected provider/standard/extra params
3. injects provider choices as FastAPI dependencies
4. injects standard params as FastAPI dependencies
5. injects extra params as FastAPI dependencies
6. replaces the return annotation with the generated model-specific `OBBject[...]` return type

This lets route functions stay generic while still exposing provider-specific schemas through FastAPI and the generated SDK.

## Command model names are the binding key

The binding key is the standard model name.

```text
Router command model="EquityHistorical"
Provider.fetcher_dict["EquityHistorical"] = SomeFetcher
ProviderInterface.params["EquityHistorical"] = generated param dataclasses
ProviderInterface.return_annotations["EquityHistorical"] = generated OBBject type
```

If these names drift, the system breaks at route loading, provider execution, or schema generation.

## Provider extension structure

Representative package: `openbb_platform/providers/yfinance`

The yfinance package declares itself as a provider extension:

```text
[tool.poetry.plugins."openbb_provider_extension"]
yfinance = "openbb_yfinance:yfinance_provider"
```

The provider module creates a `Provider` instance:

```text
yfinance_provider = Provider(
    name="yfinance",
    website="https://finance.yahoo.com",
    description="...",
    fetcher_dict={
        "EquityHistorical": YFinanceEquityHistoricalFetcher,
        ...
    },
    repr_name="Yahoo Finance",
)
```

The provider instance contributes:

- provider name
- provider website
- provider description
- credential requirements
- model-to-fetcher mapping
- optional representation name
- optional setup instructions

## Provider registration flow

Paths:

- `openbb_platform/core/openbb_core/app/extension_loader.py`
- `openbb_platform/core/openbb_core/provider/registry.py`

Flow:

```text
importlib entry points
  -> ExtensionLoader.provider_objects
  -> RegistryLoader.from_extensions()
  -> Registry.include_provider(...)
  -> registry.providers[provider.name.lower()] = provider
```

The registry is then consumed by:

- `RegistryMap`
- `ProviderInterface`
- `QueryExecutor`

## Fetcher contract

Path: `openbb_platform/core/openbb_core/provider/abstract/fetcher.py`

A fetcher has three logical stages:

```text
transform_query(params)
  -> extract_data(query, credentials, **kwargs)
  -> transform_data(query, data, **kwargs)
```

`Fetcher.fetch_data(...)` orchestrates the sequence.

### Required class behavior

A concrete fetcher must:

1. inherit from `Fetcher[ProviderQueryParams, ReturnType]`
2. implement `transform_query(...)`
3. implement either `extract_data(...)` or `aextract_data(...)`
4. implement `transform_data(...)`
5. return data matching the declared return type

If a subclass defines `aextract_data`, the base class assigns it to `extract_data` during subclass initialization. If neither extraction method is implemented, subclass initialization raises `NotImplementedError`.

## Standard model role

Representative path: `openbb_platform/core/openbb_core/provider/standard_models/equity_historical.py`

Standard models define provider-neutral contracts.

For `EquityHistorical`, the standard query model includes:

- `symbol`
- `start_date`
- `end_date`

The standard data model includes:

- `date`
- `open`
- `high`
- `low`
- `close`
- `volume`
- `vwap`

Provider-specific query/data models should extend these standard models rather than replace them.

## Provider-specific model role

Representative path: `openbb_platform/providers/yfinance/openbb_yfinance/models/equity_historical.py`

The yfinance implementation extends the standard `EquityHistorical` models.

### Query extension

`YFinanceEquityHistoricalQueryParams` extends `EquityHistoricalQueryParams` with:

- `interval`
- `extended_hours`
- `include_actions`
- `adjustment`
- private yfinance execution attributes such as progress, period, rounding, repair, group_by

It also contributes JSON schema metadata such as supported intervals and multi-symbol support.

### Data extension

`YFinanceEquityHistoricalData` extends `EquityHistoricalData` with:

- `split_ratio`
- `dividend`
- alias mapping for provider column names

### Fetcher implementation

`YFinanceEquityHistoricalFetcher`:

1. defaults `start_date` to one year before current date when missing
2. defaults `end_date` to current date when missing
3. calls the yfinance helper with provider-specific parameters
4. raises empty-data errors when download output is empty
5. warns when requested multi-symbol data is missing for a symbol
6. validates each output row into the yfinance data model

## RegistryMap: model introspection layer

Path: `openbb_platform/core/openbb_core/provider/registry_map.py`

`RegistryMap` converts installed provider fetchers into metadata maps.

It extracts from each fetcher:

- query params type
- data type
- return type
- standard fields
- provider-specific extra fields
- provider JSON schema extras
- credentials
- available providers
- available model names

The critical method is `_get_maps(...)`, which builds:

```text
standard_extra[model_name]["openbb"]["QueryParams"]
standard_extra[model_name]["openbb"]["Data"]
standard_extra[model_name][provider]["QueryParams"]
standard_extra[model_name][provider]["Data"]

original_models[model_name][provider]["query"]
original_models[model_name][provider]["data"]
original_models[model_name][provider]["results_type"]
```

The system identifies standard fields by checking whether a model class comes from the standard models folder. Provider-specific fields come from provider packages.

## ProviderInterface: schema synthesis layer

Path: `openbb_platform/core/openbb_core/app/provider_interface.py`

`ProviderInterface` consumes `RegistryMap` and creates runtime artifacts.

### Generated provider choices

For each model, it creates a dataclass like:

```text
class EquityHistorical(ProviderChoices):
    provider: Literal["fmp", "intrinio", "yfinance", ...]
```

If only one provider supports a model, that provider becomes the default.

### Generated params

For each model, it creates:

```text
params[model]["standard"]
params[model]["extra"]
```

Standard params represent provider-neutral fields. Extra params represent provider-specific fields merged across providers.

### Generated data models

For each model, it creates:

```text
data[model]["standard"]
data[model]["extra"]
return_schema[model]
return_annotations[model]
```

`return_schema` merges standard and extra data fields into a response schema. `return_annotations` builds provider-discriminated `OBBject[...]` result annotations.

## Parameter merging behavior

The provider interface performs several important merges:

### Description merge

If two providers expose similar field descriptions, it keeps a concise description and appends provider attribution.

If descriptions differ materially, it concatenates them.

### JSON schema extra merge

If both provider fields include compatible list metadata, lists are merged. Remaining metadata is added.

### Literal choices

If a provider-specific field uses `Literal[...]` and no explicit choices are declared, the system automatically derives choices from the annotation.

### Extra params

Provider-only query fields become extra params. If several providers define the same field name, the field is merged into one extra param with provider-specific metadata.

## Query filtering and provider safety

Path: `openbb_platform/core/openbb_core/app/query.py`

Extra params are filtered at query execution time.

This matters because generated commands may expose one merged extra-param surface for several providers. A user can pass an extra param that only one provider supports. `Query.filter_extra_params(...)` ensures unsupported non-default extra params produce a warning rather than being sent blindly to the selected fetcher.

## Static SDK generation and provider metadata

Path: `openbb_platform/core/openbb_core/app/static/package_builder.py`

The generated SDK surface uses provider metadata to shape:

- command method signatures
- provider parameter docs
- provider default priority descriptions
- choices in parameter descriptions
- multiple-item support notes
- `kwargs` for extra params
- route-specific class/property hierarchy

This is why provider schema changes can affect generated source files even when no router code changes.

## API generation and provider metadata

Path: `openbb_platform/core/openbb_core/api/router/commands.py`

The REST API uses the same router/provider metadata but wraps it differently:

- FastAPI dependencies expose standard and extra params
- provider choices become request parameters
- defaults are resolved from user settings
- chart/auth/custom-header fields can be injected
- output validation uses generated model-specific response schemas unless disabled

## Strengths of the model

1. **Provider independence**
   - New providers can add support for existing commands by mapping standard model names to fetchers.

2. **Domain independence**
   - Domain extensions can add new command trees without changing the central app.

3. **Shared standards**
   - Standard models make provider outputs comparable.

4. **Schema reuse**
   - The same metadata drives SDK signatures, REST schemas, provider selection, and result validation.

5. **Flexible output extension**
   - OBBject accessors and command-output callbacks can add post-processing behaviors without modifying command functions.

## Costs of the model

1. **Name coupling**
   - A plain string model name binds routers, providers, schemas, return annotations, and generated SDK methods.

2. **Dynamic failure modes**
   - Many errors appear at route loading, import/build time, or runtime rather than from static type checking.

3. **Schema collisions**
   - Provider-specific fields with the same name but different semantics can be merged into confusing command params.

4. **Generated-code fragility**
   - Small annotation or dependency changes can alter generated signatures.

5. **Testing breadth requirement**
   - Provider changes need tests beyond provider fetchers because the provider affects API schemas and SDK generation.

## Provider implementation checklist

When adding or changing a provider fetcher, verify:

- The fetcher is listed in the provider's `fetcher_dict` under the exact standard model name.
- The query params model extends the correct standard query model.
- The data model extends the correct standard data model.
- The fetcher generic type parameters are accurate.
- `transform_query` does not mutate caller-owned input unexpectedly.
- `extract_data` or `aextract_data` returns raw provider output, not already-normalized OpenBB models.
- `transform_data` returns the declared return type.
- Provider-specific fields include descriptions, defaults, and choices where possible.
- Credentials are declared at provider level if required.
- Missing credentials and empty data fail with meaningful OpenBB errors.
- Multi-symbol behavior is explicit and tested if supported.
- Output records validate through Pydantic models.

## Domain extension checklist

When adding or changing a domain command, verify:

- The route is registered through a `Router` instance.
- The model name exists in `ProviderInterface().models` after provider installation.
- The command signature includes `cc`, `provider_choices`, `standard_params`, and `extra_params` for model-bound commands.
- The command returns `OBBject` or explicitly opts out of validation when appropriate.
- Examples reference providers actually supporting the model.
- Nested routers are included with the intended prefixes.
- Generated SDK path matches the intended route path.
- REST route appears with expected provider and parameter schema.

## Recommended regression targets

### ProviderInterface snapshots

Snapshot for representative models:

- provider choices
- standard params
- extra params
- return schema fields
- return annotation names

### Route/provider binding

Assert every `@router.command(model=...)` has at least one installed provider fetcher unless intentionally providerless.

### Provider fetcher contracts

For each fetcher:

- query transform validates defaults
- raw extraction can be mocked
- transformed output validates as provider data model
- fetcher return type matches declaration

### SDK generation snapshots

For a representative route tree, snapshot generated methods and signatures.

### REST schema snapshots

For the same route tree, snapshot OpenAPI parameter and response schemas.

## Bottom line

The extension/provider model is the core of OpenBBPort. It gives the platform its composability, but it also means simple-looking changes can ripple through entry-point loading, provider maps, generated dataclasses, command signatures, SDK source generation, REST schemas, and `OBBject` serialization.

For any meaningful refactor, protect this contract first:

```text
Router model name
  == Provider fetcher_dict key
  == ProviderInterface model key
  == Query standard params class name
  == QueryExecutor model_name
```

That equality is the backbone of the system.