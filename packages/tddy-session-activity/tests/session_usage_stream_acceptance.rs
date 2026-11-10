//! Acceptance: `ActivityService.StreamSessionUsage` streams a session's per-conversation usage —
//! the snapshot its agent's spec binding yields against the usage context the session's launch
//! recorded, then each changed snapshot (`#usage-binding 2/5`).
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-11-10-usage-binding-daemon-stream.md

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use pretty_assertions::assert_eq;
use tddy_daemon_kernel::AgentActivityHub;
use tddy_rpc::{Request, Status};
use tddy_service::proto::activity::{
    ActivityService, SessionUsageSnapshot, StreamSessionUsageRequest,
};
use tddy_session_activity::{
    ActivityPorts, ActivityServiceImpl, NoSessionDeltaStores, SessionLabels,
};
use tddy_usage::{write_usage_context, UsageContextFile, UsageSection};

const TOKEN: &str = "web-token";
/// The OS user the web token maps to.
const OS_USER: &str = "tester";
const SESSION: &str = "019fb0c1-5a2e-7d11-9c3b-6f0a1e2d3c4b";
/// How long a test waits for a snapshot the poller owes it.
const DEADLINE: Duration = Duration::from_secs(5);

struct EverySessionIsLabelled;

impl SessionLabels for EverySessionIsLabelled {
    fn label_for(&self, _sessions_base: &Path, session_id: &str) -> String {
        session_id.to_string()
    }
}

/// A binding for spec `acme` that reads `<agent_home>/<conversation_id>.jsonl`: each `turn` line's
/// `in` and `out`.
fn acmes_binding() -> UsageSection {
    serde_json::from_value(serde_json::json!({
        "sources": [{
            "format": "jsonl",
            "main": { "path": "{agent_home}/{conversation_id}.jsonl" },
            "record": {
                "where": [{ "field": "type", "equals": "turn" }],
                "tokens": { "input": "in", "output": "out" }
            }
        }]
    }))
    .expect("a valid binding")
}

/// A daemon's data dir, one session in it, and the agent home that session's agent writes to.
struct ADaemon {
    data_dir: tempfile::TempDir,
    service: ActivityServiceImpl,
}

fn a_daemon() -> ADaemon {
    let data_dir = tempfile::tempdir().expect("a data dir");
    let service = ActivityServiceImpl::new(ActivityPorts {
        os_users: Arc::new(|token: &str| {
            if token == TOKEN {
                Ok(OS_USER.to_string())
            } else {
                Err(Status::unauthenticated("unknown token"))
            }
        }),
        tddy_data_dir: data_dir.path().to_path_buf(),
        activity: Arc::new(AgentActivityHub::default()),
        conversation_frames: Arc::new(
            tddy_daemon_kernel::conversation_frames::ConversationFrames::default(),
        ),
        notifications: None,
        session_labels: Arc::new(EverySessionIsLabelled),
        deltas: Arc::new(NoSessionDeltaStores),
        daemon_instance_id: "this-host".to_string(),
        usage_bindings: Arc::new(|spec: &str| (spec == "acme").then(acmes_binding)),
    });
    ADaemon { data_dir, service }
}

impl ADaemon {
    fn session_dir(&self) -> PathBuf {
        let base = tddy_daemon_kernel::user_paths::sessions_base_for_user(
            OS_USER,
            Some(self.data_dir.path()),
        )
        .expect("a sessions base");
        tddy_core::session_lifecycle::unified_session_dir_path(&base, SESSION)
    }

    fn agent_home(&self) -> PathBuf {
        self.data_dir.path().join("homes").join("acme")
    }

    /// The session's launch recorded that it runs `acme` with its files under [`Self::agent_home`].
    fn with_a_recorded_acme_session(self) -> Self {
        std::fs::create_dir_all(self.session_dir()).expect("mkdir session");
        write_usage_context(
            &self.session_dir(),
            &UsageContextFile {
                spec_id: "acme".to_string(),
                agent_home: Some(self.agent_home()),
                conversation_id: Some(SESSION.to_string()),
                worktree: None,
                model: Some("acme-model".to_string()),
            },
        )
        .expect("write the usage context");
        self
    }

    /// The agent completes a turn of `input` / `output` tokens.
    fn the_agent_takes_a_turn(&self, input: u64, output: u64) {
        std::fs::create_dir_all(self.agent_home()).expect("mkdir home");
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.agent_home().join(format!("{SESSION}.jsonl")))
            .expect("open transcript");
        writeln!(file, r#"{{"type":"turn","in":{input},"out":{output}}}"#).expect("write");
    }

    /// A tddy subagent conversation in the session's accounting file.
    fn with_a_tddy_subagent(self) -> Self {
        let egress = self.session_dir().join("egress");
        std::fs::create_dir_all(&egress).expect("mkdir egress");
        std::fs::write(
            egress.join("accounting.json"),
            r#"{"conversations":[{"agent":"fastcontext","id":"fc-1","model":"ollama","inputTokens":5,"outputTokens":1,"totalTokens":6,"turns":1}]}"#,
        )
        .expect("write accounting");
        self
    }

    async fn subscribe(
        &self,
    ) -> <ActivityServiceImpl as ActivityService>::StreamSessionUsageStream {
        self.service
            .stream_session_usage(Request::direct(StreamSessionUsageRequest {
                session_token: TOKEN.to_string(),
                session_id: SESSION.to_string(),
                daemon_instance_id: String::new(),
            }))
            .await
            .expect("the stream opens")
            .into_inner()
    }
}

/// The next snapshot `stream` delivers, as `(id, input, output)` per conversation.
async fn next_snapshot(
    stream: &mut <ActivityServiceImpl as ActivityService>::StreamSessionUsageStream,
) -> Vec<(String, u64, u64)> {
    let snapshot: SessionUsageSnapshot = tokio::time::timeout(DEADLINE, stream.next())
        .await
        .expect("a snapshot within the deadline")
        .expect("the stream is still open")
        .expect("a snapshot, not an error");
    snapshot
        .conversations
        .into_iter()
        .map(|c| (c.id, c.input_tokens, c.output_tokens))
        .collect()
}

#[tokio::test]
async fn a_subscriber_receives_the_sessions_usage_snapshot_first() {
    // Given
    let daemon = a_daemon().with_a_recorded_acme_session();
    daemon.the_agent_takes_a_turn(100, 10);

    // When
    let mut stream = daemon.subscribe().await;

    // Then
    assert_eq!(
        next_snapshot(&mut stream).await,
        vec![(SESSION.to_string(), 100, 10)]
    );
}

#[tokio::test]
async fn a_transcript_that_grows_streams_a_new_snapshot() {
    // Given
    let daemon = a_daemon().with_a_recorded_acme_session();
    daemon.the_agent_takes_a_turn(100, 10);
    let mut stream = daemon.subscribe().await;
    let _first = next_snapshot(&mut stream).await;

    // When
    daemon.the_agent_takes_a_turn(50, 5);

    // Then
    assert_eq!(
        next_snapshot(&mut stream).await,
        vec![(SESSION.to_string(), 150, 15)]
    );
}

#[tokio::test]
async fn an_unchanged_snapshot_is_not_streamed_again() {
    // Given
    let daemon = a_daemon().with_a_recorded_acme_session();
    daemon.the_agent_takes_a_turn(100, 10);
    let mut stream = daemon.subscribe().await;
    let _first = next_snapshot(&mut stream).await;

    // When — several poll ticks pass with nothing written
    let next = tokio::time::timeout(Duration::from_secs(5), stream.next()).await;

    // Then
    assert!(
        next.is_err(),
        "an unchanged snapshot was streamed again: {next:?}"
    );
}

#[tokio::test]
async fn an_ended_session_streams_its_final_usage() {
    // Given — the session ended long ago; only its files remain
    let daemon = a_daemon().with_a_recorded_acme_session();
    daemon.the_agent_takes_a_turn(7, 3);

    // When
    let mut stream = daemon.subscribe().await;

    // Then
    assert_eq!(
        next_snapshot(&mut stream).await,
        vec![(SESSION.to_string(), 7, 3)]
    );
}

#[tokio::test]
async fn a_session_without_a_usage_context_streams_only_its_tddy_subagents() {
    // Given — a session whose launch predates the usage context
    let daemon = a_daemon();
    std::fs::create_dir_all(daemon.session_dir()).expect("mkdir session");
    let daemon = daemon.with_a_tddy_subagent();

    // When
    let mut stream = daemon.subscribe().await;

    // Then
    assert_eq!(
        next_snapshot(&mut stream).await,
        vec![("fc-1".to_string(), 5, 1)]
    );
}

#[tokio::test]
async fn two_subscribers_of_one_session_see_the_same_snapshots() {
    // Given
    let daemon = a_daemon().with_a_recorded_acme_session();
    daemon.the_agent_takes_a_turn(100, 10);
    let mut first = daemon.subscribe().await;
    let mut second = daemon.subscribe().await;
    let _ = (
        next_snapshot(&mut first).await,
        next_snapshot(&mut second).await,
    );

    // When
    daemon.the_agent_takes_a_turn(1, 1);

    // Then
    assert_eq!(
        (
            next_snapshot(&mut first).await,
            next_snapshot(&mut second).await
        ),
        (
            vec![(SESSION.to_string(), 101, 11)],
            vec![(SESSION.to_string(), 101, 11)]
        )
    );
}
