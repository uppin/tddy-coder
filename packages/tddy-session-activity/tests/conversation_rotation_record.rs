//! A session follows its agent through `/clear` (`#orchestrators` 6/11): the two hook reports the
//! session's agent spec declares as a conversation rotation — Claude Code's `SessionEnd` with
//! `reason: clear` for the session's current conversation, then its `SessionStart` with
//! `source: clear` — append the new conversation to `.session.yaml` `agent_conversations`, keeping
//! every one before it, newest last. A clear that ends any other conversation — a claude-cli agent
//! tab sharing the session's hooks — rotates nothing (developer decision 2026-11-13).
//!
//! PRD `docs/ft/daemon/1-WIP/PRD-2026-11-10-conversation-rotation.md`. The path under test is the
//! real `ReportSessionStatus` handler over a real session directory, with the built-in agent specs:
//! the record on disk is the behaviour.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pretty_assertions::assert_eq;
use tddy_core::agent_conversation::{AgentConversation, AgentConversationReason};
use tddy_core::session_metadata::{write_session_metadata, SessionMetadata};
use tddy_daemon_kernel::AgentActivityHub;
use tddy_rpc::{Request, Status};
use tddy_service::proto::activity::{ActivityService as _, ReportSessionStatusRequest};
use tddy_session_activity::{
    ActivityPorts, ActivityServiceImpl, NoSessionDeltaStores, SessionLabels,
};

const OS_USER: &str = "ada";
const SESSION_ID: &str = "01900000-0000-7000-8000-0000000006b1";
const HOOK_TOKEN: &str = "tok-conversation-rotation";
const STARTED_AT_UNIX_MS: u64 = 1_762_771_200_000;

/// A conversation of a claude-cli agent tab, which shares the session's hooks, and the one its
/// own `/clear` begins.
const A_TABS_CONVERSATION: &str = "7ab00000-0000-4000-8000-0000000000a1";
const A_TABS_CONVERSATION_AFTER_ITS_CLEAR: &str = "7ab00000-0000-4000-8000-0000000000a2";

/// The conversations after the first and second `/clear`.
const AFTER_THE_FIRST_CLEAR: &str = "6c1e0a00-0000-4000-8000-0000000000c1";
const AFTER_THE_SECOND_CLEAR: &str = "6c1e0a00-0000-4000-8000-0000000000c2";

// ---------------------------------------------------------------------------
// The host under test
// ---------------------------------------------------------------------------

struct NoLabels;

impl SessionLabels for NoLabels {
    fn label_for(&self, _sessions_base: &Path, _session_id: &str) -> String {
        String::new()
    }
}

/// One daemon host holding one claude-cli session, whose record says it started in a conversation
/// of its own id.
struct AnActivityHost {
    data_dir: tempfile::TempDir,
    service: ActivityServiceImpl,
}

impl AnActivityHost {
    fn holding_a_claude_cli_session() -> Self {
        Self::holding_a_session_that_lists(vec![AgentConversation {
            id: SESSION_ID.to_string(),
            started_at_unix_ms: STARTED_AT_UNIX_MS,
            reason: AgentConversationReason::Start,
        }])
    }

    /// A claude-cli session recorded before conversations were listed.
    fn holding_a_session_recorded_before_the_list() -> Self {
        Self::holding_a_session_that_lists(Vec::new())
    }

    fn holding_a_session_that_lists(conversations: Vec<AgentConversation>) -> Self {
        let data_dir = tempfile::tempdir().expect("a temporary data dir");
        write_session(data_dir.path(), conversations);
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

    /// An agent ran `/clear` in `ending`: its `SessionEnd` hook reported that conversation ended,
    /// then its `SessionStart` hook reported `beginning`.
    async fn after_a_clear(&self, ending: &str, beginning: &str) -> &Self {
        self.reported(a_clear_ending(ending))
            .await
            .reported(a_clear_beginning(beginning))
            .await
    }

    async fn reported(&self, report: ReportSessionStatusRequest) -> &Self {
        self.service
            .report_session_status(Request::direct(report))
            .await
            .expect("the hook report is accepted");
        self
    }

    fn session_dir(&self) -> PathBuf {
        self.data_dir.path().join("sessions").join(SESSION_ID)
    }

    /// The conversations `.session.yaml` lists, oldest first, with why each began.
    fn conversations(&self) -> Vec<(String, AgentConversationReason)> {
        tddy_core::read_session_metadata(&self.session_dir())
            .expect("the session's .session.yaml")
            .agent_conversations
            .into_iter()
            .map(|conversation| (conversation.id, conversation.reason))
            .collect()
    }
}

/// What `tddy-tools session-hook` sends for Claude Code's `SessionEnd` when `/clear` leaves
/// `conversation`.
fn a_clear_ending(conversation: &str) -> ReportSessionStatusRequest {
    ReportSessionStatusRequest {
        status: "Ended".to_string(),
        hook_event_name: "SessionEnd".to_string(),
        hook_fields: HashMap::from([
            ("reason".to_string(), "clear".to_string()),
            ("session_id".to_string(), conversation.to_string()),
        ]),
        ..a_clear_beginning(conversation)
    }
}

/// What `tddy-tools session-hook` sends for Claude Code's `SessionStart` right after `/clear`.
fn a_clear_beginning(conversation: &str) -> ReportSessionStatusRequest {
    ReportSessionStatusRequest {
        session_id: SESSION_ID.to_string(),
        hook_token: HOOK_TOKEN.to_string(),
        os_user: OS_USER.to_string(),
        status: "Started".to_string(),
        hook_event_name: "SessionStart".to_string(),
        agent_conversation_id: conversation.to_string(),
        hook_fields: HashMap::from([
            ("source".to_string(), "clear".to_string()),
            ("session_id".to_string(), conversation.to_string()),
        ]),
        ..Default::default()
    }
}

/// What the hook sends for a `SessionStart` that is no clear: the agent starting or resuming.
fn a_session_start_from(source: &str, conversation: &str) -> ReportSessionStatusRequest {
    ReportSessionStatusRequest {
        hook_fields: HashMap::from([
            ("source".to_string(), source.to_string()),
            ("session_id".to_string(), conversation.to_string()),
        ]),
        ..a_clear_beginning(conversation)
    }
}

fn write_session(sessions_base: &Path, conversations: Vec<AgentConversation>) {
    let session_dir = sessions_base.join("sessions").join(SESSION_ID);
    std::fs::create_dir_all(&session_dir).expect("the session dir");
    let meta = SessionMetadata {
        session_id: SESSION_ID.to_string(),
        project_id: "a-project".to_string(),
        created_at: "2026-11-10T10:00:00Z".to_string(),
        updated_at: "2026-11-10T10:00:00Z".to_string(),
        status: "active".to_string(),
        repo_path: Some(format!("/home/{OS_USER}/work")),
        pid: Some(4242),
        tool: None,
        livekit_room: None,
        pending_elicitation: false,
        previous_session_id: None,
        session_type: Some("claude-cli".to_string()),
        model: Some("opus".to_string()),
        cursor_chat_id: None,
        harness_resume_token: None,
        activity_status: Some("Running".to_string()),
        hook_token: Some(HOOK_TOKEN.to_string()),
        sandbox: None,
        agent: Some("claude".to_string()),
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
        tmux_session: None,
        orchestration_mark: None,
        agent_conversations: conversations,
    };
    write_session_metadata(&session_dir, &meta).expect("the session's .session.yaml");
}

fn started(id: &str) -> (String, AgentConversationReason) {
    (id.to_string(), AgentConversationReason::Start)
}

fn rotated(id: &str) -> (String, AgentConversationReason) {
    (id.to_string(), AgentConversationReason::Rotate)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_rotation_appends_the_new_conversation_and_keeps_every_earlier_one() {
    // Given a claude-cli session still in the conversation it started with
    let host = AnActivityHost::holding_a_claude_cli_session();

    // When its agent clears twice
    host.after_a_clear(SESSION_ID, AFTER_THE_FIRST_CLEAR)
        .await
        .after_a_clear(AFTER_THE_FIRST_CLEAR, AFTER_THE_SECOND_CLEAR)
        .await;

    // Then the record keeps all three conversations, the newest last
    assert_eq!(
        host.conversations(),
        vec![
            started(SESSION_ID),
            rotated(AFTER_THE_FIRST_CLEAR),
            rotated(AFTER_THE_SECOND_CLEAR),
        ]
    );
}

#[tokio::test]
async fn the_same_conversation_reported_twice_is_one_entry() {
    // Given a claude-cli session whose agent cleared once
    let host = AnActivityHost::holding_a_claude_cli_session();
    host.after_a_clear(SESSION_ID, AFTER_THE_FIRST_CLEAR).await;

    // When the same SessionStart is reported again (a hook retried, or fired twice)
    host.reported(a_clear_beginning(AFTER_THE_FIRST_CLEAR))
        .await;

    // Then that conversation is listed once
    assert_eq!(
        host.conversations(),
        vec![started(SESSION_ID), rotated(AFTER_THE_FIRST_CLEAR)]
    );
}

#[tokio::test]
async fn a_session_recorded_before_the_list_keeps_its_first_conversation_when_it_rotates() {
    // Given a claude-cli session whose record lists no conversation: it began in its own id
    let host = AnActivityHost::holding_a_session_recorded_before_the_list();

    // When its agent clears
    host.after_a_clear(SESSION_ID, AFTER_THE_FIRST_CLEAR).await;

    // Then the conversation it began in is kept ahead of the new one
    assert_eq!(
        host.conversations(),
        vec![started(SESSION_ID), rotated(AFTER_THE_FIRST_CLEAR)]
    );
}

#[tokio::test]
async fn a_session_start_that_is_not_a_clear_rotates_nothing() {
    // Given a claude-cli session still in the conversation it started with
    let host = AnActivityHost::holding_a_claude_cli_session();

    // When its agent is resumed, and SessionStart reports `source: resume`
    host.reported(a_session_start_from("resume", SESSION_ID))
        .await;

    // Then the record still lists the one conversation
    assert_eq!(host.conversations(), vec![started(SESSION_ID)]);
}

#[tokio::test]
async fn an_agent_tab_clear_does_not_rotate_the_sessions_conversation() {
    // Given a claude-cli session still in the conversation it started with
    let host = AnActivityHost::holding_a_claude_cli_session();

    // When an agent tab sharing its hooks clears a conversation of its own, and then the session's
    // agent clears its own
    host.after_a_clear(A_TABS_CONVERSATION, A_TABS_CONVERSATION_AFTER_ITS_CLEAR)
        .await
        .after_a_clear(SESSION_ID, AFTER_THE_FIRST_CLEAR)
        .await;

    // Then only the session's own clear is listed: the tab's rotated nothing
    assert_eq!(
        host.conversations(),
        vec![started(SESSION_ID), rotated(AFTER_THE_FIRST_CLEAR)]
    );
}
