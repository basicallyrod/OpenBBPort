# 05 — Design Decisions

## Purpose

The previous four files describe **what** OpenBBPort is and **how** it executes. This file answers **why** the major architectural choices were made and **what each one offers compared to plausible alternatives**.

The audience is maintainers and coding agents who need to change the system without breaking its contracts. Each section names a decision, gives the problem it addresses, lists realistic alternatives that were not chosen, and states the costs the choice carries.

## Reading order

- For topology and layer boundaries, see `01_architecture_map.md`.
- For the SDK and REST execution paths, see `02_execution_flow.md`.
- For the entry-point/provider model and standard-model contract, see `03_extension_provider_model.md`.
- For risks, refactor seams, and tests, see `04_risk_refactor_notes.md`.
- This file ties their "strengths" and "costs" sections back to specific design choices.

## Decisions

### 1. Plugin architecture via Python entry points

**Decision.** Domain commands, data providers, and output extensions are discovered through `importlib.metadata` entry points rather than statically imported.

**Why.** OpenBBPort needs to ship a small `openbb-core` runtime and let users (and the OpenBB team) add data providers and command domains by installing packages. Hardcoded imports would put every supported integration into one repo and force every install to pay the cost of every dependency. Entry points let installation state determine runtime capabilities — see `openbb_platform/core/openbb_core/app/extension_loader.py` and the `[tool.poetry.plugins]` blocks in each provider/extension `pyproject.toml`.

**Alternatives.**
- Hardcoded imports in core. Eliminates dynamic discovery but couples core to every provider's dependency surface and prevents third-party providers without a fork.
- Namespace packages alone. They give you import paths but no metadata for grouping or for listing what is installed.
- Plugin manager library (pluggy, stevedore). Adds a hook-spec model and another dependency for a problem that entry points already solve cleanly.

**Tradeoffs.**
- Errors move from import time to discovery/registration time, which is harder to debug.
- A malformed provider package can perturb registry maps for unrelated providers (`registry_map.py`).
- IDE "go to definition" cannot follow a route to its provider without runtime help.

---

### 2. Three entry-point groups, not one

**Decision.** Plugins are split into `openbb_core_extension` (routers), `openbb_provider_extension` (fetchers), and `openbb_obbject_extension` (output accessors and callbacks).

**Why.** Each group has a different shape and different loading contract. Routers are imported and mounted; providers are registered into the registry; OBBject extensions attach accessors or callbacks. Putting them on separate axes lets the loader fail loudly when the wrong kind of object is registered, and lets one type of plugin be installed without the others. See `OpenBBGroups` in `extension_loader.py`.

**Alternatives.**
- One generic `openbb_extension` group with a `kind` discriminator. Forces a runtime type check on every plugin and conflates three different contracts.
- Separate sub-registries with no grouping at all. Loses the symmetry that lets `ExtensionLoader` enumerate everything once.

**Tradeoffs.**
- Plugin authors must remember which group they belong to.
- Three group names must stay synchronized between the loader, packaging templates, and provider/cookiecutter generators.

---

### 3. Standard model name string as the universal binding key

**Decision.** A bare string like `"EquityHistorical"` binds router commands, provider `fetcher_dict` entries, `ProviderInterface` keys, generated params dataclass names, and `QueryExecutor` lookups.

**Why.** Routers, providers, generated SDK code, REST schemas, and the registry live in different processes and packages. A serializable, stable identifier is the only thing they can all agree on without forcing import-time coupling. A class handle would force providers to import core's standard-model classes (already true for data shape, but not for naming).

**Alternatives.**
- A typed handle (the standard model class itself). Better for IDEs, but it means every provider must import core symbols just to register, and generated code would need to round-trip class objects.
- An enum in core. Same coupling problem and a new central registration burden every time a model is added.
- UUIDs. Stable, but unreadable in tracebacks, generated code, and REST schemas.

**Tradeoffs.**
- A typo or rename in one place silently disconnects router from provider — exactly the failure mode `04_risk_refactor_notes.md` flags as the dominant regression risk.
- No static type-check protects the contract: `Router model name == Provider fetcher_dict key == ProviderInterface model key`.

---

### 4. Standard models as a provider-neutral contract

**Decision.** Core defines provider-neutral query and data models (e.g. `EquityHistoricalQueryParams`, `EquityHistoricalData`) under `core/openbb_core/provider/standard_models/`; provider packages extend them, never replace them.

**Why.** Without a shared contract, every consumer would special-case each provider's output. With a shared contract, a notebook that uses `obb.equity.price.historical(...)` produces the same column shape whether the user picks `yfinance`, `fmp`, or `polygon`. The standard model also lets `ProviderInterface` reason about which fields are common and which are provider-specific (`registry_map.py` uses the file location of the model class to decide).

**Alternatives.**
- Per-provider schemas only. Maximum fidelity to the source, zero portability — every client downstream re-implements normalization.
- Lowest-common-denominator schema with no extensions. Loses the provider-specific fields users actually need (split_ratio, vwap, etc.).
- A registry of optional fields. Equivalent to standard + extra, but without inheritance enforcing the shape.

**Tradeoffs.**
- Any change to a standard model ripples through every provider that inherits it.
- Provider authors must resist the temptation to add "almost-standard" fields to standard models; once added, every other provider has to populate them.

---

### 5. Runtime-composed app object (BaseApp + generated Extensions)

**Decision.** `from openbb import obb` returns an instance of a dynamically created class that inherits from `BaseApp` and from a generated `Extensions` class assembled from installed routers. See `core/openbb/__init__.py` and `app_factory.create_app(...)`.

**Why.** The user-facing object needs methods like `obb.equity.price.historical(...)` that match whatever extensions are installed. A fixed `BaseApp` cannot expose those without code generation; pure `__getattr__` interception would not produce IDE autocompletion or accurate signatures.

**Alternatives.**
- Pure `__getattr__` dispatch. Smallest implementation; no autocomplete; signatures invisible to users and tools.
- A hand-maintained `App` class re-exporting routers. Works for one canonical install; drifts the moment a third-party extension is added.
- A purely-generated app class with no `BaseApp`. Loses the curated runtime surface (`obb.user`, `obb.system`, `obb.coverage`, `obb.reference`).

**Tradeoffs.**
- Import of `openbb` is no longer side-effect-free — it can trigger an auto-build.
- When generated extensions are missing, the fallback `BaseApp`-only object silently loses most of the API.

---

### 6. Static SDK generated by `PackageBuilder`

**Decision.** A `PackageBuilder` writes real Python modules under `openbb.package` mirroring the route tree, with concrete method signatures, type imports, and docstrings. See `core/openbb_core/app/static/package_builder.py`.

**Why.** OpenBB's primary user is in a Python REPL, Jupyter notebook, or IDE. Generated source code gives those users autocompletion, mouse-over signatures, and static type analysis on top of an inherently dynamic command bus. The dynamic plumbing (`CommandRunner`, `Query`, `ProviderInterface`) does the real work; the generated SDK is a typed facade.

**Alternatives.**
- Dynamic-only dispatch (`__getattr__`, kwargs). Smallest install footprint, worst DX. No editor can resolve `obb.equity.price.historical`.
- Stub files (`.pyi`) without runtime methods. Loses runtime introspection and forces stubs and dispatch logic to stay aligned manually.
- OpenAPI-driven client codegen. Works for REST but not for in-process Python use, and loses Python-native types like `pandas.DataFrame` returns.

**Tradeoffs.**
- `package_builder.py` is the largest, hardest-to-reason-about module in the codebase (flagged "very high" risk in `04_risk_refactor_notes.md`).
- Generated artifacts are an extra source of truth that can drift from runtime behavior.
- Build is a side effect of `import openbb` in many configurations.

---

### 7. `ProviderInterface` dynamic schema synthesis

**Decision.** A single object in `core/openbb_core/app/provider_interface.py` walks the registry and builds, per model: provider choices, standard params dataclass, extra params dataclass, return schema, and an annotated `OBBject[...]` return type.

**Why.** A command like `/equity/price/historical` is meant to support N providers without duplicating route code. Each provider contributes its own query fields, its own choices, its own JSON-schema hints. Something has to merge those into one command surface that both FastAPI and the SDK can consume. Doing this once, at registration time, gives every downstream layer (router, SDK builder, REST wrapper, validator) the same answer.

**Alternatives.**
- Hand-written union schemas in core. Requires syncing every time a provider adds a field.
- One route per provider (`/equity/price/historical/yfinance`). URL explosion, breaks the "pick a provider" model, and forces SDK users to re-import everywhere.
- No schema at all (open dict). Works for a CLI; useless for typed SDKs or OpenAPI.

**Tradeoffs.**
- Merging is subtle: descriptions, `Literal` choices, JSON-schema extras, optional/required flags all have to be reconciled (`04_risk_refactor_notes.md` lists the failure modes).
- A single misbehaving provider can affect the merged schema for a model used by many providers.

---

### 8. Provider-specific extra params merged into one command surface

**Decision.** Provider-specific query fields (yfinance's `interval`, `extended_hours`, etc.) become "extra params" exposed alongside standard params on one command, rather than producing per-provider commands.

**Why.** A user picks a provider at call time. If `interval` only makes sense for yfinance, exposing it on the shared command lets the user pass it when they pick yfinance and ignore it otherwise. The command tree stays small; the SDK stays predictable.

**Alternatives.**
- Provider-prefixed flags (`yfinance_interval=...`). Ugly, hard to deprecate, and makes the SDK signature provider-aware in a noisy way.
- Separate command per provider. Duplicates the command surface and forces consumers to know which provider supports which command.
- Reject all non-standard fields. Sacrifices the provider richness that justifies multi-provider support in the first place.

**Tradeoffs.**
- The merged surface advertises params that not every provider supports — `Query.filter_extra_params(...)` exists precisely because of this (see decision 15).
- Same-named extra params with different semantics across providers can collide quietly.

---

### 9. Three-stage `Fetcher` pipeline (transform_query / extract_data / transform_data)

**Decision.** Every provider's `Fetcher` implements three explicit stages with a fixed contract. See `core/openbb_core/provider/abstract/fetcher.py` and the yfinance fetcher at `providers/yfinance/openbb_yfinance/models/equity_historical.py`.

**Why.** Each stage has different inputs, different test strategies, and different external dependencies. `transform_query` is pure logic on parameters; `extract_data` is the I/O boundary; `transform_data` is pure normalization. Splitting them lets you mock the I/O stage in tests and lets the framework attach credentials, warnings, and validation around the boundary cleanly.

**Alternatives.**
- A single `fetch(query) -> data` method. Easier to write, much harder to test (mocking the network requires patching inside the method) and provides no place for the framework to wrap normalization.
- Two stages (fetch and parse). Conflates query normalization with raw extraction; defaults and validation sneak into the I/O path.

**Tradeoffs.**
- More boilerplate for very simple providers.
- The framework's separation of stages must be respected; mixing I/O into `transform_data` defeats the contract and breaks tests that mock `extract_data`.

---

### 10. Async `fetch_data` with optional `aextract_data` override

**Decision.** The framework exposes one `await fetcher.fetch_data(...)`. A fetcher implements `extract_data` (sync) for sync HTTP libraries, or `aextract_data` (async) for async clients. The base class swaps `aextract_data` into the slot when present.

**Why.** Some data sources are sync libraries (yfinance, pandas-datareader); others are async (aiohttp clients). Forcing async everywhere would push thread pools onto every sync library; exposing both surfaces would force the executor to branch. The chosen design gives provider authors one decision and the framework one call site.

**Alternatives.**
- Force async everywhere. Sync libraries need `asyncio.to_thread` wrappers in every fetcher.
- Force sync everywhere. Async clients have to bridge through `run_until_complete`, breaking nested event loops.
- Two separate executor paths. Doubles the surface that `QueryExecutor` and `CommandRunner` have to reason about.

**Tradeoffs.**
- The "subclass init swaps the method" pattern is non-obvious; readers expecting standard async semantics may be surprised.
- Sync `extract_data` can block the event loop if the executor doesn't offload it, so providers calling slow sync libraries should be aware of the runtime.

---

### 11. Thin router commands delegating to `OBBject.from_query(Query(**locals()))`

**Decision.** Every model-bound command body is essentially the same one-liner. Real logic lives in `Query`, `ProviderInterface`, and `QueryExecutor`. See command examples in `extensions/equity/openbb_equity/price/price_router.py`.

**Why.** Hundreds of commands across dozens of extensions would otherwise re-implement the same merge/dispatch sequence. Forcing them to delegate keeps the per-command surface declarative and prevents per-route divergence in how parameters are filtered or how `OBBject` is constructed.

**Alternatives.**
- Per-command implementations. Maximum flexibility, guaranteed divergence on warnings, metadata, defaults.
- A decorator that hides the body entirely. Less explicit; readers cannot see what the command does without resolving the decorator.

**Tradeoffs.**
- `Query(**locals())` is a clever idiom; reading any one command tells you almost nothing about what it does.
- The real behavior is several modules away from the route declaration.

---

### 12. `OBBject` as the universal result wrapper

**Decision.** Every command returns `OBBject[T]` — a Pydantic generic wrapping `results`, `provider`, `warnings`, `chart`, `extra`, and private route/parameter metadata, with `to_df`, `to_polars`, `to_numpy`, `to_dict`, `to_llm`, and `show` helpers. See `core/openbb_core/app/model/obbject.py`.

**Why.** A platform that targets notebooks, REST clients, charting, and LLM consumers needs one transportable object. Returning a raw DataFrame strips provider attribution, warnings, and metadata; returning provider-specific objects breaks JSON serialization and forces clients to know the provider.

**Alternatives.**
- Raw DataFrame returns. Best Python ergonomics, loses warnings/metadata/provider, no REST analog.
- Provider-specific dataclasses. Loses uniformity; REST schema becomes provider-discriminated at the top level.
- A tuple `(data, metadata)`. Two things to remember; awkward to extend with chart/warnings later.

**Tradeoffs.**
- Users have to call `.to_df()` to get a DataFrame; first-time users miss it.
- `OBBject.from_query` plus extension callbacks plus charting injection is a lot of post-processing on what looks like a return value.

---

### 13. Shared `CommandRunner` for SDK and REST

**Decision.** SDK methods and FastAPI wrappers both call `CommandRunner.run(route, user_settings, *args, **kwargs)`. See `core/openbb_core/app/command_runner.py`.

**Why.** Warnings capture, charting invocation, logging, metadata injection, command-context handling, and OBBject callbacks must work identically whether the call came from Python or HTTP. Two parallel pipelines would diverge — silently — on every one of those concerns.

**Alternatives.**
- Two execution paths. Faster to evolve each independently, guaranteed to drift on warning/metadata handling.
- One path with mode flags. Equivalent in spirit but with branches scattered through the pipeline.

**Tradeoffs.**
- `CommandRunner` carries everyone's concerns: arg merge, validation, charting, warnings, logging, metadata, dependency cleanup, callbacks. It's a high-traffic module.
- API-only inputs (auth, custom headers) have to be stripped before commands see them, which adds subtle filtering steps.

---

### 14. REST API generated from the same router metadata as the SDK

**Decision.** `core/openbb_core/api/router/commands.py` wraps each extension route into a FastAPI endpoint, rebuilding the signature for HTTP semantics but reusing the same router/provider/model metadata that drove SDK generation.

**Why.** Anything that produces SDK and REST from separate sources will drift. Generating both from `RouterLoader.from_extensions()` plus `ProviderInterface` makes the model the source of truth and the two surfaces its consumers.

**Alternatives.**
- Hand-written FastAPI app. Inevitable drift; double the work for every command.
- OpenAPI-driven codegen. Drift moves to a different boundary; loses runtime introspection.
- A second router system for REST only. Two router languages to learn.

**Tradeoffs.**
- The wrapper has its own concerns (auth, defaults, custom headers, no-validate routes, extension-modified output bypass) that don't apply to the SDK, so wrapper and SDK aren't a literal one-to-one map — they share metadata, not implementation.
- Schema generation for FastAPI must match what `PackageBuilder` produces, or REST/SDK diverge.

---

### 15. `Query.filter_extra_params` for provider safety

**Decision.** Before dispatch, `Query` compares the user's extra params against the selected provider's supported fields. Non-default extras that the provider does not support raise an `OpenBBWarning` and are dropped. See `core/openbb_core/app/query.py`.

**Why.** Decision 8 produces a merged extra-param surface where a user can supply `interval=...` regardless of whether their provider supports it. Passing it blindly to a fetcher would either crash the provider library or silently ignore it. Strict typing per provider would defeat the merged surface.

**Alternatives.**
- Strict typing per provider. Breaks decision 8's single command surface.
- Silent passthrough. Confusing failures inside provider libraries with no warning trail.
- Hard error on unsupported extras. Surprises users who set a sensible default for a different provider.

**Tradeoffs.**
- Warning fatigue if users rely on default-set extras across many providers.
- The filter compares against "default" — if a default changes, what counts as "explicitly set" can shift.

---

### 16. `OBBject` command-output callback extensions

**Decision.** A third entry-point group (`openbb_obbject_extension`) registers callbacks invoked after command execution, scoped to `*` or a specific route. Callbacks may mutate the result and flag it as extension-modified. See `core/openbb_core/app/command_runner.py` and `extension_loader.py`.

**Why.** Cross-cutting post-processing (custom formatting, transformations, analytics enrichment) shouldn't require modifying commands or providers. A subclass of `OBBject` would conflict with Pydantic's generic typing; FastAPI middleware would only catch the REST path.

**Alternatives.**
- Subclass `OBBject`. Generic typing collisions; only one subclass per process.
- FastAPI middleware. REST-only, misses SDK.
- Per-router hook. Doesn't compose across third-party extensions.

**Tradeoffs.**
- Extension-modified output bypasses normal REST response validation, which is exactly what enables the feature but also what makes it hard to debug.
- Callback ordering and the mutable-vs-immutable choice are subtle; `04_risk_refactor_notes.md` flags this as a regression surface.

---

### 17. Opt-in `cc: CommandContext` injection

**Decision.** `ParametersBuilder.build(...)` looks at the command's signature; if it accepts `cc`, it receives a `CommandContext` carrying user and system settings.

**Why.** Most commands don't need user/system settings; injecting them everywhere would clutter every SDK method's signature. Opt-in by signature lets the few commands that need credentials or preferences pull them, while the rest stay simple.

**Alternatives.**
- Always inject. Signature noise in every generated SDK method.
- Thread-local context. Test-hostile, async-unsafe, hidden coupling.
- Global singleton settings. Same problems, plus mutation hazards.

**Tradeoffs.**
- Two flavors of command signature to support (with and without `cc`).
- Generated SDK methods must hide `cc` from the user-facing surface.

---

### 18. Credentials normalized by provider-name prefix

**Decision.** A provider declares credential field names (e.g. `api_key`); the registry stores them as `<provider>_<field>` (e.g. `fmp_api_key`). See `Provider.__init__` and `core/openbb_core/provider/registry.py`.

**Why.** User settings, environment variables, and config files all prefer flat namespaces. A flat `fmp_api_key` is trivial to set via `OPENBB_FMP_API_KEY`, store in a settings JSON, or surface in a UI. A nested dict would require parsing rules everywhere.

**Alternatives.**
- Nested per-provider dict in user settings. Cleaner conceptually, painful to map to env vars.
- Unprefixed credentials. Collisions between providers using the same field name.

**Tradeoffs.**
- Renaming a provider's credential field is a breaking change for users' settings files.
- Provider name and field name are concatenated as a string, with the usual stringly-typed risks.

---

### 19. `RegistryMap` path-based standard vs. provider field detection

**Decision.** A field is "standard" if its declaring model class lives under `core/openbb_core/provider/standard_models/`; otherwise it's provider-specific. See `core/openbb_core/provider/registry_map.py`.

**Why.** Provider data models extend standard models. The framework needs to know which fields came from the standard contract and which are provider extensions, without forcing every field to carry a marker. Module file location is a property the framework can read reliably from any model class.

**Alternatives.**
- Explicit `standard=True` markers. Boilerplate on every field, easy to forget, easy to lie about.
- MRO-only inference. Mixins and re-exports can confuse the chain.
- Maintain a hand-written registry. Drifts the moment a standard model is added.

**Tradeoffs.**
- Moving standard models changes the boundary silently.
- A provider that re-defines a standard field will have that field counted as provider-specific, which can be unexpected.

---

### 20. Annotated discriminated-union return types per model

**Decision.** `ProviderInterface.return_annotations[model]` produces an `OBBject[Annotated[Union[...providers], Field(discriminator="provider")]]` so that REST responses and SDK type hints reflect which provider's data shape is returned.

**Why.** Multi-provider commands can return data with provider-specific extra fields. A flat `OBBject[Any]` gives FastAPI no schema; a single concrete data type drops the extras. A discriminated union lets the schema be precise about which fields appear for which provider.

**Alternatives.**
- `OBBject[Any]`. No schema, no IDE help, weak REST docs.
- Single concrete data type. Loses provider-specific fields.
- Untyped JSON response model. Same as `Any`, plus client-side guesswork.

**Tradeoffs.**
- Generated annotations grow with each new provider for a model.
- Pydantic's discriminator handling has sharp edges with optional fields and aliasing — exactly the area `ProviderInterface` puts effort into.

---

## Cross-cutting principles

A few themes recur across these decisions:

- **One contract, multiple surfaces.** The model name binds router, provider, schema, SDK, and REST. Most "why" answers boil down to preserving that contract while letting each surface express it natively. See decisions 3, 4, 7, 13, 14, 20.
- **Composition by installation, not by configuration.** Pip-installing a package changes what the platform can do — no central edits, no config registration. See decisions 1, 2.
- **Make dynamic plumbing look static at the edges.** `PackageBuilder`, `ProviderInterface`, and the API wrapper exist to give users typed, discoverable surfaces on top of runtime discovery. See decisions 5, 6, 7, 20.
- **Centralize execution, distribute declaration.** Routers, providers, and fetchers *declare*. `CommandRunner`, `Query`, and `QueryExecutor` *execute*. See decisions 11, 13, 15.
- **Pay for ergonomics with framework complexity.** Most user-visible benefits — typed SDK, unified REST schema, drop-in providers, transportable results — are paid for inside `package_builder.py`, `provider_interface.py`, and `command_runner.py`. The risks listed in `04_risk_refactor_notes.md` follow directly from this trade.

## How to use this doc when changing code

- When asking "why is this dynamic?", answers usually trace to the multi-provider, single-command model. See decisions 4, 7, 8.
- When asking "why is there generated code?", answers trace to the typed-SDK-over-dynamic-bus stance. See decisions 5, 6, 11, 12.
- When asking "why does SDK and REST share so much machinery?", answers trace to equivalence guarantees. See decisions 13, 14.
- When asking "why is this filtered / wrapped / merged?", answers usually point to a contract being preserved across surfaces. See decisions 7, 8, 15, 20.

If a proposed change makes one surface simpler at the cost of another, the deep-dive contract listed at the end of `03_extension_provider_model.md` is the artifact to protect first.
