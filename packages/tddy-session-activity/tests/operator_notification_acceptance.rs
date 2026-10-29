//! `ReportOperatorNotification`: an agent in a claude-cli or cursor-cli session notifying its
//! operator in its own words (`tddy-tools notify`).
//!
//! Feature doc `docs/ft/daemon/session-notifications.md` § Agent-written notifications (`#home-inbox` 1/8). The path
//! under test is the real handler over a real session directory, publishing onto a real bus: the
//! contract is the RPC, so the assertions are what the bus received — or that it received nothing.

use std::path::Path;
use std::sync::Arc;

use pretty_assertions::assert_eq;
use rstest::rstest;
use tddy_core::session_metadata::{write_session_metadata, SessionMetadata};
use tddy_daemon_kernel::session_notification::{AgentNotification, NotificationLevel};
use tddy_daemon_kernel::AgentActivityHub;
use tddy_rpc::{Code, Request, Status};
use tddy_service::proto::activity::{
    ActivityService as _, NotificationLevel as ProtoNotificationLevel,
    ReportOperatorNotificationRequest, ReportOperatorNotificationResponse,
};
use tddy_session_activity::session_notifications::{
    RecordingSessionNotificationSubscriber, SessionNotification, SessionNotificationBus,
    SessionNotificationKind, SessionNotificationSource,
};
use tddy_session_activity::{
    ActivityPorts, ActivityServiceImpl, NoSessionDeltaStores, SessionLabels,
};

const OS_USER: &str = "ada";
const SESSION_ID: &str = "01900000-0000-7000-8000-AABB00000201";
const HOOK_TOKEN: &str = "tok-operator-notification";
const THIS_HOST: &str = "host-a";
const LABEL: &str = "my-feature-branch";

const TITLE: &str = "Parallel run finished";
const BODY: &str = "4 done, **1 blocked**";

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

/// One daemon host holding one session, with a recorder on its notification bus.
struct AnActivityHost {
    _data_dir: tempfile::TempDir,
    service: ActivityServiceImpl,
    bus_received: Arc<RecordingSessionNotificationSubscriber>,
}

impl AnActivityHost {
    /// A host holding one session of `session_type` whose `.session.yaml` records [`HOOK_TOKEN`].
    fn holding_a_session_of_type(session_type: &str) -> Self {
        Self::holding(session_type, Some(HOOK_TOKEN))
    }

    /// A host holding one claude-cli session that recorded no hook token — one started without
    /// hooks, as a Telegram-started session is.
    fn holding_a_session_without_a_hook_token() -> Self {
        Self::holding("claude-cli", None)
    }

    fn holding(session_type: &str, hook_token: Option<&str>) -> Self {
        let data_dir = tempfile::tempdir().expect("a temporary data dir");
        write_session(data_dir.path(), session_type, hook_token);
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
            _data_dir: data_dir,
            service,
            bus_received,
        }
    }

    fn holding_a_claude_cli_session() -> Self {
        Self::holding_a_session_of_type("claude-cli")
    }

    async fn notify(
        &self,
        request: ReportOperatorNotificationRequest,
    ) -> Result<ReportOperatorNotificationResponse, Status> {
        self.service
            .report_operator_notification(Request::direct(request))
            .await
            .map(|response| response.into_inner())
    }

    fn published(&self) -> Vec<SessionNotification> {
        self.bus_received.received()
    }
}

fn write_session(sessions_base: &Path, session_type: &str, hook_token: Option<&str>) {
    let session_dir = sessions_base.join("sessions").join(SESSION_ID);
    std::fs::create_dir_all(&session_dir).expect("the session dir");
    let meta = SessionMetadata {
        session_id: SESSION_ID.to_string(),
        project_id: "a-project".to_string(),
        created_at: "2026-10-25T10:00:00Z".to_string(),
        updated_at: "2026-10-25T10:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: Some(format!("/home/{OS_USER}/{LABEL}")),
        pid: Some(4242),
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some(session_type.to_string()),
        model: None,
        cursor_chat_id: None,
        harness_resume_token: None,
        activity_status: Some("Running".to_string()),
        hook_token: hook_token.map(str::to_string),
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

/// What `tddy-tools notify` sends for the session above, with the credential it was launched with.
struct NotificationRequestBuilder {
    request: ReportOperatorNotificationRequest,
}

fn a_notification() -> NotificationRequestBuilder {
    NotificationRequestBuilder {
        request: ReportOperatorNotificationRequest {
            session_id: SESSION_ID.to_string(),
            hook_token: HOOK_TOKEN.to_string(),
            os_user: OS_USER.to_string(),
            title: TITLE.to_string(),
            body_markdown: BODY.to_string(),
            level: ProtoNotificationLevel::Success as i32,
            sound: true,
        },
    }
}

impl NotificationRequestBuilder {
    fn with_title(mut self, title: &str) -> Self {
        self.request.title = title.to_string();
        self
    }

    fn with_body(mut self, body: &str) -> Self {
        self.request.body_markdown = body.to_string();
        self
    }

    fn with_hook_token(mut self, hook_token: &str) -> Self {
        self.request.hook_token = hook_token.to_string();
        self
    }

    fn build(self) -> ReportOperatorNotificationRequest {
        self.request
    }
}

/// The notification the bus should carry for [`a_notification`], stamped at `at_unix_ms`.
fn the_agents_notification_at(at_unix_ms: u64) -> SessionNotification {
    SessionNotification {
        session_id: SESSION_ID.to_string(),
        os_user: OS_USER.to_string(),
        label: LABEL.to_string(),
        kind: SessionNotificationKind::AttentionRequired,
        source: SessionNotificationSource::Agent,
        text: TITLE.to_string(),
        agent: Some(AgentNotification {
            title: TITLE.to_string(),
            body_markdown: BODY.to_string(),
            level: NotificationLevel::Success,
            sound: true,
            link: format!("#/sessions/{SESSION_ID}?host={THIS_HOST}"),
        }),
        at_unix_ms,
    }
}

// ---------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------

trait PublishedAssertions {
    fn assert_exactly_one(&self) -> &SessionNotification;
    fn assert_nothing(&self) -> &Self;
}

impl PublishedAssertions for Vec<SessionNotification> {
    fn assert_exactly_one(&self) -> &SessionNotification {
        assert_eq!(
            self.len(),
            1,
            "expected exactly one notification on the bus, got {self:#?}"
        );
        &self[0]
    }

    fn assert_nothing(&self) -> &Self {
        assert!(
            self.is_empty(),
            "a refused notification must publish nothing, got {self:#?}"
        );
        self
    }
}

trait RefusalAssertions {
    fn assert_refused_as(&self, code: Code) -> &Status;
}

impl RefusalAssertions for Result<ReportOperatorNotificationResponse, Status> {
    fn assert_refused_as(&self, code: Code) -> &Status {
        match self {
            Err(status) => {
                assert_eq!(status.code(), code, "refused, but as {status:?}");
                status
            }
            Ok(response) => panic!("expected a {code:?} refusal, the call succeeded: {response:?}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Acceptance
// ---------------------------------------------------------------------------

#[rstest]
#[case::claude_cli("claude-cli")]
#[case::cursor_cli("cursor-cli")]
#[tokio::test]
async fn an_agent_notification_from_a_claude_cli_or_cursor_cli_session_is_published_with_its_title_body_level_sound_and_link(
    #[case] session_type: &str,
) {
    // Given a session of a type whose agent is launched with the notify environment
    let host = AnActivityHost::holding_a_session_of_type(session_type);

    // When its agent notifies its operator, presenting the session's own hook token
    let response = host.notify(a_notification().build()).await;

    // Then the call succeeds
    assert_eq!(
        response.expect("the notification is accepted"),
        ReportOperatorNotificationResponse { ok: true }
    );
    // And the bus carries one attention notification in the agent's words, linking to the session
    let published = host.published();
    let notification = published.assert_exactly_one();
    // The stamp is the moment of publishing, taken from the clock — only its presence is fixed.
    assert!(
        notification.at_unix_ms > 0,
        "the notification is stamped with the moment it happened"
    );
    assert_eq!(
        notification,
        &the_agents_notification_at(notification.at_unix_ms)
    );
}

#[rstest]
#[case::a_title_that_is_only_whitespace("   \t ", BODY, "title")]
#[case::a_title_of_201_characters(&"t".repeat(201), BODY, "title")]
#[case::a_body_of_2001_characters(TITLE, &"b".repeat(2001), "body")]
#[tokio::test]
async fn a_title_empty_after_trimming_or_over_200_chars_or_a_body_over_2000_chars_is_refused_and_publishes_nothing(
    #[case] title: &str,
    #[case] body: &str,
    #[case] the_part_named: &str,
) {
    // Given a claude-cli session on this host
    let host = AnActivityHost::holding_a_claude_cli_session();

    // When its agent sends a notification outside the limits
    let response = host
        .notify(a_notification().with_title(title).with_body(body).build())
        .await;

    // Then it is refused as an invalid argument
    let refusal = response.assert_refused_as(Code::InvalidArgument);
    // The wording is the implementation's; what is fixed is that it names what was too long, so
    // the agent that reads it knows what to shorten.
    assert!(
        refusal.message().contains(the_part_named),
        "the refusal names the {the_part_named}: {:?}",
        refusal.message()
    );
    // And nothing reached the bus — a limit is a refusal, never a truncation
    host.published().assert_nothing();
}

#[tokio::test]
async fn a_wrong_hook_token_is_refused_as_permission_denied() {
    // Given a claude-cli session on this host
    let host = AnActivityHost::holding_a_claude_cli_session();

    // When a notification presents a token that is not the session's
    let response = host
        .notify(
            a_notification()
                .with_hook_token("not-this-sessions-token")
                .build(),
        )
        .await;

    // Then it is refused as permission denied, and nothing reached the bus
    response.assert_refused_as(Code::PermissionDenied);
    host.published().assert_nothing();
}

#[tokio::test]
async fn a_session_that_is_not_claude_cli_or_cursor_cli_is_refused() {
    // Given a session of a type that has no hooks, and so no agent launched with a hook token
    let host = AnActivityHost::holding_a_session_of_type("workspace");

    // When a notification names it, presenting the token its record holds
    let response = host.notify(a_notification().build()).await;

    // Then it is refused before the credential is considered, and nothing reached the bus
    response.assert_refused_as(Code::FailedPrecondition);
    host.published().assert_nothing();
}

#[tokio::test]
async fn a_session_that_recorded_no_hook_token_refuses_even_an_empty_one() {
    // Given a claude-cli session that recorded no hook token
    let host = AnActivityHost::holding_a_session_without_a_hook_token();

    // When a notification presents an empty token, which a missing record would otherwise equal
    let response = host
        .notify(a_notification().with_hook_token("").build())
        .await;

    // Then it is refused as permission denied, and nothing reached the bus
    response.assert_refused_as(Code::PermissionDenied);
    host.published().assert_nothing();
}
