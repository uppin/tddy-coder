//! `codex-acp-login`: the one `codex login` retry on an auth-like `session/new` error, as an ACP
//! hooks adapter (`#agent-tools 11/12`).
//!
//! Today's `CodexLoginHooks` (`backend/codex_acp.rs`, with `backend/codex_acp/oauth.rs`), named:
//! `codex login` runs with the `codex` **dep's** resolved binary, so the binary that signs in is the
//! one `codex-acp` was resolved beside.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;

use super::acp_hooks::{AcpHooksAdapter, AcpHooksLaunch, AcpOpenScope, PreparedAcpHooks};
use crate::backend::acp_driver::{AcpAgentHooks, SessionErrorVerdict};
use crate::backend::codex_acp::oauth::{
    acp_error_suggests_retry_oauth, run_oauth_login_via_codex_cli,
};
use crate::error::BackendError;

/// The adapter that runs `codex login` and retries once on an auth-like session error.
pub const CODEX_ACP_LOGIN: &str = "codex-acp-login";

/// The dep whose binary `codex login` runs.
pub const CODEX_DEP: &str = "codex";

/// [`CODEX_ACP_LOGIN`].
#[derive(Debug, Clone, Copy, Default)]
pub struct CodexAcpLogin;

impl AcpHooksAdapter for CodexAcpLogin {
    fn name(&self) -> &str {
        CODEX_ACP_LOGIN
    }

    /// Hooks over [`CodexLoginHooks`] with the [`CODEX_DEP`] binary, and its open bracket — the
    /// session directory the login writes its authorize URL to, and the login failure reported in
    /// place of the session error. Refused, naming the dep, when the launch resolved no `codex`.
    /// The command is not changed.
    fn prepare(&self, launch: AcpHooksLaunch<'_>) -> Result<PreparedAcpHooks, BackendError> {
        let codex_cli_path = launch.deps.get(CODEX_DEP).ok_or_else(|| {
            BackendError::InvocationFailed(format!(
                "hooks adapter {CODEX_ACP_LOGIN} needs the {CODEX_DEP} dep, and the launch resolved none"
            ))
        })?;
        let hooks = Arc::new(CodexLoginHooks::new(codex_cli_path.clone()));
        Ok(PreparedAcpHooks {
            hooks: hooks.clone(),
            opens: Some(hooks),
        })
    }
}

/// The one `codex login` retry on an auth-like session error, as the driver's hooks.
///
/// The backend sets the session directory of the request it is opening, under its open lock, and
/// reads back a login that failed: that failure is what the caller is told, since the session error
/// it followed says only that the agent was not signed in.
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
}

#[async_trait]
impl AcpOpenScope for CodexLoginHooks {
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

#[async_trait]
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use pretty_assertions::assert_eq;

    use super::*;
    use crate::backend::acp_driver::AcpAgentCommand;

    #[test]
    fn acp_error_suggests_retry_oauth_detects_auth() {
        // When / Then
        assert!(acp_error_suggests_retry_oauth("authentication required"));
        assert!(acp_error_suggests_retry_oauth("HTTP 401"));
        assert!(!acp_error_suggests_retry_oauth("syntax error"));
    }

    #[test]
    fn codex_acp_login_without_a_resolved_codex_dep_is_refused_naming_it() {
        // Given — a launch whose deps resolved only `tddy-tools`
        let mut command = AcpAgentCommand {
            program: PathBuf::from("/opt/codex/bin/codex-acp"),
            args: Vec::new(),
            env: Vec::new(),
            cwd: None,
        };
        let deps = BTreeMap::from([(
            "tddy-tools".to_string(),
            PathBuf::from("/opt/tddy/bin/tddy-tools"),
        )]);

        // When
        let refused = CodexAcpLogin
            .prepare(AcpHooksLaunch {
                command: &mut command,
                route: None,
                deps: &deps,
            })
            .map(|_| ())
            .map_err(|error| error.to_string());

        // Then
        assert_eq!(
            refused,
            Err(
                "hooks adapter codex-acp-login needs the codex dep, and the \
                 launch resolved none"
                    .to_string()
            )
        );
    }
}
