//! Discovery / introspection commands.
//!
//! These help the renderer build dynamic UIs without hard-coding the
//! Python backend's surface. All three commands read `/openapi.json`
//! from the configured `crate::proxy::Proxy` and parse it into a
//! lightweight `RouteInfo` array.
//!
//! Use cases:
//! - Command palette / autocomplete: call `list_all_routes` once at
//!   boot, cache, and feed a fuzzy search into `search_routes` (or do
//!   the filtering renderer-side).
//! - Route-detail panel: `route_parameters` returns the JSON schema for
//!   one endpoint so the renderer can render a form.
//!
//! Related modules:
//! - `crate::ipc::obb` — the proxy these commands hit.
//! - `crate::ipc::obb_routes` / `obb_routes_extended` — the typed
//!   wrappers that `RouteInfo.path` entries point at.

use super::IpcError;
use crate::proxy::Proxy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

/// Full route catalog, derived from `/openapi.json`. Each entry has the
/// path, the HTTP method, the data model name (`openapi_extra.model`),
/// and any tag/category.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "../bindings/", rename_all = "camelCase"))]
pub struct RouteInfo {
    pub path: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub providers: Vec<String>,
}

#[tauri::command]
pub async fn list_all_routes(proxy: State<'_, Proxy>) -> Result<Vec<RouteInfo>, IpcError> {
    let spec: Value = proxy
        .get_raw("/openapi.json")
        .await
        .map_err(|e| IpcError::Internal(e.to_string()))?;

    let mut out = Vec::new();
    let Some(paths) = spec.get("paths").and_then(|v| v.as_object()) else {
        return Ok(out);
    };

    for (path, methods) in paths {
        let Some(methods) = methods.as_object() else { continue };
        for (method, op) in methods {
            if !matches!(method.as_str(), "get" | "post" | "put" | "delete" | "patch") {
                continue;
            }
            let model = op
                .get("x-openbb-model")
                .or_else(|| op.get("openapi_extra").and_then(|e| e.get("model")))
                .and_then(|v| v.as_str())
                .map(String::from);
            let tags = op
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let summary = op
                .get("summary")
                .and_then(|v| v.as_str())
                .map(String::from);
            let providers = op
                .get("x-openbb-providers")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            out.push(RouteInfo {
                path: path.clone(),
                method: method.to_uppercase(),
                model,
                tags,
                summary,
                providers,
            });
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// Free-text search over the route catalog (path + model + summary).
#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "../bindings/"))]
pub struct RouteSearchArgs {
    pub query: String,
}

#[tauri::command]
pub async fn search_routes(
    args: RouteSearchArgs,
    proxy: State<'_, Proxy>,
) -> Result<Vec<RouteInfo>, IpcError> {
    let all = list_all_routes(proxy).await?;
    let q = args.query.to_lowercase();
    Ok(all
        .into_iter()
        .filter(|r| {
            r.path.to_lowercase().contains(&q)
                || r.model.as_deref().unwrap_or("").to_lowercase().contains(&q)
                || r.summary.as_deref().unwrap_or("").to_lowercase().contains(&q)
        })
        .collect())
}

#[derive(Deserialize, Serialize)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "../bindings/"))]
pub struct RouteParamsArgs {
    pub path: String,
    #[serde(default = "default_method")]
    pub method: String,
}

fn default_method() -> String {
    "get".into()
}

/// Returns the parameter schema for a single route. Useful when the
/// renderer wants to render a form dynamically.
#[tauri::command]
pub async fn route_parameters(
    args: RouteParamsArgs,
    proxy: State<'_, Proxy>,
) -> Result<Value, IpcError> {
    let spec: Value = proxy
        .get_raw("/openapi.json")
        .await
        .map_err(|e| IpcError::Internal(e.to_string()))?;
    let path = if args.path.starts_with('/') {
        args.path
    } else {
        format!("/{}", args.path)
    };
    let op = spec
        .get("paths")
        .and_then(|p| p.get(&path))
        .and_then(|m| m.get(args.method.to_lowercase()))
        .cloned()
        .unwrap_or(Value::Null);
    Ok(op
        .get("parameters")
        .cloned()
        .unwrap_or_else(|| Value::Array(Vec::new())))
}
