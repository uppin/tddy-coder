//! ACP backend for OpenAI Codex via the **`codex-acp`** subprocess (Agent Client Protocol), on the
//! shared [`AcpDriver`].
//!
//! Spawns `codex-acp` on stdio (or **`tddy-acp-stub`** in tests). Resume uses ACP **`load_session`**
//! with the Codex thread id stored as **`codex_thread_id`**. When the agent reports an auth-related
//! error and [`InvokeRequest::session_dir`] is set, runs **`codex login`** with the same
//! **`BROWSER`** / **`TDDY_CODEX_OAUTH_OUT`** path as [`super::CodexBackend`] so tddy-web can poll
//! [`super::CODEX_OAUTH_AUTHORIZE_URL_FILENAME`]. That one login retry is this backend's hook on the
//! driver.

use super::acp_driver::{
    self, AcpAgentCommand, AcpAgentHooks, AcpDriver, AcpSession, AllowOnce, DriverSlot,
    OpenSession, SessionErrorVerdict,
};
use super::codex::merge_codex_prompt;
use super::routed::codex_acp_route_args;
use super::{InvokeRequest, InvokeResponse};
use crate::error::BackendError;
use crate::provider_route::HarnessRoute;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

mod oauth;
use oauth::{acp_error_suggests_retry_oauth, run_oauth_login_via_codex_cli};

/// Default `codex-acp` executable name on `PATH` for [`CodexAcpBackend::new`].
pub const DEFAULT_CODEX_ACP_BINARY: &str = "codex-acp";

/// The one `codex login` retry on an auth-like session error, as the driver's hooks.
///
/// The backend sets the session directory of the request it is opening, under its open lock, and
/// reads back a login that failed: that failure is what the caller is told, since the session error
/// it followed says only that the agent was not signed in.
// TODO(acp-agents): green moves this into `adapters::CodexAcpLogin` (`codex-acp-login`), and the
// backend is built from `codex-acp.yaml` (`tddy_coder::acp_from_spec::codex_acp_backend_from_spec`).
struct CodexLoginHooks {
    codex_cli_path: PathBuf,
    session_dir: tokio::sync::Mutex<Option<PathBuf>>,
    login_failure: tokio::sync::Mutex<Option<BackendError>>,
}

impl CodexLoginHooks {
    fn new(codex_cli_path: PathBuf) -> Self {
        Self {
            codex_cli_path,
            session_dir: tokio::sync::Mutex::new(None),
            login_failure: tokio::sync::Mutex::new(None),
        }
    }

    /// Name the session directory a login for the open about to run writes its URL to.
    async fn begin_open(&self, session_dir: Option<PathBuf>) {
        *self.session_dir.lock().await = session_dir;
        *self.login_failure.lock().await = None;
    }

    /// End the open: clear its session directory, and return a login that failed during it.
    async fn end_open(&self) -> Option<BackendError> {
        *self.session_dir.lock().await = None;
        self.login_failure.lock().await.take()
    }
}

#[async_trait::async_trait]
impl AcpAgentHooks for CodexLoginHooks {
    async fn on_session_error(&self, message: &str) -> SessionErrorVerdict {
        let session_dir = self.session_dir.lock().await.clone();
        let Some(session_dir) = session_dir else {
            return SessionErrorVerdict::Fail;
        };
        if !acp_error_suggests_retry_oauth(message) {
            return SessionErrorVerdict::Fail;
        }
        match run_oauth_login_via_codex_cli(self.codex_cli_path.clone(), session_dir).await {
            Ok(()) => SessionErrorVerdict::RetryOnce,
            Err(error) => {
                *self.login_failure.lock().await = Some(error);
                SessionErrorVerdict::Fail
            }
        }
    }
}

fn resolve_codex_cli_for_oauth() -> PathBuf {
    std::env::var_os("TDDY_CODEX_CLI")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(super::CodexBackend::DEFAULT_CLI_BINARY))
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
    agent_path: PathBuf,
    agent_args: Vec<String>,
    /// The provider route the agent runs on (`with_provider_route`); `None` for the vendor.
    provider_route: Option<HarnessRoute>,
    driver: DriverSlot,
    hooks: Arc<CodexLoginHooks>,
    /// One session is opened at a time: the login hooks serve the request whose session is opening.
    opening: tokio::sync::Mutex<()>,
}

impl std::fmt::Debug for CodexAcpBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CodexAcpBackend")
            .field("agent_path", &self.agent_path)
            .field("agent_args", &self.agent_args)
            .finish()
    }
}

impl Default for CodexAcpBackend {
    fn default() -> Self {
        let agent = std::env::var_os("TDDY_CODEX_ACP_CLI")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_CODEX_ACP_BINARY));
        Self::unstarted(agent, Vec::new(), resolve_codex_cli_for_oauth())
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
        Self::unstarted(path, Vec::new(), resolve_codex_cli_for_oauth())
    }

    /// Like [`Self::with_agent_path`] but uses a specific **`codex`** binary for `codex login` OAuth relay.
    #[must_use]
    pub fn with_agent_and_codex_paths(agent_path: PathBuf, codex_cli_path: PathBuf) -> Self {
        Self::unstarted(agent_path, Vec::new(), codex_cli_path)
    }

    fn unstarted(agent_path: PathBuf, agent_args: Vec<String>, codex_cli_path: PathBuf) -> Self {
        Self {
            agent_path,
            agent_args,
            provider_route: None,
            driver: Mutex::new(None),
            hooks: Arc::new(CodexLoginHooks::new(codex_cli_path)),
            opening: tokio::sync::Mutex::new(()),
        }
    }

    /// Run the agent on `route`. A driver started before the route was set runs the vendor: it is
    /// dropped here, which ends its child, and the next request starts one on the route.
    pub(super) fn set_provider_route(&mut self, route: HarnessRoute) {
        self.provider_route = Some(route);
        self.driver = Mutex::new(None);
    }

    /// The driver, started on first use. On a route the agent is given the route's `-c` overrides,
    /// and its environment.
    fn driver(&self) -> Result<Arc<AcpDriver>, BackendError> {
        acp_driver::started(&self.driver, || {
            let mut args = self.agent_args.clone();
            let mut env = Vec::new();
            if let Some(route) = &self.provider_route {
                args.extend(codex_acp_route_args(route));
                env = acp_driver::route_env(route);
            }
            let command = AcpAgentCommand {
                program: self.agent_path.clone(),
                args,
                env,
                cwd: None,
            };
            AcpDriver::spawn(command, Arc::new(AllowOnce), self.hooks.clone())
                .map_err(|e| missing_codex_acp(e, &self.agent_path))
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
        self.hooks.begin_open(session_dir).await;
        let opened = driver.open_session(open).await;
        match (opened, self.hooks.end_open().await) {
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
    use super::*;
    use std::fs;

    #[test]
    fn acp_error_suggests_retry_oauth_detects_auth() {
        // When / Then
        assert!(acp_error_suggests_retry_oauth("authentication required"));
        assert!(acp_error_suggests_retry_oauth("HTTP 401"));
        assert!(!acp_error_suggests_retry_oauth("syntax error"));
    }

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
