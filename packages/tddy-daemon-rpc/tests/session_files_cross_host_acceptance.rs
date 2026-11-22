//! Acceptance: an orchestrator's session files reach the sessions it starts on other hosts
//! (`#orchestra` 10/11).
//!
//! `WriteSessionFile`'s attachment names the daemon holding the file, so a `StartSession` on any
//! host can fetch it; and a session-file call addressed to another host is confirmed at the
//! caller's own host and served there, so the file always lands in the orchestrator's own session.
//!
//! Two daemons, `host-a` (holding the orchestrator) and `host-b`, run in this process over the
//! in-memory peers of `support`.
//!
//! Feature doc: docs/ft/coder/session-attachments.md § Session files an orchestrator writes.
//! Changeset: docs/dev/1-WIP/2026-11-22-session-files-cross-host.md

mod support;

use std::path::PathBuf;
use std::sync::Arc;

use pretty_assertions::assert_eq;
use support::{a_host, a_workspace_in, Host, InProcessPeers, HOST_A, HOST_B, THIS_PROJECT};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest};
use tddy_session_files::session_files::SESSION_FILES_DIR;
use tddy_session_lifecycle::test_util::TEST_TOKEN;

const BRIEF: &str = "# Brief\n\nImplement the parser.\n";

// ── the world ────────────────────────────────────────────────────────────────────────────────

/// Two hosts of one project: the orchestrator and a worker session on host-a.
struct World {
    a: Host,
    b: Host,
    orchestrator: String,
    worker: String,
}

async fn a_world() -> World {
    let peers = Arc::new(InProcessPeers::default());
    let a = a_host(HOST_A, &peers);
    let b = a_host(HOST_B, &peers);
    let orchestrator = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&orchestrator);
    let worker = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    World {
        a,
        b,
        orchestrator,
        worker,
    }
}

/// What came back from a tool call as the agent sees it: an answer, or why there is none.
struct Answer {
    refused: bool,
    text: String,
}

impl Answer {
    fn assert_answered(&self) -> serde_json::Value {
        assert!(
            !self.refused,
            "expected an answer, was refused: {}",
            self.text
        );
        serde_json::from_str(&self.text).expect("the answer is JSON")
    }
}

/// `tool` from `caller`'s agent on host-a, the request addressed to `request_daemon` ("" for its
/// own host).
async fn call(
    world: &World,
    caller: &str,
    request_daemon: &str,
    tool: &str,
    args: serde_json::Value,
) -> Answer {
    let result = world
        .a
        .daemon
        .execute_tool(Request::direct(ExecuteToolRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: caller.to_string(),
            tool_name: tool.to_string(),
            args_json: args.to_string(),
            daemon_instance_id: request_daemon.to_string(),
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

fn the_brief(content: &str) -> serde_json::Value {
    serde_json::json!({ "name": "brief.md", "content": content })
}

/// Where `host` keeps `session_id`'s session file `name`.
fn session_file_on(host: &Host, session_id: &str, name: &str) -> PathBuf {
    unified_session_dir_path(host.sessions.path(), session_id)
        .join(SESSION_FILES_DIR)
        .join(name)
}

// ── the attachment ───────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn the_written_files_attachment_names_the_orchestrators_daemon() {
    // Given an orchestrator on host-a
    let world = a_world().await;

    // When it writes a brief on its own host
    let written = call(
        &world,
        &world.orchestrator,
        "",
        "WriteSessionFile",
        the_brief(BRIEF),
    )
    .await
    .assert_answered();

    // Then the attachment it is handed names host-a, so a start on any host fetches it from there
    assert_eq!(written["attachment"]["daemon_instance_id"], HOST_A);
    assert_eq!(written["attachment"]["session_id"], world.orchestrator);
    assert_eq!(written["attachment"]["relative_path"], "files/brief.md");
}

// ── a call addressed to another host ─────────────────────────────────────────────────────────

#[tokio::test]
async fn a_session_file_call_sent_to_another_host_lands_in_the_orchestrators_own_session() {
    // Given an orchestrator on host-a
    let world = a_world().await;

    // When its agent addresses WriteSessionFile at host-b
    let written = call(
        &world,
        &world.orchestrator,
        HOST_B,
        "WriteSessionFile",
        the_brief(BRIEF),
    )
    .await
    .assert_answered();

    // Then the file is in its own session on host-a, the attachment names host-a, and host-b
    // holds nothing for it
    assert_eq!(
        std::fs::read_to_string(session_file_on(&world.a, &world.orchestrator, "brief.md"))
            .expect("the brief is in the orchestrator's session on host-a"),
        BRIEF
    );
    assert_eq!(written["attachment"]["daemon_instance_id"], HOST_A);
    assert!(
        !unified_session_dir_path(world.b.sessions.path(), &world.orchestrator).exists(),
        "host-b must not create a session directory for a session it does not hold"
    );

    // And listing through host-b lists that file
    let listed = call(
        &world,
        &world.orchestrator,
        HOST_B,
        "ListSessionFiles",
        serde_json::json!({}),
    )
    .await
    .assert_answered();
    let names: Vec<&str> = listed["files"]
        .as_array()
        .expect("a files array")
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["brief.md"]);
}

#[tokio::test]
async fn a_worker_sessions_session_file_call_is_refused_from_another_host() {
    // Given a worker session on host-a — not an orchestrator
    let world = a_world().await;

    // When its agent addresses WriteSessionFile at host-b
    let answer = call(
        &world,
        &world.worker,
        HOST_B,
        "WriteSessionFile",
        the_brief(BRIEF),
    )
    .await;

    // Then it is refused, and nothing is written on either host
    assert!(
        answer.refused,
        "a worker's session-file call must be refused, got {}",
        answer.text
    );
    assert!(!session_file_on(&world.a, &world.worker, "brief.md").exists());
    assert!(!unified_session_dir_path(world.b.sessions.path(), &world.worker).exists());
}
