//! Launching a registered CLI harness from its spec — as the PTY child, or inside tmux
//! (`#tmux-agent` 3/3) — for a start or a resume of [`super`].

use std::path::Path;
use std::sync::Arc;
use tddy_actions::tool::TemplateParams;
use tddy_cli_sessions::cli_session_manager::{CliSessionManager, ExitHook};
use tddy_coding_agent_specs::slots::RenderedSlots;
use tddy_rpc::Status;

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

/// Where a session's followers deliver, and where they are kept for its exit, stop and delete.
pub(super) struct HarnessFollowers {
    /// `None` starts no transcript follower.
    pub(super) delivery: Option<Arc<dyn crate::remote_hook_delivery::HookReportDelivery>>,
    pub(super) registry: crate::remote_hook_follower::RemoteHookFollowers,
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
    // A transcript follower on this host ends with the agent's PTY.
    let follows_transcript =
        followers.delivery.is_some() && agent.spec.activity().followed_transcript().is_some();
    let on_exit = if follows_transcript {
        Some(stopping_followers_on_exit(
            on_exit,
            followers.registry.clone(),
            session_id,
            None,
        ))
    } else {
        on_exit
    };
    let handle = launch_host_pty(
        agent,
        params,
        slots,
        worktree_path,
        host_pty_session(cli_manager, session_id, os_user, kind, model, env),
        on_exit,
    )
    .await
    .map_err(failed)?;
    follow_the_transcript(
        agent,
        &followers,
        TranscriptPlace::Host,
        (session_id, session_dir, os_user),
    )
    .await;
    Ok(LaunchedHarness {
        pid: handle.pid,
        tmux_session: None,
    })
}

/// Follow the hook log `log` of a target session that is up, delivering its hooks to the
/// session's activity and answering its waiting hooks through the session's shell; the follower is
/// registered so the session's exit stops it and removes the log.
///
/// The follower reads on from the offset persisted in the session's directory: a resume, or the
/// re-attach to an agent that kept running in tmux, misses none of the hooks fired meanwhile.
fn follow_the_hook_log(
    remote: &HarnessRemote,
    log: &str,
    (session_id, session_dir): (&str, &Path),
    os_user: &str,
    worktree_path: &Path,
    answer_wait_secs: u64,
) {
    let (Some(token), Some(delivery)) = (remote.hook_token.as_ref(), remote.hook_delivery.as_ref())
    else {
        return;
    };
    let shell = Arc::clone(&remote.shell);
    let sink = crate::remote_hook_delivery::ActivityHookSink::new(
        Arc::clone(delivery),
        session_id,
        token,
        os_user,
    )
    .answering_through(Arc::clone(&shell), worktree_path, answer_wait_secs);
    let follower = crate::remote_hook_follower::RemoteHookFollower::start_resuming(
        Arc::clone(&shell),
        log,
        token,
        Arc::new(sink),
        crate::transcript_follower::hook_log_offset(session_dir),
    );
    remote
        .hook_followers
        .insert(session_id, follower, shell, worktree_path.to_path_buf());
}

/// `on_exit`, then the session's followers stopped and its hook log removed — unless `tmux_on` is
/// the target shell of an agent run in tmux whose tmux session still lives: then the exit was its
/// client's (a dropped connection, a detach, this daemon going away), the agent keeps running, and
/// its followers keep reading. The session's stop and delete end them.
fn stopping_followers_on_exit(
    on_exit: Option<ExitHook>,
    followers: crate::remote_hook_follower::RemoteHookFollowers,
    session_id: &str,
    tmux_on: Option<Arc<dyn tddy_remote_shell::RemoteShell>>,
) -> ExitHook {
    let exited_session = session_id.to_string();
    Box::new(move |capture| {
        Box::pin(async move {
            if let Some(original) = on_exit {
                original(capture).await;
            }
            if let Some(shell) = tmux_on {
                let name = crate::remote_agent_launch::target_tmux_session_name(&exited_session);
                match crate::remote_agent_launch::target_tmux_session_lives(shell.as_ref(), &name)
                    .await
                {
                    Ok(true) => {
                        log::info!(
                            "session {exited_session}: its client exited and its agent still runs in \
                             tmux session {name:?} on the target: its followers keep reading"
                        );
                        return;
                    }
                    Ok(false) => {}
                    Err(error) => log::warn!(
                        "session {exited_session}: whether tmux session {name:?} lives could not be \
                         asked ({error}); stopping its followers"
                    ),
                }
            }
            followers.stop_and_remove_log(&exited_session).await;
        }) as std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
    })
}

/// Where a session's agent writes its transcript.
enum TranscriptPlace {
    /// On this host, as the session's OS user: followed through this host's shell, its home the
    /// one the session's usage context recorded.
    Host,
    /// On the session's target: followed through its shell, its home the target's `$HOME`, its
    /// `tail` alive while `alive_while` (the session's hook log there) exists.
    Target {
        shell: Arc<dyn tddy_remote_shell::RemoteShell>,
        alive_while: String,
    },
}

/// Follow the agent's own transcript, when its spec reads activity from one and the session has a
/// delivery: from the offset persisted in the session's directory, registered so the session's
/// stop and delete end it. A transcript that cannot be placed is logged and not followed — its hooks
/// still report the session's tool calls.
async fn follow_the_transcript(
    agent: &HostAgent,
    followers: &HarnessFollowers,
    place: TranscriptPlace,
    (session_id, session_dir, os_user): (&str, &Path, &str),
) {
    let (Some(transcript), Some(delivery)) = (
        agent.spec.activity().followed_transcript(),
        followers.delivery.as_ref(),
    ) else {
        return;
    };
    let (shell, home, alive_while, lease): (Arc<dyn tddy_remote_shell::RemoteShell>, _, _, _) =
        match place {
            TranscriptPlace::Host => {
                let lease = crate::transcript_follower::host_follow_lease(session_dir);
                let leased = lease
                    .parent()
                    .map_or(Ok(()), std::fs::create_dir_all)
                    .and_then(|()| std::fs::write(&lease, b""));
                if let Err(error) = leased {
                    log::warn!(
                        "session {session_id}: its agent's transcript is not followed: its lease \
                         {} could not be written: {error}",
                        lease.display()
                    );
                    return;
                }
                let alive = lease.to_string_lossy().into_owned();
                (
                    Arc::new(tddy_remote_shell::LocalShell::new()),
                    None,
                    alive,
                    Some(lease),
                )
            }
            TranscriptPlace::Target { shell, alive_while } => {
                match crate::transcript_follower::home_on(shell.as_ref()).await {
                    Ok(home) => (shell, Some(home), alive_while, None),
                    Err(error) => {
                        log::warn!(
                            "session {session_id}: its agent's transcript is not followed: {error}"
                        );
                        return;
                    }
                }
            }
        };
    let path = match crate::transcript_follower::transcript_path(transcript, session_dir, home) {
        Ok(path) => path,
        Err(error) => {
            log::warn!("session {session_id}: its agent's transcript is not followed: {error}");
            return;
        }
    };
    let follower = crate::transcript_follower::follow_transcript(
        shell,
        (&path, &alive_while),
        transcript.clone(),
        crate::transcript_follower::TranscriptOwner {
            session_id: session_id.to_string(),
            os_user: os_user.to_string(),
        },
        Arc::clone(delivery),
        crate::transcript_follower::transcript_offset(session_dir),
    );
    followers
        .registry
        .insert_transcript(session_id, follower, lease);
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use pretty_assertions::assert_eq;
    use tddy_remote_shell::{ExecOutput, InMemoryShell, ShellCall};

    use super::stopping_followers_on_exit;
    use crate::remote_hook_follower::{
        HookLine, HookSink, RemoteHookFollower, RemoteHookFollowers,
    };

    const SESSION_ID: &str = "01a20f05-97d8-7221-85bc-d879ececfb62";
    const WORKTREE: &str = "/home/coder/repo/.worktrees/01a20f05";

    struct NoHooks;

    #[async_trait::async_trait]
    impl HookSink for NoHooks {
        async fn deliver(&self, _line: HookLine) {}
    }

    /// A target session in tmux whose hook log is followed, `has-session` answering `status`.
    fn a_followed_tmux_session(has_session_status: i32) -> (InMemoryShell, RemoteHookFollowers) {
        let shell = InMemoryShell::new("coder:dev/remote-dev");
        shell.answers_when(move |request| {
            request
                .command
                .contains("tmux has-session")
                .then(|| ExecOutput::exited(has_session_status, ""))
        });
        shell.answers_when(|request| {
            request
                .command
                .starts_with("rm -f")
                .then(|| ExecOutput::succeeded(""))
        });
        let followers = RemoteHookFollowers::new();
        let log = format!("{WORKTREE}/.tddy/hooks.log");
        followers.insert(
            SESSION_ID,
            RemoteHookFollower::start(Arc::new(shell.clone()), &log, "tok", Arc::new(NoHooks)),
            Arc::new(shell.clone()),
            WORKTREE.into(),
        );
        (shell, followers)
    }

    fn removed_the_log(shell: &InMemoryShell) -> bool {
        shell.calls().into_iter().any(
            |call| matches!(call, ShellCall::Exec(request) if request.command.starts_with("rm -f")),
        )
    }

    async fn the_client_exits(
        shell: &InMemoryShell,
        followers: &RemoteHookFollowers,
        in_tmux: bool,
    ) {
        let tmux_on: Option<Arc<dyn tddy_remote_shell::RemoteShell>> =
            in_tmux.then(|| Arc::new(shell.clone()) as Arc<dyn tddy_remote_shell::RemoteShell>);
        let on_exit = stopping_followers_on_exit(None, followers.clone(), SESSION_ID, tmux_on);
        on_exit(Arc::new(std::sync::Mutex::new(
            tddy_task::TerminalCapture::new(),
        )))
        .await;
    }

    #[tokio::test]
    async fn a_tmux_clients_exit_leaves_the_followers_of_an_agent_still_running_in_tmux() {
        // Given a target session in tmux whose tmux session lives
        let (shell, followers) = a_followed_tmux_session(0);

        // When its attached client exits — a dropped connection, a detach
        the_client_exits(&shell, &followers, true).await;

        // Then its hook log is still followed and still there
        assert_eq!(
            (followers.follows(SESSION_ID), removed_the_log(&shell)),
            (true, false)
        );
    }

    #[tokio::test]
    async fn a_tmux_clients_exit_after_its_agent_ended_stops_the_followers_and_removes_the_log() {
        // Given a target session in tmux whose tmux session is gone
        let (shell, followers) = a_followed_tmux_session(1);

        // When its client exits
        the_client_exits(&shell, &followers, true).await;

        // Then
        assert_eq!(
            (followers.follows(SESSION_ID), removed_the_log(&shell)),
            (false, true)
        );
    }

    #[tokio::test]
    async fn the_exit_of_an_agent_run_without_tmux_stops_the_followers() {
        // Given a target session not in tmux
        let (shell, followers) = a_followed_tmux_session(0);

        // When its agent exits
        the_client_exits(&shell, &followers, false).await;

        // Then
        assert_eq!(
            (followers.follows(SESSION_ID), removed_the_log(&shell)),
            (false, true)
        );
    }
}
