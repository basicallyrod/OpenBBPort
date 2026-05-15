//! Generic Python REST proxy commands.
//!
//! These are the workhorses — `obb_call` is the catch-all for any of
//! the 184 documented `/api/v1/{ext}/{...}` routes. The 5 workspace
//! commands (`obb_widgets`, `obb_apps`, etc.) hit the Python server's
//! non-prefixed endpoints used by OpenBB Workspace.
//!
//! The coverage commands are only available when the Python server runs
//! with `OPENBB_DEV_MODE=true`.

use super::IpcError;
use crate::proxy::Proxy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "../bindings/"))]
pub struct ObbCallArgs {
    /// The route, e.g. `/equity/price/historical` (with or without the
    /// `/api/v1` prefix).
    pub route: String,
    /// Query parameters. For data-processing routes that take POST,
    /// `params.body` (if present) becomes the JSON body.
    pub params: Option<serde_json::Map<String, Value>>,
    /// Method override. Defaults to GET.
    pub method: Option<String>,
}

#[tauri::command]
pub async fn obb_call(args: ObbCallArgs, proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    let method = args
        .method
        .as_deref()
        .map(|s| s.to_uppercase())
        .unwrap_or_else(|| "GET".into());

    let params = args.params.unwrap_or_default();

    match method.as_str() {
        "GET" => proxy
            .get_with_map::<Value>(&args.route, &params)
            .await
            .map_err(|e| IpcError::Internal(e.to_string())),
        "POST" => {
            // Body is `params.body` if present, else the entire params map.
            let body = params
                .get("body")
                .cloned()
                .unwrap_or_else(|| Value::Object(params.clone()));
            let query: Vec<(&str, &str)> = params
                .iter()
                .filter(|(k, _)| *k != "body")
                .map(|(k, v)| (k.as_str(), match v {
                    Value::String(s) => s.as_str(),
                    _ => "",
                }))
                .collect();
            proxy
                .post::<_, Value>(&args.route, &body, &query)
                .await
                .map_err(|e| IpcError::Internal(e.to_string()))
        }
        other => Err(IpcError::InvalidArgument(format!(
            "unsupported HTTP method: {other}"
        ))),
    }
}

#[tauri::command]
pub fn obb_set_base_url(url: String, proxy: State<'_, Proxy>) -> Result<(), IpcError> {
    proxy.set_base_url(url);
    Ok(())
}

#[tauri::command]
pub fn obb_get_base_url(proxy: State<'_, Proxy>) -> String {
    proxy.config().base_url
}

#[tauri::command]
pub fn obb_set_basic_auth(
    username: String,
    password: String,
    proxy: State<'_, Proxy>,
) -> Result<(), IpcError> {
    proxy.set_basic_auth(username, password);
    Ok(())
}

#[tauri::command]
pub fn obb_set_bearer(token: String, proxy: State<'_, Proxy>) -> Result<(), IpcError> {
    proxy.set_bearer(token);
    Ok(())
}

#[tauri::command]
pub fn obb_clear_auth(proxy: State<'_, Proxy>) -> Result<(), IpcError> {
    proxy.clear_auth();
    Ok(())
}

#[tauri::command]
pub async fn obb_health(proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    proxy
        .get_raw::<Value>("/")
        .await
        .or_else(|_| Ok(serde_json::json!({"status": "unreachable"})))
}

// --- Workspace integration -------------------------------------------------

#[tauri::command]
pub async fn obb_openapi(proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    proxy
        .get_raw::<Value>("/openapi.json")
        .await
        .map_err(|e| IpcError::Internal(e.to_string()))
}

#[tauri::command]
pub async fn obb_widgets(proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    proxy
        .get_raw::<Value>("/widgets.json")
        .await
        .map_err(|e| IpcError::Internal(e.to_string()))
}

#[tauri::command]
pub async fn obb_apps(proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    proxy
        .get_raw::<Value>("/apps.json")
        .await
        .map_err(|e| IpcError::Internal(e.to_string()))
}

#[tauri::command]
pub async fn obb_agents(proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    proxy
        .get_raw::<Value>("/agents.json")
        .await
        .map_err(|e| IpcError::Internal(e.to_string()))
}

// --- Coverage (DEV_MODE only) ----------------------------------------------

#[tauri::command]
pub async fn obb_coverage_commands(proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    proxy
        .get::<Value>("/coverage/commands", &[])
        .await
        .map_err(|e| IpcError::Internal(e.to_string()))
}

#[tauri::command]
pub async fn obb_coverage_providers(proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    proxy
        .get::<Value>("/coverage/providers", &[])
        .await
        .map_err(|e| IpcError::Internal(e.to_string()))
}

#[tauri::command]
pub async fn obb_coverage_command_model(proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    proxy
        .get::<Value>("/coverage/command_model", &[])
        .await
        .map_err(|e| IpcError::Internal(e.to_string()))
}

// --- User (auth-required, DEV_MODE only) -----------------------------------

#[tauri::command]
pub async fn obb_user_me(proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    proxy
        .get::<Value>("/user/me", &[])
        .await
        .map_err(|e| IpcError::Internal(e.to_string()))
}

// --- System (DEV_MODE only) ------------------------------------------------

#[tauri::command]
pub async fn obb_system(proxy: State<'_, Proxy>) -> Result<Value, IpcError> {
    proxy
        .get::<Value>("/system", &[])
        .await
        .map_err(|e| IpcError::Internal(e.to_string()))
}
