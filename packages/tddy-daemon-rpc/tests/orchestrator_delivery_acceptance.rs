//! Acceptance: an orchestrator gives back a terminal it claimed, so a person's screen takes control
//! without stealing it; it cannot give back a claim it does not hold; and every session it lists
//! says how it is best messaged (`#orchestra` 7/9).
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

mod support;

use std::sync::Arc;
use std::time::Duration;

use support::{a_host, a_workspace_in, Host, InProcessPeers, HOST_A, THIS_PROJECT};
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest, ExecuteToolResponse};
use tddy_session_lifecycle::test_util::TEST_TOKEN;
use tddy_terminal_rpc::service::ControlClaim;

const BROWSER_SCREEN: &str = "browser-screen-1";

struct World {
    a: Host,
    home: String,
    child: String,
    _terminal: tempfile::TempDir,
}

async fn a_world() -> World {
    let peers = Arc::new(InProcessPeers::default());
    let a = a_host(HOST_A, &peers);
    let home = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&home);
    let child = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    let terminal = a_live_terminal_for(&a, &child).await;
    World {
        a,
        home,
        child,
        _terminal: terminal,
    }
}

/// Run a stand-in TUI as `session_id`'s main terminal, so it has a lease to claim.
async fn a_live_terminal_for(host: &Host, session_id: &str) -> tempfile::TempDir {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("stand_in_tui.sh");
    std::fs::write(&script, "#!/bin/sh\nexec sleep 600\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    host.terminals
        .start_with_options(
            session_id,
            host.worktree_of(session_id),
            "claude-opus-4-8",
            vec![script.to_str().unwrap().to_string()],
            "claude-cli",
            None,
            None,
            Vec::new(),
            Vec::new(),
            None,
        )
        .await
        .expect("the main terminal starts");
    tokio::time::sleep(Duration::from_millis(100)).await;
    dir
}

impl World {
    async fn home_calls(&self, tool: &str, args: serde_json::Value) -> ExecuteToolResponse {
        self.a
            .daemon
            .execute_tool(Request::direct(ExecuteToolRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.home.clone(),
                tool_name: tool.to_string(),
                args_json: args.to_string(),
                daemon_instance_id: String::new(),
                conversation_id: String::new(),
            }))
            .await
            .map(|r| r.into_inner())
            .unwrap_or_else(|status| ExecuteToolResponse {
                is_error: true,
                error_message: status.message().to_string(),
                ..Default::default()
            })
    }

    async fn a_browser_claims_without_stealing(&self) -> ControlClaim {
        self.a
            .daemon
            .connection()
            .terminal_control()
            .claim(&self.child, BROWSER_SCREEN, false)
            .await
    }
}

#[tokio::test]
async fn a_released_terminal_is_claimed_by_a_persons_screen_without_stealing() {
    // Given the orchestrator claimed the child's terminal
    let world = a_world().await;
    let claimed = world
        .home_calls(
            "ClaimSessionTerminal",
            serde_json::json!({ "session_id": world.child }),
        )
        .await;
    assert!(!claimed.is_error, "{}", claimed.error_message);

    // When it releases it
    let released = world
        .home_calls(
            "ReleaseSessionTerminal",
            serde_json::json!({ "session_id": world.child }),
        )
        .await;

    // Then a person's screen takes control without stealing it
    assert!(!released.is_error, "{}", released.error_message);
    assert!(
        matches!(
            world.a_browser_claims_without_stealing().await,
            ControlClaim::Granted { .. }
        ),
        "the terminal is still held"
    );
}

#[tokio::test]
async fn releasing_a_terminal_the_orchestrator_never_claimed_is_refused() {
    // Given a person's screen holds the child's terminal
    let world = a_world().await;
    world
        .a
        .daemon
        .connection()
        .terminal_control()
        .claim(&world.child, BROWSER_SCREEN, true)
        .await;

    // When the orchestrator tries to release it
    let refused = world
        .home_calls(
            "ReleaseSessionTerminal",
            serde_json::json!({ "session_id": world.child }),
        )
        .await;

    // Then it is refused, and the person keeps it
    assert!(refused.is_error, "{}", refused.result_json);
    assert!(
        refused.error_message.contains("claim"),
        "{}",
        refused.error_message
    );
    assert_eq!(
        world
            .a
            .daemon
            .connection()
            .terminal_control()
            .holder_screen_id(&world.child)
            .await
            .as_deref(),
        Some(BROWSER_SCREEN)
    );
}

#[tokio::test]
async fn every_listed_session_says_how_it_is_best_messaged() {
    // Given an orchestrator and a session of its project
    let world = a_world().await;

    // When it lists the project's sessions
    let listed = world
        .home_calls("ListProjectSessions", serde_json::json!({}))
        .await;

    // Then each one carries its delivery route
    assert!(!listed.is_error, "{}", listed.error_message);
    let answer: serde_json::Value = serde_json::from_str(&listed.result_json).unwrap();
    let routes: Vec<serde_json::Value> = answer["sessions"]
        .as_array()
        .expect("sessions")
        .iter()
        .map(|s| s["delivery_route"]["kind"].clone())
        .collect();
    assert!(!routes.is_empty(), "{answer}");
    assert!(routes.iter().all(|kind| kind.is_string()), "{answer}");
}
