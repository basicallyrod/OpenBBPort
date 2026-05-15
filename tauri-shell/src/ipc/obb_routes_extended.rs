//! Extended typed convenience wrappers for the remaining OpenBB routes.
//!
//! This module is a sibling of [`super::obb_routes`] and follows the exact
//! same pattern: one `#[tauri::command]` per OpenBB route, body is a single
//! line that forwards through the [`Proxy`] HTTP client.
//!
//! Auto-derived from the `@router.command(model="...")` decorations in
//! `openbb_platform/extensions/**/*_router.py`. The split between this file
//! and `obb_routes.rs` is purely organizational — there is no behavioural
//! difference. Together the two modules cover the full ~180 route surface.
//!
//! GET routes use the file-local `call_get` helper; the data-processing
//! extensions (`/technical/*`, `/quantitative/*`, `/econometrics/*`) accept
//! an OBBject payload in the body and therefore use `call_post`.

use super::IpcError;
use crate::proxy::Proxy;
use serde_json::{Map, Value};
use tauri::State;

type Params = Option<Map<String, Value>>;

/// GET-helper duplicated from `obb_routes.rs` because that module's helpers
/// are private. Small and stable enough that copying is cheaper than a third
/// shared module.
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

/// POST-helper duplicated from `obb_routes.rs`.
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
// Commodity
// ==========================================================================

#[tauri::command]
pub async fn commodity_psd_data(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/commodity/psd_data", params).await
}

#[tauri::command]
pub async fn commodity_psd_report(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/commodity/psd_report", params).await
}

#[tauri::command]
pub async fn commodity_short_term_energy_outlook(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/commodity/short_term_energy_outlook", params).await
}

#[tauri::command]
pub async fn commodity_weather_bulletins(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/commodity/weather_bulletins", params).await
}

#[tauri::command]
pub async fn commodity_weather_bulletins_download(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/commodity/weather_bulletins_download", params).await
}

// ==========================================================================
// Derivatives
// ==========================================================================

#[tauri::command]
pub async fn derivatives_options_surface(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/derivatives/options/surface", params).await
}

// ==========================================================================
// Economy (FRED, surveys, GDP, shipping, indicators)
// ==========================================================================

#[tauri::command]
pub async fn economy_available_indicators(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/available_indicators", params).await
}

#[tauri::command]
pub async fn economy_balance_of_payments(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/balance_of_payments", params).await
}

#[tauri::command]
pub async fn economy_central_bank_holdings(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/central_bank_holdings", params).await
}

#[tauri::command]
pub async fn economy_composite_leading_indicator(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/composite_leading_indicator", params).await
}

#[tauri::command]
pub async fn economy_country_profile(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/country_profile", params).await
}

#[tauri::command]
pub async fn economy_direction_of_trade(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/direction_of_trade", params).await
}

#[tauri::command]
pub async fn economy_export_destinations(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/export_destinations", params).await
}

#[tauri::command]
pub async fn economy_fomc_documents(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/fomc_documents", params).await
}

#[tauri::command]
pub async fn economy_fred_regional(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/fred_regional", params).await
}

#[tauri::command]
pub async fn economy_fred_release_table(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/fred_release_table", params).await
}

#[tauri::command]
pub async fn economy_house_price_index(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/house_price_index", params).await
}

#[tauri::command]
pub async fn economy_interest_rates(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/interest_rates", params).await
}

#[tauri::command]
pub async fn economy_money_measures(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/money_measures", params).await
}

#[tauri::command]
pub async fn economy_pce(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/pce", params).await
}

#[tauri::command]
pub async fn economy_primary_dealer_fails(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/primary_dealer_fails", params).await
}

#[tauri::command]
pub async fn economy_primary_dealer_positioning(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/primary_dealer_positioning", params).await
}

#[tauri::command]
pub async fn economy_retail_prices(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/retail_prices", params).await
}

#[tauri::command]
pub async fn economy_risk_premium(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/risk_premium", params).await
}

#[tauri::command]
pub async fn economy_share_price_index(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/share_price_index", params).await
}

#[tauri::command]
pub async fn economy_shipping_chokepoint_info(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/shipping/chokepoint_info", params).await
}

#[tauri::command]
pub async fn economy_shipping_chokepoint_volume(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/shipping/chokepoint_volume", params).await
}

#[tauri::command]
pub async fn economy_shipping_port_info(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/shipping/port_info", params).await
}

#[tauri::command]
pub async fn economy_shipping_port_volume(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/shipping/port_volume", params).await
}

#[tauri::command]
pub async fn economy_survey_bls_search(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/survey/bls_search", params).await
}

#[tauri::command]
pub async fn economy_survey_bls_series(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/survey/bls_series", params).await
}

#[tauri::command]
pub async fn economy_survey_economic_conditions_chicago(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/survey/economic_conditions_chicago", params).await
}

#[tauri::command]
pub async fn economy_survey_inflation_expectations(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/survey/inflation_expectations", params).await
}

#[tauri::command]
pub async fn economy_survey_manufacturing_outlook_ny(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/survey/manufacturing_outlook_ny", params).await
}

#[tauri::command]
pub async fn economy_survey_manufacturing_outlook_texas(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/survey/manufacturing_outlook_texas", params).await
}

#[tauri::command]
pub async fn economy_survey_nonfarm_payrolls(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/survey/nonfarm_payrolls", params).await
}

#[tauri::command]
pub async fn economy_survey_sloos(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/survey/sloos", params).await
}

#[tauri::command]
pub async fn economy_survey_university_of_michigan(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/survey/university_of_michigan", params).await
}

#[tauri::command]
pub async fn economy_total_factor_productivity(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/economy/total_factor_productivity", params).await
}

// ==========================================================================
// Equity (compare/darkpool/discovery/estimates/fundamental/ownership/shorts)
// ==========================================================================

#[tauri::command]
pub async fn equity_compare_company_facts(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/compare/company_facts", params).await
}

#[tauri::command]
pub async fn equity_compare_groups(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/compare/groups", params).await
}

#[tauri::command]
pub async fn equity_compare_peers(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/compare/peers", params).await
}

#[tauri::command]
pub async fn equity_darkpool_otc(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/darkpool/otc", params).await
}

#[tauri::command]
pub async fn equity_discovery_aggressive_small_caps(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/discovery/aggressive_small_caps", params).await
}

#[tauri::command]
pub async fn equity_discovery_filings(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/discovery/filings", params).await
}

#[tauri::command]
pub async fn equity_discovery_growth_tech(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/discovery/growth_tech", params).await
}

#[tauri::command]
pub async fn equity_discovery_latest_financial_reports(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/discovery/latest_financial_reports", params).await
}

#[tauri::command]
pub async fn equity_discovery_top_retail(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/discovery/top_retail", params).await
}

#[tauri::command]
pub async fn equity_discovery_undervalued_growth(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/discovery/undervalued_growth", params).await
}

#[tauri::command]
pub async fn equity_discovery_undervalued_large_caps(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/discovery/undervalued_large_caps", params).await
}

#[tauri::command]
pub async fn equity_estimates_analyst_search(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/estimates/analyst_search", params).await
}

#[tauri::command]
pub async fn equity_estimates_forward_ebitda(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/estimates/forward_ebitda", params).await
}

#[tauri::command]
pub async fn equity_estimates_forward_eps(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/estimates/forward_eps", params).await
}

#[tauri::command]
pub async fn equity_estimates_forward_pe(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/estimates/forward_pe", params).await
}

#[tauri::command]
pub async fn equity_estimates_forward_sales(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/estimates/forward_sales", params).await
}

#[tauri::command]
pub async fn equity_estimates_historical(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/estimates/historical", params).await
}

#[tauri::command]
pub async fn equity_fundamental_balance_growth(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/fundamental/balance_growth", params).await
}

#[tauri::command]
pub async fn equity_fundamental_cash_growth(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/fundamental/cash_growth", params).await
}

#[tauri::command]
pub async fn equity_fundamental_employee_count(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/fundamental/employee_count", params).await
}

#[tauri::command]
pub async fn equity_fundamental_esg_score(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/fundamental/esg_score", params).await
}

#[tauri::command]
pub async fn equity_fundamental_historical_attributes(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/fundamental/historical_attributes", params).await
}

#[tauri::command]
pub async fn equity_fundamental_historical_eps(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/fundamental/historical_eps", params).await
}

#[tauri::command]
pub async fn equity_fundamental_historical_splits(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/fundamental/historical_splits", params).await
}

#[tauri::command]
pub async fn equity_fundamental_income_growth(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/fundamental/income_growth", params).await
}

#[tauri::command]
pub async fn equity_fundamental_latest_attributes(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/fundamental/latest_attributes", params).await
}

#[tauri::command]
pub async fn equity_fundamental_management(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/fundamental/management", params).await
}

#[tauri::command]
pub async fn equity_fundamental_management_compensation(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/fundamental/management_compensation", params).await
}

#[tauri::command]
pub async fn equity_fundamental_management_discussion_analysis(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/fundamental/management_discussion_analysis", params).await
}

#[tauri::command]
pub async fn equity_fundamental_reported_financials(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/fundamental/reported_financials", params).await
}

#[tauri::command]
pub async fn equity_fundamental_revenue_per_geography(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/fundamental/revenue_per_geography", params).await
}

#[tauri::command]
pub async fn equity_fundamental_revenue_per_segment(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/fundamental/revenue_per_segment", params).await
}

#[tauri::command]
pub async fn equity_fundamental_search_attributes(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/fundamental/search_attributes", params).await
}

#[tauri::command]
pub async fn equity_fundamental_trailing_dividend_yield(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/fundamental/trailing_dividend_yield", params).await
}

#[tauri::command]
pub async fn equity_fundamental_transcript(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/fundamental/transcript", params).await
}

#[tauri::command]
pub async fn equity_ownership_form_13f(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/ownership/form_13f", params).await
}

#[tauri::command]
pub async fn equity_ownership_government_trades(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/ownership/government_trades", params).await
}

#[tauri::command]
pub async fn equity_ownership_major_holders(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/ownership/major_holders", params).await
}

#[tauri::command]
pub async fn equity_ownership_share_statistics(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/ownership/share_statistics", params).await
}

#[tauri::command]
pub async fn equity_shorts_fails_to_deliver(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/shorts/fails_to_deliver", params).await
}

#[tauri::command]
pub async fn equity_shorts_short_interest(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/shorts/short_interest", params).await
}

#[tauri::command]
pub async fn equity_shorts_short_volume(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/equity/shorts/short_volume", params).await
}

// ==========================================================================
// ETF (discovery + extras)
// ==========================================================================

#[tauri::command]
pub async fn etf_discovery_active(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/etf/discovery/active", params).await
}

#[tauri::command]
pub async fn etf_discovery_gainers(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/etf/discovery/gainers", params).await
}

#[tauri::command]
pub async fn etf_discovery_losers(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/etf/discovery/losers", params).await
}

#[tauri::command]
pub async fn etf_equity_exposure(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/etf/equity_exposure", params).await
}

#[tauri::command]
pub async fn etf_nport_disclosure(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/etf/nport_disclosure", params).await
}

#[tauri::command]
pub async fn etf_price_performance(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/etf/price_performance", params).await
}

// ==========================================================================
// Fixed Income (corporate / government / rate / spreads)
// ==========================================================================

#[tauri::command]
pub async fn fixedincome_bond_indices(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/fixedincome/bond_indices", params).await
}

#[tauri::command]
pub async fn fixedincome_corporate_bond_prices(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/fixedincome/corporate/bond_prices", params).await
}

#[tauri::command]
pub async fn fixedincome_corporate_commercial_paper(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/fixedincome/corporate/commercial_paper", params).await
}

#[tauri::command]
pub async fn fixedincome_corporate_hqm(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/fixedincome/corporate/hqm", params).await
}

#[tauri::command]
pub async fn fixedincome_corporate_spot_rates(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/fixedincome/corporate/spot_rates", params).await
}

#[tauri::command]
pub async fn fixedincome_government_svensson_yield_curve(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/fixedincome/government/svensson_yield_curve", params).await
}

#[tauri::command]
pub async fn fixedincome_government_tips_yields(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/fixedincome/government/tips_yields", params).await
}

#[tauri::command]
pub async fn fixedincome_government_treasury_auctions(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/fixedincome/government/treasury_auctions", params).await
}

#[tauri::command]
pub async fn fixedincome_government_treasury_prices(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/fixedincome/government/treasury_prices", params).await
}

#[tauri::command]
pub async fn fixedincome_mortgage_indices(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/fixedincome/mortgage_indices", params).await
}

#[tauri::command]
pub async fn fixedincome_rate_ameribor(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/fixedincome/rate/ameribor", params).await
}

#[tauri::command]
pub async fn fixedincome_rate_dpcredit(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/fixedincome/rate/dpcredit", params).await
}

#[tauri::command]
pub async fn fixedincome_rate_ecb(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/fixedincome/rate/ecb", params).await
}

#[tauri::command]
pub async fn fixedincome_rate_effr_forecast(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/fixedincome/rate/effr_forecast", params).await
}

#[tauri::command]
pub async fn fixedincome_rate_estr(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/fixedincome/rate/estr", params).await
}

#[tauri::command]
pub async fn fixedincome_rate_iorb(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/fixedincome/rate/iorb", params).await
}

#[tauri::command]
pub async fn fixedincome_rate_overnight_bank_funding(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/fixedincome/rate/overnight_bank_funding", params).await
}

#[tauri::command]
pub async fn fixedincome_rate_sonia(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/fixedincome/rate/sonia", params).await
}

#[tauri::command]
pub async fn fixedincome_spreads_tcm(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/fixedincome/spreads/tcm", params).await
}

#[tauri::command]
pub async fn fixedincome_spreads_tcm_effr(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/fixedincome/spreads/tcm_effr", params).await
}

#[tauri::command]
pub async fn fixedincome_spreads_treasury_effr(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/fixedincome/spreads/treasury_effr", params).await
}

// ==========================================================================
// Index
// ==========================================================================

#[tauri::command]
pub async fn index_sectors(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/index/sectors", params).await
}

#[tauri::command]
pub async fn index_sp500_multiples(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/index/sp500_multiples", params).await
}

// ==========================================================================
// Regulators (SEC deep)
// ==========================================================================

#[tauri::command]
pub async fn regulators_sec_cik_map(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/regulators/sec/cik_map", params).await
}

#[tauri::command]
pub async fn regulators_sec_filing_headers(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/regulators/sec/filing_headers", params).await
}

#[tauri::command]
pub async fn regulators_sec_htm_file(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/regulators/sec/htm_file", params).await
}

#[tauri::command]
pub async fn regulators_sec_institutions_search(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/regulators/sec/institutions_search", params).await
}

#[tauri::command]
pub async fn regulators_sec_rss_litigation(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/regulators/sec/rss_litigation", params).await
}

#[tauri::command]
pub async fn regulators_sec_schema_files(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/regulators/sec/schema_files", params).await
}

#[tauri::command]
pub async fn regulators_sec_sic_search(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/regulators/sec/sic_search", params).await
}

#[tauri::command]
pub async fn regulators_sec_symbol_map(
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_get(&proxy, "/regulators/sec/symbol_map", params).await
}

// ==========================================================================
// Technical indicators (POST)
// ==========================================================================

#[tauri::command]
pub async fn technical_ad(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/technical/ad", data, params).await
}

#[tauri::command]
pub async fn technical_adosc(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/technical/adosc", data, params).await
}

#[tauri::command]
pub async fn technical_adx(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/technical/adx", data, params).await
}

#[tauri::command]
pub async fn technical_aroon(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/technical/aroon", data, params).await
}

#[tauri::command]
pub async fn technical_atr(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/technical/atr", data, params).await
}

#[tauri::command]
pub async fn technical_cci(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/technical/cci", data, params).await
}

#[tauri::command]
pub async fn technical_cg(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/technical/cg", data, params).await
}

#[tauri::command]
pub async fn technical_clenow(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/technical/clenow", data, params).await
}

#[tauri::command]
pub async fn technical_cones(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/technical/cones", data, params).await
}

#[tauri::command]
pub async fn technical_demark(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/technical/demark", data, params).await
}

#[tauri::command]
pub async fn technical_donchian(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/technical/donchian", data, params).await
}

#[tauri::command]
pub async fn technical_fib(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/technical/fib", data, params).await
}

#[tauri::command]
pub async fn technical_fisher(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/technical/fisher", data, params).await
}

#[tauri::command]
pub async fn technical_hma(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/technical/hma", data, params).await
}

#[tauri::command]
pub async fn technical_ichimoku(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/technical/ichimoku", data, params).await
}

#[tauri::command]
pub async fn technical_kc(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/technical/kc", data, params).await
}

#[tauri::command]
pub async fn technical_obv(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/technical/obv", data, params).await
}

#[tauri::command]
pub async fn technical_relative_rotation(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/technical/relative_rotation", data, params).await
}

#[tauri::command]
pub async fn technical_stoch(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/technical/stoch", data, params).await
}

#[tauri::command]
pub async fn technical_vwap(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/technical/vwap", data, params).await
}

#[tauri::command]
pub async fn technical_wma(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/technical/wma", data, params).await
}

#[tauri::command]
pub async fn technical_zlma(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/technical/zlma", data, params).await
}

// ==========================================================================
// Quantitative (stats / rolling / performance) — POST
// ==========================================================================

#[tauri::command]
pub async fn quantitative_capm(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/quantitative/capm", data, params).await
}

#[tauri::command]
pub async fn quantitative_performance_sortino_ratio(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/quantitative/performance/sortino_ratio", data, params).await
}

#[tauri::command]
pub async fn quantitative_rolling_kurtosis(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/quantitative/rolling/kurtosis", data, params).await
}

#[tauri::command]
pub async fn quantitative_rolling_mean(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/quantitative/rolling/mean", data, params).await
}

#[tauri::command]
pub async fn quantitative_rolling_quantile(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/quantitative/rolling/quantile", data, params).await
}

#[tauri::command]
pub async fn quantitative_rolling_skew(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/quantitative/rolling/skew", data, params).await
}

#[tauri::command]
pub async fn quantitative_rolling_stdev(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/quantitative/rolling/stdev", data, params).await
}

#[tauri::command]
pub async fn quantitative_rolling_variance(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/quantitative/rolling/variance", data, params).await
}

#[tauri::command]
pub async fn quantitative_stats_kurtosis(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/quantitative/stats/kurtosis", data, params).await
}

#[tauri::command]
pub async fn quantitative_stats_mean(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/quantitative/stats/mean", data, params).await
}

#[tauri::command]
pub async fn quantitative_stats_quantile(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/quantitative/stats/quantile", data, params).await
}

#[tauri::command]
pub async fn quantitative_stats_skew(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/quantitative/stats/skew", data, params).await
}

#[tauri::command]
pub async fn quantitative_stats_stdev(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/quantitative/stats/stdev", data, params).await
}

#[tauri::command]
pub async fn quantitative_stats_variance(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/quantitative/stats/variance", data, params).await
}

#[tauri::command]
pub async fn quantitative_unitroot_test(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/quantitative/unitroot_test", data, params).await
}

// ==========================================================================
// Econometrics — POST
// ==========================================================================

#[tauri::command]
pub async fn econometrics_autocorrelation(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/econometrics/autocorrelation", data, params).await
}

#[tauri::command]
pub async fn econometrics_cointegration(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/econometrics/cointegration", data, params).await
}

#[tauri::command]
pub async fn econometrics_ols_regression_summary(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/econometrics/ols_regression_summary", data, params).await
}

#[tauri::command]
pub async fn econometrics_panel_between(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/econometrics/panel_between", data, params).await
}

#[tauri::command]
pub async fn econometrics_panel_first_difference(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/econometrics/panel_first_difference", data, params).await
}

#[tauri::command]
pub async fn econometrics_panel_fixed(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/econometrics/panel_fixed", data, params).await
}

#[tauri::command]
pub async fn econometrics_panel_fmac(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/econometrics/panel_fmac", data, params).await
}

#[tauri::command]
pub async fn econometrics_panel_pooled(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/econometrics/panel_pooled", data, params).await
}

#[tauri::command]
pub async fn econometrics_panel_random_effects(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/econometrics/panel_random_effects", data, params).await
}

#[tauri::command]
pub async fn econometrics_residual_autocorrelation(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/econometrics/residual_autocorrelation", data, params).await
}

#[tauri::command]
pub async fn econometrics_unit_root(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/econometrics/unit_root", data, params).await
}

#[tauri::command]
pub async fn econometrics_variance_inflation_factor(
    data: Value,
    params: Params,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    call_post(&proxy, "/econometrics/variance_inflation_factor", data, params).await
}

