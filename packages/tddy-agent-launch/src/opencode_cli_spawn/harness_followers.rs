//! The followers a harness launch starts for its session (split out of [`super::harness_launch`]):
//! the target hook log's, and the agent's own transcript's — on this host or on the target — with
//! the exit hook that stops them. An agent run in tmux on a target outlives its attached client, so
//! the client's exit stops them only once the agent's tmux session is gone.

use std::path::Path;
use std::sync::Arc;

use tddy_cli_sessions::cli_session_manager::ExitHook;

use super::harness_launch::HarnessRemote;
use crate::spec_launch::HostAgent;

/// Where a session's followers deliver, and where they are kept for its exit, stop and delete.
pub(super) struct HarnessFollowers {
    /// `None` starts no transcript follower.
    pub(super) delivery: Option<Arc<dyn crate::remote_hook_delivery::HookReportDelivery>>,
    pub(super) registry: crate::remote_hook_follower::RemoteHookFollowers,
}

impl HarnessFollowers {
    pub(super) fn new(
        delivery: Option<Arc<dyn crate::remote_hook_delivery::HookReportDelivery>>,
        registry: crate::remote_hook_follower::RemoteHookFollowers,
    ) -> Self {
        Self { delivery, registry }
    }
}

/// Follow the hook log `log` of a target session that is up, delivering its hooks to the
/// session's activity and answering its waiting hooks through the session's shell; the follower is
/// registered so the session's exit stops it and removes the log.
///
/// The follower reads on from the offset persisted in the session's directory: a resume, or the
/// re-attach to an agent that kept running in tmux, misses none of the hooks fired meanwhile.
pub(super) fn follow_the_hook_log(
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
    // A re-attach's follower replaces the one before it, which stops first: both would read on
    // from the same persisted offset.
    remote.hook_followers.abort_hook_follower(session_id);
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
pub(super) fn stopping_followers_on_exit(
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
pub(super) enum TranscriptPlace {
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
pub(super) async fn follow_the_transcript(
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
    followers.registry.abort_transcript_follower(session_id);
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
