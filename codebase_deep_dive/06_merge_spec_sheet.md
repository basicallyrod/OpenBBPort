# Backend Merge Spec Sheet

A reconciliation-ready specification for merging an external Python quantitative-analysis backend (e.g., `QuantStuff`) into the OpenBBPort Python backend.

This document is intentionally **source-codebase-agnostic on the import side**. It describes (a) the exact OpenBBPort surfaces a merge must land on, (b) a generic taxonomy of quant formulas to inventory the source against, and (c) the per-formula decision matrix needed before a single line of integration code is written. Drop the source codebase into the inventory tables below and the spec resolves.

Companion files:

- `01_architecture_map.md` — OpenBBPort topology and runtime surfaces.
- `02_execution_flow.md` — Call flow through router → query → executor → fetcher → OBBject.
- `03_extension_provider_model.md` — Extension and provider entry-point contracts.
- `04_risk_refactor_notes.md` — Maintainability risks; aggravated by large merges.

---

## 1. Scope and Conclusion

**Goal.** Land a body of external quantitative-analysis code into OpenBBPort such that its formulas become discoverable as commands on the Python SDK, REST API, and CLI; remain usable as importable pure functions for in-process composition; and conform to the existing standard-model / provider / OBBject contract.

**Merge target extensions already exist.** OpenBBPort ships three relevant extensions that any quant merge will extend or de-duplicate against:

| Extension | Package | Scope | Primary deps |
|---|---|---|---|
| `quantitative` | `openbb-quantitative` v1.6.1 | Rolling/full-sample stats, normality, CAPM, unit-root, performance ratios | scipy, statsmodels |
| `technical` | `openbb-technical` v1.6.1 | 27 TA indicators + volatility cones + relative-rotation | pandas-ta-openbb, scikit-learn |
| `econometrics` | `openbb-econometrics` v1.7.1 | 14 econometric commands (OLS, panel, ADF, causality, cointegration) | statsmodels, arch, linearmodels |

The merge does **not** require a new top-level extension by default. The first decision (see §6.1) is whether new formulas land in the existing extensions or in a new sibling (`portfolio`, `backtest`, `factor`, etc.).

**Top-line conclusion.** The merge cost is dominated by two things, not by writing math:

1. **Conforming each source function to OpenBBPort's command signature contract** (`cc, provider_choices, standard_params, extra_params → OBBject`), which forces every endpoint to either own its data fetch via a provider or invent a "caller-supplied series" provider shim.
2. **Adding standard models for any output shape that does not already exist.** Standard models are the binding key between routers, providers, and the generated SDK. Drift here breaks the system at runtime.

Everything else — entry points, REST exposure, SDK regeneration — is mechanical once the contract is satisfied.

---

## 2. Current OpenBBPort Quant Footprint (de-duplication map)

Inventory of existing commands so the source codebase can be diffed against them before merging. Anything the source codebase implements that already exists here should be de-duplicated or aliased, not re-added.

### 2.1 `quantitative` extension — `obb.quantitative.*`

Source: `openbb_platform/extensions/quantitative/openbb_quantitative/`

| Path | Command | Returns | Backing |
|---|---|---|---|
| `quantitative_router.py:44` | `normality` | `OBBject[NormalityModel]` | scipy.stats |
| `quantitative_router.py:107` | `capm` | `OBBject[CAPMModel]` | statsmodels OLS |
| `quantitative_router.py:177` | `unitroot_test` | `OBBject[UnitRootModel]` | statsmodels ADF+KPSS |
| `quantitative_router.py:252` | `summary` | `OBBject[SummaryModel]` | pandas |
| `rolling/rolling_router.py:35,107,174,244,317,405` | `skew`, `variance`, `stdev`, `kurtosis`, `quantile`, `mean` | `OBBject[list[Data]]` | pandas rolling |
| `stats/stats_router.py:34,97,154,212,271,336` | `skew`, `variance`, `stdev`, `kurtosis`, `quantile`, `mean` | `OBBject[list[Data]]` | numpy / scipy |
| `performance/performance_router.py:44,125,204` | `omega_ratio`, `sharpe_ratio`, `sortino_ratio` | `OBBject[list[Data|OmegaModel]]` | numpy |

Helpers: `helpers.get_fama_raw` (Ken French data), `helpers.validate_window`, `statistics.{kurtosis_, skew_, mean_, std_dev_, var_}`.

Pydantic models (`models.py`): `TestModel`, `NormalityModel`, `ADFTestModel`, `KPSSTestModel`, `UnitRootModel`, `OmegaModel`, `SummaryModel`, `CAPMModel`.

### 2.2 `technical` extension — `obb.technical.*`

Source: `openbb_platform/extensions/technical/openbb_technical/`

27 commands in `technical_router.py`: `atr, fib, obv, fisher, adosc, bbands, zlma, aroon, sma, demark, vwap, macd, hma, donchian, ichimoku, clenow, ad, adx, wma, cci, rsi, stoch, kc, cg, cones, ema, relative_rotation`.

Backing: `pandas-ta-openbb`, with the exception of `clenow` (`scikit-learn` LinearRegression) and `cones`/`relative_rotation` (custom volatility estimators in `helpers.py`).

Volatility estimators in `helpers.py`: `standard_deviation`, `parkinson`, `garman_klass`, `hodges_tompkins`, `rogers_satchell`, `yang_zhang`. Also `calculate_cones`, `clenow_momentum`, `calculate_fib_levels`.

### 2.3 `econometrics` extension — `obb.econometrics.*`

Source: `openbb_platform/extensions/econometrics/openbb_econometrics/`

14 commands in `econometrics_router.py`: `correlation_matrix, ols_regression, ols_regression_summary, autocorrelation, residual_autocorrelation, cointegration, causality, unit_root, panel_random_effects, panel_between, panel_pooled, panel_fixed, panel_first_difference, panel_fmac, variance_inflation_factor`.

Backing: `statsmodels`, `linearmodels` (panel), `arch`, `scipy`.

### 2.4 Known overlaps and naming collisions to resolve

| Concept | Existing locations | Resolution recommendation |
|---|---|---|
| Unit-root / ADF | `quantitative.unitroot_test` (ADF+KPSS) and `econometrics.unit_root` (ADF only) | Keep both; rename one or cross-link docstrings. Do not silently merge — they return different models. |
| Skew / kurtosis / mean / variance / quantile | `quantitative.stats.*` (full-sample) and `quantitative.rolling.*` (windowed) | If source codebase adds the same names, route to existing; do not duplicate. |
| Sharpe / Sortino | `quantitative.performance.{sharpe_ratio, sortino_ratio}` — both are **rolling**, not single-number | If source codebase has scalar Sharpe/Sortino, add as `quantitative.performance.{sharpe_summary, sortino_summary}` (or override with `window=None` semantic). |
| Correlation | `econometrics.correlation_matrix` | Source-codebase rolling/partial/EWM correlations should land under `quantitative.rolling` or `quantitative.stats`, not `econometrics`. |
| Volatility estimators | `technical.helpers.{parkinson, garman_klass, rogers_satchell, yang_zhang, hodges_tompkins}` — helpers only, not router commands | If source codebase exposes these as endpoints, decide: promote helpers to router commands in `technical`, or add to `quantitative`. |
| OLS | `econometrics.ols_regression`, `econometrics.ols_regression_summary` | Source codebase OLS variants (WLS, GLS, robust, quantile) extend `econometrics`. |
| Panel models | `econometrics.panel_*` (RE, between, pooled, fixed, first-difference, Fama-MacBeth) | Source codebase panel additions extend `econometrics`. |
| CAPM | `quantitative.capm` | Multi-factor extensions (FF3, FF5, Carhart, AQR) land under a new `quantitative.factor.*` submodule (recommended). |
| Fama-French data | `quantitative.helpers.get_fama_raw` (Dartmouth fetch) | Promote to a real provider (`famafrench` already exists as extension scaffold) before adding more factor commands. |

---

## 3. Integration Surfaces (the contract every merged function lands on)

The seven surfaces below are the **only** entry points by which external code becomes a discoverable OpenBBPort capability. Anything that does not conform to one of them is dead code from the user's perspective.

### 3.1 Entry-point discovery

OpenBBPort discovers extensions via three Python package-metadata entry-point groups (per `01_architecture_map.md` § Extension discovery, `03_extension_provider_model.md` § Entry-point groups):

```
openbb_core_extension          # domain routers (quantitative, technical, ...)
openbb_provider_extension      # data providers (yfinance, fmp, ...)
openbb_obbject_extension       # post-result mutators (charting, etc.)
```

Declared in `pyproject.toml`:

```toml
[tool.poetry.plugins."openbb_core_extension"]
quant = "openbb_quant.quant_router:router"

[tool.poetry.plugins."openbb_provider_extension"]
my_provider = "openbb_my_provider:my_provider"
```

The entry-point **name** (`quant`) becomes the first URL segment (`/quant/*`, `obb.quant.*`). The entry-point **target** must be a `Router` instance (core) or a `Provider` instance (provider) — not a class, not a function.

`ExtensionLoader` uses `importlib.metadata.entry_points()` at boot; failures here prevent platform startup. The implication is that the merge must keep its `pyproject.toml` clean and its top-level imports side-effect-free.

### 3.2 Router and command registration

Every command function must satisfy this exact signature contract (per `03_extension_provider_model.md` § Domain extension structure):

```python
from openbb_core.app.router import Router
from openbb_core.app.command_runner import CommandContext
from openbb_core.app.model.obbject import OBBject
from openbb_core.app.provider_interface import ExtraParams, ProviderChoices, StandardParams
from openbb_core.app.query import Query

router = Router(prefix="", description="...")

@router.command(model="SomeStandardModel")
async def some_command(
    cc: CommandContext,
    provider_choices: ProviderChoices,
    standard_params: StandardParams,
    extra_params: ExtraParams,
) -> OBBject:
    return await OBBject.from_query(Query(**locals()))
```

`SignatureInspector.complete(...)` validates this on load and injects FastAPI dependencies. Drift breaks at boot.

Nested submodules use `router.include_router(child_router, prefix="/sub")`. Existing example: `quantitative_router.include_router(rolling_router)`, `…(stats_router)`, `…(performance_router)`.

### 3.3 Standard models layer

Standard models live in `openbb_platform/core/openbb_core/provider/standard_models/`. They define the **provider-neutral contract** for inputs (`*QueryParams` extending `QueryParams`) and outputs (`*Data` extending `Data`). The class-name root (without suffix) is the binding key used in `@router.command(model="…")` and `Provider.fetcher_dict={"…": …}`.

**Binding-key invariant** (per `04_risk_refactor_notes.md` § Bottom line):

```
@router.command(model="X")
        ==
Provider.fetcher_dict["X"]
        ==
class XQueryParams(QueryParams)
        ==
class XData(Data)
        ==
ProviderInterface.models["X"]
```

Any drift across these five sites breaks the system at runtime.

Provider packages extend standard models to add provider-specific fields:

```python
class MyProviderXQueryParams(XQueryParams):
    extra_window: int = Field(default=20)

class MyProviderXData(XData):
    extra_metric: float
```

### 3.4 Provider registration and fetcher contract

A provider is a single `Provider` instance with a `fetcher_dict` mapping standard-model names to `Fetcher` subclasses (per `03_extension_provider_model.md` § Provider extension structure):

```python
from openbb_core.provider.abstract.provider import Provider
from openbb_core.provider.abstract.fetcher import Fetcher

class MyXFetcher(Fetcher[MyProviderXQueryParams, list[MyProviderXData]]):
    def transform_query(self, params): ...        # defaults + validation
    def extract_data(self, query, credentials, **kw): ...   # raw fetch
    # OR
    async def aextract_data(self, query, credentials, **kw): ...
    def transform_data(self, query, data, **kw) -> list[MyProviderXData]: ...

my_provider = Provider(
    name="my_provider",
    website="…",
    description="…",
    credentials=["api_key"],     # becomes "my_provider_api_key" in user settings
    fetcher_dict={"X": MyXFetcher},
)
```

`Fetcher.fetch_data` orchestrates `transform_query → (a)extract_data → transform_data`; this is the only path `QueryExecutor` ever calls.

### 3.5 OBBject return wrapping

Results are wrapped automatically by `OBBject.from_query(...)`. Surface (per `01_architecture_map.md` § Result object):

```
OBBject:
    results: T                  # the typed payload (usually list[Data])
    provider: str | None        # provider name that served the request
    warnings: list[str]         # filtered extras, fetcher warnings, etc.
    chart: Chart | None         # populated by charting extension if chart=True
    extra: dict                 # metadata, route, redacted params
```

User-facing converters: `.to_df()`, `.to_dataframe()`, `.to_dict()`, `.to_polars()`, `.to_numpy()`, `.to_llm()`, `.show()`.

### 3.6 Dynamic signature generation and SDK regeneration

After adding a command or provider, the static SDK package under `openbb/package/` **must be regenerated** by `PackageBuilder` (per `01_architecture_map.md` § Static SDK package generation). This step:

- Discovers all installed routers via entry points.
- Materializes generated `*params` dataclasses by merging provider standard + extra fields via `ProviderInterface`.
- Writes Python source files for every command path, with provider-aware `Literal[…]` provider arg, docstrings from field descriptions, and provider-merged return annotations.
- Writes `reference/extensions.json` and `reference/coverage.json` for documentation.

Failure here yields silent SDK staleness — REST works, but `obb.<path>.<cmd>` won't exist or won't expose the new fields. Treat regeneration as a **mandatory CI step on every merge PR** (see §8).

### 3.7 REST API exposure

REST routes are generated automatically by FastAPI through `build_api_wrapper(...)` (per `02_execution_flow.md` § REST API boot flow). Path mirrors SDK path exactly:

```
SDK:  obb.quant.factors.score(symbol="AAPL", provider="my_provider")
REST: GET /quant/factors/score?symbol=AAPL&provider=my_provider
```

No additional registration. The wrapper resolves user settings/credentials, injects defaults, calls `CommandRunner.run(...)`, and serializes the resulting `OBBject` against the generated response schema (unless the route is marked `no_validate=True`).

---

## 4. Quant Formula Taxonomy and Library Mapping

The categories below are the inventory checklist for whatever the source codebase contains. For each category we list (a) representative formulas, (b) likely backing libraries (so the source's `import` lines reveal the category), and (c) the natural OpenBBPort I/O shape.

Use this as a **two-column inventory**: left column is the source-codebase function/class; right column is the OpenBBPort landing surface from §2 and §3.

### 4.1 Returns Calculations

Formulas: simple, log, cumulative, total, CAGR, excess, active, period aggregation, money-weighted (IRR), time-weighted, holding-period, forward/lagged.

Likely libs: `pandas.pct_change`, `numpy.diff(np.log(...))`, `empyrical.cum_returns`, `quantstats.stats.cagr`, `numpy_financial.irr`.

OpenBBPort shape: symbol+date-range → date-indexed returns DataFrame; or caller-supplied prices → returns Series. Convention switch (`return_type: Literal["simple","log"]`) is mandatory.

Landing: new `quantitative.returns.*` submodule (does not currently exist; recommend creating).

### 4.2 Descriptive Statistics

Formulas: mean, median, mode, min/max, range, variance, stdev, MAD, quantiles, IQR, geometric/harmonic mean, `describe`.

Likely libs: `numpy`, `scipy.stats.describe`, `pandas.describe`.

OpenBBPort shape: Series → scalar or one-row record.

Landing: **partially exists** at `quantitative.stats.*` and `quantitative.summary`. Source-codebase additions extend these.

### 4.3 Distribution and Moments

Formulas: skewness, kurtosis, co-skewness/kurtosis, Jarque-Bera, Shapiro-Wilk, Anderson-Darling, Kolmogorov-Smirnov, ECDF, distribution fitting, QQ-plot data.

Likely libs: `scipy.stats`, `statsmodels.stats.diagnostic`, `empyrical.stats`.

OpenBBPort shape: Series → scalar or `(statistic, p_value)` named tuple.

Landing: **partially exists** at `quantitative.normality` (composite). Source-codebase additions extend `quantitative.stats.*` or add new top-level commands.

### 4.4 Risk Metrics

Formulas: volatility, downside deviation, semi-variance, tracking error, beta, downside/upside beta, VaR (historical / parametric / Cornish-Fisher / Monte-Carlo), CVaR/ES, max drawdown, drawdown series/duration, Ulcer, pain, CDaR, tail ratio, gain-to-pain, risk contribution, component VaR.

Likely libs: `empyrical`, `quantstats`, `pyfolio.timeseries`, `arch.bootstrap`, `riskfolio.RiskFunctions`.

OpenBBPort shape: returns Series + alpha + annualization → scalar; drawdown trajectories → Series; drawdown tables → DataFrame.

Landing: new `quantitative.risk.*` submodule (does not currently exist; recommend creating). Annualization-factor argument is mandatory and must be consistent across the submodule.

### 4.5 Performance Metrics

Formulas: Sharpe, Sortino, Calmar, Sterling, Burke, Treynor, Information ratio, Jensen's alpha, M-squared, Omega, Kappa-3, up/down capture, win rate, profit factor, payoff ratio, expectancy, Common Sense Ratio, PSR, DSR.

Likely libs: `empyrical`, `quantstats.stats`, `pyfolio.timeseries.perf_stats`, `ffn`.

OpenBBPort shape: returns Series + optional benchmark + optional `risk_free_rate` + `annualization` → scalar; or a "perf bundle" → DataFrame.

Landing: **partially exists** at `quantitative.performance.{sharpe_ratio, sortino_ratio, omega_ratio}` — note these are rolling-window forms. Scalar/aggregate versions and all other ratios are net-new. Decision needed (§6.2) on `window=None` semantic vs separate endpoints.

### 4.6 Time-Series and Rolling Operations

Formulas: rolling mean/std/min/max/median/quantile/correlation/covariance/beta/alpha/Sharpe; expanding versions; EWMA; resampling; differencing; lag/lead; decomposition; ACF/PACF; ADF/KPSS/PP; Engle-Granger/Johansen cointegration; Hurst; variance-ratio; Granger causality.

Likely libs: `pandas.rolling/expanding/ewm`, `statsmodels.tsa.stattools`, `statsmodels.tsa.seasonal`, `arch.unitroot`, `hurst`.

OpenBBPort shape: Series + window/lag → Series; tests → `(stat, p_value)` record.

Landing: **partially exists**. Rolling primitives at `quantitative.rolling.*`. Cointegration/causality/ADF at `econometrics.*`. Source-codebase additions land in whichever extension matches semantics; resolve by family, not by author.

### 4.7 Correlation and Covariance

Formulas: Pearson/Spearman/Kendall, distance correlation, sample covariance, shrinkage (Ledoit-Wolf, OAS, constant-corr), robust covariance, EWMA covariance, semi-covariance, partial correlation, rolling correlation matrix, PCA/eigendecomposition, nearest-PD repair.

Likely libs: `numpy`, `pandas.corr/cov`, `scipy.stats`, `sklearn.covariance`, `riskfolio.ParamsEstimation`, `dcor`.

OpenBBPort shape: returns DataFrame → square DataFrame indexed by symbol both axes; long-format `(asset_i, asset_j, value)` as alternate.

Landing: **basic correlation exists** at `econometrics.correlation_matrix`. All shrinkage/robust/partial/rolling extensions are net-new. Recommend a `quantitative.cov.*` or `quantitative.dependence.*` submodule.

### 4.8 Portfolio Math

Formulas: portfolio return/variance/volatility, marginal/component risk contribution, diversification ratio, effective bets, Herfindahl, turnover, rebalancing PnL, Markowitz MV optimization, min-variance, max-Sharpe/tangency, risk parity / ERC, HRP, Black-Litterman, Kelly, mean-CVaR, max diversification, equal-weight / inverse-vol, efficient frontier, Brinson attribution, performance contribution.

Likely libs: `cvxpy`, `scipy.optimize`, `PyPortfolioOpt`, `riskfolio-lib`, `skfolio`, `cvxportfolio`.

OpenBBPort shape: weights vector + (returns DataFrame OR pre-computed mu/Sigma) → weights record, scalars, or frontier DataFrame.

Landing: **does not exist**. Recommend a new `portfolio` top-level extension. This is the largest net-new surface in a typical quant merge and warrants its own pyproject. Cross-dependency on `cvxpy` is non-trivial — must be declared as an extra, not a hard dep, to keep the default install lean.

### 4.9 Factor Models

Formulas: CAPM, FF3, FF5, Carhart 4, AQR/style, generic multifactor regression, rolling loadings, cross-sectional factor returns, risk decomposition, factor-mimicking portfolios, PCA/ICA statistical factors, APT, factor-based attribution.

Likely libs: `statsmodels.OLS`, `linearmodels.PanelOLS/FamaMacBeth`, `sklearn.decomposition`, `alphalens`, `pyfolio.performance_attribution`.

OpenBBPort shape: returns Series + factor DataFrame → coefficient record (factor → loading, t, p) + R²; rolling endpoints → date-indexed DataFrame.

Landing: **partial via CAPM** at `quantitative.capm` and Fama-French raw data at `quantitative.helpers.get_fama_raw`. The `famafrench` extension scaffold exists — promote `get_fama_raw` into a proper provider there before adding multi-factor commands. New commands land in `quantitative.factor.*` (new submodule).

### 4.10 Regression and Econometrics

Formulas: OLS, WLS, GLS, ridge/lasso/elastic-net, robust, quantile, logistic, Newey-West/HAC SEs, ARMA/ARIMA/SARIMA, VAR/VECM, state-space/Kalman, HMM, Markov-switching, panel regressions, Fama-MacBeth, diagnostic tests.

Likely libs: `statsmodels`, `linearmodels`, `sklearn.linear_model`, `pykalman`, `hmmlearn`, `filterpy`.

OpenBBPort shape: y + X → structured record (params, std_errors, t_stats, p_values, conf_int, r_squared, residuals, fitted). Bigger than a DataFrame — use a typed standard model.

Landing: **substantially exists** at `econometrics.*`. Source-codebase additions (regularized regression, time-series models, state-space) extend `econometrics`. Recommend nested submodules: `econometrics.timeseries.*`, `econometrics.regression.*`, `econometrics.diagnostics.*`.

### 4.11 Volatility Models

Formulas: historical/EWMA, range estimators (Parkinson/Garman-Klass/Rogers-Satchell/Yang-Zhang), realized vol, bipower variation, ARCH/GARCH/EGARCH/GJR/FIGARCH, DCC-GARCH, stochastic vol, implied vol, IV surface.

Likely libs: `arch.arch_model`, `statsmodels.tsa`, `py_vollib`, `QuantLib`.

OpenBBPort shape: returns Series (or OHLC for range estimators) → vol Series + fitted-model param record; forecast endpoints add `horizon`.

Landing: **range estimators exist as helpers** in `technical.helpers` but are not router commands. ARCH/GARCH family does not exist. Decision: promote helpers to commands (`technical.volatility.*` or `quantitative.volatility.*`), and place ARCH/GARCH in a new `quantitative.volatility.garch.*` or in `econometrics.timeseries.*`.

### 4.12 Technical Indicators

Trend, momentum, volatility, volume, cycle, pivot/level (full list in agent inventory; ~80+ standard indicators).

Likely libs: `TA-Lib`, `pandas-ta`, `ta`, `finta`, `vectorbt`, `tulipy`.

OpenBBPort shape: OHLCV DataFrame + indicator-name + params → date-indexed Series or small DataFrame (e.g., MACD = line/signal/hist).

Landing: **27 indicators exist** at `technical.*`. Source-codebase additions extend `technical_router`. Be careful with TA-Lib vs pandas-ta divergence — OpenBBPort uses `pandas-ta-openbb` (forked from `pandas-ta`); if the source codebase uses `TA-Lib`, switching libraries can change numerical output for the same indicator. Document the underlying library in the docstring per indicator.

### 4.13 Signal Processing and Filters

Formulas: FFT/IFFT, PSD (Welch/periodogram), wavelet (DWT/CWT), Hodrick-Prescott, Christiano-Fitzgerald, Baxter-King, Kalman filter/smoother, Savitzky-Golay, Butterworth/Chebyshev, median/Hampel, detrending, fractional differencing.

Likely libs: `scipy.signal`, `scipy.fft`, `pywt`, `statsmodels.tsa.filters`, `pykalman`, `mlfinlab.features.fracdiff`.

OpenBBPort shape: Series + spec → Series (or trend+cycle tuple).

Landing: **does not exist**. Recommend either folding into `quantitative.rolling.*` (lightweight filters) or creating `quantitative.filter.*` (heavier-weight) — decision in §6.3.

### 4.14 Backtesting and Simulation

Concepts: single-asset / vectorized / event-driven backtests, walk-forward, CPCV, bootstrap (incl. stationary/block), trade stats, slippage/commission, position sizing, stop/target logic, tear sheets.

Likely libs: `vectorbt`, `backtrader`, `bt`, `zipline-reloaded`, `backtesting.py`, `nautilus_trader`, `pyfolio`, `quantstats.reports`.

OpenBBPort shape: prices/returns + signals + cost/sizing → composite result (equity curve + trade ledger + stats DataFrame + drawdown DataFrame). Does not fit a single `Data` standard model — needs a composite standard model with multiple `Data`-typed sub-fields.

Landing: **does not exist**. Recommend a new `backtest` top-level extension. **Caveat**: backtest engines typically carry heavy or opinionated dependency trees and stateful objects (strategies, brokers). Strongly prefer **vectorized-only** integration first; defer event-driven engines.

### 4.15 Monte Carlo and Stochastic Processes

Processes: GBM, ABM, OU/Vasicek, CIR, Heston, SABR, jump diffusion, bootstrap simulation, multivariate GBM, copula simulation; variance-reduction techniques; MC VaR/CVaR; MC option pricing.

Likely libs: `numpy.random`, `scipy.stats`, `QuantLib`, `stochastic`, `copulas`, `arch.bootstrap`, `pymc`.

OpenBBPort shape: process params + horizon + N + seed → (N × T) DataFrame or summary record. **Seed argument is mandatory.**

Landing: **does not exist**. Recommend `quantitative.simulation.*` submodule.

### 4.16 Options and Greeks

Formulas: Black-Scholes / Black-76, binomial/trinomial trees, finite-difference PDE, MC pricing, American (BS / LSM), exotics, implied vol, all Greeks (delta/gamma/vega/theta/rho + higher-order vanna/volga/charm/vomma/speed/color/zomma), IV surface fits (SVI/SABR), put-call parity.

Likely libs: `py_vollib`, `py_vollib_vectorized`, `QuantLib`, `mibian`.

OpenBBPort shape: pricing-parameter scalars (or DataFrame of contracts for vectorized) → scalar per metric or one-row Greeks record; IV-surface → strike-by-maturity DataFrame.

Landing: **does not exist in the quant extensions**. There is a `derivatives` extension in `extensions/derivatives/` — verify it covers options or extend it. If pricing primitives are net-new, recommend `derivatives.pricing.*` and `derivatives.greeks.*`.

### 4.17 Fixed Income Analytics

Formulas: price-from-yield, YTM, current yield, Macaulay/modified/effective duration, convexity, key-rate durations, DV01/PV01, spread duration, OAS, Z-spread, spot/zero bootstrap, forward rates, discount factors, curve interpolation (incl. NSS), CDS bootstrap, repo/forward bond, FRN pricing.

Likely libs: `QuantLib`, `numpy_financial`, `nelson_siegel_svensson`.

OpenBBPort shape: bond descriptor + curve → scalar; curve endpoints → tenor-indexed Series.

Landing: a `fixedincome` extension already exists at `extensions/fixedincome/` — extend it; do **not** put fixed-income math in the quant extensions.

### 4.18 ML Adjuncts (often co-located in modern quant codebases)

Feature engineering (lags/rolling/fracdiff/microstructural), labeling (triple-barrier/meta-labeling), purged CV / CPCV, feature importance (MDI/MDA/SFI/SHAP), tree/boosting/neural models, clustering for portfolios.

Likely libs: `sklearn`, `xgboost`, `lightgbm`, `catboost`, `mlfinlab`, `tsfresh`, `featuretools`, `shap`, `pytorch`, `tensorflow`.

Landing: **architectural caveat.** Stateful ML (trained models, model registries) does not fit OpenBBPort's stateless command pattern. Three options, in order of recommendation:

1. **Keep ML out of the router contract.** Expose ML *features* (e.g., fractional differentiation, triple-barrier labels) as stateless router commands; keep model training/prediction as importable pure functions only.
2. Build a separate **model-registry extension** that stores fitted-model handles by ID; router commands take an ID. Adds significant complexity (state, persistence, lifecycle).
3. Defer entirely.

### 4.19 Reporting / Tear-Sheet Composites

Concepts: performance / risk / round-trip / factor / capacity / position / Bayesian tear sheets.

Likely libs: `pyfolio.create_full_tear_sheet`, `quantstats.reports`.

Landing: tear sheets fit awkwardly because they bundle many metrics + charts. **Two integration shapes**:

- **Server-side composite**: a single command returns a multi-`Data`-field standard model. Heavier model, but one round-trip for users.
- **Client-side composition**: ship the primitives; users call multiple commands and compose. Lighter and more Pythonic.

Recommend client-side composition first; promote to server-side composites only if telemetry shows the bundle pattern dominates usage.

---

## 5. Quant Math ↔ OpenBBPort Mapping Worksheet

For each function in the source codebase, fill the row below before writing code. This is the **reconciliation table**.

| Source function | Source lib | Taxonomy §4.x | Existing OBB equivalent | Action | Target path | Data input form | Output shape | Standard model | Provider needed? | Notes |
|---|---|---|---|---|---|---|---|---|---|---|
| _e.g. `compute_sharpe(returns, rf=0)`_ | empyrical | §4.5 | `obb.quantitative.performance.sharpe_ratio` (rolling) | **reuse** OR **add scalar variant** | `obb.quantitative.performance.sharpe_summary` | returns Series | scalar | new `SharpeSummary` | no (caller-supplied) | decide rolling vs aggregate; pick annualization default |
| _e.g. `max_drawdown(prices)`_ | quantstats | §4.4 | — | **add** | `obb.quantitative.risk.max_drawdown` | prices or returns Series | scalar + DD series | new `MaxDrawdown` | no | needs price-vs-returns convention switch |
| _…_ | | | | | | | | | | |

**Action vocabulary** (use exactly one per row): `reuse`, `alias`, `extend`, `add`, `defer`, `reject`.

- `reuse` — existing OBB command covers it; remove from source codebase or call from it.
- `alias` — existing OBB command covers it, but expose under the source-codebase name for migration.
- `extend` — existing OBB command exists but needs new param/field; bump the standard model.
- `add` — new OBB command. Most common action.
- `defer` — out of scope for first merge.
- `reject` — does not fit (e.g., stateful ML, unstandardizable I/O).

Conservatively, plan for the table to have 50–200 rows for a meaningful quant codebase.

---

## 6. Decision Points (resolve before coding)

The merge has no defensible default for these. Each one must be explicitly answered and recorded.

### 6.1 Where new commands land

- **Option A — extend existing extensions** (`quantitative`, `technical`, `econometrics`). Lower friction. Risk: extension scope creep, especially for `quantitative`.
- **Option B — add new sibling extensions** (`portfolio`, `backtest`, `factor`, `simulation`). Cleaner boundaries; more `pyproject.toml` files to maintain; multiple new entry points.
- **Recommendation**: A for everything that fits §4.1–4.7, §4.9, §4.11–4.13; B for §4.8 (portfolio), §4.14 (backtest), §4.15 (simulation) when these are large.

### 6.2 Pure-function vs router-command boundary

- Does every formula get a router endpoint, or only the "high-value" ones (data-fetching, composite, user-facing)?
- **Recommendation**: keep all formulas available as pure functions in `openbb_<ext>/<submodule>/`; promote to router commands only when the formula owns its data fetch or is a known user-facing primitive. Avoid 200+ router commands for trivial wrappers.

### 6.3 Data input form (the single biggest design call)

OpenBBPort router commands traditionally take symbol + date range and own the fetch via a provider. Most quant libraries take pre-fetched arrays/DataFrames.

Three options, each with tradeoffs:

1. **Symbol + provider** (OpenBB-native). Forces every quant endpoint into the provider model. Means every quant command needs a provider (or a "passthrough" provider that accepts caller-supplied series). Pure.
2. **Caller-supplied `list[Data]`**. Pattern already used by `quantitative.*` extension (see `stats_router.py`, `rolling_router.py` — they take `data: list[Data], target: str`). No provider involvement. Diverges from the canonical pattern but is the lowest-friction landing for math primitives.
3. **Both**: endpoint accepts either, dispatches internally. Doubles the test surface; user confusion about which path is being taken.
- **Recommendation**: option 2 for all primitives (matches existing `quantitative` precedent), option 1 only for commands that conceptually fetch (e.g., "compute Sharpe of AAPL 2020–2024"). Do not implement option 3.

### 6.4 Returns convention

- **Default**: simple returns. Empyrical/quantstats convention; matches existing `quantitative.performance.*`.
- Each function declares whether it requires log vs simple in its docstring.
- Provide one canonical helper `prices_to_returns(prices, kind: Literal["simple","log"] = "simple")` in `quantitative.helpers`.

### 6.5 Annualization

- **Default**: `periods_per_year=252` (trading days), explicit argument, no auto-inference. Auto-infer-from-index is appealing but silently wrong on irregular indexes.
- All performance metrics expose `periods_per_year`. No exceptions.

### 6.6 Risk-free rate handling

- Accept scalar (annualized) OR Series aligned to returns.
- Internally de-annualize to the return frequency.
- No silent default of `0`; require explicit `risk_free_rate=0.0` so it shows up in calls.

### 6.7 Benchmark handling

- Accept benchmark as a Series of returns (not symbol). If symbol-based fetch is needed, add a separate convenience command that fetches and calls the primitive.
- Alignment: inner-join on dates. Document the policy in each command's docstring.

### 6.8 NaN / missing-data policy

- **Default**: drop NaNs from inputs, emit warning to `OBBject.warnings`.
- Per-function override via `nan_policy: Literal["drop","raise","propagate"] = "drop"`.

### 6.9 Library version pinning

- `empyrical`, `quantstats`, `pyfolio`, `arch`, `statsmodels`, `linearmodels`, `pandas-ta-openbb` — pin in `pyproject.toml` of the receiving extension. Document formula provenance in each command's docstring so user-visible numbers are stable across upgrades.
- Avoid pulling in heavy deps (`cvxpy`, `pytorch`) at default install; use poetry extras.

### 6.10 Backwards-compatible naming

Empyrical, quantstats, pyfolio use overlapping-but-different names for the same metric (e.g., `sharpe_ratio` vs `sharpe`, `value_at_risk` vs `var`).

- **Recommendation**: pick OpenBBPort canonical name per metric; provide a `helpers/aliases.py` for user migration; document in changelog. Do not expose multiple names in the router itself.

---

## 7. Risks Aggravated by the Merge

From `04_risk_refactor_notes.md`, the modules most affected by adding a large batch of new commands and providers:

1. **`openbb_core.app.static.package_builder`** (very high risk). Generates SDK source files. A batch merge multiplies the surface area of generation bugs (type-annotation stringification, provider-field deduplication, charting-flag insertion). **Mitigation**: snapshot tests on generated SDK files; CI step that imports the generated package and validates method signatures.
2. **`openbb_core.app.provider_interface`** (very high risk). Merges provider schemas into command signatures. Adding many new standard models and providers exposes edge cases here. **Mitigation**: fixture-based tests with 2–3 mock providers; snapshot generated params/data models.
3. **`openbb_core.app.command_runner`** (high risk). Central execution path. Do not "simplify" kwargs handling — REST/SDK divergence relies on its specific shape.
4. **`openbb_core.api.router.commands`** (high risk). REST wrapper. Test REST and SDK equivalence on a sample of new commands.

Cross-cutting risks specific to this kind of merge:

- **Provider-field name collisions.** If multiple new providers expose `volatility` with different semantics, the merged `extra_params` field is one entry but provider behavior diverges. **Mitigation**: provider-specific field names (`volatility_annualized`, `volatility_rolling_30d`).
- **Standard-model drift.** Standard models added carelessly multiply quickly. Each new model is a binding-key invariant (see §3.3). **Mitigation**: standard-model review gate in PR.
- **Default-install bloat.** Quant libraries are heavy. Don't pull `cvxpy`, `torch`, `QuantLib` into the base install. Use poetry extras and lazy imports.
- **Numerical-equivalence regressions.** If a TA indicator switches backing library (TA-Lib → pandas-ta), numbers shift. **Mitigation**: golden-output regression tests for any indicator with a known existing reference.

---

## 8. Merge Workflow

1. **Inventory.** Populate the §5 worksheet from the source codebase. Stop here if the table is empty; nothing to merge.
2. **Categorize.** For every row pick an action from `{reuse, alias, extend, add, defer, reject}`.
3. **Resolve decisions §6.** Record answers in a `merge_decisions.md` alongside this spec.
4. **Land in slices.** One §4 category per PR. Each PR:
   - Adds standard models in `openbb_platform/core/openbb_core/provider/standard_models/`.
   - Adds router commands (and submodule router if new).
   - Adds provider fetcher(s) only if data is fetched (else use caller-supplied-data pattern per §6.3).
   - Updates `pyproject.toml` entry points and dependencies.
   - Regenerates the static SDK package (`PackageBuilder().build()`) and commits the diff.
   - Adds tests: fetcher contract tests, standard-model validation tests, golden-output regression tests, SDK generation snapshot, REST/SDK equivalence on at least one command.
5. **CI gates** (recommended additions):
   - SDK regeneration drift check — fail if `obb/package/` differs from a clean rebuild.
   - Default-install size check — fail if base install grows beyond a budget.
   - Cross-provider behavior tests for any merged field shared across providers.
6. **Documentation.** Each new command's docstring includes: formula expression (or citation), input convention (prices vs returns; simple vs log), annualization handling, NaN policy, backing library + version. The reference JSON (`openbb/package/reference/extensions.json`) regenerates automatically.

---

## 9. Open Slots (fill from the source codebase)

These are intentionally blank until the source code is in hand. Drop QuantStuff's structure here:

```
SOURCE_CODEBASE_NAME:           ____________________
SOURCE_PYTHON_VERSION:          ____________________
SOURCE_TOP_LEVEL_MODULES:       ____________________
SOURCE_PUBLIC_API_SURFACE:      ____________________
SOURCE_PRIMARY_LIBRARIES:       ____________________
SOURCE_LICENSE:                 ____________________

CATEGORIES_PRESENT_FROM_§4:     ____________________ (e.g. 4.4, 4.5, 4.8, 4.14)
CATEGORIES_NET_NEW_TO_OBB:      ____________________
CATEGORIES_OVERLAPPING_OBB:     ____________________

DATA_INPUT_FORM:                ____________________ (Series? DataFrame? OHLCV?)
RETURNS_CONVENTION:             ____________________ (simple/log/mixed)
ANNUALIZATION_BASIS:            ____________________
HAS_BACKTESTER:                 ____________________
HAS_OPTIMIZER:                  ____________________
HAS_PROVIDER_FETCH_CODE:        ____________________ (or BYO-data only)
HAS_ML_MODELS:                  ____________________

DECISIONS §6.1–6.10:            ____________________ (record answers)
```

Once filled, this spec is executable: every row resolves to a §4 category, a §3 landing surface, a §6 decision, and a §8 workflow slot.

---

## 10. Quick-Reference Cheat Sheet

| Question | Answer |
|---|---|
| Where do new commands declared in pyproject? | `[tool.poetry.plugins."openbb_core_extension"]` |
| Where do new providers declared in pyproject? | `[tool.poetry.plugins."openbb_provider_extension"]` |
| Where do standard models live? | `openbb_platform/core/openbb_core/provider/standard_models/` |
| What's the command signature? | `async def f(cc, provider_choices, standard_params, extra_params) -> OBBject` |
| What's the binding key? | The class-name root of the standard model (e.g. `Sharpe`) |
| What must be regenerated after adding a command? | The static SDK via `PackageBuilder().build()` (`openbb/package/`) |
| Where do helpers / pure functions go? | `openbb_<extension>/helpers.py` or submodule (e.g. `openbb_quantitative/statistics.py`) |
| How is REST exposed? | Automatically by FastAPI from the router; path = SDK path |
| What's the default returns convention? | Simple (project-wide); each command documents in docstring |
| What's the default annualization? | `periods_per_year=252`, explicit arg |
| Default NaN policy? | Drop, emit warning |
| Heavy deps go where? | Poetry `[tool.poetry.extras]`, lazy imports |

---

*End of spec sheet. Pair this document with the source codebase's directory tree and public API listing to produce the §5 worksheet; once that is filled, implementation is mechanical.*
