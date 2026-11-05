//! Launching a registered CLI harness from its spec — as the PTY child, or inside tmux
//! (`#tmux-agent` 3/3) — for a start or a resume of [`super`].

use std::path::Path;
use std::sync::Arc;
use tddy_actions::tool::TemplateParams;
use tddy_cli_sessions::cli_session_manager::{CliSessionManager, ExitHook};
use tddy_coding_agent_specs::slots::RenderedSlots;
use tddy_rpc::Status;

use crate::spec_launch::{launch_host_pty, plan_host_pty, HostAgent, HostPtySession};

/// How a harness is run: as the PTY child, or inside tmux — and whether the launch starts the
/// session's agent or resumes it.
#[derive(Clone, Copy)]
pub(super) struct HarnessRun {
    in_tmux: bool,
    resume: bool,
}

impl HarnessRun {
    pub(super) fn start(in_tmux: bool) -> Self {
        Self {
            in_tmux,
            resume: false,
        }
    }

    pub(super) fn resume(in_tmux: bool) -> Self {
        Self {
            in_tmux,
            resume: true,
        }
    }
}

/// A launched harness: the pid the session records, and the tmux session it runs in (if any).
pub(super) struct LaunchedHarness {
    pub(super) pid: u32,
    pub(super) tmux_session: Option<String>,
}

/// The session a harness is launched for.
pub(super) struct HarnessSession<'a> {
    session_id: &'a str,
    worktree_path: &'a Path,
    /// The session's directory, which holds a tmux pane's environment file.
    session_dir: &'a Path,
    /// The session owner, whose `HOME` and configured `PATH` extra the child runs under.
    os_user: &'a str,
}

impl<'a> HarnessSession<'a> {
    pub(super) fn new(
        session_id: &'a str,
        worktree_path: &'a Path,
        session_dir: &'a Path,
        os_user: &'a str,
    ) -> Self {
        HarnessSession {
            session_id,
            worktree_path,
            session_dir,
            os_user,
        }
    }
}

/// What the harness's spec is launched with: its argv's parameters and rendered slots (the route's
/// arguments already in `slots.argv_extra`), the session type it is registered under, the model,
/// the environment on top of the spec's `run.env`, and `on_exit` run once the agent has exited.
pub(super) struct HarnessSpecLaunch<'a> {
    pub(super) agent: &'a HostAgent,
    pub(super) params: &'a TemplateParams,
    pub(super) slots: &'a RenderedSlots,
    pub(super) kind: &'a str,
    pub(super) model: &'a str,
    pub(super) env: Vec<(String, String)>,
    pub(super) on_exit: Option<ExitHook>,
}

/// Spawn the harness from its spec as the session's OS user — whose `HOME` and configured `PATH`
/// extra the child must run under, or its user-local binary and credentials are never found (a
/// systemd daemon's own `PATH` is minimal). In tmux (`run.in_tmux`) the spec's rendered argv runs in
/// a session of its own, re-attached on a resume when alive.
pub(super) async fn launch(
    cli_manager: &Arc<CliSessionManager>,
    session: HarnessSession<'_>,
    launch: HarnessSpecLaunch<'_>,
    run: HarnessRun,
) -> Result<LaunchedHarness, Status> {
    let HarnessSession {
        session_id,
        worktree_path,
        session_dir,
        os_user,
    } = session;
    let HarnessSpecLaunch {
        agent,
        params,
        slots,
        kind,
        model,
        env,
        on_exit,
    } = launch;
    let failed = |e: crate::spec_launch::SpecLaunchError| {
        Status::internal(format!("failed to spawn {kind}: {e}"))
    };
    if run.in_tmux {
        let plan = plan_host_pty(agent, params, slots).map_err(failed)?;
        let tmux_launch = crate::cli_harness_tmux::HarnessTmuxLaunch {
            os_user,
            session_id,
            worktree_path: worktree_path.to_path_buf(),
            session_dir: session_dir.to_path_buf(),
            kind,
            argv: plan.argv,
            env: plan.env.into_iter().chain(env).collect(),
            on_exit,
        };
        let launched = if run.resume {
            crate::cli_harness_tmux::resume_harness_in_tmux(cli_manager, tmux_launch).await?
        } else {
            crate::cli_harness_tmux::start_harness_in_tmux(cli_manager, tmux_launch).await?
        };
        return Ok(LaunchedHarness {
            pid: launched.pane_pid,
            tmux_session: Some(launched.tmux_session),
        });
    }
    let handle = launch_host_pty(
        agent,
        params,
        slots,
        worktree_path,
        HostPtySession {
            manager: cli_manager,
            session_id,
            session_type: kind,
            model,
            env,
            os_user: Some(os_user),
            without_inherited_tddy: true,
        },
        on_exit,
    )
    .await
    .map_err(failed)?;
    Ok(LaunchedHarness {
        pid: handle.pid,
        tmux_session: None,
    })
}
