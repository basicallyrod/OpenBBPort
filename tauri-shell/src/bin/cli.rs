//! `tauri-shell-cli` — companion command-line tool for the Tauri shell.
//!
//! This binary exercises the same logic that backs the `#[tauri::command]`
//! handlers, but without booting the Tauri runtime. Each subcommand
//! instantiates the relevant state singletons directly (e.g. `Proxy`,
//! `LOG_STORAGE`, `path_utils`), then calls into `tauri_shell::*`.
//!
//! Use cases:
//! - smoke-test a connector implementation against a real Python REST server
//!   without launching the desktop window
//! - scripting / CI: drive routes, read settings, manage routines
//! - debugging path resolution and log storage
//!
//! Output:
//! - JSON values are pretty-printed by default; pass `--raw` to get compact
//!   output, or `--json` for machine-readable single-line JSON.
//! - All commands ultimately serialize their result as `serde_json::Value`.

use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{json, Map, Value};

use tauri_shell::path_utils;
use tauri_shell::proxy::Proxy;
use tauri_shell::settings;
use tauri_shell::state::{self, LogEntry, LOG_STORAGE};

// ---------------------------------------------------------------------------
// CLI surface
// ---------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(
    name = "tauri-shell-cli",
    version,
    about = "Terminal companion for the Tauri shell — exercises the same IPC commands without booting the Tauri runtime.",
    long_about = None
)]
struct Cli {
    /// Override the Python REST base URL. Defaults to http://127.0.0.1:6900
    /// or the value provided via TAURI_SHELL_BASE_URL.
    #[arg(
        long,
        global = true,
        env = "TAURI_SHELL_BASE_URL",
        default_value = "http://127.0.0.1:6900"
    )]
    base_url: String,

    /// HTTP Basic auth username (used together with `--password`).
    #[arg(long, global = true, env = "TAURI_SHELL_USERNAME")]
    username: Option<String>,

    /// HTTP Basic auth password.
    #[arg(long, global = true, env = "TAURI_SHELL_PASSWORD")]
    password: Option<String>,

    /// Bearer token (mutually exclusive with basic auth).
    #[arg(long, global = true, env = "TAURI_SHELL_BEARER")]
    bearer: Option<String>,

    /// Emit compact (single-line) JSON instead of pretty-printed.
    #[arg(long, global = true)]
    raw: bool,

    /// Emit machine-readable JSON (alias for --raw; kept for discoverability).
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    cmd: TopCmd,
}

#[derive(Subcommand, Debug)]
enum TopCmd {
    /// Generic OpenBB Python REST proxy commands (mirror of `ipc::obb` + `ipc::openbb_meta`).
    Obb {
        #[command(subcommand)]
        cmd: ObbCmd,
    },
    /// Settings file CRUD (mirror of `ipc::settings_files`).
    Settings {
        #[command(subcommand)]
        cmd: SettingsCmd,
    },
    /// User credential vault (mirror of `ipc::credentials`).
    Credentials {
        #[command(subcommand)]
        cmd: CredentialsCmd,
    },
    /// Per-process log ring buffer (mirror of `ipc::infrastructure`).
    Logs {
        #[command(subcommand)]
        cmd: LogsCmd,
    },
    /// .openbb routine files (mirror of `ipc::routines`).
    Routines {
        #[command(subcommand)]
        cmd: RoutinesCmd,
    },
    /// Provider catalog + introspection (mirror of `ipc::provider`).
    Provider {
        #[command(subcommand)]
        cmd: ProviderCmd,
    },
    /// Path resolution helpers (mirror of `ipc::helpers` home/settings dir).
    Paths {
        #[command(subcommand)]
        cmd: PathsCmd,
    },
}

// --- obb -------------------------------------------------------------------

#[derive(Subcommand, Debug)]
enum ObbCmd {
    /// Call any REST route. `--param key=value` may be repeated.
    Call {
        /// Route path, e.g. /equity/price/historical (with or without /api/v1 prefix).
        route: String,
        #[arg(long = "param", value_parser = parse_kv)]
        params: Vec<(String, String)>,
        #[arg(long, value_enum, default_value_t = HttpMethod::Get)]
        method: HttpMethod,
    },
    /// Update the proxy's base URL (in-process only — not persisted).
    SetUrl { url: String },
    /// Fetch `/widgets.json`.
    Widgets,
    /// Fetch `/apps.json`.
    Apps,
    /// Fetch `/agents.json`.
    Agents,
    /// Fetch `/openapi.json`.
    Openapi,
    /// Fetch server `/` health endpoint.
    Health,
    /// Route catalog introspection.
    Routes {
        #[command(subcommand)]
        cmd: RoutesCmd,
    },
}

#[derive(Subcommand, Debug)]
enum RoutesCmd {
    /// List every route (path + method) from `/openapi.json`.
    List,
    /// Free-text search over path / model / summary.
    Search { query: String },
    /// Parameter schema for a single route.
    Params {
        path: String,
        #[arg(long, default_value = "get")]
        method: String,
    },
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum HttpMethod {
    Get,
    Post,
}

// --- settings --------------------------------------------------------------

#[derive(Subcommand, Debug)]
enum SettingsCmd {
    /// List the settings-file allow-list.
    List,
    /// Read a settings file's contents (JSON if applicable, else text).
    Read { file: String },
    /// Overwrite a settings file atomically.
    Write {
        file: String,
        #[arg(long)]
        content: String,
    },
}

// --- credentials -----------------------------------------------------------

#[derive(Subcommand, Debug)]
enum CredentialsCmd {
    /// Read the full user_settings.json (or default tree if missing).
    Get,
    /// Insert or replace a single key under `credentials`.
    Set { key: String, value: String },
}

// --- logs ------------------------------------------------------------------

#[derive(Subcommand, Debug)]
enum LogsCmd {
    /// Create a new buffer for the given process id (idempotent).
    Register { id: String },
    /// Print the tail of a buffer.
    Tail {
        id: String,
        #[arg(long)]
        count: Option<usize>,
    },
    /// Clear a buffer.
    Clear { id: String },
}

// --- routines --------------------------------------------------------------

#[derive(Subcommand, Debug)]
enum RoutinesCmd {
    /// List every `.openbb` file under <settings_dir>/routines/.
    List,
    /// Print a single routine's contents.
    Read { name: String },
    /// Save (overwrite) a routine.
    Save {
        name: String,
        #[arg(long)]
        content: String,
    },
    /// Delete a routine.
    Delete { name: String },
}

// --- provider --------------------------------------------------------------

#[derive(Subcommand, Debug)]
enum ProviderCmd {
    /// `/coverage/providers`.
    List,
    /// Route list for a single provider.
    Routes { provider: String },
    /// Required credential keys for a single provider.
    Creds { provider: String },
}

// --- paths -----------------------------------------------------------------

#[derive(Subcommand, Debug)]
enum PathsCmd {
    /// Print the user's home directory.
    Home,
    /// Print the resolved settings directory.
    Settings,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_kv(raw: &str) -> Result<(String, String), String> {
    let (k, v) = raw
        .split_once('=')
        .ok_or_else(|| format!("expected key=value, got `{raw}`"))?;
    Ok((k.to_string(), v.to_string()))
}

fn build_proxy(args: &Cli) -> Proxy {
    let proxy = Proxy::new();
    proxy.set_base_url(args.base_url.clone());
    if let Some(token) = &args.bearer {
        proxy.set_bearer(token.clone());
    } else if let (Some(u), Some(p)) = (&args.username, &args.password) {
        proxy.set_basic_auth(u.clone(), p.clone());
    }
    proxy
}

fn emit(v: &Value, raw: bool) {
    if raw {
        println!("{}", serde_json::to_string(v).unwrap_or_default());
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(v).unwrap_or_default()
        );
    }
}

fn err(msg: impl Into<String>) -> Value {
    json!({ "error": msg.into() })
}

fn params_to_map(params: Vec<(String, String)>) -> Map<String, Value> {
    let mut out = Map::new();
    for (k, v) in params {
        // Try JSON literal first (bool/number/object), fall back to plain
        // string. Mirrors how the Tauri obb_call handler treats values.
        let parsed: Value =
            serde_json::from_str(&v).unwrap_or_else(|_| Value::String(v.clone()));
        out.insert(k, parsed);
    }
    out
}

// ---------------------------------------------------------------------------
// Entry
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let raw = cli.raw || cli.json;

    let result: Value = match &cli.cmd {
        TopCmd::Obb { cmd } => run_obb(&cli, cmd).await,
        TopCmd::Settings { cmd } => run_settings(cmd),
        TopCmd::Credentials { cmd } => run_credentials(cmd),
        TopCmd::Logs { cmd } => run_logs(cmd),
        TopCmd::Routines { cmd } => run_routines(cmd),
        TopCmd::Provider { cmd } => run_provider(&cli, cmd).await,
        TopCmd::Paths { cmd } => run_paths(cmd),
    };

    let is_error = result.get("error").is_some();
    emit(&result, raw);
    if is_error {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

// ---------------------------------------------------------------------------
// obb
// ---------------------------------------------------------------------------

async fn run_obb(cli: &Cli, cmd: &ObbCmd) -> Value {
    let proxy = build_proxy(cli);
    match cmd {
        ObbCmd::Call {
            route,
            params,
            method,
        } => {
            let map = params_to_map(params.clone());
            let result = match method {
                HttpMethod::Get => proxy.get_with_map::<Value>(route, &map).await,
                HttpMethod::Post => {
                    // Mirror ipc::obb::obb_call: a `body` key becomes the JSON
                    // body, everything else becomes a query string parameter.
                    let body = map
                        .get("body")
                        .cloned()
                        .unwrap_or_else(|| Value::Object(map.clone()));
                    let query: Vec<(&str, &str)> = map
                        .iter()
                        .filter(|(k, _)| k.as_str() != "body")
                        .map(|(k, v)| {
                            (
                                k.as_str(),
                                match v {
                                    Value::String(s) => s.as_str(),
                                    _ => "",
                                },
                            )
                        })
                        .collect();
                    proxy.post::<_, Value>(route, &body, &query).await
                }
            };
            match result {
                Ok(v) => v,
                Err(e) => err(e.to_string()),
            }
        }
        ObbCmd::SetUrl { url } => {
            proxy.set_base_url(url.clone());
            json!({ "ok": true, "baseUrl": proxy.config().base_url })
        }
        ObbCmd::Widgets => proxy
            .get_raw::<Value>("/widgets.json")
            .await
            .unwrap_or_else(|e| err(e.to_string())),
        ObbCmd::Apps => proxy
            .get_raw::<Value>("/apps.json")
            .await
            .unwrap_or_else(|e| err(e.to_string())),
        ObbCmd::Agents => proxy
            .get_raw::<Value>("/agents.json")
            .await
            .unwrap_or_else(|e| err(e.to_string())),
        ObbCmd::Openapi => proxy
            .get_raw::<Value>("/openapi.json")
            .await
            .unwrap_or_else(|e| err(e.to_string())),
        ObbCmd::Health => proxy
            .get_raw::<Value>("/")
            .await
            .unwrap_or_else(|_| json!({ "status": "unreachable" })),
        ObbCmd::Routes { cmd } => run_routes(&proxy, cmd).await,
    }
}

async fn run_routes(proxy: &Proxy, cmd: &RoutesCmd) -> Value {
    // Re-implement list/search/params inline so we don't depend on the
    // `tauri::State` injection layer. The logic mirrors
    // `ipc::openbb_meta::{list_all_routes, search_routes, route_parameters}`.
    let spec: Value = match proxy.get_raw::<Value>("/openapi.json").await {
        Ok(v) => v,
        Err(e) => return err(e.to_string()),
    };

    let routes = collect_routes(&spec);
    match cmd {
        RoutesCmd::List => Value::Array(routes),
        RoutesCmd::Search { query } => {
            let q = query.to_lowercase();
            let filtered: Vec<Value> = routes
                .into_iter()
                .filter(|r| {
                    let p = r.get("path").and_then(|v| v.as_str()).unwrap_or("");
                    let m = r.get("model").and_then(|v| v.as_str()).unwrap_or("");
                    let s = r.get("summary").and_then(|v| v.as_str()).unwrap_or("");
                    p.to_lowercase().contains(&q)
                        || m.to_lowercase().contains(&q)
                        || s.to_lowercase().contains(&q)
                })
                .collect();
            Value::Array(filtered)
        }
        RoutesCmd::Params { path, method } => {
            let path = if path.starts_with('/') {
                path.clone()
            } else {
                format!("/{path}")
            };
            let op = spec
                .get("paths")
                .and_then(|p| p.get(&path))
                .and_then(|m| m.get(method.to_lowercase()))
                .cloned()
                .unwrap_or(Value::Null);
            op.get("parameters")
                .cloned()
                .unwrap_or_else(|| Value::Array(Vec::new()))
        }
    }
}

fn collect_routes(spec: &Value) -> Vec<Value> {
    let mut out = Vec::new();
    let Some(paths) = spec.get("paths").and_then(|v| v.as_object()) else {
        return out;
    };
    for (path, methods) in paths {
        let Some(methods) = methods.as_object() else {
            continue;
        };
        for (method, op) in methods {
            if !matches!(method.as_str(), "get" | "post" | "put" | "delete" | "patch") {
                continue;
            }
            let model = op
                .get("x-openbb-model")
                .or_else(|| op.get("openapi_extra").and_then(|e| e.get("model")))
                .and_then(|v| v.as_str())
                .map(String::from);
            let summary = op
                .get("summary")
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
            let mut entry = serde_json::Map::new();
            entry.insert("path".into(), Value::String(path.clone()));
            entry.insert("method".into(), Value::String(method.to_uppercase()));
            if let Some(m) = model {
                entry.insert("model".into(), Value::String(m));
            }
            if let Some(s) = summary {
                entry.insert("summary".into(), Value::String(s));
            }
            if !tags.is_empty() {
                entry.insert(
                    "tags".into(),
                    Value::Array(tags.into_iter().map(Value::String).collect()),
                );
            }
            out.push(Value::Object(entry));
        }
    }
    out.sort_by(|a, b| {
        let pa = a.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let pb = b.get("path").and_then(|v| v.as_str()).unwrap_or("");
        pa.cmp(pb)
    });
    out
}

// ---------------------------------------------------------------------------
// settings
// ---------------------------------------------------------------------------

const SETTINGS_FILES: &[&str] = &[
    "user_settings.json",
    "system_settings.json",
    "mcp_settings.json",
    "widget_settings.json",
    ".env",
    ".condarc",
];

fn run_settings(cmd: &SettingsCmd) -> Value {
    match cmd {
        SettingsCmd::List => Value::Array(
            SETTINGS_FILES
                .iter()
                .map(|s| Value::String((*s).into()))
                .collect(),
        ),
        SettingsCmd::Read { file } => {
            if !SETTINGS_FILES.iter().any(|f| *f == file.as_str()) {
                return err(format!("file_name not in allow-list: {file}"));
            }
            let Some(path) = path_utils::settings_file(file) else {
                return err("no settings directory");
            };
            if !path.exists() {
                return Value::Null;
            }
            if file.ends_with(".json") {
                match settings::read_json::<Value>(&path) {
                    Ok(Some(v)) => v,
                    Ok(None) => Value::Null,
                    Err(e) => err(e.to_string()),
                }
            } else {
                match std::fs::read_to_string(&path) {
                    Ok(s) => Value::String(s),
                    Err(e) => err(e.to_string()),
                }
            }
        }
        SettingsCmd::Write { file, content } => {
            if !SETTINGS_FILES.iter().any(|f| *f == file.as_str()) {
                return err(format!("file_name not in allow-list: {file}"));
            }
            let Some(path) = path_utils::settings_file(file) else {
                return err("no settings directory");
            };
            if let Err(e) = path_utils::ensure_settings_dir() {
                return err(format!("ensure settings dir: {e}"));
            }
            if file.ends_with(".json") {
                match serde_json::from_str::<Value>(content) {
                    Ok(v) => {
                        if let Err(e) = settings::write_json_atomic(&path, &v) {
                            return err(e.to_string());
                        }
                    }
                    Err(e) => return err(format!("content is not valid JSON: {e}")),
                }
            } else if let Err(e) = std::fs::write(&path, content) {
                return err(e.to_string());
            }
            json!({ "ok": true, "path": path.to_string_lossy() })
        }
    }
}

// ---------------------------------------------------------------------------
// credentials
// ---------------------------------------------------------------------------

const CRED_FILE: &str = "user_settings.json";
const CRED_DEFAULT_TREE: &str = r#"{"credentials": {}, "preferences": {}, "defaults": {}}"#;

fn run_credentials(cmd: &CredentialsCmd) -> Value {
    let Some(path) = path_utils::settings_file(CRED_FILE) else {
        return err("no settings directory");
    };
    match cmd {
        CredentialsCmd::Get => match settings::read_json::<Value>(&path) {
            Ok(Some(v)) => v,
            Ok(None) => serde_json::from_str::<Value>(CRED_DEFAULT_TREE)
                .unwrap_or_else(|_| Value::Object(Default::default())),
            Err(e) => err(e.to_string()),
        },
        CredentialsCmd::Set { key, value } => {
            if let Err(e) = path_utils::ensure_settings_dir() {
                return err(format!("ensure settings dir: {e}"));
            }
            let default: Value = serde_json::from_str(CRED_DEFAULT_TREE).unwrap();
            let result = settings::modify_json(&path, default, |tree| {
                if let Value::Object(map) = tree {
                    let creds = map
                        .entry("credentials".to_string())
                        .or_insert_with(|| Value::Object(Default::default()));
                    if let Value::Object(creds_map) = creds {
                        creds_map.insert(key.clone(), Value::String(value.clone()));
                    }
                }
                Ok(())
            });
            match result {
                Ok(()) => json!({ "ok": true, "key": key }),
                Err(e) => err(e.to_string()),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// logs
// ---------------------------------------------------------------------------

fn run_logs(cmd: &LogsCmd) -> Value {
    let storage = LOG_STORAGE.clone();
    match cmd {
        LogsCmd::Register { id } => {
            let created = state::register_process(&storage, id);
            json!({ "registered": created, "id": id })
        }
        LogsCmd::Tail { id, count } => {
            let entries: Vec<LogEntry> = state::get_logs(&storage, id, *count);
            serde_json::to_value(entries).unwrap_or(Value::Null)
        }
        LogsCmd::Clear { id } => {
            let cleared = state::clear_process_logs(&storage, id);
            json!({ "cleared": cleared, "id": id })
        }
    }
}

// ---------------------------------------------------------------------------
// routines
// ---------------------------------------------------------------------------

fn run_routines(cmd: &RoutinesCmd) -> Value {
    let Some(base) = path_utils::settings_dir() else {
        return err("no settings directory");
    };
    let dir = base.join("routines");

    match cmd {
        RoutinesCmd::List => {
            if !dir.exists() {
                return Value::Array(Vec::new());
            }
            let mut out: Vec<Value> = Vec::new();
            let entries = match std::fs::read_dir(&dir) {
                Ok(e) => e,
                Err(e) => return err(e.to_string()),
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "openbb").unwrap_or(false) {
                    let meta = match entry.metadata() {
                        Ok(m) => m,
                        Err(_) => continue,
                    };
                    let modified_unix = meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    out.push(json!({
                        "name": path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
                        "modifiedUnix": modified_unix,
                        "sizeBytes": meta.len(),
                    }));
                }
            }
            Value::Array(out)
        }
        RoutinesCmd::Read { name } => match validate_routine_name(name) {
            Err(e) => err(e),
            Ok(_) => {
                let path = dir.join(routine_filename(name));
                if !path.exists() {
                    return Value::Null;
                }
                match std::fs::read_to_string(&path) {
                    Ok(s) => Value::String(s),
                    Err(e) => err(e.to_string()),
                }
            }
        },
        RoutinesCmd::Save { name, content } => match validate_routine_name(name) {
            Err(e) => err(e),
            Ok(_) => {
                if let Err(e) = std::fs::create_dir_all(&dir) {
                    return err(e.to_string());
                }
                let path = dir.join(routine_filename(name));
                let tmp = path.with_extension("openbb.tmp");
                if let Err(e) = std::fs::write(&tmp, content.as_bytes()) {
                    return err(e.to_string());
                }
                if let Err(e) = std::fs::rename(&tmp, &path) {
                    return err(e.to_string());
                }
                json!({ "ok": true, "path": path.to_string_lossy() })
            }
        },
        RoutinesCmd::Delete { name } => match validate_routine_name(name) {
            Err(e) => err(e),
            Ok(_) => {
                let path = dir.join(routine_filename(name));
                if !path.exists() {
                    return json!({ "deleted": false });
                }
                match std::fs::remove_file(&path) {
                    Ok(_) => json!({ "deleted": true }),
                    Err(e) => err(e.to_string()),
                }
            }
        },
    }
}

fn validate_routine_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || name.starts_with('.')
    {
        return Err(format!("invalid routine name: {name}"));
    }
    Ok(())
}

fn routine_filename(name: &str) -> String {
    if name.ends_with(".openbb") {
        name.to_string()
    } else {
        format!("{name}.openbb")
    }
}

// ---------------------------------------------------------------------------
// provider
// ---------------------------------------------------------------------------

async fn run_provider(cli: &Cli, cmd: &ProviderCmd) -> Value {
    let proxy = build_proxy(cli);
    match cmd {
        ProviderCmd::List => match proxy.get::<Value>("/coverage/providers", &[]).await {
            Ok(v) => v,
            Err(e) => err(e.to_string()),
        },
        ProviderCmd::Routes { provider } => {
            let raw = match proxy.get::<Value>("/coverage/providers", &[]).await {
                Ok(v) => v,
                Err(e) => return err(e.to_string()),
            };
            let routes = raw
                .get(provider)
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| Value::String(s.to_string())))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            Value::Array(routes)
        }
        ProviderCmd::Creds { provider } => {
            let raw = match proxy.get::<Value>("/coverage/command_model", &[]).await {
                Ok(v) => v,
                Err(e) => return err(e.to_string()),
            };
            let mut creds = std::collections::BTreeSet::new();
            if let Some(obj) = raw.as_object() {
                for (_route, models) in obj {
                    if let Some(prov_map) = models.as_object() {
                        if let Some(p) = prov_map.get(provider) {
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
            Value::Array(creds.into_iter().map(Value::String).collect())
        }
    }
}

// ---------------------------------------------------------------------------
// paths
// ---------------------------------------------------------------------------

fn run_paths(cmd: &PathsCmd) -> Value {
    match cmd {
        PathsCmd::Home => match path_utils::home_dir() {
            Some(p) => Value::String(p.to_string_lossy().into_owned()),
            None => err("home directory not available"),
        },
        PathsCmd::Settings => match path_utils::settings_dir() {
            Some(p) => Value::String(p.to_string_lossy().into_owned()),
            None => err("settings directory not available"),
        },
    }
}
