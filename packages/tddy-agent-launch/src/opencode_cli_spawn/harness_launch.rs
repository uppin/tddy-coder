//! Launching a registered CLI harness from its spec — as the PTY child, or inside tmux
//! (`#tmux-agent` 3/3) — for a start or a resume of [`super`].

use std::path::Path;
use std::sync::Arc;
use tddy_actions::tool::TemplateParams;
use tddy_cli_sessions::cli_session_manager::{CliSessionManager, ExitHook};
use tddy_coding_agent_specs::slots::RenderedSlots;
use tddy_rpc::Status;

pub(super) use super::harness_followers::HarnessFollowers;
use super::harness_followers::{
    follow_the_hook_log, follow_the_transcript, stopping_followers_on_exit, TranscriptPlace,
};
use crate::remote_agent_launch::{launch_on_target, RemoteLaunch};
use crate::spec_launch::{
    in_a_pane_with_only_the_agent_env, launch_host_pty, plan_host_pty,
    without_the_panes_terminal_variables, HostAgent, HostPtySession,
};
use tddy_core::session_pointer::SessionPointer;

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
    /// The target the agent runs on, when the session named one: the agent is launched there
    /// through the remote-shell port instead of in a PTY (or tmux) on this host.
    pub(super) remote: Option<HarnessRemote>,
    /// Where the session's followed files deliver, and the registry their followers are kept in:
    /// an agent whose spec reads its own transcript (`coding_agent.activity`) has it followed, on
    /// this host or on its target.
    pub(super) followers: HarnessFollowers,
}

/// A harness launch on a session's target (`remote_agent_launch::launch_on_target`).
pub(super) struct HarnessRemote {
    /// The shell that reaches the target, resolved from its target before anything was cut:
    /// an ssh target's transport, or a Coder workspace's with its account's credentials.
    pub(super) shell: Arc<dyn tddy_remote_shell::RemoteShell>,
    /// The session pointer placed in the worktree on the target, whose owner the launch records.
    pub(super) pointer: Option<SessionPointer>,
    /// How long the agent must stay up for the launch to count as started.
    pub(super) startup_window: std::time::Duration,
    /// How long the agent may take to be started on the target.
    pub(super) start_timeout: std::time::Duration,
    /// The session's hook token, when its agent declares hooks: its hooks append to the hook log
    /// on the target, which this launch creates and then follows. `None` starts no follower.
    pub(super) hook_token: Option<String>,
    /// Where followed hooks are delivered, and the registry the follower is kept in for the
    /// session's exit, stop and delete. `None` delivery starts no follower.
    pub(super) hook_delivery: Option<Arc<dyn crate::remote_hook_delivery::HookReportDelivery>>,
    pub(super) hook_followers: crate::remote_hook_follower::RemoteHookFollowers,
    /// Where the agent's tmux session lives when it runs in tmux on the target
    /// (`#target-reconnect` 2/3): the main's client ending is watched through it.
    pub(super) tmux_location: Option<tddy_cli_sessions::tmux_location::SharedTmuxLocation>,
}

/// Spawn the harness from its spec as the session's OS user — whose `HOME` and configured `PATH`
/// extra the child must run under, or its user-local binary and credentials are never found (a
/// systemd daemon's own `PATH` is minimal). In tmux (`run.in_tmux`) the spec's rendered argv runs in
/// a session of its own, re-attached on a resume when alive.
pub(super) async fn launch(
    fs: &dyn tddy_fs::Fs,
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
        remote,
        followers,
    } = launch;
    let failed = |e: crate::spec_launch::SpecLaunchError| {
        Status::internal(format!("failed to spawn {kind}: {e}"))
    };
    if let Some(remote) = remote {
        let shell = Arc::clone(&remote.shell);
        // A hooked agent on a target appends to its hook log there: create it before the agent
        // starts, and follow it once the agent is up.
        let hook_log = match (&remote.hook_token, &remote.hook_delivery) {
            (Some(_), Some(_)) => Some(
                crate::remote_hook_follower::create_remote_hook_log(shell.as_ref(), worktree_path)
                    .await
                    .map_err(|e| Status::internal(format!("failed to spawn {kind}: {e}")))?,
            ),
            _ => None,
        };
        // The session's exit stops its followers and removes its log, after the launch's own
        // exit work: the registry entries are registered below, once the agent is up. In tmux the
        // exit is the attached client's, and the agent may well outlive it — a dropped ssh, a
        // detach, a restarted daemon: while its tmux session lives, the followers keep reading.
        let on_exit = match hook_log.as_ref() {
            Some(_) => Some(stopping_followers_on_exit(
                on_exit,
                remote.hook_followers.clone(),
                session_id,
                run.in_tmux.then(|| Arc::clone(&shell)),
            )),
            None => on_exit,
        };
        let launched = launch_on_target(
            fs,
            agent,
            params,
            slots,
            RemoteLaunch {
                shell: Arc::clone(&shell),
                worktree: worktree_path,
                session_dir,
                pointer: remote.pointer.as_ref(),
                startup_window: remote.startup_window,
                start_timeout: remote.start_timeout,
                clone_root: crate::remote_agent_launch::clone_root_of_worktree(
                    worktree_path,
                    session_id,
                ),
                tmux: run.in_tmux,
                tmux_location: remote.tmux_location.clone(),
            },
            host_pty_session(cli_manager, session_id, os_user, kind, model, env),
            on_exit,
        )
        .await
        .map_err(|e| Status::internal(format!("failed to spawn {kind}: {e}")))?;
        if let Some(log) = hook_log {
            follow_the_hook_log(
                &remote,
                &log,
                (session_id, session_dir),
                os_user,
                worktree_path,
                agent.spec.answer_wait_secs(),
            );
            // The transcript's `tail` lives as long as the session's hook log on the target.
            follow_the_transcript(
                fs,
                agent,
                &followers,
                TranscriptPlace::Target {
                    shell: Arc::clone(&shell),
                    alive_while: log,
                },
                (session_id, session_dir, os_user),
            )
            .await;
        } else if agent.spec.activity().followed_transcript().is_some() {
            log::warn!(
                "session {session_id}: its agent's transcript is not followed on the target: \
                 the target session has no hook log to keep the follower alive"
            );
        }
        return Ok(LaunchedHarness {
            pid: launched.handle.pid,
            tmux_session: run
                .in_tmux
                .then(|| crate::remote_agent_launch::target_tmux_session_name(session_id)),
        });
    }
    if run.in_tmux {
        let plan = plan_host_pty(agent, params, slots).map_err(failed)?;
        let env: Vec<(String, String)> = plan.env.into_iter().chain(env).collect();
        let env = pane_agent_env(agent, env);
        let tmux_launch = crate::cli_harness_tmux::HarnessTmuxLaunch {
            os_user,
            session_id,
            worktree_path: worktree_path.to_path_buf(),
            session_dir: session_dir.to_path_buf(),
            kind,
            argv: in_a_pane_with_only_the_agent_env(plan.argv, &env),
            env,
            on_exit,
        };
        let launched = start_or_resume_in_tmux(cli_manager, run, tmux_launch).await?;
        // An agent in tmux on this host outlives its client: its follower is stopped with the
        // session (stop, delete), not with a terminal.
        follow_the_transcript(
            fs,
            agent,
            &followers,
            TranscriptPlace::Host,
            (session_id, session_dir, os_user),
        )
        .await;
        return Ok(LaunchedHarness {
            pid: launched.pane_pid,
            tmux_session: Some(launched.tmux_session),
        });
    }
    // A transcript follower on this host ends with the agent's PTY. It starts first, so an agent
    // that exits at once finds it registered and its exit hook stops it; the transcript need not
    // exist yet (`tail -F`).
    let follows_transcript =
        followers.delivery.is_some() && agent.spec.activity().followed_transcript().is_some();
    let on_exit = if follows_transcript {
        follow_the_transcript(
            fs,
            agent,
            &followers,
            TranscriptPlace::Host,
            (session_id, session_dir, os_user),
        )
        .await;
        Some(stopping_followers_on_exit(
            on_exit,
            followers.registry.clone(),
            session_id,
            None,
        ))
    } else {
        on_exit
    };
    let handle = match launch_host_pty(
        agent,
        params,
        slots,
        worktree_path,
        host_pty_session(cli_manager, session_id, os_user, kind, model, env),
        on_exit,
    )
    .await
    {
        Ok(handle) => handle,
        Err(error) => {
            // No agent, so no exit to stop the follower with.
            followers.registry.stop_and_remove_log(session_id).await;
            return Err(failed(error));
        }
    };
    Ok(LaunchedHarness {
        pid: handle.pid,
        tmux_session: None,
    })
}

fn host_pty_session<'a>(
    cli_manager: &'a Arc<CliSessionManager>,
    session_id: &'a str,
    os_user: &'a str,
    kind: &'a str,
    model: &'a str,
    env: Vec<(String, String)>,
) -> HostPtySession<'a> {
    HostPtySession {
        manager: cli_manager,
        session_id,
        session_type: kind,
        model,
        env,
        os_user: Some(os_user),
    }
}

fn pane_agent_env(agent: &HostAgent, env: Vec<(String, String)>) -> Vec<(String, String)> {
    without_the_panes_terminal_variables(agent.env_base.agent_env(
        &agent.spec,
        std::env::vars_os(),
        &env,
    ))
}

async fn start_or_resume_in_tmux(
    cli_manager: &Arc<CliSessionManager>,
    run: HarnessRun,
    tmux_launch: crate::cli_harness_tmux::HarnessTmuxLaunch<'_>,
) -> Result<crate::claude_cli_tmux::TmuxLaunched, Status> {
    let launched = if run.resume {
        crate::cli_harness_tmux::resume_harness_in_tmux(cli_manager, tmux_launch).await?
    } else {
        crate::cli_harness_tmux::start_harness_in_tmux(cli_manager, tmux_launch).await?
    };
    Ok(launched)
}
