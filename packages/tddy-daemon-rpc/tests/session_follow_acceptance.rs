//! Acceptance: a project-home session follows a session of its project — its activity, its
//! transcript and the question it is waiting on — a page at a time, on its own host or another
//! (`#start-parity` 6/9).
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.
//! Changeset: docs/dev/1-WIP/2026-10-25-start-parity-session-follow.md

mod support;

use std::path::PathBuf;
use std::sync::Arc;

use support::{
    a_host, a_workspace_in, Host, InProcessPeers, HOST_A, HOST_B, OTHER_PROJECT, THIS_PROJECT,
};
use tddy_core::agent_activity::{append_agent_activity, AgentActivityRecord};
use tddy_core::pending_question::{write_pending_question, PendingQuestion};
use tddy_rpc::Request;
use tddy_service::proto::activity::{
    ActivityService as _, GetAcpReplayPageRequest, GetAcpToolCallDetailRequest,
};
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest, ExecuteToolResponse};
use tddy_session_lifecycle::test_util::TEST_TOKEN;

struct World {
    a: Host,
    b: Host,
    home: String,
    on_a: String,
    on_b: String,
    foreign_on_a: String,
}

async fn a_world() -> World {
    let peers = Arc::new(InProcessPeers::default());
    let a = a_host(HOST_A, &peers);
    let b = a_host(HOST_B, &peers);
    let home = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&home);
    let on_a = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    let on_b = a_workspace_in(&b.daemon, THIS_PROJECT).await;
    let foreign_on_a = a_workspace_in(&a.daemon, OTHER_PROJECT).await;
    World {
        a,
        b,
        home,
        on_a,
        on_b,
        foreign_on_a,
    }
}

fn session_dir(host: &Host, session_id: &str) -> PathBuf {
    tddy_core::session_lifecycle::unified_session_dir_path(host.sessions.path(), session_id)
}

fn a_call(seq: u64) -> AgentActivityRecord {
    AgentActivityRecord {
        call_id: format!("call-{seq}"),
        tool_name: "Bash".to_string(),
        input: serde_json::json!({ "command": format!("echo {seq}") }),
        status: "completed".to_string(),
        result: serde_json::json!({ "stdout": format!("output {seq}") }),
        error_message: String::new(),
        started_unix_ms: 1_700_000_000_000 + seq,
        completed_unix_ms: 1_700_000_000_500 + seq,
        source: "claude-cli".to_string(),
        head_commit: String::new(),
        activity_seq: seq,
        changed_paths: Vec::new(),
    }
}

fn record_calls(host: &Host, session_id: &str, seqs: std::ops::RangeInclusive<u64>) {
    let dir = session_dir(host, session_id);
    for seq in seqs {
        append_agent_activity(&dir, &a_call(seq)).unwrap();
    }
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
            .unwrap_or_else(|s| panic!("{tool} is served to the home session: {s}"))
            .into_inner()
    }

    async fn read(&self, tool: &str, args: serde_json::Value) -> serde_json::Value {
        let response = self.home_calls(tool, args).await;
        assert!(!response.is_error, "{}", response.error_message);
        serde_json::from_str(&response.result_json).expect("JSON result")
    }
}

fn activity_seqs(page: &serde_json::Value) -> Vec<u64> {
    page["items"]
        .as_array()
        .expect("an items array")
        .iter()
        .map(|item| item["activity_seq"].as_u64().unwrap())
        .collect()
}

#[tokio::test]
async fn activity_is_read_a_page_at_a_time_without_repeats_or_gaps() {
    // Given 250 tool calls in a session of the project
    let world = a_world().await;
    record_calls(&world.a, &world.on_a, 1..=250);

    // When the home session reads them 100 at a time
    let mut cursor = String::new();
    let mut pages = Vec::new();
    loop {
        let page = world
            .read(
                "ReadSessionActivity",
                serde_json::json!({ "session_id": world.on_a, "cursor": cursor, "limit": 100 }),
            )
            .await;
        cursor = page["next_cursor"].as_str().unwrap().to_string();
        let more = page["has_more"].as_bool().unwrap();
        pages.push(page);
        if !more {
            break;
        }
    }

    // Then it took three pages, which together are every call once, in order
    assert_eq!(
        pages
            .iter()
            .map(|p| activity_seqs(p).len())
            .collect::<Vec<_>>(),
        vec![100, 100, 50]
    );
    let together: Vec<u64> = pages.iter().flat_map(activity_seqs).collect();
    assert_eq!(together, (1..=250).collect::<Vec<_>>());
}

#[tokio::test]
async fn polling_from_the_last_cursor_returns_only_new_activity() {
    // Given everything read so far
    let world = a_world().await;
    record_calls(&world.a, &world.on_a, 1..=5);
    let read = world
        .read(
            "ReadSessionActivity",
            serde_json::json!({ "session_id": world.on_a }),
        )
        .await;

    // When the session does three more things
    record_calls(&world.a, &world.on_a, 6..=8);
    let poll = world
        .read(
            "ReadSessionActivity",
            serde_json::json!({ "session_id": world.on_a, "cursor": read["next_cursor"] }),
        )
        .await;

    // Then the poll is exactly those
    assert_eq!(activity_seqs(&poll), vec![6, 7, 8]);
}

#[tokio::test]
async fn a_limit_above_the_maximum_is_clamped_and_says_so() {
    // Given
    let world = a_world().await;
    record_calls(&world.a, &world.on_a, 1..=250);

    // When
    let page = world
        .read(
            "ReadSessionActivity",
            serde_json::json!({ "session_id": world.on_a, "limit": 10_000 }),
        )
        .await;

    // Then
    assert_eq!(activity_seqs(&page).len(), 200);
    assert_eq!(page["clamped_to"], 200);
}

#[tokio::test]
async fn the_question_a_session_waits_on_comes_with_its_activity() {
    // Given a session waiting on a question
    let world = a_world().await;
    write_pending_question(
        &session_dir(&world.a, &world.on_a),
        &PendingQuestion {
            kind: "permission".to_string(),
            question: "Claude needs your permission to use Bash".to_string(),
            ..Default::default()
        },
    )
    .unwrap();

    // When
    let page = world
        .read(
            "ReadSessionActivity",
            serde_json::json!({ "session_id": world.on_a }),
        )
        .await;

    // Then
    assert_eq!(
        page["pending_question"]["question"],
        "Claude needs your permission to use Bash"
    );
    assert_eq!(page["pending_question"]["kind"], "permission");
}

#[tokio::test]
async fn a_session_not_waiting_has_no_question() {
    // Given
    let world = a_world().await;

    // When
    let page = world
        .read(
            "ReadSessionActivity",
            serde_json::json!({ "session_id": world.on_a }),
        )
        .await;

    // Then
    assert!(page["pending_question"].is_null());
}

#[tokio::test]
async fn the_transcript_is_the_page_the_activities_view_reads() {
    // Given a session with some tool calls
    let world = a_world().await;
    record_calls(&world.a, &world.on_a, 1..=12);
    let view_page = world
        .a
        .daemon
        .activity_service()
        .get_acp_replay_page(Request::direct(GetAcpReplayPageRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: world.on_a.clone(),
            daemon_instance_id: String::new(),
            before_seq: u64::MAX,
            page_size: 5,
        }))
        .await
        .expect("the Activities view's page")
        .into_inner();

    // When the home session reads the newest five transcript items
    let page = world
        .read(
            "ReadSessionTranscript",
            serde_json::json!({ "session_id": world.on_a, "limit": 5 }),
        )
        .await;

    // Then they are the same frames, by position
    let items = page["items"].as_array().unwrap();
    assert_eq!(items.len(), view_page.frames.len());
    let tool_calls = items
        .iter()
        .filter(|item| item["kind"] == "tool_call")
        .count();
    assert_eq!(tool_calls, items.len());
    assert_eq!(items[0]["seq"].as_u64().unwrap(), view_page.first_seq);
    assert!(page["has_more"].as_bool().unwrap());
}

#[tokio::test]
async fn a_tool_call_is_read_whole_by_its_id() {
    // Given a session with a tool call
    let world = a_world().await;
    record_calls(&world.a, &world.on_a, 1..=1);
    let detail = world
        .a
        .daemon
        .activity_service()
        .get_acp_tool_call_detail(Request::direct(GetAcpToolCallDetailRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: world.on_a.clone(),
            daemon_instance_id: String::new(),
            tool_call_id: "call-1".to_string(),
        }))
        .await
        .expect("the Activities view's detail")
        .into_inner();

    // When the home session asks for it by id
    let whole = world
        .read(
            "ReadSessionTranscript",
            serde_json::json!({ "session_id": world.on_a, "tool_call_id": "call-1" }),
        )
        .await;

    // Then it is the detail the Activities view shows
    assert_eq!(whole["raw_output"].as_str(), detail.raw_output.as_deref());
    assert_eq!(whole["raw_input"].as_str(), detail.raw_input.as_deref());
}

#[tokio::test]
async fn a_session_of_another_project_is_refused() {
    // Given
    let world = a_world().await;

    // When
    let refused = world
        .home_calls(
            "ReadSessionActivity",
            serde_json::json!({ "session_id": world.foreign_on_a }),
        )
        .await;

    // Then
    assert!(refused.is_error);
    assert!(
        refused.error_message.contains("another project"),
        "{}",
        refused.error_message
    );
}

#[tokio::test]
async fn activity_is_read_from_a_session_on_another_host() {
    // Given tool calls in the project's session on host-b
    let world = a_world().await;
    record_calls(&world.b, &world.on_b, 1..=3);

    // When the home session reads them, naming host-b
    let page = world
        .read(
            "ReadSessionActivity",
            serde_json::json!({ "session_id": world.on_b, "daemon_instance_id": HOST_B }),
        )
        .await;

    // Then
    assert_eq!(activity_seqs(&page), vec![1, 2, 3]);
}
