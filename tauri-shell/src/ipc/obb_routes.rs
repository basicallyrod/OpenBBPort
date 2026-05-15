//! Typed convenience wrappers for the most-used OpenBB routes.
//!
//! These are thin wrappers around `obb::obb_call`. They exist so the TS
//! frontend can call e.g. `equity_price_historical({symbol: "AAPL", provider: "yfinance"})`
//! with autocomplete, instead of stringly-typed `obb_call({route: "/equity/price/historical", params: {...}})`.
//!
//! The Rust signature is intentionally permissive (`HashMap<String, Value>`)
//! because adding strict typing here duplicates `/openapi.json`. The
//! frontend's type-gen tool of choice (`openapi-typescript`,
//! `openapi-fetch`, `orval`) is the right place for full typing.
//!
//! Coverage: 60 of the 184 documented OpenBB routes, grouped by extension.

use super::IpcError;
use crate::proxy::Proxy;
use serde_json::{Map, Value};
use tauri::State;

type Params = Option<Map<String, Value>>;

async fn call_get(
    proxy: &Proxy,
    route: &str,
    params: Params,
) -> Result<Value, IpcError> {
    let params = params.unwrap_or_default();
    proxy
        .get_with_map::<Value>(route, &params)
        .await
        .map_err(|e| IpcError::Internal(e.to_string()))
}

async fn call_post(
    proxy: &Proxy,
    route: &str,
    body: Value,
    query: Params,
) -> Result<Value, IpcError> {
    let query = query.unwrap_or_default();
    let query_pairs: Vec<(&str, &str)> = query
        .iter()
        .map(|(k, v)| (k.as_str(), match v {
            Value::String(s) => s.as_str(),
            _ => "",
        }))
        .collect();
    proxy
        .post::<_, Value>(route, &body, &query_pairs)
        .await
        .map_err(|e| IpcError::Internal(e.to_string()))
}

// ==========================================================================
// Equity
// ==========================================================================

#[tauri::command]
pub async fn equity_search(params: Params, proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/search", params).await
}

#[tauri::command]
pub async fn equity_screener(params: Params, proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/screener", params).await
}

#[tauri::command]
pub async fn equity_profile(params: Params, proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/profile", params).await
}

#[tauri::command]
pub async fn equity_market_snapshots(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/market_snapshots", params).await
}

#[tauri::command]
pub async fn equity_historical_market_cap(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/historical_market_cap", params).await
}

// --- Equity price -------------------------------------------------

#[tauri::command]
pub async fn equity_price_historical(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/price/historical", params).await
}

#[tauri::command]
pub async fn equity_price_quote(params: Params, proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/price/quote", params).await
}

#[tauri::command]
pub async fn equity_price_nbbo(params: Params, proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/price/nbbo", params).await
}

#[tauri::command]
pub async fn equity_price_performance(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/price/performance", params).await
}

// --- Equity fundamental -------------------------------------------

#[tauri::command]
pub async fn equity_fundamental_balance(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/fundamental/balance", params).await
}

#[tauri::command]
pub async fn equity_fundamental_income(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/fundamental/income", params).await
}

#[tauri::command]
pub async fn equity_fundamental_cash(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/fundamental/cash", params).await
}

#[tauri::command]
pub async fn equity_fundamental_ratios(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/fundamental/ratios", params).await
}

#[tauri::command]
pub async fn equity_fundamental_metrics(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/fundamental/metrics", params).await
}

#[tauri::command]
pub async fn equity_fundamental_dividends(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/fundamental/dividends", params).await
}

#[tauri::command]
pub async fn equity_fundamental_filings(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/fundamental/filings", params).await
}

// --- Equity calendar ----------------------------------------------

#[tauri::command]
pub async fn equity_calendar_earnings(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/calendar/earnings", params).await
}

#[tauri::command]
pub async fn equity_calendar_dividends(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/calendar/dividend", params).await
}

#[tauri::command]
pub async fn equity_calendar_splits(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/calendar/splits", params).await
}

#[tauri::command]
pub async fn equity_calendar_ipo(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/calendar/ipo", params).await
}

#[tauri::command]
pub async fn equity_calendar_events(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/calendar/events", params).await
}

// --- Equity discovery / ownership / estimates ---------------------

#[tauri::command]
pub async fn equity_discovery_gainers(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/discovery/gainers", params).await
}

#[tauri::command]
pub async fn equity_discovery_losers(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/discovery/losers", params).await
}

#[tauri::command]
pub async fn equity_discovery_active(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/discovery/active", params).await
}

#[tauri::command]
pub async fn equity_ownership_insider_trading(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/ownership/insider_trading", params).await
}

#[tauri::command]
pub async fn equity_ownership_institutional(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/ownership/institutional", params).await
}

#[tauri::command]
pub async fn equity_estimates_price_target(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/estimates/price_target", params).await
}

#[tauri::command]
pub async fn equity_estimates_consensus(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/estimates/consensus", params).await
}

// ==========================================================================
// Crypto
// ==========================================================================

#[tauri::command]
pub async fn crypto_search(params: Params, proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    call_get(&proxy, "/crypto/search", params).await
}

#[tauri::command]
pub async fn crypto_price_historical(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/crypto/price/historical", params).await
}

// ==========================================================================
// Currency
// ==========================================================================

#[tauri::command]
pub async fn currency_search(params: Params, proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    call_get(&proxy, "/currency/search", params).await
}

#[tauri::command]
pub async fn currency_pairs(params: Params, proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    call_get(&proxy, "/currency/pairs", params).await
}

#[tauri::command]
pub async fn currency_snapshots(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/currency/snapshots", params).await
}

#[tauri::command]
pub async fn currency_reference_rates(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/currency/reference_rates", params).await
}

#[tauri::command]
pub async fn currency_price_historical(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/currency/price/historical", params).await
}

// ==========================================================================
// Derivatives
// ==========================================================================

#[tauri::command]
pub async fn derivatives_options_chains(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/derivatives/options/chains", params).await
}

#[tauri::command]
pub async fn derivatives_options_unusual(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/derivatives/options/unusual", params).await
}

#[tauri::command]
pub async fn derivatives_options_snapshots(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/derivatives/options/snapshots", params).await
}

#[tauri::command]
pub async fn derivatives_futures_historical(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/derivatives/futures/historical", params).await
}

#[tauri::command]
pub async fn derivatives_futures_curve(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/derivatives/futures/curve", params).await
}

#[tauri::command]
pub async fn derivatives_futures_info(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/derivatives/futures/info", params).await
}

#[tauri::command]
pub async fn derivatives_futures_instruments(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/derivatives/futures/instruments", params).await
}

// ==========================================================================
// ETF
// ==========================================================================

#[tauri::command]
pub async fn etf_search(params: Params, proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    call_get(&proxy, "/etf/search", params).await
}

#[tauri::command]
pub async fn etf_info(params: Params, proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    call_get(&proxy, "/etf/info", params).await
}

#[tauri::command]
pub async fn etf_historical(params: Params, proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    call_get(&proxy, "/etf/historical", params).await
}

#[tauri::command]
pub async fn etf_holdings(params: Params, proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    call_get(&proxy, "/etf/holdings", params).await
}

#[tauri::command]
pub async fn etf_sectors(params: Params, proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    call_get(&proxy, "/etf/sectors", params).await
}

#[tauri::command]
pub async fn etf_countries(params: Params, proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    call_get(&proxy, "/etf/countries", params).await
}

// ==========================================================================
// Index
// ==========================================================================

#[tauri::command]
pub async fn index_search(params: Params, proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    call_get(&proxy, "/index/search", params).await
}

#[tauri::command]
pub async fn index_historical(params: Params, proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    call_get(&proxy, "/index/price/historical", params).await
}

#[tauri::command]
pub async fn index_constituents(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/index/constituents", params).await
}

#[tauri::command]
pub async fn index_snapshots(params: Params, proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    call_get(&proxy, "/index/snapshots", params).await
}

#[tauri::command]
pub async fn index_available(params: Params, proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    call_get(&proxy, "/index/available", params).await
}

// ==========================================================================
// Economy
// ==========================================================================

#[tauri::command]
pub async fn economy_cpi(params: Params, proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/cpi", params).await
}

#[tauri::command]
pub async fn economy_calendar(params: Params, proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/calendar", params).await
}

#[tauri::command]
pub async fn economy_indicators(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/indicators", params).await
}

#[tauri::command]
pub async fn economy_gdp_real(params: Params, proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/gdp/real", params).await
}

#[tauri::command]
pub async fn economy_gdp_nominal(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/gdp/nominal", params).await
}

#[tauri::command]
pub async fn economy_gdp_forecast(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/gdp/forecast", params).await
}

#[tauri::command]
pub async fn economy_unemployment(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/unemployment", params).await
}

#[tauri::command]
pub async fn economy_fred_search(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/fred_search", params).await
}

#[tauri::command]
pub async fn economy_fred_series(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/fred_series", params).await
}

// ==========================================================================
// Fixed Income
// ==========================================================================

#[tauri::command]
pub async fn fixedincome_government_yield_curve(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/fixedincome/government/yield_curve", params).await
}

#[tauri::command]
pub async fn fixedincome_government_treasury_rates(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/fixedincome/government/treasury_rates", params).await
}

#[tauri::command]
pub async fn fixedincome_corporate_bond_indices(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/fixedincome/corporate/bond_indices", params).await
}

#[tauri::command]
pub async fn fixedincome_rate_sofr(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/fixedincome/rate/sofr", params).await
}

#[tauri::command]
pub async fn fixedincome_rate_fed_funds(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/fixedincome/rate/effr", params).await
}

// ==========================================================================
// News
// ==========================================================================

#[tauri::command]
pub async fn news_world(params: Params, proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    call_get(&proxy, "/news/world", params).await
}

#[tauri::command]
pub async fn news_company(params: Params, proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    call_get(&proxy, "/news/company", params).await
}

// ==========================================================================
// Regulators
// ==========================================================================

#[tauri::command]
pub async fn regulators_sec_filings(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/regulators/sec/filings", params).await
}

#[tauri::command]
pub async fn regulators_sec_company_filings(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/regulators/sec/company_filings", params).await
}

#[tauri::command]
pub async fn regulators_cftc_cot(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/regulators/cftc/cot", params).await
}

// ==========================================================================
// Commodity
// ==========================================================================

#[tauri::command]
pub async fn commodity_price_spot(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/commodity/price/spot", params).await
}

#[tauri::command]
pub async fn commodity_petroleum_status(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/commodity/petroleum_status_report", params).await
}

#[tauri::command]
pub async fn commodity_weather(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/commodity/weather", params).await
}

// ==========================================================================
// Technical / Quantitative / Econometrics — POST endpoints
// All take an OBBject `data` in the body and return a transformed OBBject.
// ==========================================================================

#[tauri::command]
pub async fn technical_sma(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/technical/sma", data, params).await
}

#[tauri::command]
pub async fn technical_ema(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/technical/ema", data, params).await
}

#[tauri::command]
pub async fn technical_rsi(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/technical/rsi", data, params).await
}

#[tauri::command]
pub async fn technical_macd(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/technical/macd", data, params).await
}

#[tauri::command]
pub async fn technical_bbands(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/technical/bbands", data, params).await
}

#[tauri::command]
pub async fn quantitative_summary(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/quantitative/summary", data, params).await
}

#[tauri::command]
pub async fn quantitative_normality(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/quantitative/normality", data, params).await
}

#[tauri::command]
pub async fn quantitative_unit_root(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/quantitative/unitroot", data, params).await
}

#[tauri::command]
pub async fn quantitative_performance_omega(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/quantitative/performance/omega_ratio", data, params).await
}

#[tauri::command]
pub async fn quantitative_performance_sharpe(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/quantitative/performance/sharpe_ratio", data, params).await
}

#[tauri::command]
pub async fn econometrics_correlation(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/econometrics/correlation_matrix", data, params).await
}

#[tauri::command]
pub async fn econometrics_ols(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/econometrics/ols_regression", data, params).await
}

#[tauri::command]
pub async fn econometrics_granger(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/econometrics/causality", data, params).await
}
