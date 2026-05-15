//! Integration test for the [`Connector`] trait surface.
//!
//! Demonstrates the override pattern documented in SPEC.md Slice B:
//! - The shell ships `NoopConnector` (every method returns
//!   `ConnectorError::NotImplemented`).
//! - Users can override individual methods by writing their own trait impl
//!   and registering it via `.manage::<Arc<dyn Connector>>(...)` in
//!   `main.rs`. Methods that are NOT overridden inherit the default
//!   `NotImplemented` body, which is what the IPC layer surfaces to the
//!   renderer.

use std::sync::Arc;

use async_trait::async_trait;
use tauri_shell::connector::{
    Connector, ConnectorError, ExecuteInEnvironmentArgs, NoopConnector,
};
use tauri_shell::ipc::backends::BackendService;
use tauri_shell::ipc::environments::{CondaEnvironment, ExecResult};

/// A custom connector that overrides four methods covering each error
/// category: success-returning, success-returning-Vec, an InvalidArgument
/// failure, and an Internal failure. All other methods inherit the
/// default `NotImplemented` body from the trait.
#[derive(Default)]
struct TestConnector;

#[async_trait]
impl Connector for TestConnector {
    async fn toggle_theme(&self, theme: String) -> Result<bool, ConnectorError> {
        match theme.as_str() {
            "light" | "dark" | "system" => Ok(true),
            other => Err(ConnectorError::invalid(format!("unknown theme: {other}"))),
        }
    }

    async fn get_working_directory(
        &self,
        default_dir: Option<String>,
    ) -> Result<String, ConnectorError> {
        Ok(default_dir.unwrap_or_else(|| "/tmp/test".into()))
    }

    async fn list_conda_environments(
        &self,
        _directory: Option<String>,
    ) -> Result<Vec<CondaEnvironment>, ConnectorError> {
        Ok(vec![CondaEnvironment {
            name: "test-env".into(),
            python_version: "3.11".into(),
            path: "/opt/conda/envs/test-env".into(),
        }])
    }

    async fn execute_in_environment(
        &self,
        args: ExecuteInEnvironmentArgs,
    ) -> Result<ExecResult, ConnectorError> {
        if args.command.is_empty() {
            return Err(ConnectorError::invalid("empty command"));
        }
        Ok(ExecResult {
            stdout: format!("[{}] {}", args.environment, args.command),
            stderr: String::new(),
            exit_code: 0,
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime")
}

// ---------------------------------------------------------------------------
// Tests — overridden methods
// ---------------------------------------------------------------------------

#[test]
fn override_toggle_theme_accepts_known_value() {
    let c: Arc<dyn Connector> = Arc::new(TestConnector);
    let out = rt().block_on(c.toggle_theme("dark".into()));
    assert!(matches!(out, Ok(true)));
}

#[test]
fn override_toggle_theme_rejects_unknown_value() {
    let c: Arc<dyn Connector> = Arc::new(TestConnector);
    let err = rt()
        .block_on(c.toggle_theme("rainbow".into()))
        .expect_err("expected error");
    match err {
        ConnectorError::InvalidArgument(m) => assert!(m.contains("rainbow"), "got: {m}"),
        other => panic!("wrong variant: {other:?}"),
    }
}

#[test]
fn override_get_working_directory_uses_default() {
    let c: Arc<dyn Connector> = Arc::new(TestConnector);
    let out = rt()
        .block_on(c.get_working_directory(Some("/custom/path".into())))
        .expect("ok");
    assert_eq!(out, "/custom/path");

    let fallback = rt().block_on(c.get_working_directory(None)).expect("ok");
    assert_eq!(fallback, "/tmp/test");
}

#[test]
fn override_list_conda_environments_returns_seeded_list() {
    let c: Arc<dyn Connector> = Arc::new(TestConnector);
    let envs = rt().block_on(c.list_conda_environments(None)).expect("ok");
    assert_eq!(envs.len(), 1);
    assert_eq!(envs[0].name, "test-env");
    assert_eq!(envs[0].python_version, "3.11");
}

#[test]
fn override_execute_in_environment_round_trips_args() {
    let c: Arc<dyn Connector> = Arc::new(TestConnector);
    let res = rt()
        .block_on(c.execute_in_environment(ExecuteInEnvironmentArgs {
            command: "pip list".into(),
            environment: "test-env".into(),
            directory: "/tmp".into(),
        }))
        .expect("ok");
    assert_eq!(res.exit_code, 0);
    assert_eq!(res.stdout, "[test-env] pip list");
    assert!(res.stderr.is_empty());
}

#[test]
fn override_execute_in_environment_rejects_empty_command() {
    let c: Arc<dyn Connector> = Arc::new(TestConnector);
    let err = rt()
        .block_on(c.execute_in_environment(ExecuteInEnvironmentArgs {
            command: "".into(),
            environment: "test-env".into(),
            directory: "/tmp".into(),
        }))
        .expect_err("expected error");
    assert!(matches!(err, ConnectorError::InvalidArgument(_)));
}

// ---------------------------------------------------------------------------
// Tests — un-overridden methods fall through to NotImplemented
// ---------------------------------------------------------------------------

#[test]
fn unoverridden_method_returns_not_implemented() {
    let c: Arc<dyn Connector> = Arc::new(TestConnector);
    // `list_backend_services` is not overridden by TestConnector — should
    // fall back to the default trait body.
    let err = rt()
        .block_on(c.list_backend_services())
        .expect_err("expected error");
    match err {
        ConnectorError::NotImplemented(name) => assert_eq!(name, "list_backend_services"),
        other => panic!("wrong variant: {other:?}"),
    }
}

#[test]
fn unoverridden_server_spawn_returns_not_implemented() {
    let c: Arc<dyn Connector> = Arc::new(TestConnector);
    // We can't construct an AppHandle here, but we can at least confirm
    // the result type by exercising an un-async method first. Use a
    // method that does NOT need AppHandle.
    let err = rt().block_on(c.server_list()).expect_err("expected error");
    match err {
        ConnectorError::NotImplemented(name) => assert_eq!(name, "server_list"),
        other => panic!("wrong variant: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Tests — NoopConnector returns NotImplemented for every probed method
// ---------------------------------------------------------------------------

#[test]
fn noop_connector_returns_not_implemented_for_every_method() {
    let c: Arc<dyn Connector> = Arc::new(NoopConnector);
    let rt = rt();

    // Probe a representative subset across all stub families to make sure
    // every domain inherits the default body.
    macro_rules! assert_not_impl {
        ($fut:expr, $name:literal) => {{
            match rt.block_on($fut) {
                Err(ConnectorError::NotImplemented(n)) => assert_eq!(n, $name),
                other => panic!("expected NotImplemented({}), got {:?}", $name, other),
            }
        }};
    }

    assert_not_impl!(c.toggle_theme("dark".into()), "toggle_theme");
    assert_not_impl!(c.get_working_directory(None), "get_working_directory");
    assert_not_impl!(c.save_working_directory("/tmp".into()), "save_working_directory");
    assert_not_impl!(c.abort_installation("/tmp".into()), "abort_installation");
    assert_not_impl!(c.create_default_backend_services(), "create_default_backend_services");
    assert_not_impl!(c.get_installation_directory(), "get_installation_directory");
    assert_not_impl!(c.get_userdata_directory(), "get_userdata_directory");
    assert_not_impl!(c.list_conda_environments(None), "list_conda_environments");
    assert_not_impl!(c.select_requirements_file(), "select_requirements_file");
    assert_not_impl!(
        c.get_environment_extensions("env".into()),
        "get_environment_extensions"
    );
    assert_not_impl!(c.remove_environment("env".into()), "remove_environment");
    assert_not_impl!(c.list_backend_services(), "list_backend_services");
    assert_not_impl!(
        c.create_backend_service(BackendService::default()),
        "create_backend_service"
    );
    assert_not_impl!(c.check_jupyter_server("env".into()), "check_jupyter_server");
    assert_not_impl!(c.list_jupyter_servers(), "list_jupyter_servers");
    assert_not_impl!(c.server_status("id".into()), "server_status");
    assert_not_impl!(c.server_list(), "server_list");
    assert_not_impl!(c.mcp_status("id".into()), "mcp_status");
    assert_not_impl!(c.mcp_list(), "mcp_list");
    assert_not_impl!(c.mcp_list_tools("id".into()), "mcp_list_tools");
}

// ---------------------------------------------------------------------------
// Tests — IpcError conversion preserves the variant
// ---------------------------------------------------------------------------

#[test]
fn connector_error_maps_to_ipc_error() {
    use tauri_shell::ipc::IpcError;
    let err: IpcError = ConnectorError::NotImplemented("foo").into();
    match err {
        IpcError::NotImplemented(m) => assert_eq!(m, "foo"),
        other => panic!("wrong variant: {other:?}"),
    }

    let err: IpcError = ConnectorError::InvalidArgument("bar".into()).into();
    assert!(matches!(err, IpcError::InvalidArgument(_)));

    let err: IpcError = ConnectorError::Conflict("baz".into()).into();
    assert!(matches!(err, IpcError::Conflict(_)));

    let err: IpcError = ConnectorError::Unauthorized("qux".into()).into();
    assert!(matches!(err, IpcError::Unauthorized(_)));

    let err: IpcError = ConnectorError::Internal("oops".into()).into();
    assert!(matches!(err, IpcError::Internal(_)));

    let err: IpcError = ConnectorError::Io("disk".into()).into();
    assert!(matches!(err, IpcError::Io(_)));
}
