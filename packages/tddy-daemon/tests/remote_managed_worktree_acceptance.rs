//! Acceptance tests: splitting a session's *codebase* placement from its *agent* placement.
//!
//! `StartSessionRequest.codebase_daemon_instance_id` names the daemon whose filesystem holds the
//! session's git worktree. Empty or self-matching keeps today's co-located behaviour; naming a
//! different eligible daemon makes the session split — the agent runs here with no repository on
//! disk and reaches the worktree only through `mcp__tddy-tools__*` over LiveKit.
//!
//! Split placement is refused unless `managed_codebase` is set (an agent holding native filesystem
//! tools has nothing to proxy) and the session type is `claude-cli` (cursor-agent cannot enforce a
//! tool allowlist — see docs/dev/TODO.md).
//!
//! These tests need no LiveKit: a `MockEligibleDaemonSource` supplies the peer list and the room
//! slot stays `None`, so a *valid* split request reaches the routing layer and fails there with
//! `FailedPrecondition` while every *invalid* one is rejected earlier with `InvalidArgument`. That
//! two-code split is the assertion axis, mirroring `relay_peer_forwarding_acceptance.rs`.
//!
//! PRD: docs/ft/daemon/remote-managed-worktree.md.

use std::path::PathBuf;
use std::sync::Arc;

use tddy_daemon::claude_cli_session::ClaudeCliSessionManager;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::{
    classify_codebase_placement, CodebasePlacement, ConnectionServiceImpl,
};
use tddy_daemon::livekit_peer_discovery::LiveKitDiscoveryHandles;
use tddy_daemon::multi_host::{DaemonInstanceId, EligibleDaemonInfo, EligibleDaemonSource};
use tddy_daemon::test_util::TEST_TOKEN;
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, StartSessionRequest,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const LOCAL_INSTANCE_ID: &str = "laptop-a";
const CODEBASE_PEER_ID: &str = "workstation-b";
const UNKNOWN_PEER_ID: &str = "nowhere-9";
const PROJECT_ID: &str = "019d105b-ac0f-78d3-9a89-409731145a36";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

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
daemon_instance_id: "{LOCAL_INSTANCE_ID}"
"#
    );
    serde_yaml::from_str(&yaml).expect("config must parse")
}

fn user_resolver_valid() -> UserResolver {
    Arc::new(|token| {
        if token == TEST_TOKEN {
            Some("testuser".to_string())
        } else {
            None
        }
    })
}

/// A service that knows `workstation-b` as an eligible peer but holds no LiveKit room, so a valid
/// split request gets as far as routing and then fails there.
fn service_with_known_codebase_peer(sessions_base: PathBuf) -> ConnectionServiceImpl {
    let resolver: SessionsBaseResolver = {
        let base = sessions_base.clone();
        Arc::new(move |_| Some(base.clone()))
    };
    let discovery = LiveKitDiscoveryHandles {
        eligible_daemon_source: Arc::new(MockEligibleDaemonSource {
            ids: vec![CODEBASE_PEER_ID.to_string()],
        }) as Arc<dyn EligibleDaemonSource>,
        common_room_livekit_room: Arc::new(tokio::sync::RwLock::new(None)),
    };
    ConnectionServiceImpl::new(
        test_config(),
        resolver,
        sessions_base,
        user_resolver_valid(),
        None,
        Some(discovery),
        None,
        Arc::new(ClaudeCliSessionManager::new()),
    )
}

/// A managed claude-cli start request placing the codebase on `codebase_daemon_instance_id`.
fn a_split_claude_cli_request(codebase_daemon_instance_id: &str) -> StartSessionRequest {
    StartSessionRequest {
        session_token: TEST_TOKEN.to_string(),
        project_id: PROJECT_ID.to_string(),
        session_type: "claude-cli".to_string(),
        model: "claude-opus-5".to_string(),
        managed_codebase: true,
        codebase_daemon_instance_id: codebase_daemon_instance_id.to_string(),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// classify_codebase_placement — the pure validation core
// ---------------------------------------------------------------------------

#[test]
fn an_empty_codebase_daemon_is_co_located() {
    // When
    let placement =
        classify_codebase_placement(LOCAL_INSTANCE_ID, "", &[], true, "claude-cli");

    // Then — every session created before this feature existed takes exactly this path
    assert_eq!(placement, Ok(CodebasePlacement::CoLocated));
}

#[test]
fn a_codebase_daemon_matching_the_local_instance_is_co_located() {
    // When
    let placement = classify_codebase_placement(
        LOCAL_INSTANCE_ID,
        LOCAL_INSTANCE_ID,
        &[CODEBASE_PEER_ID.to_string()],
        true,
        "claude-cli",
    );

    // Then — naming your own daemon is the explicit spelling of "co-located", not a split
    assert_eq!(placement, Ok(CodebasePlacement::CoLocated));
}

#[test]
fn a_known_peer_with_managed_codebase_on_a_claude_cli_session_is_a_split_placement() {
    // When
    let placement = classify_codebase_placement(
        LOCAL_INSTANCE_ID,
        CODEBASE_PEER_ID,
        &[CODEBASE_PEER_ID.to_string()],
        true,
        "claude-cli",
    );

    // Then
    assert_eq!(
        placement,
        Ok(CodebasePlacement::Split {
            codebase_instance_id: CODEBASE_PEER_ID.to_string(),
        })
    );
}

#[test]
fn a_split_placement_without_managed_codebase_is_rejected_naming_the_flag() {
    // When
    let error = classify_codebase_placement(
        LOCAL_INSTANCE_ID,
        CODEBASE_PEER_ID,
        &[CODEBASE_PEER_ID.to_string()],
        false,
        "claude-cli",
    )
    .expect_err("a split placement without managed_codebase must be refused");

    // Then — an agent that kept its native filesystem tools has nothing to proxy through
    assert!(
        error.contains("managed_codebase"),
        "the refusal must name the missing flag so the caller can fix it; got '{error}'"
    );
}

#[test]
fn a_split_placement_on_a_cursor_cli_session_is_rejected_naming_the_session_type() {
    // When
    let error = classify_codebase_placement(
        LOCAL_INSTANCE_ID,
        CODEBASE_PEER_ID,
        &[CODEBASE_PEER_ID.to_string()],
        true,
        "cursor-cli",
    )
    .expect_err("cursor-cli cannot be split in v1");

    // Then — cursor-agent has no tool-allowlist mechanism, so managed codebase is unenforceable
    assert!(
        error.contains("cursor-cli"),
        "the refusal must name the offending session type; got '{error}'"
    );
}

#[test]
fn a_split_placement_on_a_tool_session_is_rejected_naming_the_session_type() {
    // When
    let error = classify_codebase_placement(
        LOCAL_INSTANCE_ID,
        CODEBASE_PEER_ID,
        &[CODEBASE_PEER_ID.to_string()],
        true,
        "tool",
    )
    .expect_err("tddy-coder sessions cannot be split in v1");

    // Then
    assert!(
        error.contains("tool"),
        "the refusal must name the offending session type; got '{error}'"
    );
}

#[test]
fn an_unknown_codebase_daemon_is_rejected_naming_the_id_and_the_common_room() {
    // Given a peer list that does not contain the requested daemon
    let eligible = [CODEBASE_PEER_ID.to_string()];

    // When
    let error =
        classify_codebase_placement(LOCAL_INSTANCE_ID, UNKNOWN_PEER_ID, &eligible, true, "claude-cli")
            .expect_err("an unreachable codebase daemon must be refused");

    // Then — matching `classify_peer_route`'s error shape, which names both so an operator knows
    // where to look
    assert!(
        error.contains(UNKNOWN_PEER_ID),
        "the refusal must name the unreachable daemon; got '{error}'"
    );
    assert!(
        error.contains("livekit.common_room"),
        "the refusal must point at the config key that governs peer visibility; got '{error}'"
    );
}

// ---------------------------------------------------------------------------
// StartSession — the request-level contract
// ---------------------------------------------------------------------------

#[tokio::test]
async fn start_session_with_a_codebase_daemon_but_without_managed_codebase_is_refused() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let service = service_with_known_codebase_peer(sessions_tmp.path().to_path_buf());
    let request = StartSessionRequest {
        managed_codebase: false,
        ..a_split_claude_cli_request(CODEBASE_PEER_ID)
    };

    // When
    let status = service
        .start_session(Request::new(request))
        .await
        .expect_err("split placement without managed_codebase must be refused");

    // Then — rejected as a malformed request, not silently downgraded to a co-located session
    assert_eq!(
        status.code(),
        tddy_rpc::Code::InvalidArgument,
        "expected InvalidArgument; got {:?}: {}",
        status.code(),
        status.message()
    );
    assert!(
        status.message().contains("managed_codebase"),
        "the refusal must name the missing flag; got '{}'",
        status.message()
    );
}

#[tokio::test]
async fn start_session_with_a_codebase_daemon_on_a_cursor_cli_session_is_refused() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let service = service_with_known_codebase_peer(sessions_tmp.path().to_path_buf());
    let request = StartSessionRequest {
        session_type: "cursor-cli".to_string(),
        ..a_split_claude_cli_request(CODEBASE_PEER_ID)
    };

    // When
    let status = service
        .start_session(Request::new(request))
        .await
        .expect_err("cursor-cli split placement must be refused in v1");

    // Then
    assert_eq!(
        status.code(),
        tddy_rpc::Code::InvalidArgument,
        "expected InvalidArgument; got {:?}: {}",
        status.code(),
        status.message()
    );
    assert!(
        status.message().contains("cursor-cli"),
        "the refusal must name the offending session type; got '{}'",
        status.message()
    );
}

#[tokio::test]
async fn start_session_with_an_unknown_codebase_daemon_is_refused() {
    // Given
    let sessions_tmp = tempfile::tempdir().unwrap();
    let service = service_with_known_codebase_peer(sessions_tmp.path().to_path_buf());

    // When
    let status = service
        .start_session(Request::new(a_split_claude_cli_request(UNKNOWN_PEER_ID)))
        .await
        .expect_err("an unreachable codebase daemon must be refused");

    // Then
    assert_eq!(
        status.code(),
        tddy_rpc::Code::InvalidArgument,
        "expected InvalidArgument; got {:?}: {}",
        status.code(),
        status.message()
    );
    assert!(
        status.message().contains(UNKNOWN_PEER_ID),
        "the refusal must name the unreachable daemon; got '{}'",
        status.message()
    );
}

#[tokio::test]
async fn start_session_with_a_known_codebase_daemon_and_no_livekit_room_fails_precondition() {
    // Given a valid split request whose peer is eligible but unreachable — no room is connected
    let sessions_tmp = tempfile::tempdir().unwrap();
    let service = service_with_known_codebase_peer(sessions_tmp.path().to_path_buf());

    // When
    let status = service
        .start_session(Request::new(a_split_claude_cli_request(CODEBASE_PEER_ID)))
        .await
        .expect_err("a split start with no LiveKit room must fail");

    // Then — the request itself was well-formed, so this is a precondition failure rather than an
    // argument error: the distinction is what tells an operator whether to fix the request or the
    // deployment
    assert_eq!(
        status.code(),
        tddy_rpc::Code::FailedPrecondition,
        "expected FailedPrecondition; got {:?}: {}",
        status.code(),
        status.message()
    );
}
