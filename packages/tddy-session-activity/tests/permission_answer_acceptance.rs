//! An orchestrated claude-cli child's `PermissionRequest` hook waits, for a bounded time, for its
//! orchestrator's answer (`#home-inbox` 6/8).
//!
//! PRD `docs/ft/daemon/1-WIP/PRD-2026-10-25-permission-answer.md`. The path under test is the real
//! `AwaitQuestionAnswer` handler over a real session directory, answered through the real
//! `answer_permission` and the one `PendingAnswers` both share. The orchestrator's presence is a
//! fake port — whether this daemon serves a terminal is not what these tests are about — and the
//! deadline is injected, so no test waits longer than a fraction of a second.
//!
//! The pending question is written straight into the child's session directory, as `#home-inbox`
//! 3/8's capture records it, so these tests depend on this node alone.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pretty_assertions::assert_eq;
use rstest::rstest;
use tddy_core::pending_question::{read_pending_question, write_pending_question, PendingQuestion};
use tddy_core::session_metadata::{write_session_metadata, SessionMetadata};
use tddy_daemon_kernel::AgentActivityHub;
use tddy_discovery::subagent::PermissionAnswer;
use tddy_rpc::{Request, Status};
use tddy_service::proto::activity::{
    ActivityService as _, AwaitQuestionAnswerRequest, AwaitQuestionAnswerResponse,
    AwaitQuestionOutcome,
};
use tddy_session_activity::answers::{
    answer_permission, AnswerContext, AnswerRefusal, AnsweredPermission, AnsweringCaller,
    ALLOW_ONCE, REJECT_ONCE,
};
use tddy_session_activity::orchestrator_presence::OrchestratorPresence;
use tddy_session_activity::pending_answers::PendingAnswers;
use tddy_session_activity::{
    ActivityPorts, ActivityServiceImpl, AnswerWaitPorts, NoSessionDeltaStores, SessionLabels,
};

const OS_USER: &str = "ada";
const THIS_HOST: &str = "host-a";
const OTHER_HOST: &str = "host-b";
const CHILD: &str = "01900000-0000-7000-8000-AABB00000601";
const HOME: &str = "01900000-0000-7000-8000-AABB00000600";
const OTHER_HOME: &str = "01900000-0000-7000-8000-AABB00000602";
const HOOK_TOKEN: &str = "tok-permission-answer";
const REQUEST_ID: &str = "req-cargo-publish";
const A_COMMAND: &str = r#"{"command":"cargo publish"}"#;

/// Long enough that a hook still waiting when a test finishes is plainly a hook that was never woken.
const A_LONG_WAIT: Duration = Duration::from_secs(30);
/// The deadline of the test that lets it pass.
const A_SHORT_WAIT: Duration = Duration::from_millis(200);
/// How long a hook with nobody to wait for may take to be told so: "at once", with room for a slow
/// machine and none for a wait.
const AT_ONCE: Duration = Duration::from_secs(1);
/// How long a test gives the hook to open its wait before it answers.
const THE_HOOK_OPENS_WITHIN: Duration = Duration::from_secs(2);

// ---------------------------------------------------------------------------
// The host under test
// ---------------------------------------------------------------------------

struct EverySessionIsLabelled;

impl SessionLabels for EverySessionIsLabelled {
    fn label_for(&self, _sessions_base: &Path, _session_id: &str) -> String {
        "a-child".to_string()
    }
}

/// The orchestrators this daemon serves a terminal for: the fake presence port.
#[derive(Default)]
struct OrchestratorsServedHere {
    active: Mutex<HashSet<String>>,
}

#[async_trait::async_trait]
impl OrchestratorPresence for OrchestratorsServedHere {
    async fn is_active(
        &self,
        orchestrator_session_id: &str,
        _orchestrator_daemon_instance_id: Option<&str>,
    ) -> bool {
        self.active
            .lock()
            .expect("the active set")
            .contains(orchestrator_session_id)
    }
}

/// Who the child's changeset names as its orchestrator.
enum Orchestrator {
    None,
    OnThisHost(&'static str),
    OnAnotherHost(&'static str),
}

/// A host holding one claude-cli child waiting on a permission prompt, built step by step.
struct HostBuilder {
    orchestrator: Orchestrator,
    active: Vec<&'static str>,
    wait: Duration,
}

/// A child orchestrated by [`HOME`], which is running here; its hook may wait [`A_LONG_WAIT`].
fn a_host() -> HostBuilder {
    HostBuilder {
        orchestrator: Orchestrator::OnThisHost(HOME),
        active: vec![HOME, OTHER_HOME],
        wait: A_LONG_WAIT,
    }
}

impl HostBuilder {
    fn whose_child_has_no_orchestrator(mut self) -> Self {
        self.orchestrator = Orchestrator::None;
        self
    }

    fn whose_childs_orchestrator_runs_on_another_host(mut self) -> Self {
        self.orchestrator = Orchestrator::OnAnotherHost(HOME);
        self
    }

    fn whose_home_is_not_running(mut self) -> Self {
        self.active = vec![OTHER_HOME];
        self
    }

    fn holding_a_hook_for(mut self, wait: Duration) -> Self {
        self.wait = wait;
        self
    }

    fn build(self) -> AnActivityHost {
        let data_dir = tempfile::tempdir().expect("a temporary data dir");
        let child_dir = data_dir.path().join("sessions").join(CHILD);
        write_claude_cli_session(&child_dir);
        write_orchestrator(&child_dir, &self.orchestrator);
        write_pending_question(&child_dir, &the_permission_question())
            .expect("the child's pending question");
        let presence = OrchestratorsServedHere::default();
        presence
            .active
            .lock()
            .expect("the active set")
            .extend(self.active.iter().map(|id| id.to_string()));
        let pending = Arc::new(PendingAnswers::new());
        let service = ActivityServiceImpl::new(ActivityPorts {
            os_users: Arc::new(|_token: &str| {
                Err(Status::unauthenticated(
                    "no session token is presented on the hook path",
                ))
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
            pending: Arc::clone(&pending),
            presence: Arc::new(presence),
            wait: self.wait,
        });
        AnActivityHost {
            data_dir,
            service: Arc::new(service),
            pending,
        }
    }
}

struct AnActivityHost {
    data_dir: tempfile::TempDir,
    service: Arc<ActivityServiceImpl>,
    pending: Arc<PendingAnswers>,
}

/// The child's hook, waiting in `AwaitQuestionAnswer`.
struct AWaitingHook {
    waiting: tokio::task::JoinHandle<Result<AwaitQuestionAnswerResponse, Status>>,
}

impl AWaitingHook {
    /// What the wait ended with, however long it takes — the deadline is the host's.
    async fn outcome(self) -> AwaitQuestionAnswerResponse {
        self.waiting
            .await
            .expect("the hook's wait ran to completion")
            .expect("the hook's wait is answered")
    }

    /// What the wait ended with, provided it ended within `limit`.
    async fn outcome_within(self, limit: Duration) -> AwaitQuestionAnswerResponse {
        tokio::time::timeout(limit, self.outcome())
            .await
            .unwrap_or_else(|_| panic!("the hook was still waiting after {limit:?}"))
    }
}

impl AnActivityHost {
    fn child_dir(&self) -> PathBuf {
        self.data_dir.path().join("sessions").join(CHILD)
    }

    /// The child's `PermissionRequest` hook asks for an answer to [`REQUEST_ID`].
    fn the_hook_awaits(&self) -> AWaitingHook {
        let service = Arc::clone(&self.service);
        let request = AwaitQuestionAnswerRequest {
            session_id: CHILD.to_string(),
            hook_token: HOOK_TOKEN.to_string(),
            os_user: OS_USER.to_string(),
            request_id: REQUEST_ID.to_string(),
            permission_suggestions_json: String::new(),
        };
        AWaitingHook {
            waiting: tokio::spawn(async move {
                service
                    .await_question_answer(Request::direct(request))
                    .await
                    .map(|response| response.into_inner())
            }),
        }
    }

    /// The child's hook, once it has opened its wait — so an answer cannot overtake it.
    async fn a_hook_waiting(&self) -> AWaitingHook {
        let hook = self.the_hook_awaits();
        let deadline = tokio::time::Instant::now() + THE_HOOK_OPENS_WITHIN;
        while self.pending.open_request(REQUEST_ID).is_none() {
            assert!(
                tokio::time::Instant::now() < deadline,
                "the hook never opened its wait for {REQUEST_ID}"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        hook
    }

    /// `caller`, on this host, answers `answer` with `message`.
    fn answers(
        &self,
        caller: &str,
        answer: PermissionAnswer,
        message: Option<&str>,
    ) -> Result<AnsweredPermission, AnswerRefusal> {
        answer_permission(
            &AnswerContext {
                sessions_base: self.data_dir.path(),
                this_daemon_instance_id: THIS_HOST,
                pending: &self.pending,
            },
            &AnsweringCaller {
                session_id: caller.to_string(),
                daemon_instance_id: THIS_HOST.to_string(),
            },
            CHILD,
            answer,
            message.map(str::to_string),
        )
    }

    fn pending_question(&self) -> Option<PendingQuestion> {
        read_pending_question(&self.child_dir()).expect("the pending question file is readable")
    }

    /// The child's activity record of the answer to [`REQUEST_ID`]: its raw output.
    fn recorded_answer(&self) -> Option<serde_json::Value> {
        tddy_core::agent_activity::read_agent_activity(&self.child_dir())
            .expect("the child's activity log is readable")
            .into_iter()
            .find(|record| record.call_id == format!("permission:{REQUEST_ID}"))
            .map(|record| record.result)
    }
}

fn write_claude_cli_session(session_dir: &Path) {
    std::fs::create_dir_all(session_dir).expect("the session dir");
    let meta = SessionMetadata {
        session_id: CHILD.to_string(),
        project_id: "a-project".to_string(),
        created_at: "2026-10-28T10:00:00Z".to_string(),
        updated_at: "2026-10-28T10:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: Some(format!("/home/{OS_USER}/a-child")),
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

fn write_orchestrator(session_dir: &Path, orchestrator: &Orchestrator) {
    let mut changeset = tddy_core::Changeset::default();
    match orchestrator {
        Orchestrator::None => {}
        Orchestrator::OnThisHost(id) => changeset.orchestrator_session_id = Some(id.to_string()),
        Orchestrator::OnAnotherHost(id) => {
            changeset.orchestrator_session_id = Some(id.to_string());
            changeset.orchestrator_daemon_instance_id = Some(OTHER_HOST.to_string());
        }
    }
    tddy_core::write_changeset(session_dir, &changeset).expect("the child's changeset");
}

/// What `#home-inbox` 3/8 records for Claude asking to run `cargo publish`.
fn the_permission_question() -> PendingQuestion {
    PendingQuestion {
        kind: "permission".to_string(),
        question: "Run Bash: cargo publish".to_string(),
        request_id: REQUEST_ID.to_string(),
        tool_name: "Bash".to_string(),
        tool_input_json: A_COMMAND.to_string(),
        at_unix_ms: 1_793_000_000_000,
        ..Default::default()
    }
}

fn selecting(option_id: &str) -> PermissionAnswer {
    PermissionAnswer::Selected {
        request_id: REQUEST_ID.to_string(),
        option_id: option_id.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------

trait AwaitOutcomeAssertions {
    fn assert_answered(&self, option_id: &str, by: &str, message: &str) -> &Self;
    fn assert_outcome(&self, outcome: AwaitQuestionOutcome) -> &Self;
}

impl AwaitOutcomeAssertions for AwaitQuestionAnswerResponse {
    fn assert_answered(&self, option_id: &str, by: &str, message: &str) -> &Self {
        assert_eq!(
            self,
            &AwaitQuestionAnswerResponse {
                outcome: AwaitQuestionOutcome::Answered as i32,
                option_id: option_id.to_string(),
                cancelled: false,
                message: message.to_string(),
                answered_by: by.to_string(),
                ..Default::default()
            },
            "how the hook's wait ended"
        );
        self
    }

    fn assert_outcome(&self, outcome: AwaitQuestionOutcome) -> &Self {
        assert_eq!(
            AwaitQuestionOutcome::try_from(self.outcome),
            Ok(outcome),
            "how the hook's wait ended: {self:?}"
        );
        self
    }
}

// ---------------------------------------------------------------------------
// Acceptance
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_orchestrated_childs_await_returns_allow_when_its_home_answers_allow() {
    // Given a child whose running home started it, and whose hook waits on Claude's permission prompt
    let host = a_host().build();
    let hook = host.a_hook_waiting().await;

    // When the home allows it once
    let answered = host
        .answers(HOME, selecting(ALLOW_ONCE), None)
        .expect("the child's own home may answer");

    // Then the hook is woken with the allow, naming the home
    hook.outcome_within(AT_ONCE)
        .await
        .assert_answered(ALLOW_ONCE, HOME, "");
    assert_eq!(
        (answered.answered_by.as_str(), answered.option_id.as_str()),
        (HOME, ALLOW_ONCE),
        "what the answer recorded"
    );
    // And the child's activity says who answered what, and it waits on nothing any more
    assert_eq!(
        host.recorded_answer(),
        Some(serde_json::json!({ "answeredBy": HOME, "optionId": ALLOW_ONCE }))
    );
    assert_eq!(host.pending_question(), None);
}

#[tokio::test]
async fn deny_with_a_message_returns_deny_with_that_message() {
    // Given a child whose hook waits on Claude's permission prompt
    let host = a_host().build();
    let hook = host.a_hook_waiting().await;

    // When its home rejects it, saying why
    host.answers(
        HOME,
        selecting(REJECT_ONCE),
        Some("publish from the release branch, not this one"),
    )
    .expect("the child's own home may answer");

    // Then the hook is woken with the rejection and that message
    hook.outcome_within(AT_ONCE).await.assert_answered(
        REJECT_ONCE,
        HOME,
        "publish from the release branch, not this one",
    );
}

#[tokio::test]
async fn with_no_answer_before_the_deadline_the_await_times_out() {
    // Given a child whose hook may wait only a moment
    let host = a_host().holding_a_hook_for(A_SHORT_WAIT).build();

    // When nobody answers
    let hook = host.the_hook_awaits();

    // Then the wait ends as timed out, and the question is still the child's to answer in Claude
    hook.outcome_within(AT_ONCE)
        .await
        .assert_outcome(AwaitQuestionOutcome::TimedOut);
    assert_eq!(host.pending_question(), Some(the_permission_question()));
    // And an answer arriving after it is refused: nobody is waiting for it
    assert_eq!(
        host.answers(HOME, selecting(ALLOW_ONCE), None),
        Err(AnswerRefusal::UnknownRequest {
            request_id: REQUEST_ID.to_string()
        })
    );
}

#[rstest]
#[case::its_home_is_not_running(a_host().whose_home_is_not_running())]
#[case::it_has_no_orchestrator(a_host().whose_child_has_no_orchestrator())]
#[case::its_orchestrator_runs_on_another_host(
    a_host().whose_childs_orchestrator_runs_on_another_host()
)]
#[tokio::test]
async fn a_child_without_an_active_orchestrator_returns_no_orchestrator_at_once(
    #[case] host: HostBuilder,
) {
    // Given a child that nobody on this host will answer, whose hook would otherwise wait long
    let host = host.holding_a_hook_for(A_LONG_WAIT).build();

    // When its hook asks for an answer
    let hook = host.the_hook_awaits();

    // Then it is told at once that there is nobody to wait for
    hook.outcome_within(AT_ONCE)
        .await
        .assert_outcome(AwaitQuestionOutcome::NoOrchestrator);
}

#[tokio::test]
async fn another_home_an_unknown_id_and_an_already_answered_question_are_refused_and_the_waiter_keeps_waiting(
) {
    // Given a child whose hook waits on Claude's permission prompt
    let host = a_host().build();
    let hook = host.a_hook_waiting().await;

    // When another home answers it, and the child's home answers a request it is not asked
    let by_another_home = host.answers(OTHER_HOME, selecting(REJECT_ONCE), None);
    let unknown = host.answers(
        HOME,
        PermissionAnswer::Selected {
            request_id: "req-never-asked".to_string(),
            option_id: REJECT_ONCE.to_string(),
        },
        None,
    );

    // Then both are refused with their reasons
    assert_eq!(
        by_another_home,
        Err(AnswerRefusal::NotTheOrchestrator {
            caller: OTHER_HOME.to_string(),
            target: CHILD.to_string(),
            orchestrator: Some(HOME.to_string()),
        })
    );
    assert_eq!(
        unknown,
        Err(AnswerRefusal::UnknownRequest {
            request_id: "req-never-asked".to_string()
        })
    );

    // And when the child's home then allows it, and answers again
    host.answers(HOME, selecting(ALLOW_ONCE), None)
        .expect("the hook was still waiting for its home");
    let again = host.answers(HOME, selecting(REJECT_ONCE), None);

    // Then the second answer is refused naming who answered first, and the hook got the first one
    assert_eq!(
        again,
        Err(AnswerRefusal::AlreadyAnswered {
            request_id: REQUEST_ID.to_string(),
            by: HOME.to_string(),
        })
    );
    hook.outcome_within(AT_ONCE)
        .await
        .assert_answered(ALLOW_ONCE, HOME, "");
}
