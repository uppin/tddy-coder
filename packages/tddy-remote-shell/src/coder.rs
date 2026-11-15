//! A Coder workspace as a remote-shell transport: the `coder` CLI on this host, given the
//! deployment's URL and a session token in its own environment and nowhere else.
//!
//! `coder ssh` is run with `--disable-autostart` always: this transport never starts a workspace.
//! One that is not running is [`RemoteShellError::Unreachable`], with what the CLI said.

use std::fmt;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tddy_credentials::SecretString;

use crate::{
    ExecOutput, ExecRequest, InteractiveCommand, RemoteShell, RemoteShellError, ShellAddress,
    StreamedExec,
};

/// What the `coder` CLI needs to reach a deployment: where it is, and the session token to
/// present there.
///
/// Held in memory for the life of the shell that owns it. `Debug` shows the URL and never the
/// token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoderCredentials {
    /// The deployment's root, e.g. `https://coder.example.com`, as the CLI takes it in `CODER_URL`.
    pub url: String,
    /// The session token, as the CLI takes it in `CODER_SESSION_TOKEN`.
    pub token: SecretString,
}

impl CoderCredentials {
    pub fn new(url: impl Into<String>, token: SecretString) -> Self {
        Self {
            url: url.into(),
            token,
        }
    }
}

/// One workspace of a deployment, and optionally one of its agents.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CoderWorkspaceRef {
    /// The login name of the user the workspace belongs to.
    pub owner: String,
    /// The workspace's name, unique among its owner's.
    pub name: String,
    /// The agent to reach, for a workspace with more than one; `None` leaves the choice to the
    /// CLI, which is right for a workspace with exactly one.
    pub agent: Option<String>,
}

impl CoderWorkspaceRef {
    /// `owner`'s workspace `name`, on whichever agent it has.
    pub fn new(owner: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            owner: owner.into(),
            name: name.into(),
            agent: None,
        }
    }

    /// This workspace, on its agent `agent`.
    pub fn on_agent(mut self, agent: impl Into<String>) -> Self {
        self.agent = Some(agent.into());
        self
    }
}

/// The workspace as `coder ssh` is given it: `<owner>/<name>`, then `.<agent>` when one is named.
impl fmt::Display for CoderWorkspaceRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // TODO(coder-shell): implement
        let _ = self;
        f.write_str("")
    }
}

/// A shell on a Coder workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoderShell {
    coder: PathBuf,
    credentials: CoderCredentials,
    workspace: CoderWorkspaceRef,
}

impl CoderShell {
    /// The tag of this transport in a [`ShellAddress`].
    pub const TRANSPORT: &'static str = "coder";

    /// The variable the `coder` CLI reads its deployment's URL from.
    pub const URL_VARIABLE: &'static str = "CODER_URL";

    /// The variable the `coder` CLI reads its session token from.
    pub const SESSION_TOKEN_VARIABLE: &'static str = "CODER_SESSION_TOKEN";

    /// The shell on `workspace`, reached through the `coder` on `PATH` as `credentials`.
    pub fn new(credentials: CoderCredentials, workspace: CoderWorkspaceRef) -> Self {
        Self::through("coder", credentials, workspace)
    }

    /// [`Self::new`], reaching the workspace through the CLI `coder_binary`.
    pub fn through(
        coder_binary: impl Into<PathBuf>,
        credentials: CoderCredentials,
        workspace: CoderWorkspaceRef,
    ) -> Self {
        Self {
            coder: coder_binary.into(),
            credentials,
            workspace,
        }
    }

    /// The `coder` CLI this shell runs.
    pub fn coder_binary(&self) -> &Path {
        &self.coder
    }

    /// The deployment and token this shell presents.
    pub fn credentials(&self) -> &CoderCredentials {
        &self.credentials
    }

    /// The workspace this shell reaches.
    pub fn workspace(&self) -> &CoderWorkspaceRef {
        &self.workspace
    }
}

#[async_trait]
impl RemoteShell for CoderShell {
    /// The transport's tag and the workspace as `coder ssh` names it.
    fn address(&self) -> ShellAddress {
        ShellAddress::new(Self::TRANSPORT, self.workspace.to_string())
    }

    /// `coder ssh --disable-autostart <workspace> -- sh -c '<command>'`, with
    /// [`Self::URL_VARIABLE`] and [`Self::SESSION_TOKEN_VARIABLE`] set on that child alone. A
    /// `coder` that exits non-zero without having run the command — a workspace that is stopped,
    /// an agent that is not connected, a token the deployment refuses — is
    /// [`RemoteShellError::Unreachable`] with what it wrote to stderr.
    async fn exec(&self, request: ExecRequest) -> Result<ExecOutput, RemoteShellError> {
        // TODO(coder-shell): implement
        let _ = request;
        Err(RemoteShellError::Spawn(
            "CoderShell::exec is not implemented".to_string(),
        ))
    }

    async fn exec_streamed(&self, command: &str) -> Result<StreamedExec, RemoteShellError> {
        // TODO(coder-shell): implement
        let _ = command;
        Err(RemoteShellError::Spawn(
            "CoderShell::exec_streamed is not implemented".to_string(),
        ))
    }

    /// `coder ssh --disable-autostart -t <workspace> -- <command>`, with the two variables in the
    /// command's `env`: the caller's PTY child is the one process that is given them.
    fn interactive(&self, command: &str) -> InteractiveCommand {
        // TODO(coder-shell): implement
        let _ = command;
        InteractiveCommand::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const ADAS_TOKEN: &str = "adas-coder-session-token";

    fn adas_credentials() -> CoderCredentials {
        CoderCredentials::new("https://coder.example.com", SecretString::new(ADAS_TOKEN))
    }

    #[test]
    fn a_workspace_reads_as_its_owner_and_name() {
        // Given
        let workspace = CoderWorkspaceRef::new("ada", "devbox");

        // When
        let shown = workspace.to_string();

        // Then
        assert_eq!(shown, "ada/devbox");
    }

    #[test]
    fn a_workspace_on_a_named_agent_reads_with_the_agent_after_a_dot() {
        // Given
        let workspace = CoderWorkspaceRef::new("ada", "devbox").on_agent("main");

        // When
        let shown = workspace.to_string();

        // Then
        assert_eq!(shown, "ada/devbox.main");
    }

    #[test]
    fn a_coder_shell_is_addressed_by_its_workspace() {
        // Given
        let shell = CoderShell::through(
            "/opt/bin/coder",
            adas_credentials(),
            CoderWorkspaceRef::new("ada", "devbox").on_agent("main"),
        );

        // When
        let address = shell.address();

        // Then
        assert_eq!(address, ShellAddress::new("coder", "ada/devbox.main"));
        assert_eq!(shell.coder_binary(), Path::new("/opt/bin/coder"));
    }

    #[test]
    fn a_coder_shell_never_shows_its_token() {
        // Given
        let shell = CoderShell::new(adas_credentials(), CoderWorkspaceRef::new("ada", "devbox"));

        // When
        let shown = format!("{shell:?}");

        // Then
        assert_eq!(shown.contains(ADAS_TOKEN), false, "got {shown}");
        assert_eq!(
            shown.contains("https://coder.example.com"),
            true,
            "got {shown}"
        );
    }
}
