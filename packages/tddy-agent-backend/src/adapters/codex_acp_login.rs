//! `codex-acp-login`: the one `codex login` retry on an auth-like `session/new` error, as an ACP
//! hooks adapter (`#agent-tools 11/12`).
//!
//! Today's `CodexLoginHooks` (`backend/codex_acp.rs`, with `backend/codex_acp/oauth.rs`), named:
//! `codex login` runs with the `codex` **dep's** resolved binary, so the binary that signs in is the
//! one `codex-acp` was resolved beside.

use super::acp_hooks::{AcpHooksAdapter, AcpHooksLaunch, PreparedAcpHooks};
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

    /// Hooks over `CodexLoginHooks` with the [`CODEX_DEP`] binary, and its open bracket — the
    /// session directory the login writes its authorize URL to, and the login failure reported in
    /// place of the session error. Refused, naming the dep, when the launch resolved no `codex`.
    /// The command is not changed.
    fn prepare(&self, launch: AcpHooksLaunch<'_>) -> Result<PreparedAcpHooks, BackendError> {
        // TODO(acp-agents): implement — green moves `CodexLoginHooks` here and
        // `CodexAcpBackend` holds the prepared hooks.
        let _ = (launch, CODEX_DEP);
        todo!("acp-agents: CodexAcpLogin::prepare")
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
                "backend invocation failed: hooks adapter codex-acp-login needs the codex dep, and the \
                 launch resolved none"
                    .to_string()
            )
        );
    }
}
