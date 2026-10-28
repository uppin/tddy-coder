//! An orchestrated claude-cli child's `AskUserQuestion` hook waits, for a bounded time, for its
//! orchestrator's answers (`#home-inbox` 8/8).
//!
//! PRD `docs/ft/daemon/1-WIP/PRD-2026-10-25-question-answer.md`. The path under test is the real
//! `AwaitQuestionAnswer` handler over a real session directory, answered through the real
//! `answer_question` and the one `PendingAnswers` both share — `6/8`'s, extended with the question
//! waiters. The orchestrator's presence is a fake port and the deadline is injected, so no test
//! waits longer than a fraction of a second.
//!
//! The pending question is written straight into the child's session directory, as `#home-inbox`
//! 3/8's capture records it.
//!
//! **Blocked on `permission-answer` (`6/8`):** the two tests that hold a hook open go through
//! `await_question_answer_impl`, whose body is `6/8`'s. Until it is green they fail at "the hook
//! never opened its wait" — before reaching anything this node owns. The third test needs no hook.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pretty_assertions::assert_eq;
use tddy_core::pending_question::{
    read_pending_question, write_pending_question, PendingQuestion, PendingQuestionItem,
    PendingQuestionOption,
};
use tddy_core::session_metadata::{write_session_metadata, SessionMetadata};
use tddy_daemon_kernel::AgentActivityHub;
use tddy_rpc::{Request, Status};
use tddy_service::proto::activity::{
    ActivityService as _, AwaitQuestionAnswerRequest, AwaitQuestionAnswerResponse,
    AwaitQuestionOutcome,
};
use tddy_session_activity::answers::{
    answer_question, AnswerContext, AnswerRefusal, AnsweredQuestion, AnsweringCaller,
    QuestionAnswer,
};
use tddy_session_activity::orchestrator_presence::OrchestratorPresence;
use tddy_session_activity::pending_answers::PendingAnswers;
use tddy_session_activity::{
    ActivityPorts, ActivityServiceImpl, AnswerWaitPorts, NoSessionDeltaStores, SessionLabels,
};

const OS_USER: &str = "ada";
const THIS_HOST: &str = "host-a";
const CHILD: &str = "01900000-0000-7000-8000-AABB00000801";
const HOME: &str = "01900000-0000-7000-8000-AABB00000800";
const HOOK_TOKEN: &str = "tok-question-answer";
const REQUEST_ID: &str = "req-which-language";

/// Long enough that a hook still waiting when a test finishes is plainly a hook that was never woken.
const A_LONG_WAIT: Duration = Duration::from_secs(30);
/// How long a woken hook may take to return: "at once", with room for a slow machine and none for a
/// wait.
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
struct OrchestratorsServedHere {
    active: Mutex<HashSet<String>>,
}

#[async_trait::async_trait]
impl OrchestratorPresence for OrchestratorsServedHere {
    async fn is_active(&self, orchestrator_session_id: &str) -> bool {
        self.active
            .lock()
            .expect("the active set")
            .contains(orchestrator_session_id)
    }
}

/// What `#home-inbox` 3/8 records for an `AskUserQuestion` of two questions: which language (one of
/// two), and which checks to run (any of three).
fn the_two_questions() -> PendingQuestion {
    let option = |label: &str| PendingQuestionOption {
        label: label.to_string(),
        description: String::new(),
    };
    PendingQuestion {
        kind: "select".to_string(),
        header: "Language".to_string(),
        question: "Which language?".to_string(),
        options: vec!["Rust".to_string(), "Go".to_string()],
        allow_other: true,
        request_id: REQUEST_ID.to_string(),
        tool_name: "AskUserQuestion".to_string(),
        tool_input_json: serde_json::json!({
            "questions": [
                {
                    "question": "Which language?", "header": "Language", "multiSelect": false,
                    "options": [{"label": "Rust"}, {"label": "Go"}]
                },
                {
                    "question": "Which checks should run?", "header": "Checks", "multiSelect": true,
                    "options": [{"label": "fmt"}, {"label": "clippy"}, {"label": "tests"}]
                }
            ]
        })
        .to_string(),
        questions: vec![
            PendingQuestionItem {
                header: "Language".to_string(),
                question: "Which language?".to_string(),
                options: vec![option("Rust"), option("Go")],
                multi_select: false,
            },
            PendingQuestionItem {
                header: "Checks".to_string(),
                question: "Which checks should run?".to_string(),
                options: vec![option("fmt"), option("clippy"), option("tests")],
                multi_select: true,
            },
        ],
        at_unix_ms: 1_793_000_000_000,
    }
}

/// What `#home-inbox` 3/8 records for Claude asking to run `cargo publish`.
fn a_permission_question() -> PendingQuestion {
    PendingQuestion {
        kind: "permission".to_string(),
        question: "Run Bash: cargo publish".to_string(),
        request_id: REQUEST_ID.to_string(),
        tool_name: "Bash".to_string(),
        tool_input_json: r#"{"command":"cargo publish"}"#.to_string(),
        at_unix_ms: 1_793_000_000_000,
        ..Default::default()
    }
}

/// A host holding one claude-cli child, orchestrated by [`HOME`] (running here), waiting on
/// `question`; its hook may wait [`A_LONG_WAIT`].
fn a_host_whose_child_asks(question: PendingQuestion) -> AnActivityHost {
    let data_dir = tempfile::tempdir().expect("a temporary data dir");
    let child_dir = data_dir.path().join("sessions").join(CHILD);
    write_claude_cli_session(&child_dir);
    let changeset = tddy_core::Changeset {
        orchestrator_session_id: Some(HOME.to_string()),
        ..Default::default()
    };
    tddy_core::write_changeset(&child_dir, &changeset).expect("the child's changeset");
    write_pending_question(&child_dir, &question).expect("the child's pending question");
    let presence = OrchestratorsServedHere {
        active: Mutex::new(HashSet::from([HOME.to_string()])),
    };
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
        wait: A_LONG_WAIT,
    });
    AnActivityHost {
        data_dir,
        service: Arc::new(service),
        pending,
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
    /// What the wait ended with, provided it ended within [`AT_ONCE`] of the answer.
    async fn outcome_at_once(self) -> AwaitQuestionAnswerResponse {
        tokio::time::timeout(AT_ONCE, self.waiting)
            .await
            .unwrap_or_else(|_| panic!("the hook was still waiting {AT_ONCE:?} after the answer"))
            .expect("the hook's wait ran to completion")
            .expect("the hook's wait is answered")
    }
}

impl AnActivityHost {
    fn child_dir(&self) -> PathBuf {
        self.data_dir.path().join("sessions").join(CHILD)
    }

    /// The child's `AskUserQuestion` hook, once it has opened its wait for [`REQUEST_ID`] — so an
    /// answer cannot overtake it.
    async fn a_hook_waiting(&self) -> AWaitingHook {
        let service = Arc::clone(&self.service);
        let request = AwaitQuestionAnswerRequest {
            session_id: CHILD.to_string(),
            hook_token: HOOK_TOKEN.to_string(),
            os_user: OS_USER.to_string(),
            request_id: REQUEST_ID.to_string(),
            permission_suggestions_json: String::new(),
        };
        let hook = AWaitingHook {
            waiting: tokio::spawn(async move {
                service
                    .await_question_answer(Request::direct(request))
                    .await
                    .map(|response| response.into_inner())
            }),
        };
        let deadline = tokio::time::Instant::now() + THE_HOOK_OPENS_WITHIN;
        while self.pending.open_question_request(REQUEST_ID).is_none() {
            assert!(
                tokio::time::Instant::now() < deadline,
                "the hook never opened its wait for {REQUEST_ID}"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        hook
    }

    /// [`HOME`], on this host, answers the child's question with `answers`.
    fn the_home_answers(
        &self,
        answers: Vec<QuestionAnswer>,
    ) -> Result<AnsweredQuestion, AnswerRefusal> {
        answer_question(
            &AnswerContext {
                sessions_base: self.data_dir.path(),
                this_daemon_instance_id: THIS_HOST,
                pending: &self.pending,
            },
            &AnsweringCaller {
                session_id: HOME.to_string(),
                daemon_instance_id: THIS_HOST.to_string(),
            },
            CHILD,
            REQUEST_ID,
            answers,
        )
    }

    /// The child's activity record of the answer to [`REQUEST_ID`]: its raw output.
    fn recorded_answer(&self) -> Option<serde_json::Value> {
        tddy_core::agent_activity::read_agent_activity(&self.child_dir())
            .expect("the child's activity log is readable")
            .into_iter()
            .find(|record| record.call_id == format!("question:{REQUEST_ID}"))
            .map(|record| record.result)
    }

    fn pending_question(&self) -> Option<PendingQuestion> {
        read_pending_question(&self.child_dir()).expect("the pending question file is readable")
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

/// `labels` chosen for the question at `question_index`.
fn choosing(question_index: u32, labels: &[&str]) -> QuestionAnswer {
    QuestionAnswer {
        question_index,
        selected: labels.iter().map(|label| label.to_string()).collect(),
        other: None,
    }
}

/// The answers a sensible orchestrator gives the two questions.
fn rust_with_fmt_and_clippy() -> Vec<QuestionAnswer> {
    vec![choosing(0, &["Rust"]), choosing(1, &["fmt", "clippy"])]
}

// ---------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------

trait RefusalAssertions {
    /// Refused as invalid answers to [`REQUEST_ID`], whose reason mentions `needle`.
    fn assert_invalid_because_it_mentions(&self, needle: &str) -> &Self;
}

impl RefusalAssertions for Result<AnsweredQuestion, AnswerRefusal> {
    fn assert_invalid_because_it_mentions(&self, needle: &str) -> &Self {
        match self {
            Err(AnswerRefusal::InvalidAnswers { request_id, reason }) => {
                assert_eq!(request_id, REQUEST_ID, "which request was refused");
                assert!(
                    reason.contains(needle),
                    "refused, but the reason does not mention {needle:?}: {reason}"
                );
            }
            other => panic!("expected the answers to be refused as invalid, got {other:?}"),
        }
        self
    }
}

// ---------------------------------------------------------------------------
// Acceptance
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_orchestrated_childs_question_await_returns_the_home_agents_answers() {
    // Given a child whose running home started it, and whose hook waits on its two questions
    let host = a_host_whose_child_asks(the_two_questions());
    let hook = host.a_hook_waiting().await;

    // When the home answers both
    let answered = host
        .the_home_answers(rust_with_fmt_and_clippy())
        .expect("the child's own home may answer, with options it offered");

    // Then the hook is woken with those answers, naming the home
    assert_eq!(
        hook.outcome_at_once().await,
        AwaitQuestionAnswerResponse {
            outcome: AwaitQuestionOutcome::Answered as i32,
            answered_by: HOME.to_string(),
            answers: rust_with_fmt_and_clippy(),
            ..Default::default()
        },
        "how the hook's wait ended"
    );
    // And the tool-side result says who answered what, and the child waits on nothing any more
    assert_eq!(
        (
            answered.answered_by.as_str(),
            answered.request_id.as_str(),
            answered.answers
        ),
        (HOME, REQUEST_ID, rust_with_fmt_and_clippy()),
        "what the answer recorded"
    );
    assert_eq!(host.pending_question(), None);
    // And the child's activity says who answered what, as it does for a permission
    assert_eq!(
        host.recorded_answer(),
        Some(serde_json::json!({
            "answeredBy": HOME,
            "answers": [
                { "questionIndex": 0, "selected": ["Rust"] },
                { "questionIndex": 1, "selected": ["fmt", "clippy"] }
            ]
        }))
    );
}

#[tokio::test]
async fn an_unknown_question_index_an_unoffered_label_or_two_selections_on_a_single_choice_question_are_refused_and_the_waiter_keeps_waiting(
) {
    // Given a child whose hook waits on its two questions
    let host = a_host_whose_child_asks(the_two_questions());
    let hook = host.a_hook_waiting().await;

    // When its home answers a question the call does not have, a label the question does not
    // offer, and two labels on the single-choice question
    let no_such_question = host.the_home_answers(vec![choosing(2, &["Rust"])]);
    let no_such_label = host.the_home_answers(vec![choosing(0, &["Python"])]);
    let two_for_one = host.the_home_answers(vec![choosing(0, &["Rust", "Go"])]);

    // Then each is refused, saying what was wrong with it
    no_such_question.assert_invalid_because_it_mentions("2");
    no_such_label.assert_invalid_because_it_mentions("Python");
    two_for_one.assert_invalid_because_it_mentions("one");
    // And the child still waits on the same question
    assert_eq!(host.pending_question(), Some(the_two_questions()));

    // And the hook is woken by the valid answer that follows, not by any refused one
    host.the_home_answers(rust_with_fmt_and_clippy())
        .expect("the hook was still waiting for its home");
    let outcome = hook.outcome_at_once().await;
    assert_eq!(
        outcome.answers,
        rust_with_fmt_and_clippy(),
        "the answers the hook was woken with: {outcome:?}"
    );
}

#[tokio::test]
async fn answer_session_question_on_a_permission_question_is_refused_naming_answer_session_permission(
) {
    // Given a child that asks for a permission, not a question
    let host = a_host_whose_child_asks(a_permission_question());

    // When its home answers it as a question
    let refused = host.the_home_answers(vec![choosing(0, &["allow_once"])]);

    // Then it is refused as not a question, and the refusal points at the tool that answers it
    let Err(refusal @ AnswerRefusal::NotAQuestion { .. }) = refused else {
        panic!("expected a NotAQuestion refusal, got {refused:?}");
    };
    assert!(
        refusal.to_string().contains("AnswerSessionPermission"),
        "the refusal does not name the right tool: {refusal}"
    );
    // And the child still waits on its permission
    assert_eq!(host.pending_question(), Some(a_permission_question()));
}
