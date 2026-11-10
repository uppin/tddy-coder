//! `SpawnStreamed`: a binary run as another OS user, its stderr streamed line by line, its exit
//! reported with the stderr tail, and the child placed in a cgroup scope (`#agent-tools` 32/32).
//!
//! The supervisor knows nothing about what the binary is — these tools are shell scripts, not git.
//! What needs a live child is Linux-only, like every spawning test (see `support`'s header); the
//! refusal is decided before anything is forked, so it is asserted on every host.

mod support;

use support::{a_service, a_supervisor, a_trusted_tool, current_username};
use tddy_supervisor::proto::supervisor::SpawnStreamedRequest;

fn a_streamed_spawn_of(tool: &std::path::Path) -> SpawnStreamedRequest {
    SpawnStreamedRequest {
        os_user: current_username(),
        tool_path: tool.display().to_string(),
        args: Vec::new(),
        env: Default::default(),
        working_dir: None,
        scope: None,
    }
}

#[tokio::test]
async fn a_streamed_spawn_of_a_binary_outside_every_exec_root_is_refused() {
    // Given a tool, and exec roots that do not cover it
    let tool = a_trusted_tool("talker", "exit 0");
    let elsewhere = a_trusted_tool("other", "exit 0");
    let supervisor = a_supervisor()
        .managing(a_service("tddy-daemon").that_stays_alive())
        .allowing_the_current_user()
        .with_exec_root(elsewhere.path())
        .start()
        .await;

    // When
    let refused = supervisor
        .client()
        .await
        .spawn_streamed(a_streamed_spawn_of(tool.path()))
        .await;

    // Then — the call fails; there is no stream to report on
    assert!(refused.is_err(), "a binary outside every exec root runs");
}

#[cfg(target_os = "linux")]
mod live {
    use std::time::Duration;

    use super::a_streamed_spawn_of;
    use super::support::{
        a_service, a_supervisor, a_trusted_tool, RunningSupervisor, ScopeAssertions,
    };
    use tddy_supervisor::proto::supervisor::streamed_event::Event;
    use tddy_supervisor::proto::supervisor::{SpawnStreamedRequest, StreamedExited};
    use tddy_supervisor::CreateScopeRequest;

    /// How long any one event may take to arrive.
    const EVENT_DEADLINE: Duration = Duration::from_secs(30);

    /// The child's pid, every stderr line it reported, and how it ended.
    async fn stream_of(
        supervisor: &RunningSupervisor,
        request: SpawnStreamedRequest,
    ) -> (u32, Vec<String>, StreamedExited) {
        let mut events = supervisor
            .client()
            .await
            .spawn_streamed(request)
            .await
            .expect("the streamed spawn starts");
        let mut pid = 0;
        let mut lines = Vec::new();
        while let Some(event) = tokio::time::timeout(EVENT_DEADLINE, events.recv())
            .await
            .expect("an event within the deadline")
        {
            match event.expect("a streamed event").event.expect("a variant") {
                Event::Started(started) => pid = started.pid,
                Event::Line(line) => lines.push(line.line),
                Event::Exited(exited) => return (pid, lines, exited),
            }
        }
        panic!("the stream ended without saying how the child exited; lines: {lines:?}");
    }

    #[tokio::test]
    async fn a_streamed_spawn_reports_started_each_stderr_line_and_its_exit_with_the_tail() {
        // Given a tool under an exec root that redraws a line in place, then fails
        let tool = a_trusted_tool(
            "talker",
            "printf 'Counting: 10%%\\rCounting: 100%%, done.\\nfatal: it went wrong\\n' >&2\nexit 3",
        );
        let supervisor = a_supervisor()
            .managing(a_service("tddy-daemon").that_stays_alive())
            .allowing_the_current_user()
            .with_exec_root(tool.directory())
            .start()
            .await;

        // When
        let (pid, lines, exited) = stream_of(&supervisor, a_streamed_spawn_of(tool.path())).await;

        // Then
        assert_ne!(pid, 0, "the first event named the child's pid");
        assert_eq!(
            (lines, exited.code),
            (
                vec![
                    "Counting: 10%".to_string(),
                    "Counting: 100%, done.".to_string(),
                    "fatal: it went wrong".to_string(),
                ],
                Some(3),
            )
        );
        assert!(
            exited.stderr_tail.ends_with("fatal: it went wrong"),
            "the tail carries the child's last words: {exited:?}"
        );
    }

    #[tokio::test]
    async fn a_streamed_spawn_runs_in_a_cgroup_scope() {
        // Given a scope, and a tool under an exec root that stays up until it is stopped
        let tool = a_trusted_tool("waiter", "echo started >&2\nexec sleep 600");
        let supervisor = a_supervisor()
            .managing(a_service("tddy-daemon").that_stays_alive())
            .allowing_the_current_user()
            .with_exec_root(tool.directory())
            .start()
            .await;
        let client = supervisor.client().await;
        let scope = client
            .create_scope(CreateScopeRequest {
                name: "streamed-scoped".to_string(),
                limits: Default::default(),
            })
            .await
            .expect("create a cgroup scope");

        // When it is spawned into the scope
        let mut events = client
            .spawn_streamed(SpawnStreamedRequest {
                scope: Some("streamed-scoped".to_string()),
                ..a_streamed_spawn_of(tool.path())
            })
            .await
            .expect("the streamed spawn starts");
        let pid = match tokio::time::timeout(EVENT_DEADLINE, events.recv())
            .await
            .expect("the first event within the deadline")
            .expect("the stream is open")
            .expect("a streamed event")
            .event
        {
            Some(Event::Started(started)) => started.pid,
            other => panic!("the first event is Started, got {other:?}"),
        };

        // Then the child joined the scope before it exec'd
        scope.assert_wrote("cgroup.procs", &pid.to_string());
        client.stop_session(pid).await.expect("stop the child");
    }
}
