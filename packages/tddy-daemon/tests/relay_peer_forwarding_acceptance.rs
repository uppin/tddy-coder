//! Acceptance tests: relay peer forwarding and exec-tool routing (Gap A).
//!
//! AC: `classify_peer_route` (generic rename of `classify_start_session_peer_route`) routes
//! correctly for local, matching-local, known-remote, and unknown-remote cases.
//!
//! AC: `execute_tool` and `list_exec_tools` must classify the `daemon_instance_id` BEFORE
//! session lookup so a non-local peer routes to Forward / errors instead of falling through to
//! local execution. Without a LiveKit room, routing to a known peer returns `failed_precondition`.

use std::path::PathBuf;
use std::sync::Arc;

use tddy_daemon::claude_cli_session::ClaudeCliSessionManager;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_daemon::livekit_peer_discovery::{
    classify_peer_route, LiveKitDiscoveryHandles, PeerRoute,
};
use tddy_daemon::multi_host::{DaemonInstanceId, EligibleDaemonInfo, EligibleDaemonSource};
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ExecuteToolRequest, ListExecToolsRequest,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const VALID_TOKEN: &str = "valid-token";
const LOCAL_INSTANCE_ID: &str = "local-relay";
const REMOTE_PEER_ID: &str = "remote-peer-1";
/// A valid UUID-format session_id (doesn't exist in any session dir).
const NONEXISTENT_SESSION_ID: &str = "019d105b-ac0f-78d3-9a89-409731145a36";

/// A test-only eligible daemon source that lists a fixed set of instance IDs as eligible.
struct MockEligibleDaemonSource {
    ids: Vec<String>,
}

impl EligibleDaemonSource for MockEligibleDaemonSource {
    fn list_eligible_daemons(&self) -> Vec<EligibleDaemonInfo> {
        self.ids
            .iter()
            .map(|id| EligibleDaemonInfo {
                instance_id: DaemonInstanceId(id.clone()),
                label: id.clone(),
            })
            .collect()
    }
}

fn test_config() -> DaemonConfig {
    let yaml = format!(
        r#"
users:
  - github_user: "testuser"
    os_user: "testuser"
daemon_instance_id: "{}"
"#,
        LOCAL_INSTANCE_ID
    );
    serde_yaml::from_str(&yaml).expect("config must parse")
}

fn sessions_resolver(base: PathBuf) -> SessionsBaseResolver {
    Arc::new(move |_| Some(base.clone()))
}

fn user_resolver_valid() -> UserResolver {
    Arc::new(|token| {
        if token == VALID_TOKEN {
            Some("testuser".to_string())
        } else {
            None
        }
    })
}

fn service_with_known_remote_peer(
    config: DaemonConfig,
    sessions_base: PathBuf,
) -> ConnectionServiceImpl {
    let room_slot = Arc::new(tokio::sync::RwLock::new(None)); // no LiveKit room
    let discovery = LiveKitDiscoveryHandles {
        eligible_daemon_source: Arc::new(MockEligibleDaemonSource {
            ids: vec![REMOTE_PEER_ID.to_string()],
        }) as Arc<dyn EligibleDaemonSource>,
        common_room_livekit_room: room_slot,
    };
    ConnectionServiceImpl::new(
        config,
        sessions_resolver(sessions_base),
        user_resolver_valid(),
        None,
        Some(discovery),
        None,
        Arc::new(ClaudeCliSessionManager::new()),
    )
}

// ── classify_peer_route unit tests (new generic name — compile error until renamed) ──────────────

/// AC: `classify_peer_route` with an empty `requested_instance_id` routes `Local`.
#[test]
fn classify_peer_route_empty_requested_id_routes_local() {
    let result = classify_peer_route(LOCAL_INSTANCE_ID, "", &[]);
    assert_eq!(
        result,
        Ok(PeerRoute::Local),
        "empty requested_instance_id must route Local; got: {:?}",
        result
    );
}

/// AC: `classify_peer_route` when requested id matches local id routes `Local`.
#[test]
fn classify_peer_route_requested_matches_local_routes_local() {
    let result = classify_peer_route(LOCAL_INSTANCE_ID, LOCAL_INSTANCE_ID, &[]);
    assert_eq!(
        result,
        Ok(PeerRoute::Local),
        "requested_instance_id matching local must route Local; got: {:?}",
        result
    );
}

/// AC: `classify_peer_route` with a known remote id routes `Forward { peer_instance_id }`.
#[test]
fn classify_peer_route_known_remote_id_routes_forward() {
    let eligible = vec![REMOTE_PEER_ID.to_string()];
    let result = classify_peer_route(LOCAL_INSTANCE_ID, REMOTE_PEER_ID, &eligible);
    assert_eq!(
        result,
        Ok(PeerRoute::Forward {
            peer_instance_id: REMOTE_PEER_ID.to_string()
        }),
        "known remote id must route Forward; got: {:?}",
        result
    );
}

/// AC: `classify_peer_route` with an unknown id (not in eligible list) returns `Err`.
#[test]
fn classify_peer_route_unknown_id_returns_err() {
    let result = classify_peer_route(LOCAL_INSTANCE_ID, "unknown-daemon-xyz", &[]);
    assert!(
        result.is_err(),
        "unknown daemon_instance_id must return Err; got: {:?}",
        result
    );
    let msg = result.unwrap_err();
    assert!(
        msg.contains("unknown-daemon-xyz"),
        "error message must mention the unknown id; got: {:?}",
        msg
    );
}

// ── execute_tool routing ─────────────────────────────────────────────────────────────────────────

/// AC: `execute_tool` with a known non-local `daemon_instance_id` (eligible peer, no room) must
/// return `failed_precondition` — routing is applied before session lookup.
///
/// Currently `execute_tool` ignores `daemon_instance_id` and falls through to local session
/// lookup, which returns `not_found` (wrong). After the fix it returns `failed_precondition`.
#[tokio::test]
async fn execute_tool_with_known_remote_instance_id_returns_failed_precondition_without_room() {
    let sessions_tmp = tempfile::tempdir().unwrap();
    let service = service_with_known_remote_peer(test_config(), sessions_tmp.path().to_path_buf());

    let status = service
        .execute_tool(Request::new(ExecuteToolRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: NONEXISTENT_SESSION_ID.to_string(),
            tool_name: "Read".to_string(),
            args_json: r#"{"path":"file.txt"}"#.to_string(),
            daemon_instance_id: REMOTE_PEER_ID.to_string(), // known peer, no LiveKit room
        }))
        .await
        .expect_err("execute_tool must fail when forwarding a known remote peer with no room");

    assert_eq!(
        status.code(),
        tddy_rpc::Code::FailedPrecondition,
        "execute_tool with known remote daemon_instance_id and no room must return \
         FailedPrecondition; got {:?}: {}",
        status.code(),
        status.message()
    );
}

/// AC: `execute_tool` with an unknown (not eligible) `daemon_instance_id` must return
/// `invalid_argument` — routing is applied and the unknown peer is rejected.
///
/// Currently it falls through to local session lookup (wrong code).
#[tokio::test]
async fn execute_tool_with_unknown_remote_instance_id_returns_invalid_argument() {
    let sessions_tmp = tempfile::tempdir().unwrap();
    // No eligible_daemon_source → only local is reachable.
    let service = ConnectionServiceImpl::new(
        test_config(),
        sessions_resolver(sessions_tmp.path().to_path_buf()),
        user_resolver_valid(),
        None,
        None, // no livekit discovery
        None,
        Arc::new(ClaudeCliSessionManager::new()),
    );

    let status = service
        .execute_tool(Request::new(ExecuteToolRequest {
            session_token: VALID_TOKEN.to_string(),
            session_id: NONEXISTENT_SESSION_ID.to_string(),
            tool_name: "Read".to_string(),
            args_json: r#"{"path":"file.txt"}"#.to_string(),
            daemon_instance_id: "totally-unknown-daemon".to_string(), // not in eligible list
        }))
        .await
        .expect_err("execute_tool must fail for unknown daemon_instance_id");

    assert_eq!(
        status.code(),
        tddy_rpc::Code::InvalidArgument,
        "execute_tool with unknown daemon_instance_id must return InvalidArgument \
         (routing applied before session lookup); got {:?}: {}",
        status.code(),
        status.message()
    );
}

// ── list_exec_tools routing ──────────────────────────────────────────────────────────────────────

/// AC: `list_exec_tools` with a known non-local `daemon_instance_id` (eligible peer, no room)
/// must return `failed_precondition` — routing is applied.
///
/// Currently `list_exec_tools` ignores `daemon_instance_id` and returns the local catalog (wrong).
#[tokio::test]
async fn list_exec_tools_with_known_remote_instance_id_returns_failed_precondition_without_room() {
    let sessions_tmp = tempfile::tempdir().unwrap();
    let service = service_with_known_remote_peer(test_config(), sessions_tmp.path().to_path_buf());

    let status = service
        .list_exec_tools(Request::new(ListExecToolsRequest {
            session_token: VALID_TOKEN.to_string(),
            daemon_instance_id: REMOTE_PEER_ID.to_string(), // known peer, no room
        }))
        .await
        .expect_err(
            "list_exec_tools must fail when forwarding to a known remote peer with no room",
        );

    assert_eq!(
        status.code(),
        tddy_rpc::Code::FailedPrecondition,
        "list_exec_tools with known remote daemon_instance_id and no room must return \
         FailedPrecondition; got {:?}: {}",
        status.code(),
        status.message()
    );
}

/// AC: `list_exec_tools` with an unknown `daemon_instance_id` must return `invalid_argument`.
///
/// Currently it returns Ok with the local tool catalog (routing is not applied).
#[tokio::test]
async fn list_exec_tools_with_unknown_instance_id_returns_invalid_argument() {
    let sessions_tmp = tempfile::tempdir().unwrap();
    let service = ConnectionServiceImpl::new(
        test_config(),
        sessions_resolver(sessions_tmp.path().to_path_buf()),
        user_resolver_valid(),
        None,
        None, // no livekit discovery
        None,
        Arc::new(ClaudeCliSessionManager::new()),
    );

    let status = service
        .list_exec_tools(Request::new(ListExecToolsRequest {
            session_token: VALID_TOKEN.to_string(),
            daemon_instance_id: "totally-unknown-daemon".to_string(),
        }))
        .await
        .expect_err("list_exec_tools must fail for unknown daemon_instance_id");

    assert_eq!(
        status.code(),
        tddy_rpc::Code::InvalidArgument,
        "list_exec_tools with unknown daemon_instance_id must return InvalidArgument; \
         got {:?}: {}",
        status.code(),
        status.message()
    );
}
