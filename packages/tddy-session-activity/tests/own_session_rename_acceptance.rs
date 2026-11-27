//! `RenameOwnSession`: an agent in a claude-cli or cursor-cli session naming the session it runs in
//! (`tddy-tools rename-session`), authenticated by the session's hook token exactly as its
//! notifications are.
//!
//! Feature doc: docs/ft/web/session-drawer.md § Session names. The path under test is the real
//! handler over a real session directory: the assertions are what the session's record holds.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use pretty_assertions::assert_eq;
use tddy_daemon_kernel::AgentActivityHub;
use tddy_rpc::{Code, Request, Status};
use tddy_service::proto::activity::{ActivityService as _, RenameOwnSessionRequest};
use tddy_session_activity::{
    ActivityPorts, ActivityServiceImpl, NoSessionDeltaStores, SessionLabels,
};

const OS_USER: &str = "ada";
const SESSION_ID: &str = "01900000-0000-7000-8000-AABB00000301";
const HOOK_TOKEN: &str = "tok-own-session-name";
const NAME: &str = "Fix the login flow";

struct NoLabels;

impl SessionLabels for NoLabels {
    fn label_for(&self, _: &Path, session_id: &str) -> String {
        session_id.to_string()
    }
}

/// One daemon host holding one session of a given type, recording a given hook token.
struct AnActivityHost {
    data_dir: tempfile::TempDir,
    service: ActivityServiceImpl,
}

impl AnActivityHost {
    fn holding(session_type: &str, hook_token: Option<&str>) -> Self {
        let data_dir = tempfile::tempdir().expect("a temporary data dir");
        let session_dir = data_dir.path().join("sessions").join(SESSION_ID);
        std::fs::create_dir_all(&session_dir).unwrap();
        let hook_token_line = hook_token
            .map(|token| format!("hook_token: {token}\n"))
            .unwrap_or_default();
        std::fs::write(
            session_dir.join(".session.yaml"),
            format!(
                "session_id: {SESSION_ID}\nproject_id: a-project\ncreated_at: \"2026-11-10T10:00:00Z\"\n\
                 updated_at: \"2026-11-10T10:00:00Z\"\nstatus: active\nsession_type: {session_type}\n\
                 repo_path: /home/{OS_USER}/my-feature-branch\n{hook_token_line}"
            ),
        )
        .unwrap();
        let service = ActivityServiceImpl::new(ActivityPorts {
            fs: Arc::new(tddy_fs::DiskFs),
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
            session_labels: Arc::new(NoLabels),
            deltas: Arc::new(NoSessionDeltaStores),
            daemon_instance_id: "host-a".to_string(),
            usage_bindings: Arc::new(|_spec: &str| None),
        })
        .with_agents(Arc::new(
            tddy_coding_agent_specs::CodingAgentRegistry::builtin()
                .expect("the built-in specs form a registry"),
        ));
        Self { data_dir, service }
    }

    fn holding_a_claude_cli_session() -> Self {
        Self::holding("claude-cli", Some(HOOK_TOKEN))
    }

    fn session_dir(&self) -> PathBuf {
        self.data_dir.path().join("sessions").join(SESSION_ID)
    }

    fn recorded_name(&self) -> Option<String> {
        tddy_core::read_session_metadata(&self.session_dir())
            .unwrap()
            .display_name
    }

    async fn renamed_by_its_agent(&self, hook_token: &str, name: &str) -> Result<String, Status> {
        self.service
            .rename_own_session(Request::direct(RenameOwnSessionRequest {
                session_id: SESSION_ID.to_string(),
                hook_token: hook_token.to_string(),
                os_user: OS_USER.to_string(),
                display_name: name.to_string(),
            }))
            .await
            .map(|response| response.into_inner().display_name)
    }
}

#[tokio::test]
async fn an_agent_names_its_own_session() {
    // Given a claude-cli session
    let host = AnActivityHost::holding_a_claude_cli_session();

    // When its agent names it, presenting the session's hook token
    let answered = host
        .renamed_by_its_agent(HOOK_TOKEN, &format!("  {NAME} "))
        .await
        .expect("the agent's rename is accepted");

    // Then the trimmed name is answered and recorded
    assert_eq!(
        (answered, host.recorded_name()),
        (NAME.to_string(), Some(NAME.to_string()))
    );
}

#[tokio::test]
async fn an_agent_clears_its_sessions_name_with_an_empty_one() {
    // Given a named session
    let host = AnActivityHost::holding_a_claude_cli_session();
    host.renamed_by_its_agent(HOOK_TOKEN, NAME).await.unwrap();

    // When its agent names it with nothing
    let answered = host.renamed_by_its_agent(HOOK_TOKEN, "").await.unwrap();

    // Then it has no name
    assert_eq!((answered, host.recorded_name()), (String::new(), None));
}

#[tokio::test]
async fn a_wrong_hook_token_names_nothing() {
    // Given
    let host = AnActivityHost::holding_a_claude_cli_session();

    // When a caller presents another token
    let refused = host
        .renamed_by_its_agent("not-the-hook-token", NAME)
        .await
        .expect_err("a wrong token is refused");

    // Then it is refused, and the session is unnamed
    assert_eq!(refused.code, Code::PermissionDenied, "{refused:?}");
    assert_eq!(host.recorded_name(), None);
}

#[tokio::test]
async fn a_session_that_recorded_no_hook_token_cannot_be_named_this_way() {
    // Given a session started without hooks
    let host = AnActivityHost::holding("claude-cli", None);

    // When a caller presents an empty token
    let refused = host
        .renamed_by_its_agent("", NAME)
        .await
        .expect_err("an empty token matches nothing");

    // Then it is refused
    assert_eq!(refused.code, Code::PermissionDenied, "{refused:?}");
    assert_eq!(host.recorded_name(), None);
}

#[tokio::test]
async fn a_session_whose_agent_does_not_notify_is_refused_before_its_token_is_read() {
    // Given a workspace session, which runs no agent
    let host = AnActivityHost::holding("workspace", Some(HOOK_TOKEN));

    // When a caller names it with the right token
    let refused = host
        .renamed_by_its_agent(HOOK_TOKEN, NAME)
        .await
        .expect_err("a session with no notifying agent is refused");

    // Then it is a failed precondition naming the session type
    assert_eq!(refused.code, Code::FailedPrecondition, "{refused:?}");
    assert!(refused.message.contains("workspace"), "{}", refused.message);
}

#[tokio::test]
async fn an_agent_is_refused_a_name_over_the_limit() {
    // Given
    let host = AnActivityHost::holding_a_claude_cli_session();

    // When its agent names it with more than eighty characters
    let refused = host
        .renamed_by_its_agent(HOOK_TOKEN, &"x".repeat(81))
        .await
        .expect_err("a name over the limit is refused");

    // Then it is an invalid argument naming the limit
    assert_eq!(refused.code, Code::InvalidArgument, "{refused:?}");
    assert!(refused.message.contains("80"), "{}", refused.message);
    assert_eq!(host.recorded_name(), None);
}
