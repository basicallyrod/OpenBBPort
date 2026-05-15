# Merge Agent Handoff

You are picking up the OpenBBPort backend-merge task. This file is the **operational summary** — it tells you what exists, what's missing, and the exact next steps. Read this first; consult the linked docs only as needed.

---

## 0. The 30-second version

- A spec sheet has been written at `codebase_deep_dive/06_merge_spec_sheet.md` for merging an external Python quant codebase (working name: `QuantStuff`) into OpenBBPort.
- The spec is **codebase-agnostic on the import side**. It does not yet know what the source codebase actually contains.
- Your job: (1) obtain the source codebase, (2) fill in the §5 worksheet and §9 open slots in the spec, (3) get §6 decisions answered by the human, (4) implement the merge in slices per §8.
- Develop on branch `claude/plan-backend-merge-TKGUY`. Push to that branch only.

---

## 1. What's in this repo state

| Path | Purpose |
|---|---|
| `codebase_deep_dive/README.md` | Index of the deep-dive series. |
| `codebase_deep_dive/01_architecture_map.md` | Topology and runtime surfaces of OpenBBPort. |
| `codebase_deep_dive/02_execution_flow.md` | Call flow: SDK/REST → router → query → executor → fetcher → OBBject. |
| `codebase_deep_dive/03_extension_provider_model.md` | Extension/provider entry-point contracts. |
| `codebase_deep_dive/04_risk_refactor_notes.md` | High-risk modules; aggravated by large merges. |
| `codebase_deep_dive/06_merge_spec_sheet.md` | **The spec.** Read this end-to-end before coding. |
| `codebase_deep_dive/07_merge_agent_handoff.md` | This file. |

Note: `05_agent_handoff.md` is referenced in the README but does not exist. Either ignore it or create it as a general handoff distinct from this merge-specific one.

---

## 2. What is known about the merge target (OpenBBPort)

Three existing extensions are the primary landing surfaces:

- `openbb_platform/extensions/quantitative/` — 16 commands across `rolling/`, `stats/`, `performance/`, plus top-level `normality`, `capm`, `unitroot_test`, `summary`. Backed by scipy + statsmodels.
- `openbb_platform/extensions/technical/` — 27 indicators backed by `pandas-ta-openbb`, plus volatility-estimator helpers.
- `openbb_platform/extensions/econometrics/` — 14 commands (OLS, panel models, diagnostics, ADF, causality, cointegration) backed by statsmodels/linearmodels/arch.

Complete inventory with file:line references: `06_merge_spec_sheet.md` §2.

Other existing extensions worth knowing about for landing decisions: `derivatives/`, `fixedincome/`, `famafrench/` (scaffold), `econometrics/`, plus the standard data extensions (`equity/`, `etf/`, `crypto/`, etc.).

---

## 3. What is unknown — must be obtained before coding

The spec sheet has placeholder slots in §9 (open slots) that you must fill:

```
SOURCE_CODEBASE_NAME, SOURCE_PYTHON_VERSION,
SOURCE_TOP_LEVEL_MODULES, SOURCE_PUBLIC_API_SURFACE,
SOURCE_PRIMARY_LIBRARIES, SOURCE_LICENSE,
CATEGORIES_PRESENT_FROM_§4, CATEGORIES_NET_NEW_TO_OBB,
CATEGORIES_OVERLAPPING_OBB, DATA_INPUT_FORM,
RETURNS_CONVENTION, ANNUALIZATION_BASIS,
HAS_BACKTESTER, HAS_OPTIMIZER, HAS_PROVIDER_FETCH_CODE, HAS_ML_MODELS
```

The QuantStuff repo (`https://github.com/basicallyrod/QuantStuff`) is **private or non-existent** — direct clone returns 404 anonymously, and the GitHub MCP scope for this environment is restricted to `basicallyrod/openbbport` only. To obtain the source you need one of:

1. The human flips QuantStuff to public, then you `git clone` anonymously.
2. The human pastes the directory tree + public API into the chat.
3. The human grants MCP access to the QuantStuff repo and you read it via `mcp__github__get_file_contents` / `mcp__github__search_code`.

Ask explicitly which path the human prefers; do not guess.

---

## 4. The §5 worksheet — the actual work product

Once you have the source code, your primary deliverable before any implementation is the §5 mapping table in the spec sheet, with one row per source function. Each row:

| Column | What goes here |
|---|---|
| Source function | Fully qualified name from the source codebase. |
| Source lib | The Python lib it calls (empyrical, pandas-ta, scipy, statsmodels, cvxpy, ...). |
| Taxonomy §4.x | The category from spec §4 (1 of 19). |
| Existing OBB equivalent | Path under `obb.*`, or `—`. |
| Action | `reuse` / `alias` / `extend` / `add` / `defer` / `reject`. |
| Target path | Where it lands (e.g. `obb.quantitative.risk.max_drawdown`). |
| Data input form | Series? DataFrame? OHLCV? Provider-fetched? |
| Output shape | Scalar / Series / DataFrame / typed record. |
| Standard model | Existing or new model name (the binding key, see §6). |
| Provider needed? | Yes (with name) / No (caller-supplied data). |
| Notes | Decisions, caveats, version notes. |

For a meaningful quant codebase expect 50–200 rows. Persist the worksheet either inline in `06_merge_spec_sheet.md` or as `codebase_deep_dive/08_merge_worksheet.md` if it grows large.

---

## 5. The §6 decisions — ask the human, do not invent

Ten decisions in `06_merge_spec_sheet.md` §6 have no defensible default. Before coding, get explicit answers and record them in `codebase_deep_dive/merge_decisions.md`:

1. Where new commands land (extend existing vs new sibling extension).
2. Pure-function vs router-command boundary.
3. Data input form (symbol+provider vs caller-supplied `list[Data]` vs both).
4. Returns convention (simple vs log; recommended default: simple).
5. Annualization (recommended: explicit `periods_per_year=252`, no auto-infer).
6. Risk-free rate handling (scalar vs Series; require explicit `risk_free_rate=0.0`).
7. Benchmark handling (Series only; alignment policy).
8. NaN policy (recommended: drop + warn).
9. Library version pinning + heavy-dep extras (cvxpy/torch/QuantLib should NOT be base install).
10. Backwards-compatible naming for overlapping libs (empyrical vs quantstats).

Use `AskUserQuestion` for these — present recommendations from the spec as the first option in each.

---

## 6. Non-negotiable operational rules

These are the rules every code change must respect. From `06_merge_spec_sheet.md` §3 and `04_risk_refactor_notes.md`:

### 6.1 Binding-key invariant

```
@router.command(model="X")
    == Provider.fetcher_dict["X"]
    == class XQueryParams(QueryParams)
    == class XData(Data)
    == ProviderInterface.models["X"]
```

Any drift across these five sites breaks the system at runtime. Audit on every new model.

### 6.2 Command signature contract

```python
@router.command(model="…")
async def f(
    cc: CommandContext,
    provider_choices: ProviderChoices,
    standard_params: StandardParams,
    extra_params: ExtraParams,
) -> OBBject:
    return await OBBject.from_query(Query(**locals()))
```

`SignatureInspector.complete(...)` validates this at boot. Drift → boot failure.

### 6.3 Standard models live in one place

`openbb_platform/core/openbb_core/provider/standard_models/`. Do not create parallel hierarchies in extensions.

### 6.4 SDK regeneration is mandatory

After adding any command or provider, run `PackageBuilder().build()` and commit the diff in `openbb/package/`. CI must fail if generated files drift from a clean rebuild.

### 6.5 Do not touch high-risk modules casually

Without snapshot tests, leave alone:

- `openbb_core.app.static.package_builder`
- `openbb_core.app.provider_interface`
- `openbb_core.app.command_runner`
- `openbb_core.api.router.commands`

Safe areas for the merge: `openbb_platform/extensions/<name>/`, `openbb_platform/providers/<name>/`, `openbb_platform/core/openbb_core/provider/standard_models/`.

### 6.6 Heavy deps go in extras, not base

`cvxpy`, `pytorch`, `tensorflow`, `QuantLib`, `xgboost`, `lightgbm`, full `mlfinlab`. Use `[tool.poetry.extras]` + lazy imports inside command bodies.

---

## 7. PR slicing strategy

From spec §8. One §4 category per PR. Each PR includes:

1. Standard models in `…/standard_models/` (new + extensions of existing).
2. Router commands (and new submodule router if first command in a new submodule).
3. Provider fetcher(s) — only if the command fetches data; otherwise use caller-supplied-data pattern per §6.3 decision.
4. `pyproject.toml` entry points + dependency updates.
5. Regenerated static SDK package committed (`openbb/package/` diff).
6. Tests:
   - Fetcher contract (transform/extract/transform).
   - Standard-model validation.
   - Golden-output regression for any formula with a published reference value.
   - SDK generation snapshot.
   - REST/SDK equivalence on at least one command per PR.
7. Docstrings on every command including: formula expression or citation; input convention (prices vs returns; simple vs log); annualization; NaN policy; backing library + version.

Do not exceed ~15 commands per PR. Recommended PR order (lightest first):

1. Returns calculations (§4.1).
2. Risk metrics (§4.4) — high-value, mostly caller-supplied data.
3. Performance metrics (§4.5) — fills the gap of scalar Sharpe/Sortino.
4. Correlation/covariance (§4.7).
5. Volatility models (§4.11) — promote existing helpers + add GARCH.
6. Technical indicators (§4.12) — incremental adds to existing extension.
7. Factor models (§4.9) — requires `famafrench` provider promotion first.
8. Portfolio math (§4.8) — new extension; cvxpy as extra.
9. Monte Carlo (§4.15) — new submodule.
10. Backtesting (§4.14) — last; vectorized-only.
11. ML adjuncts (§4.18) — defer or reject by default.

---

## 8. Git / branch rules

- Branch: `claude/plan-backend-merge-TKGUY`.
- Commit each PR slice with a clear message; never amend pushed commits.
- Push: `git push -u origin claude/plan-backend-merge-TKGUY` (already tracked).
- Do not push to any other branch.
- Do not create pull requests unless the human asks.
- Latest commit on this branch as of handoff: the spec-sheet addition.

---

## 9. First-three-actions checklist

1. Ask the human how to obtain QuantStuff source (handoff §3).
2. Once obtained, walk its public API and produce the §5 worksheet (handoff §4).
3. Resolve §6 decisions with the human via `AskUserQuestion` (handoff §5). Record in `codebase_deep_dive/merge_decisions.md`.

Stop after step 3 and confirm scope with the human before opening the first slice PR.

---

## 10. Cheat sheet (copy from spec §10)

| Question | Answer |
|---|---|
| Entry point for new domain router | `[tool.poetry.plugins."openbb_core_extension"]` |
| Entry point for new provider | `[tool.poetry.plugins."openbb_provider_extension"]` |
| Standard models live at | `openbb_platform/core/openbb_core/provider/standard_models/` |
| Command signature | `async def f(cc, provider_choices, standard_params, extra_params) -> OBBject` |
| Binding key | Class-name root of the standard model |
| Regenerate SDK | `PackageBuilder().build()`; commit `openbb/package/` diff |
| Helpers / pure functions | `openbb_<ext>/<submodule>/` (e.g. `openbb_quantitative/statistics.py`) |
| REST exposure | Automatic; path = SDK path |
| Default returns convention | Simple |
| Default annualization | `periods_per_year=252`, explicit |
| Default NaN policy | Drop + warn |
| Heavy deps | Poetry extras + lazy import |

---

*End of handoff. The spec sheet (`06_merge_spec_sheet.md`) is the authoritative reference; this file is the operational shortcut to it.*
