use crate::cli_session_manager::pty_handle;

use super::CliSessionManager;

use std::path::Path;

use tddy_core::cli_harness::{CliHarness, CliLaunch};

use super::pty_spawn::ExitHook;

/// What a [`CliSessionManager::launch_cli_harness`] launch adds to the harness's own argv.
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
    /// Spawn `harness` for `session_id` in `worktree_path`, its argv built by the registry from
    /// `launch` (`launch.resume` continues the worktree's session instead of starting one).
    ///
    /// `extras` adds to that launch: arguments after the harness's own argv, environment on top of
    /// the daemon's, and the work to finish once the process exits (see [`HarnessLaunchExtras`]).
    ///
    /// `os_user` — the session owner, whose `HOME` and configured `PATH` extra the child must run
    /// under (the same overrides a claude-cli launch applies); `None` runs the child as the daemon.
    //
    // TODO(harness-clis): no daemon launch calls this any more — the OpenCode, Muse and Warp starts
    // launch from their specs through `spec_launch::launch_host_pty`. Pinned by `tests/exit_hook.rs`;
    // removal is deferred: docs/dev/todo/2026-11-03-harness-clis-legacy-cli-harness-removal.md
    pub async fn launch_cli_harness(
        &self,
        harness: &CliHarness,
        session_id: &str,
        worktree_path: PathBuf,
        launch: &CliLaunch<'_>,
        extras: HarnessLaunchExtras,
        os_user: Option<&str>,
    ) -> anyhow::Result<Arc<pty_handle::PtyHandle>> {
        let mut argv = (harness.argv)(launch);
        argv.extend(extras.args);
        self.spawn_tool(
            session_id,
            MAIN_TERMINAL_ID,
            harness.session_type,
            worktree_path,
            launch.model,
            argv,
            extras.env,
            os_user,
            extras.on_exit,
            None,
        )
        .await
    }

    /// Spawn `argv` — already rendered, by a launcher that owns the agent's argument template — as
    /// `session_id`'s main terminal, registered under `session_type`. `env` is applied on top of the
    /// daemon's; `os_user` is the session owner, or `None` to run as the daemon. `on_exit` runs once
    /// the process has exited, before its terminal is deregistered (see [`HarnessLaunchExtras`]).
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
        self.spawn_tool(
            session_id,
            MAIN_TERMINAL_ID,
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

    /// Spawn a new claude CLI process for `session_id` in `worktree_path`, with managed-workflow
    /// launch options. Returns an `Arc<PtyHandle>`; the child is monitored in a background std
    /// thread and the session leaves the registry when it exits.
    ///
    /// `initial_prompt` — when `Some` and non-empty, appended as a positional CLI argument so that
    /// `claude` receives it as the first user turn. **Resume** always passes `None`: re-injecting
    /// the original prompt would create a duplicate user turn.
    ///
    /// `permission_mode` — forwarded as `--permission-mode <mode>`. `None` or empty/whitespace
    /// defaults to `"auto"`.
    ///
    /// `resume` — when true, continue the existing on-disk transcript via `--resume <id>` instead of
    /// assigning the id to a fresh session via `--session-id <id>`. Only [`Self::resume_with_options`]
    /// passes true; fresh starts pass false.
    ///
    /// `append_system_prompt_file` — when `Some`, inserted as `--append-system-prompt-file <path>`
    /// (before any positional prompt) so `claude` appends that file to its system prompt (a managed
    /// workflow's orchestration prompt).
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
        binary_path: &str,
        initial_prompt: Option<&str>,
        permission_mode: Option<&str>,
        dangerously_skip_permissions: bool,
        resume: bool,
        append_system_prompt_file: Option<&Path>,
        extra_args: Vec<String>,
        env: Vec<(String, String)>,
        os_user: Option<&str>,
    ) -> anyhow::Result<Arc<pty_handle::PtyHandle>> {
        let argv = Self::with_claude_launch_flags(
            Self::build_claude_argv(
                binary_path,
                model,
                session_id,
                initial_prompt,
                permission_mode,
                dangerously_skip_permissions,
                resume,
            ),
            initial_prompt,
            append_system_prompt_file,
            extra_args,
        );
        self.spawn_tool(
            session_id,
            MAIN_TERMINAL_ID,
            "claude-cli",
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
