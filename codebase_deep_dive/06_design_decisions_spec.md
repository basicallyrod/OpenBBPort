# 06 — Design Decisions Spec Sheet

A quick-reference matrix of OpenBBPort's architectural decisions, grouped by subsystem. Each cell is a compact summary; for full rationale, alternatives, and tradeoffs prose see `05_design_decisions.md`. Decision numbers map 1:1 with the headings in that file.

**Field key**

- **Where** — primary source path
- **Why** — one-line problem the decision solves
- **Chosen over** — realistic alternatives that were not picked
- **Cost** — the price this choice carries

---

## Extension & Discovery

| | |
|---|---|
| **#1 Plugin via entry points**<br>**Where:** `core/openbb_core/app/extension_loader.py`<br>**Why:** Add providers/domains by installing packages; keep core small.<br>**Chosen over:** hardcoded imports; namespace packages; pluggy/stevedore.<br>**Cost:** errors surface at discovery time; one bad plugin can perturb the registry. | **#2 Three entry-point groups**<br>**Where:** `extension_loader.py` (`OpenBBGroups`)<br>**Why:** Routers, providers, and OBBject extensions have different shapes and lifecycles.<br>**Chosen over:** single generic group with a runtime `kind` discriminator.<br>**Cost:** plugin authors must remember which group; three names to keep in sync. |
| **#3 Model-name string as binding key**<br>**Where:** `Router`/`Provider.fetcher_dict`/`ProviderInterface`<br>**Why:** Stable serializable identifier across packages/processes that don't share imports.<br>**Chosen over:** class handle (import coupling); central enum; UUIDs.<br>**Cost:** no static check protects the cross-surface name contract. | **#18 Credentials prefixed by provider**<br>**Where:** `Provider.__init__`, `core/openbb_core/provider/registry.py`<br>**Why:** Flat namespace maps cleanly to env vars and settings files.<br>**Chosen over:** nested per-provider dict; unprefixed credentials.<br>**Cost:** renaming a credential field is a breaking change for users. |

---

## Provider & Schema

| | |
|---|---|
| **#4 Standard models as contract**<br>**Where:** `core/openbb_core/provider/standard_models/`<br>**Why:** Provider-neutral query/data shapes so outputs are comparable.<br>**Chosen over:** per-provider schemas only; lowest-common-denominator only.<br>**Cost:** any standard-model change ripples through every provider that inherits it. | **#7 `ProviderInterface` dynamic schemas**<br>**Where:** `core/openbb_core/app/provider_interface.py`<br>**Why:** One merged schema per model spans all installed providers.<br>**Chosen over:** hand-written unions; per-provider routes; no schema.<br>**Cost:** merge logic (descriptions, Literals, aliases) is subtle; one bad provider can poison the merged schema. |
| **#8 Merged extra params**<br>**Where:** `ProviderInterface` extra params + `Query`<br>**Why:** Single command surface even when providers expose different fields.<br>**Chosen over:** provider-prefixed flags; per-provider commands; reject non-standard fields.<br>**Cost:** advertises params not every provider supports; same-named extras can collide semantically. | **#9 Three-stage Fetcher pipeline**<br>**Where:** `core/openbb_core/provider/abstract/fetcher.py`<br>**Why:** Split pure normalization, I/O, and post-normalization for testability.<br>**Chosen over:** single `fetch()` method; two-stage fetch+parse.<br>**Cost:** boilerplate for trivial providers; contract must be respected for tests to work. |
| **#10 Async `fetch_data` + optional `aextract_data`**<br>**Where:** `fetcher.py`<br>**Why:** Support both sync libs (yfinance) and async clients behind one framework call.<br>**Chosen over:** force-async everywhere; force-sync; two executor paths.<br>**Cost:** non-obvious init-time method swap; sync extracts can block the event loop. | **#19 Path-based standard vs provider field detection**<br>**Where:** `core/openbb_core/provider/registry_map.py`<br>**Why:** Classify fields by module location instead of per-field markers.<br>**Chosen over:** explicit `standard=True` flags; MRO-only inference; manual registry.<br>**Cost:** moving standard models silently shifts the boundary. |
| **#20 Annotated discriminated-union return types**<br>**Where:** `ProviderInterface.return_annotations`<br>**Why:** Precise schemas/REST docs even when providers add their own data fields.<br>**Chosen over:** `OBBject[Any]`; single concrete data type; untyped JSON.<br>**Cost:** annotations grow with each provider; Pydantic discriminator edges with optional/aliased fields. | |

---

## Command Execution

| | |
|---|---|
| **#11 Thin router commands**<br>**Where:** `extensions/*/...router.py`<br>**Why:** Uniform `OBBject.from_query(Query(**locals()))` delegation prevents per-route divergence.<br>**Chosen over:** per-command implementations; hide body in a decorator.<br>**Cost:** reading one command reveals almost nothing about behavior. | **#13 Shared `CommandRunner` for SDK + REST**<br>**Where:** `core/openbb_core/app/command_runner.py`<br>**Why:** Identical warnings/logging/metadata/charting/callbacks across both surfaces.<br>**Chosen over:** two parallel execution pipelines; one path with mode flags.<br>**Cost:** high-traffic module carrying many concerns; API-only inputs need filtering. |
| **#15 `Query.filter_extra_params`**<br>**Where:** `core/openbb_core/app/query.py`<br>**Why:** Accept the merged extra-param surface without leaking unsupported fields into fetchers.<br>**Chosen over:** strict per-provider typing; silent passthrough; hard error.<br>**Cost:** warning fatigue; depends on default detection, sensitive to default changes. | **#17 Opt-in `CommandContext` injection**<br>**Where:** `command_runner.ParametersBuilder.build`<br>**Why:** Only commands that need user/system settings get them.<br>**Chosen over:** always inject; thread-local; global singleton.<br>**Cost:** two signature flavors to support; SDK generator must hide `cc`. |

---

## SDK Surface

| | |
|---|---|
| **#5 Runtime-composed app (BaseApp + Extensions)**<br>**Where:** `core/openbb/__init__.py`, `app_factory.create_app`<br>**Why:** Dynamic SDK surface that still inherits a curated runtime base.<br>**Chosen over:** pure `__getattr__`; hand-maintained `App`; fully generated only.<br>**Cost:** `import openbb` has side effects; silent degraded fallback when extensions missing. | **#6 Static SDK via `PackageBuilder`**<br>**Where:** `core/openbb_core/app/static/package_builder.py`<br>**Why:** IDE autocompletion and signatures over an inherently dynamic command bus.<br>**Chosen over:** dynamic-only dispatch; `.pyi` stubs only; OpenAPI codegen.<br>**Cost:** largest/most complex module in the repo; generated artifacts can drift; build-at-import. |
| **#12 `OBBject[T]` universal result wrapper**<br>**Where:** `core/openbb_core/app/model/obbject.py`<br>**Why:** One transportable object for notebooks, REST, charting, LLM consumers.<br>**Chosen over:** raw DataFrame; provider-specific objects; tuple returns.<br>**Cost:** users must call `.to_df()`; heavy post-processing on a return value. | |

---

## REST & Output Extensibility

| | |
|---|---|
| **#14 REST generated from same router metadata**<br>**Where:** `core/openbb_core/api/router/commands.py`<br>**Why:** Single source of truth (routes + `ProviderInterface`) drives both SDK and HTTP surfaces.<br>**Chosen over:** hand-written FastAPI app; OpenAPI codegen; second router system.<br>**Cost:** wrapper has REST-only concerns (auth, defaults, headers, no-validate); shares metadata, not implementation. | **#16 OBBject command-output callbacks**<br>**Where:** `extension_loader.on_command_output_callbacks` + `command_runner._trigger_command_output_callbacks`<br>**Why:** Cross-cutting post-processing without modifying commands or providers.<br>**Chosen over:** subclass `OBBject`; FastAPI middleware (REST-only); per-router hooks.<br>**Cost:** extension-modified output bypasses REST validation; callback ordering subtle. |

---

## Cross-cutting principles (one-liners)

| Principle | Embodied by |
|---|---|
| One contract, multiple surfaces | #3, #4, #7, #13, #14, #20 |
| Composition by installation, not configuration | #1, #2 |
| Static at the edges, dynamic underneath | #5, #6, #7, #20 |
| Centralize execution, distribute declaration | #11, #13, #15 |
| Pay for ergonomics with framework complexity | #6, #7, #12, #13 |

---

## Change-impact quick lookup

| If you change... | Re-validate these decisions |
|---|---|
| A standard model field | #4, #7, #20, plus every provider fetcher |
| A provider's `fetcher_dict` key | #3, #7, #20 |
| A router command's signature | #11, #13, #17, plus generated SDK (#6) and REST wrapper (#14) |
| A `Fetcher` stage | #9, #10, #15 |
| Anything in `ProviderInterface` | #7, #8, #20 — and SDK snapshots, REST schema |
| Anything in `CommandRunner` | #13, #15, #16, #17 |
| Anything in `PackageBuilder` | #5, #6 — and re-import the generated package |
| The REST wrapper | #13, #14, #16 |
