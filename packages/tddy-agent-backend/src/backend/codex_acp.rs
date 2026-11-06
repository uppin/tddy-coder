//! ACP backend for OpenAI Codex via the **`codex-acp`** subprocess (Agent Client Protocol), on the
//! shared [`AcpDriver`].
//!
//! Spawns `codex-acp` on stdio (or **`tddy-acp-stub`** in tests). Resume uses ACP **`load_session`**
//! with the Codex thread id stored as **`codex_thread_id`**. When the agent reports an auth-related
//! error and [`InvokeRequest::session_dir`] is set, runs **`codex login`** with the same
//! **`BROWSER`** / **`TDDY_CODEX_OAUTH_OUT`** path as [`super::CodexBackend`] so tddy-web can poll
//! [`super::CODEX_OAUTH_AUTHORIZE_URL_FILENAME`]. That one login retry is the `codex-acp-login`
//! hooks adapter ([`crate::adapters::CodexAcpLogin`]), which the spec names and the driver runs with.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::acp_driver::{
    self, AcpAgentCommand, AcpDriver, AcpSession, AllowOnce, DriverSlot, OpenSession,
};
use super::codex::merge_codex_prompt;
use super::routed::codex_acp_route_args;
use super::{InvokeRequest, InvokeResponse};
use crate::adapters::{AcpHooksAdapter, AcpHooksLaunch, AcpOpenScope, CodexAcpLogin, CODEX_DEP};
use crate::error::BackendError;
use crate::provider_route::HarnessRoute;

pub(crate) mod oauth;

/// Default `codex-acp` executable name on `PATH` for [`CodexAcpBackend::new`].
pub const DEFAULT_CODEX_ACP_BINARY: &str = "codex-acp";

fn resolve_codex_cli_for_oauth() -> PathBuf {
    std::env::var_os("TDDY_CODEX_CLI")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(super::codex::CODEX_DEFAULT_CLI_BINARY))
}

/// A `codex-acp` that cannot be found is reported with where to get one.
fn missing_codex_acp(error: BackendError, program: &Path) -> BackendError {
    match error {
        BackendError::BinaryNotFound(_) => {
            let p = program.display();
            BackendError::BinaryNotFound(format!(
                "{p} — install the Codex ACP stdio agent on PATH, place `codex-acp` next to your `codex` binary, or set TDDY_CODEX_ACP_CLI / --codex-acp-cli-path / coder-config `codex_acp_cli_path` (see github.com/zed-industries/codex-acp or your Codex CLI distribution)"
            ))
        }
        other => other,
    }
}

/// Backend that invokes **`codex-acp`** over ACP (stdio JSON-RPC).
///
/// The driver spawns the agent when the first request needs it, so it starts on first use rather
/// than at construction: a route set by [`CodexAcpBackend::with_provider_route`] has to be known
/// before the agent exists.
pub struct CodexAcpBackend {
    /// The command the agent is spawned with, as its spec renders it; a route's arguments are added
    /// to it when the driver starts.
    command: AcpAgentCommand,
    /// The hooks adapter the driver runs with, by the name the spec gives (`codex-acp-login`).
    hooks: Arc<dyn AcpHooksAdapter>,
    /// The resolved deps the hooks adapter reads (`codex`, the binary `codex login` runs).
    deps: BTreeMap<String, PathBuf>,
    /// The provider route the agent runs on (`with_provider_route`); `None` for the vendor.
    provider_route: Option<HarnessRoute>,
    driver: DriverSlot,
    /// The open bracket of the running driver's hooks, set when that driver starts.
    open_scope: Mutex<Option<Arc<dyn AcpOpenScope>>>,
    /// One session is opened at a time: the login hooks serve the request whose session is opening.
    opening: tokio::sync::Mutex<()>,
}

impl std::fmt::Debug for CodexAcpBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CodexAcpBackend")
            .field("agent_path", &self.command.program)
            .field("agent_args", &self.command.args)
            .finish()
    }
}

impl Default for CodexAcpBackend {
    fn default() -> Self {
        let agent = std::env::var_os("TDDY_CODEX_ACP_CLI")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_CODEX_ACP_BINARY));
        Self::with_agent_path(agent)
    }
}

impl CodexAcpBackend {
    /// Spawn **`codex-acp`** from `PATH` (or **`TDDY_CODEX_ACP_CLI`**), using **`codex`** for OAuth capture.
    pub fn new() -> Self {
        Self::default()
    }

    /// Spawn the given ACP agent binary (e.g. **`tddy-acp-stub`** in integration tests).
    #[must_use]
    pub fn with_agent_path(path: PathBuf) -> Self {
        Self::unstarted(path, resolve_codex_cli_for_oauth())
    }

    /// Like [`Self::with_agent_path`] but uses a specific **`codex`** binary for `codex login` OAuth relay.
    #[must_use]
    pub fn with_agent_and_codex_paths(agent_path: PathBuf, codex_cli_path: PathBuf) -> Self {
        Self::unstarted(agent_path, codex_cli_path)
    }

    /// The backend that spawns `command` (rendered from `codex-acp.yaml`), its driver run with the
    /// hooks adapter the spec names and the resolved `deps` that adapter reads.
    #[must_use]
    pub fn with_launch(
        command: AcpAgentCommand,
        hooks: Arc<dyn AcpHooksAdapter>,
        deps: BTreeMap<String, PathBuf>,
    ) -> Self {
        Self {
            command,
            hooks,
            deps,
            provider_route: None,
            driver: Mutex::new(None),
            open_scope: Mutex::new(None),
            opening: tokio::sync::Mutex::new(()),
        }
    }

    fn unstarted(agent_path: PathBuf, codex_cli_path: PathBuf) -> Self {
        Self::with_launch(
            AcpAgentCommand {
                program: agent_path,
                args: Vec::new(),
                env: Vec::new(),
                cwd: None,
            },
            Arc::new(CodexAcpLogin),
            BTreeMap::from([(CODEX_DEP.to_string(), codex_cli_path)]),
        )
    }

    /// Run the agent on `route`. A driver started before the route was set runs the vendor: it is
    /// dropped here, which ends its child, and the next request starts one on the route.
    pub(super) fn set_provider_route(&mut self, route: HarnessRoute) {
        self.provider_route = Some(route);
        self.driver = Mutex::new(None);
        self.open_scope = Mutex::new(None);
    }

    /// The driver, started on first use. On a route the agent is given the route's `-c` overrides,
    /// and its environment; the hooks adapter then prepares the command and the hooks the driver
    /// runs with.
    fn driver(&self) -> Result<Arc<AcpDriver>, BackendError> {
        acp_driver::started(&self.driver, || {
            let mut command = self.command.clone();
            if let Some(route) = &self.provider_route {
                command.args.extend(codex_acp_route_args(route));
                command.env.extend(acp_driver::route_env(route));
            }
            let prepared = self.hooks.prepare(AcpHooksLaunch {
                command: &mut command,
                route: self.provider_route.as_ref(),
                deps: &self.deps,
            })?;
            let program = command.program.clone();
            let driver = AcpDriver::spawn(command, Arc::new(AllowOnce), prepared.hooks)
                .map_err(|e| missing_codex_acp(e, &program))?;
            *self.open_scope_slot()? = prepared.opens;
            Ok(driver)
        })
    }

    fn open_scope_slot(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, Option<Arc<dyn AcpOpenScope>>>, BackendError> {
        self.open_scope.lock().map_err(|_| {
            BackendError::InvocationFailed("the codex-acp open scope slot is poisoned".to_string())
        })
    }

    /// Open a session for a request. A login the hooks ran and that failed is the error reported.
    async fn open_session(
        &self,
        driver: &AcpDriver,
        open: OpenSession,
        session_dir: Option<PathBuf>,
    ) -> Result<AcpSession, BackendError> {
        let _one_at_a_time = self.opening.lock().await;
        let scope = self.open_scope_slot()?.clone();
        if let Some(scope) = &scope {
            scope.begin_open(session_dir).await;
        }
        let opened = driver.open_session(open).await;
        let login_failure = match &scope {
            Some(scope) => scope.end_open().await,
            None => None,
        };
        match (opened, login_failure) {
            (Err(_), Some(login_failure)) => Err(login_failure),
            (opened, _) => opened,
        }
    }
}

#[async_trait::async_trait]
impl super::CodingBackend for CodexAcpBackend {
    async fn invoke(&self, request: InvokeRequest) -> Result<InvokeResponse, BackendError> {
        let prompt = merge_codex_prompt(&request)?;
        let driver = self.driver()?;
        let open = acp_driver::open_for_request(&request, self.provider_route.is_some())?;
        let session = self
            .open_session(&driver, open, request.session_dir.clone())
            .await?;
        acp_driver::answer(&driver, session, prompt, request.progress_sink).await
    }

    fn name(&self) -> &str {
        "codex-acp"
    }

    async fn list_models(&self) -> Result<super::BackendModels, BackendError> {
        self.driver()?.list_models().await
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    #[test]
    fn writes_https_oauth_url_to_session_file() {
        // Given
        let tmp =
            std::env::temp_dir().join(format!("tddy-codex-acp-oauth-test-{}", std::process::id()));
        let _ = fs::create_dir_all(&tmp);
        let path = tmp.join(crate::backend::CODEX_OAUTH_AUTHORIZE_URL_FILENAME);

        // When
        fs::write(&path, "https://auth.example.com/o?x=1\n").expect("write");
        let got = fs::read_to_string(&path).expect("read");

        // Then
        assert!(got.trim().starts_with("https://"));
    }
}
