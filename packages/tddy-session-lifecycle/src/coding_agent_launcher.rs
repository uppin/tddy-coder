//! The daemon's [`CodingAgentLauncher`]: turns a coding-agent launch into the ACP agent command.
//!
//! What it does today, and — as loudly — what it refuses. A coding agent is a subagent that can
//! write, so a launch the daemon cannot honour *exactly* is refused with a typed reason rather
//! than started some weaker way:
//!
//! - **The session's OS user.** The ACP driver needs piped stdio to its child. No existing daemon
//!   spawn path (`spawn_as_user`, the spawn worker) hands back piped stdio for a non-PTY child run
//!   as another user, so the agent can be started only when the daemon already runs as the
//!   session's user (the desktop deployment). Any other launch is refused, never run as the
//!   daemon's own user. FIXME(acp-subagent): a spawn-as-user path with piped stdio.
//! - **A provider account's route.** Resolving one reads the session owner's vault
//!   (`tddy-agent-launch`'s `SessionAccountAccess`), which this launcher does not hold. A profile
//!   naming a `provider_account` is refused. FIXME(acp-subagent): apply the route.
//! - **A managed session.** Withdrawing the harness's native tools (see
//!   `tddy_coding_subagent::confinement`) and giving `tddy-tools --mcp` its transport
//!   credential are not applied here, so a managed launch is refused. FIXME(acp-subagent).
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-10-24-coagents-acp-subagent.md

use std::path::PathBuf;

use tddy_coding_subagent::{CodingAgentLaunch, CodingAgentLauncher};
use tddy_core::AcpAgentCommand;
use tddy_discovery::subagent::SubagentError;

/// Env var naming the `codex-acp` binary, as the workflow backend reads it.
const CODEX_ACP_CLI_ENV: &str = "TDDY_CODEX_ACP_CLI";

/// Starts the harness named by a launch as the daemon's own OS user, on the session's worktree.
#[derive(Debug, Clone)]
pub struct DaemonCodingAgentLauncher {
    daemon_os_user: Option<String>,
}

impl DaemonCodingAgentLauncher {
    /// A launcher for a daemon running as the current process's user.
    #[must_use]
    pub fn for_this_process() -> Self {
        #[cfg(unix)]
        let daemon_os_user =
            tddy_session_activity::user_sessions_path::username_for_uid(unsafe { libc::geteuid() });
        #[cfg(not(unix))]
        let daemon_os_user = None;
        Self { daemon_os_user }
    }

    /// A launcher for a daemon running as `os_user` (tests).
    #[must_use]
    pub fn running_as(os_user: &str) -> Self {
        Self {
            daemon_os_user: Some(os_user.to_string()),
        }
    }

    fn refuse(launch: &CodingAgentLaunch, reason: &str) -> SubagentError {
        SubagentError::from(format!(
            "coding agent '{}' ({}) cannot be launched: {reason}",
            launch.def.name, launch.def.harness
        ))
    }

    fn harness_command(
        launch: &CodingAgentLaunch,
    ) -> Result<(PathBuf, Vec<String>), SubagentError> {
        match launch.def.harness.as_str() {
            "claude-acp" => Ok((PathBuf::from("bunx"), vec!["claude-agent-acp".to_string()])),
            "codex-acp" => Ok((
                std::env::var_os(CODEX_ACP_CLI_ENV)
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("codex-acp")),
                Vec::new(),
            )),
            "opencode" => Ok((PathBuf::from("opencode"), vec!["acp".to_string()])),
            other => Err(Self::refuse(
                launch,
                &format!("this daemon has no ACP command for the '{other}' harness"),
            )),
        }
    }
}

#[async_trait::async_trait]
impl CodingAgentLauncher for DaemonCodingAgentLauncher {
    async fn command_for(
        &self,
        launch: &CodingAgentLaunch,
    ) -> Result<AcpAgentCommand, SubagentError> {
        match self.daemon_os_user.as_deref() {
            Some(daemon_user) if daemon_user == launch.session_os_user => {}
            daemon_user => {
                return Err(Self::refuse(
                    launch,
                    &format!(
                        "the session runs as '{}' but this daemon runs as '{}', and its launcher \
                         cannot start a piped-stdio child as another user; it will not run the \
                         agent as the daemon's own user",
                        launch.session_os_user,
                        daemon_user.unwrap_or("an unresolvable user"),
                    ),
                ))
            }
        }
        if let Some(account) = launch.def.provider_account.as_deref() {
            return Err(Self::refuse(
                launch,
                &format!(
                    "it runs on provider account '{account}', and this daemon's launcher does not \
                     apply a provider route yet; use an agent on the harness's own login"
                ),
            ));
        }
        if launch.managed {
            return Err(Self::refuse(
                launch,
                "the session is a managed codebase, and this daemon's launcher does not yet \
                 withdraw the harness's native tools or give it tddy-tools' transport",
            ));
        }
        let (program, args) = Self::harness_command(launch)?;
        Ok(AcpAgentCommand {
            program,
            args,
            env: Vec::new(),
            cwd: Some(launch.worktree.clone()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_coding_subagent::CodingAgentDef;

    fn a_launch(harness: &str, session_user: &str) -> CodingAgentLaunch {
        CodingAgentLaunch {
            def: CodingAgentDef {
                name: "coder".to_string(),
                label: None,
                harness: harness.to_string(),
                model: "m".to_string(),
                provider_account: None,
            },
            session_id: "s".to_string(),
            session_os_user: session_user.to_string(),
            worktree: PathBuf::from("/work/tree"),
            managed: false,
            mcp_servers: Vec::new(),
        }
    }

    async fn command(
        launcher: &DaemonCodingAgentLauncher,
        launch: &CodingAgentLaunch,
    ) -> Result<AcpAgentCommand, String> {
        launcher
            .command_for(launch)
            .await
            .map_err(|e| e.to_string())
    }

    #[tokio::test]
    async fn a_session_of_the_daemons_own_user_starts_the_harness_on_the_worktree() {
        let launcher = DaemonCodingAgentLauncher::running_as("dev");

        let command = command(&launcher, &a_launch("opencode", "dev"))
            .await
            .expect("launches");

        assert_eq!(command.program, PathBuf::from("opencode"));
        assert_eq!(command.args, ["acp"]);
        assert_eq!(command.cwd, Some(PathBuf::from("/work/tree")));
    }

    #[tokio::test]
    async fn a_session_of_another_user_is_refused_naming_both_users() {
        let launcher = DaemonCodingAgentLauncher::running_as("daemon");

        let refusal = command(&launcher, &a_launch("opencode", "dev"))
            .await
            .expect_err("refused");

        assert!(
            refusal.contains("'dev'") && refusal.contains("'daemon'"),
            "{refusal}"
        );
    }

    #[tokio::test]
    async fn a_provider_account_is_refused_rather_than_run_on_the_vendor() {
        let launcher = DaemonCodingAgentLauncher::running_as("dev");
        let mut launch = a_launch("codex-acp", "dev");
        launch.def.provider_account = Some("meta".to_string());

        let refusal = command(&launcher, &launch).await.expect_err("refused");

        assert!(refusal.contains("provider account 'meta'"), "{refusal}");
    }

    #[tokio::test]
    async fn a_managed_launch_is_refused_rather_than_run_unconfined() {
        let launcher = DaemonCodingAgentLauncher::running_as("dev");
        let mut launch = a_launch("claude-acp", "dev");
        launch.managed = true;

        let refusal = command(&launcher, &launch).await.expect_err("refused");

        assert!(refusal.contains("managed codebase"), "{refusal}");
    }

    #[tokio::test]
    async fn a_harness_with_no_command_is_refused() {
        let launcher = DaemonCodingAgentLauncher::running_as("dev");

        let refusal = command(&launcher, &a_launch("claude", "dev"))
            .await
            .expect_err("refused");

        assert!(refusal.contains("'claude' harness"), "{refusal}");
    }
}
