//! Forces `ts-rs` to write a `.ts` file for every IPC type to
//! `tauri-shell/bindings/`. Only built when the `bindings` Cargo feature
//! is enabled.
//!
//! Run with:
//!
//! ```bash
//! cargo test --features bindings --test bindings_export
//! ```
//!
//! `ts-rs` already auto-emits a `#[test]` for every `#[ts(export)]` type,
//! so the existence of these calls is belt-and-braces — we want a hard
//! compile-time guarantee that every binding-relevant type still implements
//! `TS` even if upstream macros change. Failures here mean a struct lost
//! its annotation.

#![cfg(feature = "bindings")]

use ts_rs::TS;

use tauri_shell::events::{
    BackendUrlEvent, InstallProgressEvent, JupyterStatusEvent, NavigateEvent, ProcessOutputEvent,
};
use tauri_shell::ipc::backends::BackendService;
use tauri_shell::ipc::certs::GenerateCertArgs;
use tauri_shell::ipc::credentials::UpdateCredentialsArgs;
use tauri_shell::ipc::environments::{CondaEnvironment, ExecResult, Extension};
use tauri_shell::ipc::jupyter::JupyterStatus;
use tauri_shell::ipc::mcp::{McpSpec, McpStatus};
use tauri_shell::ipc::obb::ObbCallArgs;
use tauri_shell::ipc::openbb_meta::{RouteInfo, RouteParamsArgs, RouteSearchArgs};
use tauri_shell::ipc::provider::{ProviderSummary, ProviderValidateArgs};
use tauri_shell::ipc::routines::{
    RoutineMetadata, RoutinesDeleteArgs, RoutinesReadArgs, RoutinesRenameArgs, RoutinesSaveArgs,
};
use tauri_shell::ipc::server::{ServerSpec, ServerStatus};
use tauri_shell::ipc::settings_files::{ReadJsonArgs, ReadTextArgs, WriteJsonArgs, WriteTextArgs};
use tauri_shell::ipc::IpcError;
use tauri_shell::state::{InstallationProgress, InstallationSnapshot, LogEntry};

/// Touch every exportable type. ts-rs writes one .ts file per type into
/// `bindings/` as a side effect of the test harness invoking the auto-
/// generated `#[test] fn export_<TYPE>()` functions; calling
/// `export_all_to` here is extra insurance — and the calls also make
/// every type a hard build-time dependency of this test, so the binding
/// surface can't drift without the test failing to compile.
#[test]
fn force_export_all_bindings() {
    let out = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("bindings");
    std::fs::create_dir_all(&out).expect("create bindings dir");

    // -- state -----------------------------------------------------------
    LogEntry::export_all_to(&out).expect("LogEntry");
    InstallationSnapshot::export_all_to(&out).expect("InstallationSnapshot");
    InstallationProgress::export_all_to(&out).expect("InstallationProgress");

    // -- events ----------------------------------------------------------
    ProcessOutputEvent::export_all_to(&out).expect("ProcessOutputEvent");
    InstallProgressEvent::export_all_to(&out).expect("InstallProgressEvent");
    BackendUrlEvent::export_all_to(&out).expect("BackendUrlEvent");
    JupyterStatusEvent::export_all_to(&out).expect("JupyterStatusEvent");
    NavigateEvent::export_all_to(&out).expect("NavigateEvent");

    // -- ipc payloads ----------------------------------------------------
    BackendService::export_all_to(&out).expect("BackendService");
    GenerateCertArgs::export_all_to(&out).expect("GenerateCertArgs");
    UpdateCredentialsArgs::export_all_to(&out).expect("UpdateCredentialsArgs");
    CondaEnvironment::export_all_to(&out).expect("CondaEnvironment");
    Extension::export_all_to(&out).expect("Extension");
    ExecResult::export_all_to(&out).expect("ExecResult");
    JupyterStatus::export_all_to(&out).expect("JupyterStatus");
    McpSpec::export_all_to(&out).expect("McpSpec");
    McpStatus::export_all_to(&out).expect("McpStatus");
    ObbCallArgs::export_all_to(&out).expect("ObbCallArgs");
    RouteInfo::export_all_to(&out).expect("RouteInfo");
    RouteSearchArgs::export_all_to(&out).expect("RouteSearchArgs");
    RouteParamsArgs::export_all_to(&out).expect("RouteParamsArgs");
    ProviderSummary::export_all_to(&out).expect("ProviderSummary");
    ProviderValidateArgs::export_all_to(&out).expect("ProviderValidateArgs");
    RoutineMetadata::export_all_to(&out).expect("RoutineMetadata");
    RoutinesReadArgs::export_all_to(&out).expect("RoutinesReadArgs");
    RoutinesSaveArgs::export_all_to(&out).expect("RoutinesSaveArgs");
    RoutinesDeleteArgs::export_all_to(&out).expect("RoutinesDeleteArgs");
    RoutinesRenameArgs::export_all_to(&out).expect("RoutinesRenameArgs");
    ServerSpec::export_all_to(&out).expect("ServerSpec");
    ServerStatus::export_all_to(&out).expect("ServerStatus");
    ReadJsonArgs::export_all_to(&out).expect("ReadJsonArgs");
    WriteJsonArgs::export_all_to(&out).expect("WriteJsonArgs");
    ReadTextArgs::export_all_to(&out).expect("ReadTextArgs");
    WriteTextArgs::export_all_to(&out).expect("WriteTextArgs");

    // -- error -----------------------------------------------------------
    IpcError::export_all_to(&out).expect("IpcError");

    // Touch the index.ts generator. We don't shell out to bash from the
    // test (CI portability); the script under scripts/ is for humans.
    rebuild_index(&out).expect("rebuild bindings/index.ts");
}

/// Build `dir/index.ts` as a barrel that re-exports every per-type module.
/// Idempotent. We use re-exports rather than concatenation so that each
/// emitted type file remains the single source of truth — and so that
/// nested imports (e.g. `serde_json/JsonValue`) resolve unambiguously
/// without duplicate `import` statements polluting the barrel.
fn rebuild_index(dir: &std::path::Path) -> std::io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let n = e.file_name();
            let n = n.to_string_lossy();
            n.ends_with(".ts") && n.as_ref() != "index.ts"
        })
        .collect();
    entries.sort_by_key(|e| e.file_name());

    let mut out = String::new();
    out.push_str("// AUTO-GENERATED by tests/bindings_export.rs.\n");
    out.push_str("// Do not edit; re-run `cargo test --features bindings --test bindings_export`.\n");
    out.push_str("// Each per-type module under this directory remains the source of truth.\n\n");

    for entry in &entries {
        let name = entry.file_name().to_string_lossy().replace(".ts", "");
        out.push_str(&format!("export type {{ {name} }} from \"./{name}\";\n"));
    }

    std::fs::write(dir.join("index.ts"), out)?;
    Ok(())
}
