//! Self-signed certificate generation — stub.
//!
//! Wire this to either:
//! - shell-out to `openssl` (matches OpenBB reference)
//! - the `rcgen` Rust crate (pure Rust)
//! - the `node-forge` npm package via your TS connector

use super::IpcError;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
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
pub fn generate_self_signed_cert(_args: GenerateCertArgs) -> Result<serde_json::Value, IpcError> {
    Err(IpcError::not_implemented("generate_self_signed_cert"))
}
