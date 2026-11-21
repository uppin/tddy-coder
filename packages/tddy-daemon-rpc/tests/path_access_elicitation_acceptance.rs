//! Acceptance: an orchestrator names paths beyond its reach with `RequestPathAccess`; the request
//! raises an attention notification and waits in its session pane's list
//! (`ListPathAccessRequests`); the operator's `AnswerPathAccess` releases the waiting call, and an
//! allowed path is then readable — or, for a write grant, writable — while a denied one is not
//! (`#orchestra` 6/9).
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Orchestrator sessions.

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use support::{
    a_host_recording_notifications, a_workspace_in, Host, InProcessPeers, HOST_A, THIS_PROJECT,
};
use tddy_daemon_kernel::session_notification::SessionNotificationSource;
use tddy_rpc::Request;
use tddy_service::proto::activity::{
    ActivityService as _, AnswerPathAccessRequest, ListPathAccessRequestsRequest, PathAccessRequest,
};
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest, ExecuteToolResponse};
use tddy_session_activity::path_grants::{PathGrant, PathGrantMode, PathGrantStore};
use tddy_session_activity::session_notifications::RecordingSessionNotificationSubscriber;
use tddy_session_lifecycle::test_util::TEST_TOKEN;

const WAIT: Duration = Duration::from_secs(10);

struct World {
    host: Arc<Host>,
    notifications: Arc<RecordingSessionNotificationSubscriber>,
    orchestrator: String,
    /// A directory outside every tddy worktree and agent home, holding `shared.txt`.
    outside: tempfile::TempDir,
}

async fn a_world() -> World {
    let peers = Arc::new(InProcessPeers::default());
    let (host, notifications) = a_host_recording_notifications(HOST_A, &peers);
    let orchestrator = a_workspace_in(&host.daemon, THIS_PROJECT).await;
    host.make_project_home(&orchestrator);
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("shared.txt"), "shared fixtures\n").unwrap();
    World {
        host: Arc::new(host),
        notifications,
        orchestrator,
        outside,
    }
}

impl World {
    fn outside_dir(&self) -> PathBuf {
        self.outside.path().canonicalize().unwrap()
    }

    async fn call(&self, tool: &str, args: serde_json::Value) -> ExecuteToolResponse {
        call(&self.host, &self.orchestrator, tool, args).await
    }

    /// The orchestrator asks for `mode` access to the outside directory, in a task: the call waits.
    fn asks_for(&self, mode: &str) -> tokio::task::JoinHandle<ExecuteToolResponse> {
        let (host, orchestrator) = (Arc::clone(&self.host), self.orchestrator.clone());
        let args = serde_json::json!({
            "paths": [self.outside_dir()],
            "mode": mode,
            "reason": "read the shared fixtures",
        });
        tokio::spawn(async move { call(&host, &orchestrator, "RequestPathAccess", args).await })
    }

    /// The orchestrator's waiting requests, once there is one.
    async fn the_pending_requests(&self) -> Vec<PathAccessRequest> {
        let deadline = tokio::time::Instant::now() + WAIT;
        loop {
            let listed = self
                .host
                .daemon
                .connection()
                .activity_service()
                .list_path_access_requests(Request::direct(ListPathAccessRequestsRequest {
                    session_token: TEST_TOKEN.to_string(),
                    session_id: self.orchestrator.clone(),
                }))
                .await
                .map(|r| r.into_inner().requests)
                .unwrap_or_default();
            if !listed.is_empty() || tokio::time::Instant::now() >= deadline {
                return listed;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    async fn the_operator_answers(&self, request_id: &str, allow: bool) {
        self.host
            .daemon
            .connection()
            .activity_service()
            .answer_path_access(Request::direct(AnswerPathAccessRequest {
                session_token: TEST_TOKEN.to_string(),
                session_id: self.orchestrator.clone(),
                request_id: request_id.to_string(),
                allow,
            }))
            .await
            .expect("the answer is taken");
    }

    fn session_dir(&self) -> PathBuf {
        tddy_core::session_lifecycle::unified_session_dir_path(
            self.host.sessions.path(),
            &self.orchestrator,
        )
    }
}

async fn call(
    host: &Host,
    session_id: &str,
    tool: &str,
    args: serde_json::Value,
) -> ExecuteToolResponse {
    host.daemon
        .execute_tool(Request::direct(ExecuteToolRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: session_id.to_string(),
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

async fn answered(waiting: tokio::task::JoinHandle<ExecuteToolResponse>) -> serde_json::Value {
    let response = tokio::time::timeout(WAIT, waiting)
        .await
        .expect("the waiting call returns once answered")
        .expect("the call's task");
    assert!(!response.is_error, "{}", response.error_message);
    serde_json::from_str(&response.result_json).expect("JSON")
}

fn read_args(path: &Path) -> serde_json::Value {
    serde_json::json!({ "path": path })
}

#[tokio::test]
async fn a_request_waits_in_the_pane_and_raises_an_attention_notification() {
    // Given an orchestrator
    let world = a_world().await;

    // When it asks for read access to a directory outside its reach
    let waiting = world.asks_for("read");

    // Then the request waits in its pane's list, with its paths, mode and reason
    let pending = world.the_pending_requests().await;
    assert_eq!(
        pending
            .iter()
            .map(|r| (r.paths.clone(), r.mode.as_str(), r.reason.as_str()))
            .collect::<Vec<_>>(),
        vec![(
            vec![world.outside_dir().display().to_string()],
            "read",
            "read the shared fixtures"
        )]
    );
    // And the operator was notified, as a question of the orchestrator's
    assert!(
        world
            .notifications
            .received()
            .iter()
            .any(|n| n.session_id == world.orchestrator
                && n.source == SessionNotificationSource::PendingQuestion
                && n.question.as_ref().is_some_and(|q| q.kind == "path_access")),
        "{:?}",
        world.notifications.received()
    );
    world
        .the_operator_answers(&pending[0].request_id, false)
        .await;
    answered(waiting).await;
}

#[tokio::test]
async fn an_allowed_read_request_makes_the_path_readable() {
    // Given an orchestrator waiting on a read request
    let world = a_world().await;
    let waiting = world.asks_for("read");
    let request_id = world.the_pending_requests().await[0].request_id.clone();

    // When the operator allows it
    world.the_operator_answers(&request_id, true).await;

    // Then the call answers allowed, and the path is readable
    assert_eq!(answered(waiting).await["answer"], "allowed");
    let read = world
        .call("Read", read_args(&world.outside_dir().join("shared.txt")))
        .await;
    assert!(!read.is_error, "{}", read.error_message);
    assert!(
        read.result_json.contains("shared fixtures"),
        "{}",
        read.result_json
    );
}

#[tokio::test]
async fn a_denied_request_answers_denied_and_the_path_stays_out_of_reach() {
    // Given an orchestrator waiting on a read request
    let world = a_world().await;
    let waiting = world.asks_for("read");
    let request_id = world.the_pending_requests().await[0].request_id.clone();

    // When the operator denies it
    world.the_operator_answers(&request_id, false).await;

    // Then the call answers denied, and the path is still refused
    assert_eq!(answered(waiting).await["answer"], "denied");
    let read = world
        .call("Read", read_args(&world.outside_dir().join("shared.txt")))
        .await;
    assert!(read.is_error, "{}", read.result_json);
}

#[tokio::test]
async fn a_write_inside_a_write_grant_succeeds_and_outside_it_is_refused() {
    // Given an orchestrator holding a write grant on the outside directory
    let world = a_world().await;
    PathGrantStore::open(&world.session_dir())
        .add(PathGrant {
            path: world.outside_dir(),
            mode: PathGrantMode::Write,
            granted_at_unix_ms: 1_795_000_000_000,
            reason: "update the shared fixtures".to_string(),
        })
        .unwrap();
    let elsewhere = tempfile::tempdir().unwrap();

    // When it writes inside the grant and outside it
    let inside = world
        .call(
            "Write",
            serde_json::json!({ "path": world.outside_dir().join("new.txt"), "contents": "x" }),
        )
        .await;
    let outside = world
        .call(
            "Write",
            serde_json::json!({ "path": elsewhere.path().join("new.txt"), "contents": "x" }),
        )
        .await;

    // Then only the write inside the grant lands
    assert!(!inside.is_error, "{}", inside.error_message);
    assert!(outside.is_error, "{}", outside.result_json);
    assert!(world.outside_dir().join("new.txt").exists());
    assert!(!elsewhere.path().join("new.txt").exists());
}
