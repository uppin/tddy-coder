//! Acceptance: a project-home session's `StartSession` names a derived host with `target`, and the
//! daemon resolves it for the caller — or refuses it, naming what the caller could have named
//! (`#remote-shell` 11/11).
//!
//! The hosts here reach no derived host: the test user's operator home has no SSH config and no
//! Coder account. What is under test is the refusal — that a `target` reaches the daemon at all, is
//! looked up, and is answered with the listing rather than silently dropped into a local start.
//! A start that lands on a listed host is `tddy-session-lifecycle`'s to prove, with its stub-target
//! fixtures (`derived_host_start_acceptance`).
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-16-remote-shell-orchestrator-target-start.md

mod support;

use std::sync::Arc;

use support::{a_host, a_workspace_in, InProcessPeers, HOST_A, THIS_PROJECT};
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest, ExecuteToolResponse};
use tddy_session_lifecycle::test_util::TEST_TOKEN;

const START_SESSION: &str = "StartSession";
/// A derived host nobody listed.
const NOBODYS_HOST: &str = "host-a/ssh/remote-dev";

struct World {
    a: support::Host,
    home: String,
}

async fn a_world() -> World {
    let peers = Arc::new(InProcessPeers::default());
    let a = a_host(HOST_A, &peers);
    let home = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&home);
    World { a, home }
}

impl World {
    async fn home_starts(&self, args: serde_json::Value) -> ExecuteToolResponse {
        self.a
            .daemon
            .execute_tool(Request::direct(ExecuteToolRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.home.clone(),
                tool_name: START_SESSION.to_string(),
                args_json: args.to_string(),
                daemon_instance_id: String::new(),
                conversation_id: String::new(),
            }))
            .await
            .unwrap_or_else(|s| panic!("{START_SESSION} is served to the home session: {s}"))
            .into_inner()
    }
}

#[tokio::test]
async fn a_start_naming_an_unknown_derived_host_is_refused_naming_the_listed_ones() {
    // Given a home session on a host that reaches no derived host
    let world = a_world().await;

    // When the agent names a host nobody listed, as the one it guessed from a wildcard
    let answer = world
        .home_starts(serde_json::json!({
            "session_type": "workspace",
            "target": NOBODYS_HOST,
        }))
        .await;

    // Then the start is refused — not begun locally — and the refusal names the id and the listing
    assert!(
        answer.is_error,
        "a target nobody listed is refused, not dropped: {}",
        answer.result_json
    );
    assert!(
        answer.error_message.contains(NOBODYS_HOST)
            && answer.error_message.contains("reaches none"),
        "the refusal names the id and says what is reached: {}",
        answer.error_message
    );
}
