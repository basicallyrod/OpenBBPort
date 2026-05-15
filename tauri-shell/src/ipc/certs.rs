//! Self-signed certificate generation — stub.
//!
//! Wire this to either:
//! - shell-out to `openssl` (matches OpenBB reference)
//! - the `rcgen` Rust crate (pure Rust)
//! - the `node-forge` npm package via your TS connector

use std::sync::Arc;

use super::IpcError;
use crate::connector::Connector;
use serde::{Deserialize, Serialize};
use tauri::State;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "../bindings/", rename_all = "camelCase"))]
pub struct GenerateCertArgs {
    pub common_name: String,
    pub org_name: String,
    pub alt_names: Vec<String>,
    pub output_dir: String,
    pub days_valid: u32,
    pub password: Option<String>,
    pub install_in_trust_store: bool,
}

#[tauri::command]
pub async fn generate_self_signed_cert(
    args: GenerateCertArgs,
    connector: State<'_, Arc<dyn Connector>>,
) -> Result<serde_json::Value, IpcError> {
    connector
        .generate_self_signed_cert(args)
        .await
        .map_err(IpcError::from)
}
