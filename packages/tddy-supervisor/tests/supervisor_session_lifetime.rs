//! How a spawned session is addressable once it exists.
//!
//! The daemon signals sessions it no longer owns. `terminate_sandbox_process`
//! (`packages/tddy-daemon/src/sandbox_session.rs`) sends `kill(-pid, SIGTERM)` then
//! `kill(-pid, SIGKILL)` — a *process group* signal — and `CliSessionManager::kill_all` reaches for
//! pids the same way. Both assume the session is the leader of its own group, which was true while
//! the daemon forked it directly.
//!
//! A supervisor-spawned session inherits the supervisor's process group unless something puts it in
//! its own. Then `kill(-pid)` either fails with `ESRCH` or, if a pid ever collides with the
//! supervisor's group id, signals the supervisor and every service it manages. That is worth a test
//! of its own rather than a comment.

mod support;

use support::{
    a_service, a_supervisor, a_tool_that_exits_with, a_tool_that_records_its_parent,
    current_username, process_group_of, process_is_alive, DenialAssertions,
};
use tddy_supervisor::{SessionState, SpawnSessionRequest};

fn a_session_spawn(os_user: &str, tool_path: &std::path::Path) -> SpawnSessionRequest {
    SpawnSessionRequest {
        os_user: os_user.to_string(),
        tool_path: tool_path.to_path_buf(),
        args: Vec::new(),
        env: Default::default(),
        working_dir: None,
        scope: None,
    }
}

#[tokio::test]
async fn makes_each_spawned_session_the_leader_of_its_own_process_group() {
    // Given
    let tool = a_tool_that_records_its_parent();
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").that_stays_alive())
        .allowing_the_current_user()
        .allowing_tool(tool.path())
        .start()
        .await;

    // When
    let spawned = supervisor
        .client()
        .await
        .spawn_session(a_session_spawn(&current_username(), tool.path()))
        .await
        .expect("spawn a session");

    // Then a group signal aimed at the session reaches the session and nothing else.
    assert_eq!(
        tool.await_recorded_process_group().await,
        spawned.pid,
        "the session must lead its own process group"
    );
}

#[tokio::test]
async fn keeps_a_spawned_session_out_of_the_supervisors_process_group() {
    // Given
    let tool = a_tool_that_records_its_parent();
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").that_stays_alive())
        .allowing_the_current_user()
        .allowing_tool(tool.path())
        .start()
        .await;

    // When
    supervisor
        .client()
        .await
        .spawn_session(a_session_spawn(&current_username(), tool.path()))
        .await
        .expect("spawn a session");

    // Then — compared against the supervisor's actual process group, not its pid: the supervisor
    // inherits a group from whoever started it, so its pid and its pgid are different numbers, and
    // asserting against the pid would pass without proving anything.
    let session_group = tool.await_recorded_process_group().await;
    assert_ne!(
        session_group,
        process_group_of(supervisor.pid()),
        "a session must not share the supervisor's process group: a `kill(-pid)` aimed at the \
         session would otherwise take down the supervisor and every service under it"
    );
}

#[tokio::test]
async fn reports_the_pid_that_the_session_itself_observes() {
    // Given
    let tool = a_tool_that_records_its_parent();
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").that_stays_alive())
        .allowing_the_current_user()
        .allowing_tool(tool.path())
        .start()
        .await;

    // When
    let spawned = supervisor
        .client()
        .await
        .spawn_session(a_session_spawn(&current_username(), tool.path()))
        .await
        .expect("spawn a session");

    // Then — the daemon stores this pid and later signals it, so a reply naming the wrong process
    // would have it signalling a stranger.
    assert_eq!(tool.await_recorded_pid().await, spawned.pid);
}

#[tokio::test]
async fn reports_a_session_as_running_while_it_is_alive() {
    // Given
    let tool = a_tool_that_records_its_parent();
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").that_stays_alive())
        .allowing_the_current_user()
        .allowing_tool(tool.path())
        .start()
        .await;
    let client = supervisor.client().await;
    let spawned = client
        .spawn_session(a_session_spawn(&current_username(), tool.path()))
        .await
        .expect("spawn a session");

    // When
    let status = client
        .session_status(spawned.pid)
        .await
        .expect("read session status");

    // Then
    assert_eq!(status.pid, spawned.pid);
    assert_eq!(status.state, SessionState::Running);
    assert_eq!(status.exit_code, None);
}

#[tokio::test]
async fn reports_the_exit_code_of_a_session_that_has_exited() {
    // Given a tool that exits with a distinctive status
    let tool = a_tool_that_exits_with(42);
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").that_stays_alive())
        .allowing_the_current_user()
        .allowing_tool(tool.path())
        .start()
        .await;
    let client = supervisor.client().await;
    let spawned = client
        .spawn_session(a_session_spawn(&current_username(), tool.path()))
        .await
        .expect("spawn a session");

    // When
    let status = supervisor.await_session_exit(&client, spawned.pid).await;

    // Then — the supervisor reaps its own children, so it is the only process that can answer this:
    // the daemon cannot `waitpid` a process it did not fork.
    assert_eq!(status.state, SessionState::Exited);
    assert_eq!(status.exit_code, Some(42));
}

#[tokio::test]
async fn keeps_a_sessions_exit_code_available_for_a_caller_that_asks_after_the_reap() {
    // Given a session that has already exited and been reaped
    let tool = a_tool_that_exits_with(7);
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").that_stays_alive())
        .allowing_the_current_user()
        .allowing_tool(tool.path())
        .start()
        .await;
    let client = supervisor.client().await;
    let spawned = client
        .spawn_session(a_session_spawn(&current_username(), tool.path()))
        .await
        .expect("spawn a session");
    supervisor.await_session_exit(&client, spawned.pid).await;

    // When the same status is asked for again
    let status = client
        .session_status(spawned.pid)
        .await
        .expect("read session status after the reap");

    // Then — a caller's poll always arrives after the reap, so a status discarded at reap time
    // would be a status no caller could ever observe.
    assert_eq!(status.exit_code, Some(7));
}

#[tokio::test]
async fn denies_a_status_query_for_a_pid_it_never_spawned() {
    // Given
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").that_stays_alive())
        .allowing_the_current_user()
        .start()
        .await;

    // When a caller asks about its own pid, which the supervisor did not spawn
    let result = supervisor
        .client()
        .await
        .session_status(std::process::id())
        .await;

    // Then — answering would turn the privileged surface into a way to probe arbitrary processes
    // on the host for liveness.
    result.assert_denied_without_disclosure();
}

#[tokio::test]
async fn stops_a_session_on_request_and_reports_that_it_exited() {
    // Given a session that would otherwise run for ten minutes
    let tool = a_tool_that_records_its_parent();
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").that_stays_alive())
        .allowing_the_current_user()
        .allowing_tool(tool.path())
        .start()
        .await;
    let client = supervisor.client().await;
    let spawned = client
        .spawn_session(a_session_spawn(&current_username(), tool.path()))
        .await
        .expect("spawn a session");

    // When
    client
        .stop_session(spawned.pid)
        .await
        .expect("stop the session");

    // Then
    let status = supervisor.await_session_exit(&client, spawned.pid).await;
    assert_eq!(status.state, SessionState::Exited);
    assert!(
        !process_is_alive(spawned.pid),
        "session {} survived being stopped",
        spawned.pid
    );
}
