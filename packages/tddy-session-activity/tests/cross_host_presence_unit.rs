//! The pieces of waiting for an orchestrator on another host (`#home-inbox` 10/11), one at a time:
//! which host the gate asks the presence port about, and what the peer-side handler answers for a
//! session its own daemon serves or does not.
//!
//! The port is a recording fake, so these say nothing about terminals or peers; the end-to-end
//! behaviour is `packages/tddy-daemon-rpc/tests/cross_host_answer_wait_acceptance.rs`.

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pretty_assertions::assert_eq;
use rstest::rstest;
use tddy_core::pending_question::{write_pending_question, PendingQuestion};
use tddy_core::session_metadata::{write_session_metadata, SessionMetadata};
use tddy_daemon_kernel::AgentActivityHub;
use tddy_rpc::{Request, RequestMetadata, RequestTransport, Status};
use tddy_service::proto::activity::{
    ActivityService as _, AwaitQuestionAnswerRequest, AwaitQuestionOutcome,
    IsSessionTerminalServedRequest,
};
use tddy_session_activity::orchestrator_presence::OrchestratorPresence;
use tddy_session_activity::pending_answers::PendingAnswers;
use tddy_session_activity::{
    ActivityPorts, ActivityServiceImpl, AnswerWaitPorts, NoSessionDeltaStores, SessionLabels,
};

const THIS_HOST: &str = "host-a";
const OTHER_HOST: &str = "host-b";
const CHILD: &str = "01900000-0000-7000-8000-AABB00001000";
const HOME: &str = "01900000-0000-7000-8000-AABB00001001";
const HOOK_TOKEN: &str = "tok-cross-host";
const REQUEST_ID: &str = "req-cross-host";
/// How long a hook that is waiting is held: a fraction of a second, so no test waits for it.
const A_SHORT_WAIT: Duration = Duration::from_millis(100);
/// Bounds every hook in these tests.
const AT_MOST: Duration = Duration::from_secs(2);

/// Every call the port was asked, and the orchestrators it says are active.
#[derive(Default)]
struct RecordingPresence {
    active: Mutex<HashSet<String>>,
    asked: Mutex<Vec<(String, Option<String>)>>,
}

#[async_trait::async_trait]
impl OrchestratorPresence for RecordingPresence {
    async fn is_active(&self, session_id: &str, host: Option<&str>) -> bool {
        self.asked
            .lock()
            .unwrap()
            .push((session_id.to_string(), host.map(str::to_string)));
        self.active.lock().unwrap().contains(session_id)
    }
}

struct EverySessionIsLabelled;

impl SessionLabels for EverySessionIsLabelled {
    fn label_for(&self, _sessions_base: &Path, _session_id: &str) -> String {
        "a-child".to_string()
    }
}

struct AHost {
    data_dir: tempfile::TempDir,
    service: ActivityServiceImpl,
    presence: Arc<RecordingPresence>,
}

/// A host whose port says `active` are running, holding a claude-cli child whose changeset names
/// [`HOME`] as orchestrator on `orchestrator_host` (`None`: no host recorded), waiting on a
/// permission prompt.
fn a_host_whose_child_is_orchestrated_on(
    orchestrator_host: Option<&str>,
    active: &[&str],
) -> AHost {
    let data_dir = tempfile::tempdir().expect("a temporary data dir");
    let child_dir = data_dir.path().join("sessions").join(CHILD);
    write_claude_cli_session(&child_dir);
    let changeset = tddy_core::Changeset {
        orchestrator_session_id: Some(HOME.to_string()),
        orchestrator_daemon_instance_id: orchestrator_host.map(str::to_string),
        ..Default::default()
    };
    tddy_core::write_changeset(&child_dir, &changeset).expect("the child's changeset");
    write_pending_question(
        &child_dir,
        &PendingQuestion {
            kind: "permission".to_string(),
            question: "Run Bash: cargo publish".to_string(),
            request_id: REQUEST_ID.to_string(),
            tool_name: "Bash".to_string(),
            tool_input_json: r#"{"command":"cargo publish"}"#.to_string(),
            ..Default::default()
        },
    )
    .expect("the child's pending question");
    let presence = Arc::new(RecordingPresence::default());
    presence
        .active
        .lock()
        .unwrap()
        .extend(active.iter().map(|id| id.to_string()));
    let service = ActivityServiceImpl::new(ActivityPorts {
        os_users: Arc::new(|_token: &str| {
            Err(Status::unauthenticated("no token on the hook path"))
        }),
        tddy_data_dir: data_dir.path().to_path_buf(),
        activity: Arc::new(AgentActivityHub::default()),
        conversation_frames: Arc::new(
            tddy_daemon_kernel::conversation_frames::ConversationFrames::default(),
        ),
        notifications: None,
        session_labels: Arc::new(EverySessionIsLabelled),
        deltas: Arc::new(NoSessionDeltaStores),
        daemon_instance_id: THIS_HOST.to_string(),
    })
    .with_question_answers(AnswerWaitPorts {
        pending: Arc::new(PendingAnswers::new()),
        presence: Arc::clone(&presence) as Arc<dyn OrchestratorPresence>,
        wait: A_SHORT_WAIT,
    });
    AHost {
        data_dir,
        service,
        presence,
    }
}

impl AHost {
    /// The child's hook asks for an answer, and how its wait ended.
    async fn the_childs_hook_ends_as(&self) -> AwaitQuestionOutcome {
        let response = tokio::time::timeout(
            AT_MOST,
            self.service
                .await_question_answer(Request::direct(AwaitQuestionAnswerRequest {
                    session_id: CHILD.to_string(),
                    hook_token: HOOK_TOKEN.to_string(),
                    os_user: "ada".to_string(),
                    request_id: REQUEST_ID.to_string(),
                    permission_suggestions_json: String::new(),
                })),
        )
        .await
        .expect("the hook's wait ended")
        .expect("the hook's wait is answered")
        .into_inner();
        AwaitQuestionOutcome::try_from(response.outcome).expect("a known outcome")
    }

    /// What the port was asked, in order: the session and the host named.
    fn the_port_was_asked(&self) -> Vec<(String, Option<String>)> {
        self.presence.asked.lock().unwrap().clone()
    }

    /// A peer asks whether this daemon serves `session_id`'s main terminal.
    async fn is_asked_by_a_peer_whether_it_serves(&self, session_id: &str) -> bool {
        self.service
            .is_session_terminal_served(Request::with_metadata(
                IsSessionTerminalServedRequest {
                    session_id: session_id.to_string(),
                },
                RequestMetadata::over(RequestTransport::LiveKit),
            ))
            .await
            .expect("the peer's question is answered")
            .into_inner()
            .served
    }
}

fn write_claude_cli_session(session_dir: &Path) {
    std::fs::create_dir_all(session_dir).expect("the session dir");
    let meta = SessionMetadata {
        session_id: CHILD.to_string(),
        project_id: "a-project".to_string(),
        created_at: "2026-10-29T10:00:00Z".to_string(),
        updated_at: "2026-10-29T10:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: Some("/home/ada/a-child".to_string()),
        pid: Some(4242),
        tool: None,
        livekit_room: None,
        pending_elicitation: true,
        previous_session_id: None,
        session_type: Some("claude-cli".to_string()),
        model: None,
        cursor_chat_id: None,
        harness_resume_token: None,
        activity_status: Some("WaitingForInput".to_string()),
        hook_token: Some(HOOK_TOKEN.to_string()),
        sandbox: None,
        agent: None,
        recipe: None,
        agents: Vec::new(),
        agents_rev: 0,
        legacy_specialized_agents: Vec::new(),
        codebase_daemon_instance_id: None,
        codebase_session_id: None,
        agent_daemon_instance_id: None,
        agent_session_id: None,
        ssh_config_host: None,
        managed_codebase: false,
        session_role: None,
    };
    write_session_metadata(session_dir, &meta).expect("the session's .session.yaml");
}

// ---------------------------------------------------------------------------
// Which host the gate asks about
// ---------------------------------------------------------------------------

#[rstest]
#[case::no_host_recorded(None, None)]
#[case::an_empty_host(Some(""), None)]
#[case::this_host(Some(THIS_HOST), None)]
#[case::another_host(Some(OTHER_HOST), Some(OTHER_HOST))]
#[tokio::test]
async fn the_gate_asks_the_port_about_the_host_only_when_it_is_not_this_one(
    #[case] recorded: Option<&str>,
    #[case] asked_about: Option<&str>,
) {
    // Given a child whose changeset records `recorded` as its orchestrator's host
    let host = a_host_whose_child_is_orchestrated_on(recorded, &[HOME]);

    // When its hook asks for an answer
    host.the_childs_hook_ends_as().await;

    // Then the port was asked about the orchestrator, naming the host only when it is another one
    assert_eq!(
        host.the_port_was_asked(),
        vec![(HOME.to_string(), asked_about.map(str::to_string))],
        "what the presence port was asked"
    );
}

#[rstest]
#[case::the_port_says_it_is_active(&[HOME], AwaitQuestionOutcome::TimedOut)]
#[case::the_port_says_it_is_not(&[], AwaitQuestionOutcome::NoOrchestrator)]
#[tokio::test]
async fn an_orchestrator_on_another_host_is_waited_for_when_the_port_says_it_is_active(
    #[case] active: &[&str],
    #[case] outcome: AwaitQuestionOutcome,
) {
    // Given a child whose orchestrator runs on another host, which the port reports as `active`
    let host = a_host_whose_child_is_orchestrated_on(Some(OTHER_HOST), active);

    // When its hook asks for an answer and nobody gives one
    let ended_as = host.the_childs_hook_ends_as().await;

    // Then the hook waited, or was told nobody is there, as the port said
    assert_eq!(ended_as, outcome, "how the hook's wait ended");
}

// ---------------------------------------------------------------------------
// The peer-side handler
// ---------------------------------------------------------------------------

#[rstest]
#[case::a_session_this_daemon_serves(HOME, true)]
#[case::a_session_it_does_not(CHILD, false)]
#[tokio::test]
async fn a_peer_is_told_whether_this_daemon_serves_the_session(
    #[case] session_id: &str,
    #[case] served: bool,
) {
    // Given a daemon whose port reports only the home as served
    let host = a_host_whose_child_is_orchestrated_on(None, &[HOME]);

    // When a peer asks about `session_id`
    let answer = host.is_asked_by_a_peer_whether_it_serves(session_id).await;

    // Then it is told, from this daemon's own terminals alone: no host is named to the port
    assert_eq!(answer, served, "what the peer was told");
    assert_eq!(
        host.the_port_was_asked(),
        vec![(session_id.to_string(), None)],
        "what the presence port was asked"
    );
    let _ = &host.data_dir;
}
