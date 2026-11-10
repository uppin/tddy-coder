use std::sync::Arc;

/// The split agent's PTY: where it runs, what it runs and as whom (`#agent-tools 28/28`).
pub(crate) struct SplitAgentPty<'a> {
    pub(crate) session_id: &'a str,
    pub(crate) session_type: &'a str,
    pub(crate) context_dir: std::path::PathBuf,
    pub(crate) model: &'a str,
    pub(crate) argv: Vec<String>,
    pub(crate) os_user: Option<&'a str>,
    pub(crate) on_exit: Option<tddy_cli_sessions::cli_session_manager::ExitHook>,
}

/// Spawn the split agent `spec` names as its session's main terminal, from the allow-listed
/// environment `base` computes for it (`AgentEnvBase::agent_env`: the base set's variables of the
/// daemon and those `spec`'s `pass_env` names). The agent process is given no `TDDY_*` variable.
pub(crate) async fn spawn_split_agent_pty(
    cli_manager: &tddy_cli_sessions::cli_session_manager::CliSessionManager,
    spec: &tddy_coding_agent_specs::CodingAgentSpec,
    base: &tddy_coding_agent_specs::AgentEnvBase,
    pty: SplitAgentPty<'_>,
) -> anyhow::Result<Arc<tddy_cli_sessions::cli_session_manager::pty_handle::PtyHandle>> {
    cli_manager
        .spawn_rendered_main(
            pty.session_id,
            pty.session_type,
            pty.context_dir,
            pty.model,
            pty.argv,
            base.agent_env(spec, std::env::vars_os(), &[]),
            pty.os_user,
            pty.on_exit,
        )
        .await
}
