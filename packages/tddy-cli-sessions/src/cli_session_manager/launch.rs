use crate::cli_session_manager::pty_handle;

use super::CliSessionManager;

use std::path::Path;

use super::pty_spawn::ExitHook;

/// What a rendered launch adds to the argv its launcher rendered.
#[derive(Default)]
pub struct HarnessLaunchExtras {
    /// Follow the harness's argv (a provider route's `--model`).
    pub args: Vec<String>,
    /// Applied to the child on top of the daemon's (a route's configuration variable and key).
    pub env: Vec<(String, String)>,
    /// Runs to completion once the process has exited, before its terminal is deregistered.
    pub on_exit: Option<ExitHook>,
}

use super::MAIN_TERMINAL_ID;

use std::sync::Arc;

use std::path::PathBuf;

impl CliSessionManager {
    /// Spawn `argv` — already rendered, by a launcher that owns the agent's argument template — as
    /// `session_id`'s main terminal, registered under `session_type`. `env` is applied on top of the
    /// daemon's; `os_user` is the session owner, or `None` to run as the daemon. `on_exit` runs once
    /// the process has exited, before its terminal is deregistered (see [`HarnessLaunchExtras`]).
    ///
    /// An agent main starts from exactly `env` (`#agent-tools 28/28`): its whole environment, which
    /// the caller computed from the allow-list (`CodingAgentSpec::agent_env`: the base set, the
    /// spec's `pass_env` values, the launch's own variables). Nothing else the daemon holds reaches
    /// it. A user's terminal ([`Self::start_terminal`]) still inherits the daemon's environment.
    #[allow(clippy::too_many_arguments)]
    pub async fn spawn_rendered_main(
        &self,
        session_id: &str,
        session_type: &str,
        worktree_path: PathBuf,
        model: &str,
        argv: Vec<String>,
        env: Vec<(String, String)>,
        os_user: Option<&str>,
        on_exit: Option<ExitHook>,
    ) -> anyhow::Result<Arc<pty_handle::PtyHandle>> {
        self.spawn_tool_on(
            session_id,
            MAIN_TERMINAL_ID,
            session_type,
            worktree_path,
            model,
            argv,
            (env, tddy_pty::PtyEnvBase::Cleared),
            os_user,
            on_exit,
            None,
        )
        .await
    }

    /// Spawn a new CLI process for `session_id` in `worktree_path` from an argv the caller
    /// rendered from the agent's spec, with managed-workflow launch options. Returns an
    /// `Arc<PtyHandle>`; the child is monitored in a background std thread and the session
    /// leaves the registry when it exits.
    ///
    /// `argv` — the agent's launch argv, already rendered (binary first, the model, the
    /// session id, the prompt as its trailing positional when it carries one).
    ///
    /// `initial_prompt` — the prompt bound in `argv`, named only so the launch flags below
    /// can be spliced before it. **Resume** always passes `None`: re-injecting the original
    /// prompt would create a duplicate user turn.
    ///
    /// `session_type` — registered as the session's tool label (`<id>-cli`).
    ///
    /// `append_system_prompt_file` — when `Some`, inserted as
    /// `--append-system-prompt-file <path>` (before any positional prompt) so the agent
    /// appends that file to its system prompt (a managed workflow's orchestration prompt).
    ///
    /// `env` — extra environment variables set on the spawned process (e.g. a managed session's
    /// per-session `TDDY_SOCKET` and a `PATH` that resolves `tddy-tools`).
    ///
    /// `extra_args` — flags appended before any positional prompt (a split session's tool allowlist
    /// and `--mcp-config`). They go before the prompt because `--mcp-config` is variadic and would
    /// otherwise swallow a bare positional as another config path.
    #[allow(clippy::too_many_arguments)]
    pub async fn start_with_options(
        &self,
        session_id: &str,
        worktree_path: PathBuf,
        model: &str,
        argv: Vec<String>,
        session_type: &str,
        initial_prompt: Option<&str>,
        append_system_prompt_file: Option<&Path>,
        extra_args: Vec<String>,
        env: Vec<(String, String)>,
        os_user: Option<&str>,
    ) -> anyhow::Result<Arc<pty_handle::PtyHandle>> {
        let argv =
            Self::with_launch_flags(argv, initial_prompt, append_system_prompt_file, &extra_args);
        self.spawn_tool(
            session_id,
            MAIN_TERMINAL_ID,
            session_type,
            worktree_path,
            model,
            argv,
            env,
            os_user,
            None,
            None,
        )
        .await
    }
}
