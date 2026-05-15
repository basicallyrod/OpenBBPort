//! Provider catalog and credential validation.
//!
//! Wraps the Python server's `/coverage/providers` endpoint with a
//! convenience layer that the renderer can call without parsing OpenAPI
//! itself.

use super::IpcError;
use crate::proxy::Proxy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSummary {
    pub name: String,
    pub credentials: Vec<String>,
    pub route_count: usize,
}

/// List every provider the running server knows about. Returns the raw
/// `/coverage/providers` payload PLUS a `summary` array with credential
/// lists pulled from `/coverage/command_model`.
///
/// Note: `/coverage/*` is only available when `OPENBB_DEV_MODE=true`.
#[tauri::command]
pub async fn provider_list(proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    let raw = proxy
        .get::<Value>("/coverage/providers", &[])
        .await
        .map_err(|e| IpcError::Internal(e.to_string()))?;
    Ok(raw)
}

#[tauri::command]
pub async fn provider_routes(
    provider: String,
    proxy: State<'_, Proxy>,
) -> Result<Vec<String>, IpcError> {
    let raw = proxy
        .get::<Value>("/coverage/providers", &[])
        .await
        .map_err(|e| IpcError::Internal(e.to_string()))?;
    Ok(raw
        .get(&provider)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default())
}

#[tauri::command]
pub async fn provider_credentials(
    provider: String,
    proxy: State<'_, Proxy>,
) -> Result<Vec<String>, IpcError> {
    let raw = proxy
        .get::<Value>("/coverage/command_model", &[])
        .await
        .map_err(|e| IpcError::Internal(e.to_string()))?;
    let mut creds = std::collections::HashSet::new();
    if let Some(obj) = raw.as_object() {
        for (_route, models) in obj {
            if let Some(prov_map) = models.as_object() {
                if let Some(p) = prov_map.get(&provider) {
                    if let Some(c) = p.get("credentials").and_then(|v| v.as_array()) {
                        for entry in c {
                            if let Some(s) = entry.as_str() {
                                creds.insert(s.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    let mut v: Vec<String> = creds.into_iter().collect();
    v.sort();
    Ok(v)
}

/// Probe a provider's credentials by invoking a known cheap route.
/// The connector decides which route is "cheap" per provider; for the
/// generic implementation we let the caller specify.
#[derive(Deserialize)]
pub struct ProviderValidateArgs {
    pub provider: String,
    pub probe_route: String,
    pub probe_params: Option<serde_json::Map<String, Value>>,
}

#[tauri::command]
pub async fn provider_validate(
    args: ProviderValidateArgs,
    proxy: State<'_, Proxy>,
) -> Result<bool, IpcError> {
    let mut params = args.probe_params.unwrap_or_default();
    params.insert("provider".to_string(), Value::String(args.provider.clone()));
    match proxy.get_with_map::<Value>(&args.probe_route, &params).await {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}
