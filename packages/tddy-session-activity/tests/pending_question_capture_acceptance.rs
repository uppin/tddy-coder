//! What a claude-cli session is asking: the `PermissionRequest` and `AskUserQuestion` hooks report
//! `WaitingForInput` with a tool and its input, and the daemon records, keeps, publishes and clears
//! the question (`#home-inbox` 3/8).
//!
//! PRD `docs/ft/daemon/1-WIP/PRD-2026-10-25-elicitation-content.md`. The path under test is the
//! real `ReportSessionStatus` handler over a real session directory, publishing onto a real bus:
//! the record on disk, the session's `pending_elicitation` flag and what the bus received are the
//! behaviour.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use pretty_assertions::assert_eq;
use rstest::rstest;
use tddy_core::pending_question::{
    read_pending_question, PendingQuestion, PendingQuestionItem, PendingQuestionOption,
};
use tddy_core::session_metadata::{write_session_metadata, SessionMetadata};
use tddy_daemon_kernel::session_notification::{
    QuestionNotice, QuestionNoticeItem, QuestionNoticeOption,
};
use tddy_daemon_kernel::AgentActivityHub;
use tddy_rpc::{Code, Request, Status};
use tddy_service::proto::activity::{
    ActivityService as _, ReportSessionStatusRequest, ReportSessionStatusResponse,
};
use tddy_session_activity::session_notifications::{
    RecordingSessionNotificationSubscriber, SessionNotification, SessionNotificationBus,
    SessionNotificationKind, SessionNotificationSource,
};
use tddy_session_activity::{
    ActivityPorts, ActivityServiceImpl, NoSessionDeltaStores, SessionLabels,
};

const OS_USER: &str = "ada";
const SESSION_ID: &str = "01900000-0000-7000-8000-AABB00000501";
const HOOK_TOKEN: &str = "tok-pending-question";
const THIS_HOST: &str = "host-a";
const LABEL: &str = "my-feature-branch";

const A_COMMAND: &str = r#"{"command":"cargo publish"}"#;
const A_PERMISSION_MESSAGE: &str = "Claude needs your permission to use Bash";

// ---------------------------------------------------------------------------
// The host under test
// ---------------------------------------------------------------------------

/// Every session on this host is shown under [`LABEL`], so a notification's label is a fixed value
/// the assertions can name.
struct EverySessionIsLabelled;

impl SessionLabels for EverySessionIsLabelled {
    fn label_for(&self, _sessions_base: &Path, _session_id: &str) -> String {
        LABEL.to_string()
    }
}

/// One daemon host holding one claude-cli session, with a recorder on its notification bus.
struct AnActivityHost {
    data_dir: tempfile::TempDir,
    service: ActivityServiceImpl,
    bus_received: Arc<RecordingSessionNotificationSubscriber>,
}

impl AnActivityHost {
    /// A host holding one claude-cli session whose `.session.yaml` records [`HOOK_TOKEN`], that
    /// waits on nothing.
    fn holding_a_claude_cli_session() -> Self {
        let data_dir = tempfile::tempdir().expect("a temporary data dir");
        write_session(data_dir.path());
        let bus_received = Arc::new(RecordingSessionNotificationSubscriber::new());
        let bus = SessionNotificationBus::new().with_subscriber(Arc::clone(&bus_received));
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
            notifications: Some(Arc::new(bus)),
            session_labels: Arc::new(EverySessionIsLabelled),
            deltas: Arc::new(NoSessionDeltaStores),
            daemon_instance_id: THIS_HOST.to_string(),
        });
        Self {
            data_dir,
            service,
            bus_received,
        }
    }

    /// [`Self::holding_a_claude_cli_session`], whose agent has asked for a permission to run
    /// `cargo publish` — and the daemon recorded it. Returns the host and the id it recorded.
    async fn waiting_on_a_permission() -> (Self, String) {
        let host = Self::holding_a_claude_cli_session();
        let response = host
            .report(a_report().of_a_permission_request())
            .await
            .expect("the permission request is accepted");
        host.pending_question()
            .assert_is_the_question_with_id(&response.request_id);
        assert!(host.pending_elicitation(), "the session is waiting");
        (host, response.request_id)
    }

    async fn report(
        &self,
        request: ReportSessionStatusRequest,
    ) -> Result<ReportSessionStatusResponse, Status> {
        self.service
            .report_session_status(Request::direct(request))
            .await
            .map(|response| response.into_inner())
    }

    fn session_dir(&self) -> PathBuf {
        self.data_dir.path().join("sessions").join(SESSION_ID)
    }

    /// The question the session's directory says it waits on.
    fn pending_question(&self) -> Option<PendingQuestion> {
        read_pending_question(&self.session_dir()).expect("the pending question file is readable")
    }

    /// The `pending_elicitation` flag in the session's `.session.yaml`.
    fn pending_elicitation(&self) -> bool {
        tddy_core::read_session_metadata(&self.session_dir())
            .expect("the session's .session.yaml")
            .pending_elicitation
    }

    fn published(&self) -> Vec<SessionNotification> {
        self.bus_received.received()
    }
}

fn write_session(sessions_base: &Path) {
    let session_dir = sessions_base.join("sessions").join(SESSION_ID);
    std::fs::create_dir_all(&session_dir).expect("the session dir");
    let meta = SessionMetadata {
        session_id: SESSION_ID.to_string(),
        project_id: "a-project".to_string(),
        created_at: "2026-10-26T10:00:00Z".to_string(),
        updated_at: "2026-10-26T10:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: Some(format!("/home/{OS_USER}/{LABEL}")),
        pid: Some(4242),
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("claude-cli".to_string()),
        model: None,
        cursor_chat_id: None,
        harness_resume_token: None,
        activity_status: Some("Running".to_string()),
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
    write_session_metadata(&session_dir, &meta).expect("the session's .session.yaml");
}

// ---------------------------------------------------------------------------
// Builders
// ---------------------------------------------------------------------------

/// What `tddy-tools session-hook` sends for the session above, with the credential it was launched
/// with. Bare, it reports a status with no question behind it.
struct ReportBuilder {
    request: ReportSessionStatusRequest,
}

fn a_report() -> ReportBuilder {
    ReportBuilder {
        request: ReportSessionStatusRequest {
            session_id: SESSION_ID.to_string(),
            hook_token: HOOK_TOKEN.to_string(),
            os_user: OS_USER.to_string(),
            status: "Running".to_string(),
            ..Default::default()
        },
    }
}

impl ReportBuilder {
    fn with_status(mut self, status: &str) -> Self {
        self.request.status = status.to_string();
        self
    }

    fn with_hook_token(mut self, hook_token: &str) -> Self {
        self.request.hook_token = hook_token.to_string();
        self
    }

    /// The `PermissionRequest` hook: Claude asks to run `cargo publish`.
    fn of_a_permission_request(mut self) -> ReportSessionStatusRequest {
        self.request.status = "WaitingForInput".to_string();
        self.request.hook_event_name = "PermissionRequest".to_string();
        self.request.tool_name = "Bash".to_string();
        self.request.tool_input_json = A_COMMAND.to_string();
        self.request
    }

    /// The `PreToolUse` hook of `AskUserQuestion`: two questions, the second multi-select.
    fn of_an_ask_user_question(mut self) -> ReportSessionStatusRequest {
        self.request.status = "WaitingForInput".to_string();
        self.request.hook_event_name = "PreToolUse".to_string();
        self.request.tool_name = "AskUserQuestion".to_string();
        self.request.tool_input_json = TWO_QUESTIONS_INPUT.to_string();
        self.request
    }

    /// The `Notification` hook that follows a prompt: the agent's message and nothing structured.
    fn of_the_notification_that_follows(mut self) -> ReportSessionStatusRequest {
        self.request.status = "WaitingForInput".to_string();
        self.request.hook_event_name = "Notification".to_string();
        self.request.message = A_PERMISSION_MESSAGE.to_string();
        self.request
    }

    fn build(self) -> ReportSessionStatusRequest {
        self.request
    }
}

/// `AskUserQuestion`'s `tool_input`, as Claude Code sends it.
const TWO_QUESTIONS_INPUT: &str = r#"{"questions":[
    {"question":"Which language should the service use?","header":"Language","multiSelect":false,
     "options":[{"label":"Rust","description":"Memory safe, fast"},{"label":"Go","description":"Simple to deploy"}]},
    {"question":"Which targets should it build for?","header":"Targets","multiSelect":true,
     "options":[{"label":"linux","description":"x86_64"},{"label":"macos","description":"aarch64"},{"label":"windows","description":"x86_64"}]}
]}"#;

/// The record the daemon keeps for the permission request of [`ReportBuilder::of_a_permission_request`].
fn the_permission_question(request_id: &str, at_unix_ms: u64) -> PendingQuestion {
    PendingQuestion {
        kind: "permission".to_string(),
        header: String::new(),
        question: "Run Bash: cargo publish".to_string(),
        options: Vec::new(),
        allow_other: false,
        request_id: request_id.to_string(),
        tool_name: "Bash".to_string(),
        tool_input_json: A_COMMAND.to_string(),
        questions: Vec::new(),
        at_unix_ms,
    }
}

/// The record the daemon keeps for [`TWO_QUESTIONS_INPUT`]: the flat fields mirror the first
/// question, and every question is kept whole.
fn the_two_questions(request_id: &str, at_unix_ms: u64) -> PendingQuestion {
    PendingQuestion {
        kind: "select".to_string(),
        header: "Language".to_string(),
        question: "Which language should the service use?".to_string(),
        options: vec!["Rust".to_string(), "Go".to_string()],
        allow_other: true,
        request_id: request_id.to_string(),
        tool_name: "AskUserQuestion".to_string(),
        tool_input_json: TWO_QUESTIONS_INPUT.to_string(),
        questions: vec![
            PendingQuestionItem {
                header: "Language".to_string(),
                question: "Which language should the service use?".to_string(),
                options: vec![
                    an_option("Rust", "Memory safe, fast"),
                    an_option("Go", "Simple to deploy"),
                ],
                multi_select: false,
            },
            PendingQuestionItem {
                header: "Targets".to_string(),
                question: "Which targets should it build for?".to_string(),
                options: vec![
                    an_option("linux", "x86_64"),
                    an_option("macos", "aarch64"),
                    an_option("windows", "x86_64"),
                ],
                multi_select: true,
            },
        ],
        at_unix_ms,
    }
}

fn an_option(label: &str, description: &str) -> PendingQuestionOption {
    PendingQuestionOption {
        label: label.to_string(),
        description: description.to_string(),
    }
}

fn a_notice_option(label: &str, description: &str) -> QuestionNoticeOption {
    QuestionNoticeOption {
        label: label.to_string(),
        description: description.to_string(),
    }
}

/// The notice a notification carries for [`the_permission_question`].
fn the_permission_notice(request_id: &str) -> QuestionNotice {
    QuestionNotice {
        request_id: request_id.to_string(),
        kind: "permission".to_string(),
        tool_name: "Bash".to_string(),
        summary: "cargo publish".to_string(),
        questions: Vec::new(),
    }
}

/// The notice a notification carries for [`the_two_questions`].
fn the_two_questions_notice(request_id: &str) -> QuestionNotice {
    QuestionNotice {
        request_id: request_id.to_string(),
        kind: "select".to_string(),
        tool_name: "AskUserQuestion".to_string(),
        summary: "Which language should the service use?".to_string(),
        questions: vec![
            QuestionNoticeItem {
                header: "Language".to_string(),
                question: "Which language should the service use?".to_string(),
                options: vec![
                    a_notice_option("Rust", "Memory safe, fast"),
                    a_notice_option("Go", "Simple to deploy"),
                ],
                multi_select: false,
            },
            QuestionNoticeItem {
                header: "Targets".to_string(),
                question: "Which targets should it build for?".to_string(),
                options: vec![
                    a_notice_option("linux", "x86_64"),
                    a_notice_option("macos", "aarch64"),
                    a_notice_option("windows", "x86_64"),
                ],
                multi_select: true,
            },
        ],
    }
}

// ---------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------

trait PendingQuestionAssertions {
    /// The session waits on a question, recorded under `request_id`.
    fn assert_is_the_question_with_id(&self, request_id: &str) -> &PendingQuestion;
    fn assert_nothing(&self);
}

impl PendingQuestionAssertions for Option<PendingQuestion> {
    fn assert_is_the_question_with_id(&self, request_id: &str) -> &PendingQuestion {
        assert!(!request_id.is_empty(), "the report returned no request id");
        let Some(question) = self else {
            panic!("the session waits on no question; expected one with id {request_id}");
        };
        assert_eq!(question.request_id, request_id);
        // Stamped from the clock: only its presence is fixed.
        assert!(
            question.at_unix_ms > 0,
            "the question is stamped with the moment it was recorded"
        );
        question
    }

    fn assert_nothing(&self) {
        assert_eq!(self, &None, "the session must wait on no question");
    }
}

trait PublishedAssertions {
    /// The notifications' `(source, request id of the question they carry)`, in publish order.
    fn sources_and_question_ids(&self) -> Vec<(SessionNotificationSource, Option<String>)>;
}

impl PublishedAssertions for Vec<SessionNotification> {
    fn sources_and_question_ids(&self) -> Vec<(SessionNotificationSource, Option<String>)> {
        self.iter()
            .map(|n| {
                (
                    n.source,
                    n.question
                        .as_ref()
                        .map(|question| question.request_id.clone()),
                )
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Acceptance
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_permission_request_records_the_tool_and_its_input_as_the_pending_question_and_publishes_it_once(
) {
    // Given a claude-cli session that waits on nothing
    let host = AnActivityHost::holding_a_claude_cli_session();

    // When Claude asks for a permission to run `cargo publish`
    let response = host
        .report(a_report().of_a_permission_request())
        .await
        .expect("the permission request is accepted");

    // Then the report names the question it recorded
    let question = host.pending_question();
    let recorded = question.assert_is_the_question_with_id(&response.request_id);
    // And the record holds the tool, its input and the sentence an operator reads
    assert_eq!(
        recorded,
        &the_permission_question(&response.request_id, recorded.at_unix_ms)
    );
    // And the session says it is waiting
    assert!(host.pending_elicitation());
    // And the bus carries one attention notification with the question, and nothing else
    let published = host.published();
    assert_eq!(
        published.len(),
        1,
        "one notification per request, got {published:#?}"
    );
    let notification = &published[0];
    assert_eq!(notification.session_id, SESSION_ID);
    assert_eq!(notification.os_user, OS_USER);
    assert_eq!(notification.label, LABEL);
    assert_eq!(
        notification.kind,
        SessionNotificationKind::AttentionRequired
    );
    assert_eq!(
        notification.source,
        SessionNotificationSource::PendingQuestion
    );
    assert_eq!(
        notification.question,
        Some(the_permission_notice(&response.request_id))
    );
}

#[tokio::test]
async fn an_ask_user_question_pre_tool_use_records_every_question_and_option() {
    // Given a claude-cli session that waits on nothing
    let host = AnActivityHost::holding_a_claude_cli_session();

    // When Claude is about to ask two questions, the second of them multi-select
    let response = host
        .report(a_report().of_an_ask_user_question())
        .await
        .expect("the question is accepted");

    // Then the record holds both questions with every option, and the flat fields mirror the first
    let question = host.pending_question();
    let recorded = question.assert_is_the_question_with_id(&response.request_id);
    assert_eq!(
        recorded,
        &the_two_questions(&response.request_id, recorded.at_unix_ms)
    );
    // And the notification carries the same questions
    let published = host.published();
    assert_eq!(
        published
            .iter()
            .map(|n| n.question.clone())
            .collect::<Vec<_>>(),
        vec![Some(the_two_questions_notice(&response.request_id))]
    );
}

#[tokio::test]
async fn the_notification_that_follows_keeps_the_structured_question() {
    // Given a session whose permission request was recorded
    let (host, request_id) = AnActivityHost::waiting_on_a_permission().await;
    let before = host.pending_question();

    // When the Notification hook of the same prompt reports the agent's message
    host.report(a_report().of_the_notification_that_follows())
        .await
        .expect("the notification is accepted");

    // Then the record is the one it was — the message replaced nothing
    assert_eq!(host.pending_question(), before);
    assert!(host.pending_elicitation());
    // And the question was published once: the follow-up raises the status notification, and that
    // one carries the question it belongs to, so a surface can tell it is the same prompt
    assert_eq!(
        host.published().sources_and_question_ids(),
        vec![
            (
                SessionNotificationSource::PendingQuestion,
                Some(request_id.clone())
            ),
            (SessionNotificationSource::ActivityStatus, Some(request_id)),
        ]
    );
}

#[rstest]
#[case::post_tool_use("ExecutingTool")]
#[case::user_prompt_submit("Running")]
#[case::stop("Done")]
#[case::session_end("Ended")]
#[tokio::test]
async fn post_tool_use_user_prompt_submit_stop_and_session_end_each_clear_it_and_reset_pending_elicitation(
    #[case] status: &str,
) {
    // Given a session waiting on a recorded permission request
    let (host, _request_id) = AnActivityHost::waiting_on_a_permission().await;

    // When the session reports a status that is not waiting
    host.report(a_report().with_status(status).build())
        .await
        .expect("the status is accepted");

    // Then it waits on no question, and says so
    host.pending_question().assert_nothing();
    assert!(!host.pending_elicitation());
}

#[tokio::test]
async fn a_wrong_hook_token_is_refused_and_records_nothing() {
    // Given a session waiting on a recorded permission request
    let (host, request_id) = AnActivityHost::waiting_on_a_permission().await;
    let before = host.pending_question();

    // When another permission request presents a token that is not the session's
    let response = host
        .report(
            a_report()
                .with_hook_token("not-this-sessions-token")
                .of_a_permission_request(),
        )
        .await;

    // Then it is refused as permission denied
    let refusal = response.expect_err("a wrong token must be refused");
    assert_eq!(refusal.code(), Code::PermissionDenied);
    // And the record is the one it was, and nothing more reached the bus
    assert_eq!(host.pending_question(), before);
    assert_eq!(
        host.published().sources_and_question_ids(),
        vec![(SessionNotificationSource::PendingQuestion, Some(request_id))]
    );
}
