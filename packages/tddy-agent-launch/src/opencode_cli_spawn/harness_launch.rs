//! Launching a registered CLI harness — as the PTY child, or inside tmux (`#tmux-agent` 3/3) — for
//! a start or a resume of [`super`].

use crate::cli_route_launch::CliRouteLaunch;
use std::path::Path;
use std::sync::Arc;
use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_cli_sessions::cli_session_manager::{ExitHook, HarnessLaunchExtras};
use tddy_core::cli_harness::{CliHarness, CliLaunch};
use tddy_rpc::Status;

/// What a launch adds to the harness's argv: the route's arguments, `env` with the route's
/// environment on top, and `on_exit` run once the process has exited.
pub(super) fn launch_extras(
    route: CliRouteLaunch,
    mut env: Vec<(String, String)>,
    on_exit: ExitHook,
) -> HarnessLaunchExtras {
    env.extend(route.env);
    HarnessLaunchExtras {
        args: route.args,
        env,
        on_exit: Some(on_exit),
    }
}

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
    pub(super) session_id: &'a str,
    pub(super) worktree_path: &'a Path,
    /// The session's directory, which holds a tmux pane's environment file.
    pub(super) session_dir: &'a Path,
    /// The session owner, whose `HOME` and configured `PATH` extra the child runs under.
    pub(super) os_user: &'a str,
}

/// Spawn `harness` with `launch`, followed by `extras`, as the session's OS user — whose `HOME` and
/// configured `PATH` extra the child must run under, or its user-local binary and credentials are
/// never found (a systemd daemon's own `PATH` is minimal). In tmux (`run.in_tmux`) the harness runs
/// in a session of its own, re-attached on a resume when alive.
pub(super) async fn launch(
    cli_manager: &Arc<CliSessionManager>,
    harness: &CliHarness,
    session: HarnessSession<'_>,
    launch: &CliLaunch<'_>,
    extras: HarnessLaunchExtras,
    run: HarnessRun,
) -> Result<LaunchedHarness, Status> {
    let HarnessSession {
        session_id,
        worktree_path,
        session_dir,
        os_user,
    } = session;
    if run.in_tmux {
        let mut argv = (harness.argv)(launch);
        argv.extend(extras.args);
        let tmux_launch = crate::cli_harness_tmux::HarnessTmuxLaunch {
            os_user,
            session_id,
            worktree_path: worktree_path.to_path_buf(),
            session_dir: session_dir.to_path_buf(),
            kind: harness.session_type,
            argv,
            env: extras.env,
            on_exit: extras.on_exit,
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
    let handle = cli_manager
        .launch_cli_harness(
            harness,
            session_id,
            worktree_path.to_path_buf(),
            launch,
            extras,
            Some(os_user),
        )
        .await
        .map_err(|e| Status::internal(format!("failed to spawn {}: {e}", harness.session_type)))?;
    Ok(LaunchedHarness {
        pid: handle.pid,
        tmux_session: None,
    })
}
