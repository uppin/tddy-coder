//! Acceptance: a session with a standing role — the project-home main, which has no shell to run
//! `tddy-tools notify` in — notifies its operator with the `NotifyOperator` exec tool, and a session
//! without the role is not offered it (`#home-inbox` 11/11).
//!
//! The call is the real exec-tool call the main's agent makes through `tddy-tools`; what it reached
//! is read off the daemon's own notification bus, where Telegram and the desktop subscribe. It is
//! the notification `tddy-tools notify` publishes (`#home-inbox` 1/8): one `AGENT` attention
//! notification in the agent's words.
//!
//! Feature doc: docs/ft/daemon/session-notifications.md § Agent-written notifications.
//! Changeset: docs/dev/changesets/2026-10-29-home-inbox-sweep.md

mod support;

use std::sync::Arc;

use support::{
    a_host_recording_notifications, a_workspace_in, Host, InProcessPeers, HOST_A, THIS_PROJECT,
};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon_kernel::session_notification::{AgentNotification, NotificationLevel};
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest, ListExecToolsRequest};
use tddy_session_activity::session_notifications::{
    RecordingSessionNotificationSubscriber, SessionNotification, SessionNotificationKind,
    SessionNotificationSource,
};
use tddy_session_lifecycle::test_util::TEST_TOKEN;

const NOTIFY_OPERATOR: &str = "NotifyOperator";
const HOOK_TOKEN: &str = "tok-home-main-hook";

struct World {
    a: Host,
    bus: Arc<RecordingSessionNotificationSubscriber>,
    home: String,
    ordinary: String,
    /// A project-home session that is not claude-cli: it has no hooks, so no hook token.
    home_without_hooks: String,
}

/// One host holding the project's main — a claude-cli session with the project-home role, its hooks
/// installed as every claude-cli start installs them — and an ordinary claude-cli session, with a
/// recorder on the daemon's notification bus.
async fn a_world() -> World {
    let peers = Arc::new(InProcessPeers::default());
    let (a, bus) = a_host_recording_notifications(HOST_A, &peers);
    let home = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_claude_cli(&home);
    a.make_project_home(&home);
    let ordinary = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_claude_cli(&ordinary);
    let home_without_hooks = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&home_without_hooks);
    World {
        a,
        bus,
        home,
        ordinary,
        home_without_hooks,
    }
}

impl Host {
    /// Make `session_id` a claude-cli session whose hooks authenticate with [`HOOK_TOKEN`].
    fn make_claude_cli(&self, session_id: &str) {
        let dir = unified_session_dir_path(self.sessions.path(), session_id);
        let mut meta = tddy_core::read_session_metadata(&dir).unwrap();
        meta.session_type = Some("claude-cli".to_string());
        meta.hook_token = Some(HOOK_TOKEN.to_string());
        tddy_core::session_metadata::write_session_metadata(&dir, &meta).unwrap();
    }
}

/// What came back from a tool call as the agent sees it. A call the daemon refuses outright is a
/// refusal here too, so a tool it does not serve reads as one.
struct Answer {
    refused: bool,
    text: String,
}

impl Answer {
    fn assert_answered(&self) -> &Self {
        assert!(
            !self.refused,
            "expected an answer, was refused: {}",
            self.text
        );
        self
    }

    fn assert_refused(&self) -> &Self {
        assert!(self.refused, "expected a refusal, got {}", self.text);
        self
    }

    fn assert_refused_saying(&self, reason: &str) -> &Self {
        self.assert_refused();
        assert!(
            self.text.contains(reason),
            "refused, but not for {reason:?}: {}",
            self.text
        );
        self
    }

    fn assert_not_saying(&self, said: &str) -> &Self {
        assert!(!self.text.contains(said), "said {said:?}: {}", self.text);
        self
    }
}

impl World {
    /// `NotifyOperator` with `args`, called by `caller`'s agent.
    async fn notifies(&self, caller: &str, args: serde_json::Value) -> Answer {
        let result = self
            .a
            .daemon
            .execute_tool(Request::direct(ExecuteToolRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: caller.to_string(),
                tool_name: NOTIFY_OPERATOR.to_string(),
                args_json: args.to_string(),
                daemon_instance_id: String::new(),
                conversation_id: String::new(),
            }))
            .await;
        match result {
            Ok(response) => {
                let response = response.into_inner();
                Answer {
                    refused: response.is_error,
                    text: if response.is_error {
                        response.error_message
                    } else {
                        response.result_json
                    },
                }
            }
            Err(status) => Answer {
                refused: true,
                text: format!("{:?}: {}", status.code(), status.message()),
            },
        }
    }

    /// The exec tools `session_id`'s agent is offered.
    async fn tools_offered_to(&self, session_id: &str) -> Vec<String> {
        self.a
            .daemon
            .list_exec_tools(Request::direct(ListExecToolsRequest {
                session_token: TEST_TOKEN.to_string(),
                daemon_instance_id: String::new(),
                session_id: session_id.to_string(),
            }))
            .await
            .expect("the session's tools are listed")
            .into_inner()
            .tools
            .into_iter()
            .map(|tool| tool.name)
            .collect()
    }

    /// What the daemon's notification bus carried, the agent's notifications alone.
    fn agent_notifications(&self) -> Vec<SessionNotification> {
        self.bus
            .received()
            .into_iter()
            .filter(|notification| notification.source == SessionNotificationSource::Agent)
            .collect()
    }
}

#[tokio::test]
async fn a_project_home_mains_notify_operator_publishes_one_agent_attention_notification() {
    // Given the project's main, with news for an operator who is away
    let world = a_world().await;

    // When its agent notifies them
    world
        .notifies(
            &world.home,
            serde_json::json!({
                "title": "Stack green",
                "body_markdown": "**3 of 4** nodes merged; `n4` waits on review",
                "level": "success",
                "sound": true,
            }),
        )
        .await
        .assert_answered()
        .assert_not_saying(HOOK_TOKEN);

    // Then the bus carries one attention notification in the agent's words, linking to the main
    let published = world.agent_notifications();
    assert_eq!(
        published.len(),
        1,
        "agent notifications on the bus: {published:#?}"
    );
    let notification = &published[0];
    assert_eq!(
        (
            notification.session_id.as_str(),
            notification.kind,
            notification.text.as_str(),
        ),
        (
            world.home.as_str(),
            SessionNotificationKind::AttentionRequired,
            "Stack green",
        ),
        "the notification: {notification:#?}"
    );
    assert_eq!(
        notification.agent,
        Some(AgentNotification {
            title: "Stack green".to_string(),
            body_markdown: "**3 of 4** nodes merged; `n4` waits on review".to_string(),
            level: NotificationLevel::Success,
            sound: true,
            link: format!("#/sessions/{}?host={HOST_A}", world.home),
        }),
        "what the agent wrote"
    );
}

#[tokio::test]
async fn a_project_home_main_is_offered_notify_operator() {
    // Given the project's main
    let world = a_world().await;

    // When its agent's tools are listed
    let offered = world.tools_offered_to(&world.home).await;

    // Then it may notify its operator
    assert!(
        offered.iter().any(|tool| tool == NOTIFY_OPERATOR),
        "the main's tools: {offered:?}"
    );
}

#[tokio::test]
async fn a_session_without_a_standing_role_is_not_offered_notify_operator_and_publishes_nothing() {
    // Given an ordinary claude-cli session, which notifies with `tddy-tools notify` instead
    let world = a_world().await;

    // When its agent's tools are listed, and it calls the tool anyway
    let offered = world.tools_offered_to(&world.ordinary).await;
    let called = world
        .notifies(
            &world.ordinary,
            serde_json::json!({ "title": "Done", "body_markdown": "" }),
        )
        .await;

    // Then it is not offered, the call is refused, and nothing reached the bus
    assert!(
        !offered.iter().any(|tool| tool == NOTIFY_OPERATOR),
        "the ordinary session's tools: {offered:?}"
    );
    called.assert_refused();
    assert_eq!(
        world.agent_notifications(),
        Vec::new(),
        "agent notifications on the bus"
    );
}

#[tokio::test]
async fn a_main_with_no_hook_token_is_refused_naming_the_tool_and_why_and_publishes_nothing() {
    // Given a project-home session that is not a claude-cli or cursor-cli one: it has no hooks
    let world = a_world().await;

    // When its agent notifies its operator
    let refused = world
        .notifies(
            &world.home_without_hooks,
            serde_json::json!({ "title": "Stack green", "body_markdown": "" }),
        )
        .await;

    // Then it is refused, naming the tool and the reason in the agent's terms — not the activity
    // service's own session-type error — and nothing reached the bus
    refused
        .assert_refused_saying("NotifyOperator")
        .assert_refused_saying("no hook token")
        .assert_refused_saying("claude-cli and cursor-cli")
        .assert_not_saying("session_type");
    assert_eq!(
        world.agent_notifications(),
        Vec::new(),
        "agent notifications on the bus"
    );
}

#[tokio::test]
async fn a_main_naming_another_session_still_notifies_as_itself() {
    // Given the project's main and another claude-cli session of the project
    let world = a_world().await;

    // When the main's agent names the other session in its call
    world
        .notifies(
            &world.home,
            serde_json::json!({
                "title": "Stack green",
                "body_markdown": "",
                "session_id": world.ordinary,
            }),
        )
        .await
        .assert_answered()
        .assert_not_saying(HOOK_TOKEN);

    // Then the one notification published is the main's: the tool publishes for its caller alone
    let published: Vec<String> = world
        .agent_notifications()
        .into_iter()
        .map(|notification| notification.session_id)
        .collect();
    assert_eq!(published, vec![world.home.clone()], "whose notifications");
}
