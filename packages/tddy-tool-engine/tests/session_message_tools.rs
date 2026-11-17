//! The message tools' own rules — who is offered them, `ReadSessionMessages`' cap, and how a
//! read's arguments reach the port (`#activity-wake` 5/5).
//!
//! The inbox and the addressing rule are the daemon's: here the port is a fake that records what
//! it was asked.
//!
//! Feature: docs/ft/daemon/1-WIP/PRD-2026-11-17-session-messages.md

use std::sync::Mutex;

use async_trait::async_trait;
use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use tddy_tool_engine::project_home_tools::{session_offered_catalog_for, SessionToolRole};
use tddy_tool_engine::session_message_tools::{
    execute_session_message_tool, PostMessageRequest, ReadMessagesRequest, SessionMessagePort,
    POST_SESSION_MESSAGE, READ_SESSION_MESSAGES,
};
use tddy_tool_engine::start_session_args::StartChoices;
use tddy_tool_engine::ToolOutcome;
use tddy_worktree_service::session_target::HomeCaller;

fn a_child() -> HomeCaller {
    HomeCaller {
        session_id: "child".to_string(),
        project_id: "proj-a".to_string(),
    }
}

/// A port that records each read and answers an empty one.
#[derive(Default)]
struct FakeInbox {
    reads: Mutex<Vec<ReadMessagesRequest>>,
}

impl FakeInbox {
    fn reads(&self) -> Vec<ReadMessagesRequest> {
        self.reads.lock().unwrap().clone()
    }
}

/// What [`FakeInbox`] answers every read.
fn an_empty_read() -> Value {
    json!({ "messages": [], "lost": 0, "has_more": false })
}

#[async_trait]
impl SessionMessagePort for FakeInbox {
    async fn post_message(&self, _: &HomeCaller, _: PostMessageRequest) -> Result<Value, String> {
        Err("not used by these tests".to_string())
    }

    async fn read_messages(
        &self,
        _: &HomeCaller,
        request: ReadMessagesRequest,
    ) -> Result<Value, String> {
        self.reads.lock().unwrap().push(request);
        Ok(an_empty_read())
    }
}

async fn reading(inbox: &FakeInbox, args: Value) -> ToolOutcome {
    execute_session_message_tool(inbox, &a_child(), READ_SESSION_MESSAGES, &args.to_string()).await
}

/// The message tools a session of `role` is offered, in catalogue order.
fn message_tools_offered_to(role: SessionToolRole) -> Vec<String> {
    session_offered_catalog_for(
        role,
        &StartChoices {
            hosts: Vec::new(),
            recipes: Vec::new(),
            agents: Vec::new(),
            session_types: Vec::new(),
        },
    )
    .into_iter()
    .map(|tool| tool.name)
    .filter(|name| [POST_SESSION_MESSAGE, READ_SESSION_MESSAGES].contains(&name.as_str()))
    .collect()
}

#[test]
fn the_message_tools_are_offered_to_every_role() {
    // Given each role a session can have
    let roles = [
        SessionToolRole::ProjectHome,
        SessionToolRole::ConversationSpawner,
        SessionToolRole::Ordinary,
    ];

    // When each is offered its catalogue
    let offered = roles.map(message_tools_offered_to);

    // Then every one finds both message tools in it
    let both = vec![
        POST_SESSION_MESSAGE.to_string(),
        READ_SESSION_MESSAGES.to_string(),
    ];
    assert_eq!(offered, [both.clone(), both.clone(), both]);
}

#[tokio::test]
async fn a_read_of_more_than_fifty_messages_is_refused() {
    // Given an inbox that would answer any read
    let inbox = FakeInbox::default();

    // When a read asks for 51 messages, and another for exactly 50
    let over = reading(&inbox, json!({ "limit": 51 })).await;
    let at_the_cap = reading(&inbox, json!({ "limit": 50 })).await;

    // Then the first is refused naming the cap before the port is asked, and the second is served
    assert!(
        over.is_error && over.error_message.contains("50"),
        "a read over the cap was not refused naming it: {:?}",
        over.error_message
    );
    assert!(!at_the_cap.is_error, "{}", at_the_cap.error_message);
    assert_eq!(
        inbox.reads(),
        vec![ReadMessagesRequest {
            after_message_id: None,
            limit: Some(50),
        }]
    );
}

#[tokio::test]
async fn a_read_hands_the_port_where_to_read_from() {
    // Given an inbox that answers a read
    let inbox = FakeInbox::default();

    // When a read names the message to read after
    let outcome = reading(&inbox, json!({ "after_message_id": "m-7" })).await;

    // Then the port is asked exactly that, and its answer is the tool's
    assert!(!outcome.is_error, "{}", outcome.error_message);
    assert_eq!(
        inbox.reads(),
        vec![ReadMessagesRequest {
            after_message_id: Some("m-7".to_string()),
            limit: None,
        }]
    );
    assert_eq!(
        serde_json::from_str::<Value>(&outcome.result_json).expect("the answer is JSON"),
        an_empty_read()
    );
}
