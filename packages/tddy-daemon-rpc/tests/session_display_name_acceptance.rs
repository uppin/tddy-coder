//! Acceptance: a session can be given a name — over the `RenameSession` RPC the web's rename calls,
//! at its start, or by an orchestrator's `RenameSession` tool on its own host or another — and the
//! name is what `ListSessions` lists and what the orchestrator's tools report the session by
//! (`ListProjectSessions`, `ReadSessionActivity`, `GetSessionStart`).
//!
//! The caller of the tools is the project's main (`project-home`), on host-a.
//!
//! Feature docs: docs/ft/web/session-drawer.md § Session names,
//! docs/ft/daemon/project-concept.md § Project-home tools.

mod support;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use pretty_assertions::assert_eq;
use support::{a_host, a_workspace_in, Host, InProcessPeers, HOST_A, HOST_B, THIS_PROJECT};
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_rpc::{Code, Request};
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest};
use tddy_service::proto::session::{
    ListSessionsRequest, RenameSessionRequest, SessionService as _, StartSessionRequest,
};
use tddy_session_lifecycle::test_util::TEST_TOKEN;

const NAME: &str = "Fix the login flow";

// ── the world ────────────────────────────────────────────────────────────────────────────────

/// Two hosts of one project; main runs on host-a.
struct World {
    a: Host,
    b: Host,
    main: String,
}

async fn a_world() -> World {
    let peers = Arc::new(InProcessPeers::default());
    let a = a_host(HOST_A, &peers);
    let b = a_host(HOST_B, &peers);
    let main = a_workspace_in(&a.daemon, THIS_PROJECT).await;
    a.make_project_home(&main);
    World { a, b, main }
}

impl Host {
    fn session_dir(&self, session_id: &str) -> PathBuf {
        unified_session_dir_path(self.sessions.path(), session_id)
    }

    /// The name `session_id`'s `.session.yaml` records.
    fn recorded_name_of(&self, session_id: &str) -> Option<String> {
        tddy_core::read_session_metadata(&self.session_dir(session_id))
            .unwrap()
            .display_name
    }

    /// The name `ListSessions` lists for `session_id` — empty for an unnamed session.
    async fn listed_name_of(&self, session_id: &str) -> String {
        self.daemon
            .list_sessions(Request::direct(ListSessionsRequest {
                session_token: TEST_TOKEN.to_string(),
                include_peers: false,
            }))
            .await
            .expect("ListSessions")
            .into_inner()
            .sessions
            .into_iter()
            .find(|entry| entry.session_id == session_id)
            .expect("the session is listed")
            .display_name
    }

    fn session_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = std::fs::read_dir(
            self.sessions
                .path()
                .join(tddy_core::output::SESSIONS_SUBDIR),
        )
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
        ids.sort();
        ids
    }

    /// `RenameSession` as the web's rename sends it to this host.
    async fn renames(
        &self,
        session_id: &str,
        display_name: &str,
    ) -> Result<String, tddy_rpc::Status> {
        self.daemon
            .rename_session(Request::direct(RenameSessionRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: session_id.to_string(),
                display_name: display_name.to_string(),
            }))
            .await
            .map(|response| response.into_inner().display_name)
    }

    /// A workspace session of the project started with `display_name`, as the create pane starts it.
    async fn starts_a_workspace_named(
        &self,
        display_name: &str,
    ) -> Result<String, tddy_rpc::Status> {
        self.daemon
            .start_session(Request::direct(StartSessionRequest {
                session_token: TEST_TOKEN.to_string(),
                project_id: THIS_PROJECT.to_string(),
                session_type: "workspace".to_string(),
                display_name: display_name.to_string(),
                ..Default::default()
            }))
            .await
            .map(|response| response.into_inner().session_id)
    }
}

// ── what a tool call answered ────────────────────────────────────────────────────────────────

/// A tool call's answer as the agent sees it, or why there is none.
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

    fn assert_refused_saying(&self, reason: &str) {
        assert!(self.refused, "expected a refusal, got {}", self.text);
        assert!(
            self.text.contains(reason),
            "refused, but not for {reason:?}: {}",
            self.text
        );
    }
}

impl World {
    /// `tool` from main with `args`, on host-a.
    async fn main_calls(&self, tool: &str, args: serde_json::Value) -> Answer {
        let result = self
            .a
            .daemon
            .execute_tool(Request::direct(ExecuteToolRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.main.clone(),
                tool_name: tool.to_string(),
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

    /// The row `ListProjectSessions` answers main for `session_id`.
    async fn project_listing_row_of(&self, session_id: &str) -> serde_json::Value {
        let listed = self
            .main_calls("ListProjectSessions", serde_json::json!({}))
            .await
            .assert_answered();
        listed["sessions"]
            .as_array()
            .expect("a sessions array")
            .iter()
            .find(|row| row["session_id"] == session_id)
            .unwrap_or_else(|| panic!("{session_id} is not listed: {listed}"))
            .clone()
    }

    /// The record `GetSessionStart` answers once the start in `started` is no longer running.
    async fn finished_start(&self, started: &serde_json::Value) -> serde_json::Value {
        let start_id = started["start_id"].as_str().expect("a start_id");
        let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
        loop {
            let record = self
                .main_calls(
                    "GetSessionStart",
                    serde_json::json!({ "start_id": start_id }),
                )
                .await
                .assert_answered();
            if record["state"] != "running" {
                return record;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "start {start_id} is still running: {record}"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

// ── the RenameSession RPC ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn renaming_a_session_records_its_trimmed_name_and_lists_it() {
    // Given a session of the project
    let world = a_world().await;
    let session = a_workspace_in(&world.a.daemon, THIS_PROJECT).await;

    // When the web renames it
    let answered = world
        .a
        .renames(&session, &format!("  {NAME} "))
        .await
        .expect("the rename is accepted");

    // Then the trimmed name is answered, recorded and listed
    assert_eq!(
        (
            answered,
            world.a.recorded_name_of(&session),
            world.a.listed_name_of(&session).await,
        ),
        (NAME.to_string(), Some(NAME.to_string()), NAME.to_string())
    );
}

#[tokio::test]
async fn renaming_to_nothing_clears_the_name() {
    // Given a named session
    let world = a_world().await;
    let session = a_workspace_in(&world.a.daemon, THIS_PROJECT).await;
    world.a.renames(&session, NAME).await.unwrap();

    // When the web renames it to blank
    let answered = world.a.renames(&session, "   ").await.unwrap();

    // Then it has no name anywhere
    assert_eq!(
        (
            answered,
            world.a.recorded_name_of(&session),
            world.a.listed_name_of(&session).await,
        ),
        (String::new(), None, String::new())
    );
}

#[tokio::test]
async fn a_name_over_the_limit_is_refused_and_the_old_name_stands() {
    // Given a named session
    let world = a_world().await;
    let session = a_workspace_in(&world.a.daemon, THIS_PROJECT).await;
    world.a.renames(&session, NAME).await.unwrap();

    // When the web renames it to a name over eighty characters
    let refused = world
        .a
        .renames(&session, &"x".repeat(81))
        .await
        .expect_err("a name over the limit is refused");

    // Then it is an invalid argument naming the limit, and the old name stands
    assert_eq!(refused.code, Code::InvalidArgument, "{refused:?}");
    assert!(refused.message.contains("80"), "{}", refused.message);
    assert_eq!(world.a.recorded_name_of(&session).as_deref(), Some(NAME));
}

#[tokio::test]
async fn renaming_a_session_this_host_does_not_hold_is_not_found() {
    // Given a session on host-b
    let world = a_world().await;
    let on_b = a_workspace_in(&world.b.daemon, THIS_PROJECT).await;

    // When host-a is asked to rename it
    let refused = world
        .a
        .renames(&on_b, NAME)
        .await
        .expect_err("host-a holds no such session");

    // Then it is not found there, and host-b's session is unnamed
    assert_eq!(refused.code, Code::NotFound, "{refused:?}");
    assert_eq!(world.b.recorded_name_of(&on_b), None);
}

#[tokio::test]
async fn a_rename_with_an_unknown_token_is_unauthenticated() {
    // Given a session of the project
    let world = a_world().await;
    let session = a_workspace_in(&world.a.daemon, THIS_PROJECT).await;

    // When a caller with no valid token renames it
    let refused = world
        .a
        .daemon
        .rename_session(Request::direct(RenameSessionRequest {
            session_token: "not-a-token".to_string(),
            session_id: session.clone(),
            display_name: NAME.to_string(),
        }))
        .await
        .expect_err("an unknown token is refused");

    // Then nothing is named
    assert_eq!(refused.code, Code::Unauthenticated, "{refused:?}");
    assert_eq!(world.a.recorded_name_of(&session), None);
}

// ── naming a session at its start ────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_session_started_with_a_name_is_recorded_and_listed_by_it() {
    // Given
    let world = a_world().await;

    // When the create pane starts a session named
    let session = world
        .a
        .starts_a_workspace_named(&format!(" {NAME}  "))
        .await
        .expect("the start succeeds");

    // Then its record holds the trimmed name, and it is listed by it
    assert_eq!(
        (
            world.a.recorded_name_of(&session),
            world.a.listed_name_of(&session).await
        ),
        (Some(NAME.to_string()), NAME.to_string())
    );
}

#[tokio::test]
async fn a_start_with_an_invalid_name_is_refused_before_anything_is_created() {
    // Given
    let world = a_world().await;
    let before = world.a.session_ids();

    // When a start names its session with a newline in it
    let refused = world
        .a
        .starts_a_workspace_named("Fix the\nlogin flow")
        .await
        .expect_err("a two-line name is refused");

    // Then it is an invalid argument, and no session was created
    assert_eq!(refused.code, Code::InvalidArgument, "{refused:?}");
    assert_eq!(world.a.session_ids(), before);
}

// ── the orchestrator's tools ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn main_renames_a_session_and_lists_and_follows_it_by_its_name() {
    // Given a session of the project
    let world = a_world().await;
    let session = a_workspace_in(&world.a.daemon, THIS_PROJECT).await;

    // When main renames it
    let renamed = world
        .main_calls(
            "RenameSession",
            serde_json::json!({ "session_id": session, "display_name": format!(" {NAME} ") }),
        )
        .await
        .assert_answered();

    // Then the answer, the record, ListProjectSessions and ReadSessionActivity all name it
    let listed = world.project_listing_row_of(&session).await;
    let followed = world
        .main_calls(
            "ReadSessionActivity",
            serde_json::json!({ "session_id": session }),
        )
        .await
        .assert_answered();
    assert_eq!(
        (
            renamed,
            world.a.recorded_name_of(&session),
            listed["display_name"].clone(),
            followed["display_name"].clone(),
        ),
        (
            serde_json::json!({ "session_id": session, "display_name": NAME }),
            Some(NAME.to_string()),
            serde_json::json!(NAME),
            serde_json::json!(NAME),
        )
    );
}

#[tokio::test]
async fn an_unnamed_session_is_listed_and_followed_with_no_name() {
    // Given an unnamed session of the project
    let world = a_world().await;
    let session = a_workspace_in(&world.a.daemon, THIS_PROJECT).await;

    // When main lists and follows it
    let listed = world.project_listing_row_of(&session).await;
    let followed = world
        .main_calls(
            "ReadSessionActivity",
            serde_json::json!({ "session_id": session }),
        )
        .await
        .assert_answered();

    // Then both say it has no name
    assert_eq!(
        (
            listed["display_name"].clone(),
            followed["display_name"].clone()
        ),
        (serde_json::Value::Null, serde_json::Value::Null)
    );
}

#[tokio::test]
async fn main_clears_a_sessions_name_with_an_empty_one() {
    // Given a named session
    let world = a_world().await;
    let session = a_workspace_in(&world.a.daemon, THIS_PROJECT).await;
    world.a.renames(&session, NAME).await.unwrap();

    // When main renames it to nothing
    let renamed = world
        .main_calls(
            "RenameSession",
            serde_json::json!({ "session_id": session, "display_name": "" }),
        )
        .await
        .assert_answered();

    // Then the answer and the record say it has none
    assert_eq!(
        (
            renamed["display_name"].clone(),
            world.a.recorded_name_of(&session)
        ),
        (serde_json::Value::Null, None)
    );
}

#[tokio::test]
async fn main_is_refused_a_name_over_the_limit() {
    // Given a session of the project
    let world = a_world().await;
    let session = a_workspace_in(&world.a.daemon, THIS_PROJECT).await;

    // When main names it with more than eighty characters
    let answer = world
        .main_calls(
            "RenameSession",
            serde_json::json!({ "session_id": session, "display_name": "x".repeat(81) }),
        )
        .await;

    // Then it is refused naming the limit, and nothing is recorded
    answer.assert_refused_saying("80");
    assert_eq!(world.a.recorded_name_of(&session), None);
}

#[tokio::test]
async fn main_renames_a_session_on_another_host_and_lists_it_from_there() {
    // Given a session of the project on host-b
    let world = a_world().await;
    let on_b = a_workspace_in(&world.b.daemon, THIS_PROJECT).await;

    // When main renames it, naming host-b
    world
        .main_calls(
            "RenameSession",
            serde_json::json!({
                "session_id": on_b,
                "display_name": NAME,
                "daemon_instance_id": HOST_B,
            }),
        )
        .await
        .assert_answered();

    // Then host-b records the name, and main's listing — which asks host-b — reports it
    let listed = world.project_listing_row_of(&on_b).await;
    assert_eq!(
        (
            world.b.recorded_name_of(&on_b),
            listed["daemon_instance_id"].clone(),
            listed["display_name"].clone(),
        ),
        (
            Some(NAME.to_string()),
            serde_json::json!(HOST_B),
            serde_json::json!(NAME)
        )
    );
}

#[tokio::test]
async fn a_session_main_starts_with_a_name_is_reported_by_it_when_the_start_succeeds() {
    // Given
    let world = a_world().await;

    // When main starts a session named
    let started = world
        .main_calls(
            "StartSession",
            serde_json::json!({ "session_type": "workspace", "display_name": NAME }),
        )
        .await
        .assert_answered();

    // Then the finished start names its session, and the session is recorded with the name
    let record = world.finished_start(&started).await;
    assert_eq!(record["state"], "succeeded", "{record}");
    let session = record["session_id"]
        .as_str()
        .expect("a session")
        .to_string();
    assert_eq!(
        (
            record["display_name"].clone(),
            world.a.recorded_name_of(&session)
        ),
        (serde_json::json!(NAME), Some(NAME.to_string()))
    );
}
