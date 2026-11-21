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

/// The local process that is a target tab's terminal ([`CliSessionManager::spawn_rendered_client_tab`]):
/// a transport's client, already rendered by the launcher that reaches the target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientTab {
    /// The tab's kind as a listing reports it: `"bash"` for a shell on the target.
    pub kind: String,
    /// The client's argv (`ssh -tt … <command>`, `coder ssh … -- <command>`).
    pub argv: Vec<String>,
    /// The client's whole environment: its child starts from nothing else.
    pub env: Vec<(String, String)>,
    /// How the status of what ran on the target is read from the client's last output, for a
    /// client that does not exit with it (Coder's).
    pub exit_code_in_output: Option<tddy_pty::ExitCodeInOutput>,
}

use std::sync::Arc;

use std::path::PathBuf;

impl CliSessionManager {
    /// Spawn `argv` — already rendered, by a launcher that owns the agent's argument template — as
    /// `session_id`'s main terminal, registered under `session_type`. `os_user` is the session
    /// owner, or `None` to run as the daemon. `on_exit` runs once the process has exited, before its
    /// terminal is deregistered (see [`HarnessLaunchExtras`]).
    ///
    /// An agent main starts from exactly `env` (`#agent-tools 28/28`): its whole environment, which
    /// the caller computed from the allow-list (`AgentEnvBase::agent_env`: the base set, the
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
        self.spawn_rendered_client_main(
            session_id,
            session_type,
            worktree_path,
            model,
            argv,
            env,
            os_user,
            on_exit,
            None,
        )
        .await
    }

    /// [`Self::spawn_rendered_main`] for a main whose process is a transport's client to the agent
    /// on a target, not the agent: `exit_code_in_output`, when the client does not exit with the
    /// agent's status, says how that status is read from what the client printed last, so the
    /// terminal's status is the agent's own.
    #[allow(clippy::too_many_arguments)]
    pub async fn spawn_rendered_client_main(
        &self,
        session_id: &str,
        session_type: &str,
        worktree_path: PathBuf,
        model: &str,
        argv: Vec<String>,
        env: Vec<(String, String)>,
        os_user: Option<&str>,
        on_exit: Option<ExitHook>,
        exit_code_in_output: Option<tddy_pty::ExitCodeInOutput>,
    ) -> anyhow::Result<Arc<pty_handle::PtyHandle>> {
        self.spawn_tool_on(
            session_id,
            MAIN_TERMINAL_ID,
            session_type,
            worktree_path,
            model,
            argv,
            env,
            tddy_pty::PtyEnvBase::Cleared,
            exit_code_in_output,
            os_user,
            on_exit,
            None,
        )
        .await
    }

    /// [`Self::spawn_rendered_client_main`] for a **tab** whose process is a transport's client to
    /// what runs on a target (`#session-liveness` 6/8): registered under `terminal_id` — never
    /// [`MAIN_TERMINAL_ID`] — so nothing of the session's main (its recording, its exit hook, its
    /// status) is touched, and its exit ends the tab alone. Refused for the main's id and for an id
    /// already live in the session.
    pub async fn spawn_rendered_client_tab(
        &self,
        session_id: &str,
        terminal_id: &str,
        worktree_path: PathBuf,
        client: ClientTab,
    ) -> anyhow::Result<Arc<pty_handle::PtyHandle>> {
        if terminal_id == MAIN_TERMINAL_ID {
            anyhow::bail!("the {MAIN_TERMINAL_ID} terminal is the session's main, not a tab");
        }
        if self.get_terminal(session_id, terminal_id).await.is_some() {
            anyhow::bail!("terminal {terminal_id} of session {session_id} is already running");
        }
        self.spawn_tool_on(
            session_id,
            terminal_id,
            &client.kind,
            worktree_path,
            "",
            client.argv,
            client.env,
            tddy_pty::PtyEnvBase::Cleared,
            client.exit_code_in_output,
            None,
            None,
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
