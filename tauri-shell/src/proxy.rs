//! HTTP client targeting the Python REST server (`openbb-api`).
//!
//! Wraps `reqwest` with:
//! - Configurable base URL (set via `Proxy::set_base_url` from IPC)
//! - Optional HTTP Basic auth (when `OPENBB_API_AUTH=true` on the server)
//! - 60s default timeout (override per-call)
//! - JSON request/response only — the server's wire format is JSON OBBject
//!
//! Use it from IPC handlers like:
//! ```ignore
//! let proxy = state.inner();
//! let obb: serde_json::Value = proxy.get("/equity/price/historical",
//!     &[("symbol", "AAPL"), ("provider", "yfinance")]).await?;
//! ```

use once_cell::sync::OnceCell;
use reqwest::Client;
use std::sync::Mutex;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ProxyConfig {
    pub base_url: String,
    pub auth: Option<(String, String)>, // (username, password) HTTP Basic
    pub bearer: Option<String>,
    pub timeout_seconds: u64,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:6900".into(),
            auth: None,
            bearer: None,
            timeout_seconds: 60,
        }
    }
}

pub struct Proxy {
    config: Mutex<ProxyConfig>,
    client: OnceCell<Client>,
}

impl Proxy {
    pub fn new() -> Self {
        Self {
            config: Mutex::new(ProxyConfig::default()),
            client: OnceCell::new(),
        }
    }

    pub fn config(&self) -> ProxyConfig {
        self.config.lock().map(|g| g.clone()).unwrap_or_default()
    }

    pub fn set_base_url(&self, url: impl Into<String>) {
        if let Ok(mut g) = self.config.lock() {
            g.base_url = url.into().trim_end_matches('/').to_string();
        }
    }

    pub fn set_basic_auth(&self, user: impl Into<String>, pass: impl Into<String>) {
        if let Ok(mut g) = self.config.lock() {
            g.auth = Some((user.into(), pass.into()));
            g.bearer = None;
        }
    }

    pub fn set_bearer(&self, token: impl Into<String>) {
        if let Ok(mut g) = self.config.lock() {
            g.bearer = Some(token.into());
            g.auth = None;
        }
    }

    pub fn clear_auth(&self) {
        if let Ok(mut g) = self.config.lock() {
            g.auth = None;
            g.bearer = None;
        }
    }

    fn client(&self) -> &Client {
        self.client.get_or_init(|| {
            Client::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .expect("reqwest client build")
        })
    }

    fn apply_auth(&self, mut req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let cfg = self.config();
        if let Some((u, p)) = &cfg.auth {
            req = req.basic_auth(u, Some(p));
        } else if let Some(b) = &cfg.bearer {
            req = req.bearer_auth(b);
        }
        req
    }

    /// GET request returning JSON.
    pub async fn get<T: serde::de::DeserializeOwned>(
        &self,
        route: &str,
        params: &[(&str, &str)],
    ) -> Result<T, ProxyError> {
        let url = self.build_url(route)?;
        let req = self.client().get(&url).query(params);
        let req = self.apply_auth(req);
        let resp = req.send().await?;
        decode(resp).await
    }

    /// POST request with a JSON body.
    pub async fn post<B: serde::Serialize, T: serde::de::DeserializeOwned>(
        &self,
        route: &str,
        body: &B,
        params: &[(&str, &str)],
    ) -> Result<T, ProxyError> {
        let url = self.build_url(route)?;
        let req = self.client().post(&url).query(params).json(body);
        let req = self.apply_auth(req);
        let resp = req.send().await?;
        decode(resp).await
    }

    /// Like `get` but accepts a `HashMap` of params (more ergonomic from IPC).
    pub async fn get_with_map<T: serde::de::DeserializeOwned>(
        &self,
        route: &str,
        params: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<T, ProxyError> {
        let url = self.build_url(route)?;
        let mut req = self.client().get(&url);
        for (k, v) in params {
            req = req.query(&[(k.as_str(), &json_param(v))]);
        }
        let req = self.apply_auth(req);
        let resp = req.send().await?;
        decode(resp).await
    }

    /// Raw GET that does NOT prepend the `/api/v1` prefix. Use for
    /// `/widgets.json`, `/apps.json`, `/openapi.json`, etc.
    pub async fn get_raw<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
    ) -> Result<T, ProxyError> {
        let url = format!(
            "{}{}",
            self.config().base_url,
            if path.starts_with('/') {
                path.to_string()
            } else {
                format!("/{path}")
            }
        );
        let req = self.client().get(&url);
        let req = self.apply_auth(req);
        let resp = req.send().await?;
        decode(resp).await
    }

    fn build_url(&self, route: &str) -> Result<String, ProxyError> {
        let cfg = self.config();
        let trimmed = route.trim_start_matches('/');
        if cfg.base_url.is_empty() {
            return Err(ProxyError::NotConfigured);
        }
        // Caller can pass a route with or without `/api/v1` prefix.
        let path = if trimmed.starts_with("api/v") {
            trimmed.to_string()
        } else {
            format!("api/v1/{trimmed}")
        };
        Ok(format!("{}/{}", cfg.base_url, path))
    }
}

fn json_param(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

async fn decode<T: serde::de::DeserializeOwned>(resp: reqwest::Response) -> Result<T, ProxyError> {
    let status = resp.status();
    if status.as_u16() == 204 {
        // No content — let the caller decide what to deserialize.
        return serde_json::from_value(serde_json::Value::Null).map_err(Into::into);
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(ProxyError::Http {
            status: status.as_u16(),
            body,
        });
    }
    let body = resp.text().await?;
    serde_json::from_str(&body).map_err(Into::into)
}

#[derive(Debug, thiserror::Error)]
pub enum ProxyError {
    #[error("proxy not configured (base URL is empty)")]
    NotConfigured,
    #[error("HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error("network error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("internal: {0}")]
    Internal(String),
}

impl Default for Proxy {
    fn default() -> Self {
        Self::new()
    }
}
