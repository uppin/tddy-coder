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
//!
//! The exec-tool half of `tddy-session-lifecycle`'s `remote_managed_worktree_acceptance.rs`, split
//! out because the exec tools are served by `tddy-daemon-rpc`'s `ExecToolRpcHandler`. The fixture
//! is that suite's, trimmed to what these tests reach.

use std::path::PathBuf;
use std::sync::Arc;

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_livekit::livekit_peer_discovery::LiveKitDiscoveryHandles;
use tddy_daemon_rpc::test_util::TestDaemon;
use tddy_host_service::multi_host::{DaemonInstanceId, EligibleDaemonInfo, EligibleDaemonSource};
use tddy_rpc::Request;
use tddy_service::proto::exec_tools::{ExecToolService, ExecuteToolRequest};
use tddy_session_lifecycle::claude_cli_session::ClaudeCliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;
use tddy_session_lifecycle::test_util::TEST_TOKEN;

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const LOCAL_INSTANCE_ID: &str = "laptop-a";
const CODEBASE_PEER_ID: &str = "workstation-b";
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
fn service_with_known_codebase_peer(sessions_base: PathBuf) -> TestDaemon {
    service_with_known_codebase_peer_and_config(sessions_base, test_config())
}

fn service_with_known_codebase_peer_and_config(
    sessions_base: PathBuf,
    config: DaemonConfig,
) -> TestDaemon {
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
    TestDaemon::from_host(DaemonSessionHost::new(
        config,
        resolver,
        sessions_base,
        user_resolver_valid(),
        None,
        Some(discovery),
        None,
        Arc::new(ClaudeCliSessionManager::new()),
    ))
}

/// The same service, but every token resolves to a GitHub user this daemon has no OS mapping for —
/// the shape a split session takes when the codebase host was never told about the caller.
fn service_with_a_user_this_daemon_does_not_map(sessions_base: PathBuf) -> TestDaemon {
    let resolver: SessionsBaseResolver = {
        let base = sessions_base.clone();
        Arc::new(move |_| Some(base.clone()))
    };
    let unmapped_user: UserResolver = Arc::new(|_| Some("someone-else".to_string()));
    TestDaemon::from_host(DaemonSessionHost::new(
        test_config(),
        resolver,
        sessions_base,
        unmapped_user,
        None,
        None,
        None,
        Arc::new(ClaudeCliSessionManager::new()),
    ))
}

fn an_exec_tool_request(session_token: &str) -> ExecuteToolRequest {
    ExecuteToolRequest {
        session_token: session_token.to_string(),
        session_id: "019d105b-ac0f-78d3-9a89-40973114cc03".to_string(),
        tool_name: "Read".to_string(),
        args_json: r#"{"path":"README.md"}"#.to_string(),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// classify_codebase_placement — the pure validation core
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// StartSession — the request-level contract
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Seeding a split session's agent roster at start
// ---------------------------------------------------------------------------
//
// An agent is placeable on any host. One co-located with the authoritative worktree reads that
// worktree directly; one anywhere else reads a clone the session's worktree sync keeps current and
// proxies its writes back. Neither of those depends on *where the codebase is*, so a split placement
// withdraws nothing from `specialized_agents` — it only decides which host the roster and the clone
// end up on.
//
// What a split placement still refuses is work with no host-independent meaning: a workflow
// recipe resolves `TDDY_REPO_DIR` on the daemon running the agent, which a split session does not
// have. A sandbox, by contrast, confines the codebase half on a split placement (the host holding
// the checkout), so it is admissible — see `a_split_start_asking_for_a_sandbox_is_admitted_and_fails_over_the_missing_room`.
//
// The fixture holds no LiveKit room, so the two codes say everything: `InvalidArgument` means the
// daemon refused the combination outright, `FailedPrecondition` means it accepted it and got as far
// as looking for the codebase host.

// ---------------------------------------------------------------------------
// The split forward's deadline
// ---------------------------------------------------------------------------
//
// A split start is served by the codebase daemon resolving the project — cloning it first if it does
// not have it — and cutting a worktree, work that daemon bounds by its own
// `spawn_worker_request_timeout` (300 s by default). The ordinary `PEER_FORWARD_TIMEOUT` is 30 s, so
// a plain forward would give up while the peer was still building and leave the checkout behind on a
// host the operator may not be watching. That is one of the two criticals this changeset fixes; the
// other half — naming the B-side session before asking for it — is pinned by
// `a_worktree_failure_on_the_codebase_daemon_leaves_no_session_behind` in the cross-host suite.
//
// A real slow peer is not reproducible here, and simulating one with sleeps would test the sleep.
// What is worth pinning is the property: the deadline is derived from the configured budget and
// strictly exceeds it.

// ---------------------------------------------------------------------------
// Exec-tool refusals — which daemon said no
// ---------------------------------------------------------------------------
//
// A split session's tool calls are served on the *codebase* daemon, but every failure they return
// is rendered in the agent's transcript on the *agent* daemon, where a bare "invalid or expired
// session" reads as if the host the operator is looking at refused. The two likeliest split
// misconfigurations both land in exactly these two refusals — daemons not sharing
// `livekit.api_secret` (a session token is a stateless HMAC only its co-signers can verify), and a
// GitHub user mapped on the agent host but not on the codebase host — so each names the daemon that
// refused.

#[tokio::test]
async fn an_exec_tool_refused_over_an_unverifiable_token_names_the_daemon_that_refused_it() {
    // Given a daemon that cannot verify the caller's token
    let sessions_tmp = tempfile::tempdir().unwrap();
    let service = service_with_known_codebase_peer(sessions_tmp.path().to_path_buf());

    // When
    let status = service
        .execute_tool(Request::new(an_exec_tool_request("minted-elsewhere")))
        .await
        .expect_err("an unverifiable session token must be refused");

    // Then
    assert_eq!(
        status.code(),
        tddy_rpc::Code::Unauthenticated,
        "expected Unauthenticated; got {:?}: {}",
        status.code(),
        status.message()
    );
    assert!(
        status.message().contains(LOCAL_INSTANCE_ID),
        "the refusal must name the daemon that refused, or a split session's operator reads it as the agent host's answer; got '{}'",
        status.message()
    );
}

#[tokio::test]
async fn an_exec_tool_refused_for_an_unmapped_user_names_the_daemon_that_refused_it() {
    // Given a daemon that verifies the token but has no OS user for the GitHub user behind it
    let sessions_tmp = tempfile::tempdir().unwrap();
    let service = service_with_a_user_this_daemon_does_not_map(sessions_tmp.path().to_path_buf());

    // When
    let status = service
        .execute_tool(Request::new(an_exec_tool_request(TEST_TOKEN)))
        .await
        .expect_err("a user with no OS mapping must be refused");

    // Then
    assert_eq!(
        status.code(),
        tddy_rpc::Code::PermissionDenied,
        "expected PermissionDenied; got {:?}: {}",
        status.code(),
        status.message()
    );
    assert!(
        status.message().contains(LOCAL_INSTANCE_ID),
        "the refusal must name the daemon whose users[] mapping is missing; got '{}'",
        status.message()
    );
    assert!(
        status.message().contains("someone-else"),
        "the refusal must name the unmapped user so the operator knows what to add; got '{}'",
        status.message()
    );
}
