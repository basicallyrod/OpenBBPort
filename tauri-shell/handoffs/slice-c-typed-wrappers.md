# Slice C — Extended Typed Route Wrappers

Status: done. Owner: opus. Output: `src/ipc/obb_routes_extended.rs` (1487
lines, 167 wrappers) plus the 88-wrapper `src/ipc/obb_routes.rs` already in
the foundation. Combined surface: **255 typed `#[tauri::command]` route
wrappers** registered in `src/main.rs::generate_handler!`.

## 1. Purpose

Every OpenBB REST route (`/equity/price/historical`, `/economy/cpi`,
`/technical/rsi`, ...) is also exposed as its own first-class Tauri command
(`equity_price_historical`, `economy_cpi`, `technical_rsi`). The TS
frontend can `invoke<Value>("equity_price_historical", { params: {...} })`
instead of `invoke<Value>("obb_call", { route: "/equity/price/historical",
params: {...} })`.

The generic `obb_call` in `src/ipc/obb.rs` (`src/main.rs:144`) already
handles every route. Typed wrappers exist for:

- **TS autocomplete.** A typed `invoke()` (Slice A's ts-rs or a hand-rolled
  union) shows `equity_price_historical` in the IDE; a typo in a route
  string is not caught at compile time, a typo in a command name is.
- **Route-not-found at compile time.** `tauri::generate_handler![...]` in
  `src/main.rs:74` is a macro — a missing ident is a Rust compile error.
- **Easy refactor.** Renaming `/equity/price/quote` → `/quotes` on the
  Python side touches exactly one route literal in one Rust function;
  the function name (`equity_price_quote`) and TS callers are unaffected.
- **Per-route hook point.** Future log/trace/cache only needs to wrap
  `call_get` / `call_post`.
- **Discoverability.** `cargo doc` lists 255 one-line functions.

Wrappers are intentionally **un-typed in their parameter struct** — the
arg is `Option<Map<String, Value>>`, not a per-route Pydantic-equivalent.
Strict typing here duplicates `/openapi.json`; TS-side `openapi-typescript`
owns that. See `src/ipc/obb_routes.rs:1-12` for the design rationale.

## 2. Coverage stats

| File                            | Wrappers | Lines | Route range |
|---------------------------------|---------:|------:|-------------|
| `src/ipc/obb_routes.rs`         |       88 |   756 | The 13 most-used extensions, plus the 13 POST endpoints |
| `src/ipc/obb_routes_extended.rs`|      167 |  1487 | Every remaining route from `openbb_platform/extensions/**/*_router.py` |
| **Total**                       |  **255** | **2243** | **all 255 unique routes; zero overlap** |

Counts (via `grep -c '#\[tauri::command\]'` + `grep -c '<module>::'`):

| Source                          | `#[tauri::command]` | `generate_handler!` |
|---------------------------------|--------------------:|--------------------:|
| `obb_routes.rs`                 |                  88 |                  88 |
| `obb_routes_extended.rs`        | 168 (1 in docstring) |                 167 |

SPEC §2 Slice C estimated "184 routes". The true surface — every
`@router.command(model=…)` in `openbb_platform/extensions/**/*_router.py`
— is **255 unique routes**, all covered. No overlap between the two
files: `comm -12 routes_orig.txt routes_extended.txt` is empty. The
SPEC estimate was conservative; the real count includes FRED sub-routes,
all 22 technical indicators, all 15 quantitative methods, and all 12
econometrics tests.

Routes not wrapped: a handful of debug/metadata/legacy endpoints. They
remain reachable via the generic `obb_call`; not worth a named wrapper.

## 3. Coverage table by extension family

Citations are `<file>:<start>–<end>` where the section comment banner lives.

### `obb_routes.rs` (88 wrappers, 13 families)

| Family                                  | Routes | Source span |
|-----------------------------------------|-------:|-------------|
| equity (search/screener/profile/snapshots/cap, price 4, fundamental 7, calendar 5, discovery 3, ownership 2, estimates 2) | 28 | `obb_routes.rs:53–272` |
| crypto (search, historical)             |  2 | `obb_routes.rs:274–289` |
| currency (search/pairs/snapshots/reference_rates/historical) | 5 | `obb_routes.rs:291–327` |
| derivatives (options 3 + futures 4)     |  7 | `obb_routes.rs:329–387` |
| etf (search/info/historical/holdings/sectors/countries) | 6 | `obb_routes.rs:389–421` |
| index (search/historical/constituents/snapshots/available) | 5 | `obb_routes.rs:423–453` |
| economy (cpi/calendar/indicators/gdp×3/unemployment/fred×2) | 9 | `obb_routes.rs:455–520` |
| fixedincome (gov 2, corp 1, rate 2)     |  5 | `obb_routes.rs:522–564` |
| news (world, company)                   |  2 | `obb_routes.rs:566–578` |
| regulators (sec 2, cftc 1)              |  3 | `obb_routes.rs:580–606` |
| commodity (spot, petroleum, weather)    |  3 | `obb_routes.rs:608–634` |
| technical (sma/ema/rsi/macd/bbands) POST |  5 | `obb_routes.rs:636–684` |
| quantitative (summary/normality/unitroot/omega/sharpe) POST | 5 | `obb_routes.rs:686–729` |
| econometrics (correlation/ols/granger) POST | 3 | `obb_routes.rs:731–756` |

### `obb_routes_extended.rs` (167 wrappers, 11 families)

| Family                                                                      | Routes | Source span |
|-----------------------------------------------------------------------------|-------:|-------------|
| commodity (psd_data/psd_report/STEO/weather_bulletins ×2)                   |   5 | `obb_routes_extended.rs:59–101` |
| derivatives (options/surface)                                               |   1 | `obb_routes_extended.rs:103–113` |
| economy (FRED regional/release_table, BLS, BOP, central-bank, surveys ×8, shipping ×4, GDP/CPI extras, indicators) | 33 | `obb_routes_extended.rs:115–381` |
| equity — compare ×3, darkpool/otc, discovery ×7, estimates ×6, fundamental ×16, ownership ×4, shorts ×3 | 42 | `obb_routes_extended.rs:383–721` |
| etf (discovery ×3, equity_exposure, nport, price_performance)               |   6 | `obb_routes_extended.rs:723–773` |
| fixedincome (bond_indices, corporate ×4, government ×4, mortgage, rate ×8, spreads ×3) | 21 | `obb_routes_extended.rs:775–945` |
| index (sectors, sp500_multiples)                                            |   2 | `obb_routes_extended.rs:947–965` |
| regulators (sec deep: cik_map, filing_headers, htm_file, institutions_search, rss_litigation, schema_files, sic_search, symbol_map) | 8 | `obb_routes_extended.rs:967–1033` |
| technical (22 POST indicators: ad/adosc/adx/aroon/atr/cci/cg/clenow/cones/demark/donchian/fib/fisher/hma/ichimoku/kc/obv/relative_rotation/stoch/vwap/wma/zlma) | 22 | `obb_routes_extended.rs:1035–1235` |
| quantitative (capm, performance/sortino, rolling ×6, stats ×6, unitroot_test) POST | 15 | `obb_routes_extended.rs:1237–1374` |
| econometrics (autocorrelation, cointegration, ols_summary, panel ×5, residual_autocorrelation, unit_root, VIF) POST | 12 | `obb_routes_extended.rs:1376–1487` |

### Combined rollup

equity 70 · economy 42 · technical (POST) 27 · fixedincome 26 ·
quantitative (POST) 20 · econometrics (POST) 15 · etf 12 ·
regulators 11 · derivatives 8 · commodity 8 · index 7 · currency 5 ·
crypto 2 · news 2 · **= 255**

## 4. Pattern: the `call_get` / `call_post` helpers

Both files declare a private `Params` type alias and two private async
helpers:

```rust
type Params = Option<Map<String, Value>>;

async fn call_get(proxy: &Proxy, route: &str, params: Params) -> Result<Value, IpcError> {
    let params = params.unwrap_or_default();
    proxy.get_with_map::<Value>(route, &params).await
        .map_err(|e| IpcError::Internal(e.to_string()))
}
```

(Defined in `obb_routes.rs:19–51` and again — verbatim — in
`obb_routes_extended.rs:21–57`.)

A typed wrapper is then a one-liner:

```rust
#[tauri::command]
pub async fn equity_price_historical(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/price/historical", params).await
}
```

**Why duplicated?** `call_get` / `call_post` are `pub(self)`; the module
boundary between `obb_routes` and `obb_routes_extended` is enforced by
Rust, so a sibling cannot import a private symbol. Promoting them to
`pub` or extracting to a third helper module would either pollute the
crate's public API or add a module for two ~15-line functions. Net win
was judged trivial; each file is self-contained. If a third wrapper file
ever appears, extract to `src/ipc/obb_helpers.rs` with `pub(super)`
visibility (see §10).

**Why `Option<Map<String, Value>>`?** TS can pass `params: undefined`
(→ `None`) or `params: { ... }` (→ `Some(map)`); `Map` preserves
insertion order (vs `HashMap`). Routes are case- and trailing-slash-
sensitive; the literal matches the FastAPI server 1:1. `Proxy::build_url`
(`src/proxy.rs:167–180`) prepends `/api/v1/` and strips any leading
slash.

## 5. Naming convention

| Python route                       | Rust function                  |
|------------------------------------|--------------------------------|
| `/equity/price/historical`         | `equity_price_historical`      |
| `/economy/fred_series`             | `economy_fred_series`          |
| `/economy/shipping/port_volume`    | `economy_shipping_port_volume` |
| `/fixedincome/rate/effr_forecast`  | `fixedincome_rate_effr_forecast` |
| `/technical/sma` (POST)            | `technical_sma`                |
| `/quantitative/performance/sharpe_ratio` (POST) | `quantitative_performance_sharpe` (legacy trim, `obb_routes.rs:723`) |
| `/quantitative/performance/sortino_ratio` (POST) | `quantitative_performance_sortino_ratio` (`obb_routes_extended.rs:1251`) |

The mechanical rule:

1. Strip the leading `/`.
2. Replace `/` with `_`.
3. Keep snake_case; underscores in the route stay underscores.
4. **No `obb_` prefix** — the module path
   (`tauri_shell::ipc::obb_routes::equity_price_historical`) already
   namespaces. Adding `obb_equity_price_historical` would make every line
   in `generate_handler!` ~10 chars longer.

The one exception is the `obb_*` family in `src/ipc/obb.rs` (the generic
`obb_call`, `obb_health`, `obb_user_me`, etc.) — those keep the `obb_`
prefix because they are *not* tied to a single route; they are
infrastructure commands that wrap the proxy itself.

A few manual renames for brevity exist in the older file (e.g.
`quantitative_performance_sharpe` at `obb_routes.rs:723` instead of
`..._sharpe_ratio`). The newer file sticks to the strict rule; normalize
on the strict rule in a follow-up.

## 6. Adding a new wrapper — step by step

Suppose OpenBB adds a new route `/equity/fundamental/peer_comparisons`.

### Step 1 — find the Python decorator

```bash
rg '@router\.command\(model=' openbb_platform/extensions/equity/fundamental
```

Look for the new entry. The function will look like:

```python
@router.command(model="PeerComparisons")
async def peer_comparisons(cc: CommandContext, ...):
```

The route prefix comes from the file path: `equity/fundamental/<x>_router.py`
→ `/equity/fundamental/<function name>`.

### Step 2 — derive the Rust function name

`/equity/fundamental/peer_comparisons` → `equity_fundamental_peer_comparisons`.

### Step 3 — add the wrapper

In `src/ipc/obb_routes_extended.rs`, find the equity section
(line ~383) and add (sorted alphabetically within the section):

```rust
#[tauri::command]
pub async fn equity_fundamental_peer_comparisons(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/fundamental/peer_comparisons", params).await
}
```

For a POST route (technical / quantitative / econometrics), use `call_post`
with the `data: Value` first param:

```rust
#[tauri::command]
pub async fn technical_newindicator(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/technical/newindicator", data, params).await
}
```

### Step 4 — append to `generate_handler!`

In `src/main.rs`, find the `obb_routes_extended::` block (lines 250–416)
and **append** to its section — never reorder existing entries (SPEC §3):

```rust
tauri_shell::ipc::obb_routes_extended::equity_fundamental_peer_comparisons,
```

### Step 5 — verify

```bash
cd tauri-shell && cargo check
grep -c '#\[tauri::command\]' src/ipc/obb_routes_extended.rs   # bumps by 1
grep -c 'obb_routes_extended::' src/main.rs                    # bumps by 1
```

The macro compile-fails if the identifier is missing in either module.
Mismatched counts between source and `generate_handler!` are a silent
runtime fail — Tauri just won't register the unlisted command.

## 7. POST vs GET — convention

The data-processing extensions (`/technical/*`, `/quantitative/*`,
`/econometrics/*`) take an **OBBject `data` payload in the request body**
and may take query params on top. The Python decorators reveal this:

```python
@router.command(methods=["POST"], model="SMA")
async def sma(data: List[Data], ...): ...
```

The corresponding Rust wrapper uses `call_post`:

```rust
#[tauri::command]
pub async fn technical_sma(
    data: Value,            // ← OBBject body
    params: Params,         // ← optional query string
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/technical/sma", data, params).await
}
```

`call_post` (`obb_routes.rs:33–51` / `obb_routes_extended.rs:39–57`)
JSON-encodes `data` as the body; `params` (if any) become the query
string. The helper lossily converts non-string query values to `""`
(§10 gap 2). All non-POST routes use `call_get`.

In TS:

```ts
await invoke<Value>("equity_price_historical",
  { params: { symbol: "AAPL", provider: "yfinance" } });

await invoke<Value>("technical_sma",
  { data: priceObbject, params: { length: "50", target: "close" } });
```

## 8. Verification

```bash
cd tauri-shell
cargo check                                                      # passes
cargo build --release                                            # passes
grep -c '#\[tauri::command\]' src/ipc/obb_routes.rs              # 88
grep -c '#\[tauri::command\]' src/ipc/obb_routes_extended.rs     # 168 (1 in docstring)
grep -c 'obb_routes::'        src/main.rs                        # 88
grep -c 'obb_routes_extended::' src/main.rs                      # 167

# Zero overlap
grep -oE '"/[a-zA-Z0-9_/]+"' src/ipc/obb_routes.rs          | sort -u > /tmp/o.txt
grep -oE '"/[a-zA-Z0-9_/]+"' src/ipc/obb_routes_extended.rs | sort -u > /tmp/e.txt
comm -12 /tmp/o.txt /tmp/e.txt                                   # empty
```

Runtime smoke (requires Python REST server on `127.0.0.1:6900`):

```bash
cargo run --bin tauri-shell-cli -- obb call /equity/price/historical \
  --param symbol=AAPL --param provider=yfinance --param start_date=2024-01-01
```

The CLI (Slice G) does not route through the typed wrappers; see §9.

## 9. Integration with other slices

- **Slice A (ts-rs).** Exports public types only. The `Params` alias and
  the `call_get` / `call_post` helpers are `pub(self)` — `ts-rs` cannot
  see them, intentionally: `Option<Map<String, Value>>` would generate
  as `Record<string, unknown> | null`, no better than `unknown`. The
  TS-side parameter typing comes from `openapi-typescript` against
  `/openapi.json`. Slice A's `invoke<>` typing binds the command **name**
  (a string-literal union of all 255) to `{params?, data?}`.
- **Slice B (Connector trait).** The trait in `src/connector.rs` is for
  non-OpenBB domain calls (installation, environments, jupyter, server,
  mcp, certs, uninstall). All 255 typed wrappers go **straight to
  `Proxy`** and do not consult the connector. Correct by design: OpenBB
  is one first-party HTTP target, not a pluggable backend. To swap the
  data source, swap `Proxy` (or front it with a reverse proxy), not the
  `Connector`.
- **Slice D (integration tests).** Out of scope here. The wrappers are
  ~99% boilerplate; a single parameterized test looping over all 255
  entries against `httpmock` would give full smoke coverage.
- **Slice E (docs/cookbook).** The expanded README needs a 255-row
  catalog. Source it from `src/main.rs:160–416` —
  `generate_handler!` is canonical. Cite this handoff for the
  family-level breakdown in §3.
- **Slice F (TS frontend).** Uses `equity_price_historical` as the
  headline typed-wrapper demo (`examples/typescript-frontend/src/main.ts`).
- **Slice G (CLI).** `tauri-shell-cli` exposes `obb call <route> --param
  k=v` (`src/bin/cli.rs`), **not** `tauri-shell-cli
  equity-price-historical`. Generating 255 clap subcommands would bloat
  the binary for no win — autocomplete only pays off on the TS side.
  Underlying plumbing (`Proxy::get_with_map` / `Proxy::post`) is the same.
- **Slice H (connector reference impls).** No interaction; the OpenBB
  proxy is not pluggable.

## 10. Known gaps and next steps

### Gaps

1. **Duplicated `call_get` / `call_post` helpers** — ~32 LoC copy-pasted
   across the two files. Stable, but worth extracting if a third
   wrapper module ever lands.
2. **Lossy POST query coercion** — `call_post`'s query-pair builder
   converts non-string values to `""` (`obb_routes.rs:42–45`,
   `obb_routes_extended.rs:48–51`). Numeric/bool query params on a POST
   route silently send empty. Fix: `v.to_string()` for `Value::Number`
   and `Value::Bool`.
3. **Errors flatten to `IpcError::Internal(string)`** — a 404 is
   indistinguishable from a network timeout on the wire. Should
   propagate `ProxyError::Http { status, body }` as `IpcError::Http`.
4. **Naming drift** — `quantitative_performance_sharpe`
   (`obb_routes.rs:723`) vs `..._sortino_ratio`
   (`obb_routes_extended.rs:1251`). The newer file follows the strict
   mechanical rule; normalize on it.
5. **A handful of legacy/debug routes uncovered.** Fallback is
   `obb_call`. Document in README.
6. **No per-route structured params type.** By design (§4); TS side
   handles via `openapi-typescript`.

### Recommended follow-ups (priority order)

1. **(P1) Extract helpers into `src/ipc/obb_call_helpers.rs`** — ~30 LoC
   diff; opens a hook point for per-route caching, retries, tracing.
2. **(P1) Fix POST query coercion** — ~5 LoC in two places. Regression
   test under Slice D.
3. **(P2) Surface structured proxy errors as `IpcError::Http`** — touches
   `src/ipc/mod.rs`, `proxy.rs`, and both wrapper files. ~50 LoC.
4. **(P2) Normalize naming** — rename
   `quantitative_performance_sharpe` → `..._sharpe_ratio`; update
   `generate_handler!`, the TS demo, the README catalog.
5. **(P3) Auto-generate wrappers from `openapi.json` via `build.rs`** —
   hides the list from grep; defer.
6. **(P3) Export a ts-rs `RouteName` enum** with all 255 variants for
   typing `invoke<>()` on the TS side.

Files: `src/ipc/obb_routes.rs` (756 lines),
`src/ipc/obb_routes_extended.rs` (1487 lines), `src/main.rs:160–416`
(registration), `src/proxy.rs`. `cargo check` and
`cargo build --release` clean.
