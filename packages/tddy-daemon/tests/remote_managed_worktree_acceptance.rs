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

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tddy_daemon::claude_cli_session::ClaudeCliSessionManager;
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::{
    classify_codebase_placement, CodebasePlacement, ConnectionServiceImpl,
};
use tddy_daemon::livekit_peer_discovery::{LiveKitDiscoveryHandles, PEER_FORWARD_TIMEOUT};
use tddy_daemon::multi_host::{DaemonInstanceId, EligibleDaemonInfo, EligibleDaemonSource};
use tddy_daemon::test_util::TEST_TOKEN;
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, DeleteSessionRequest, ExecuteToolRequest,
    StartSessionRequest,
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

/// A daemon that gives a spawn — its own, and by assumption a peer's — `secs` to finish.
///
/// Deliberately not the 300 s default: a deadline that merely *happened* to exceed the budget on
/// the default configuration would look right while ignoring the setting entirely.
fn config_allowing_a_worktree_to_take_secs(secs: u64) -> DaemonConfig {
    let yaml = format!(
        r#"
users:
  - github_user: "testuser"
    os_user: "testuser"
daemon_instance_id: "{LOCAL_INSTANCE_ID}"
spawn_worker_request_timeout_secs: {secs}
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
    service_with_known_codebase_peer_and_config(sessions_base, test_config())
}

fn service_with_known_codebase_peer_and_config(
    sessions_base: PathBuf,
    config: DaemonConfig,
) -> ConnectionServiceImpl {
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
        config,
        resolver,
        sessions_base,
        user_resolver_valid(),
        None,
        Some(discovery),
        None,
        Arc::new(ClaudeCliSessionManager::new()),
    )
}

/// The same service, but every token resolves to a GitHub user this daemon has no OS mapping for —
/// the shape a split session takes when the codebase host was never told about the caller.
fn service_with_a_user_this_daemon_does_not_map(sessions_base: PathBuf) -> ConnectionServiceImpl {
    let resolver: SessionsBaseResolver = {
        let base = sessions_base.clone();
        Arc::new(move |_| Some(base.clone()))
    };
    let unmapped_user: UserResolver = Arc::new(|_| Some("someone-else".to_string()));
    ConnectionServiceImpl::new(
        test_config(),
        resolver,
        sessions_base,
        unmapped_user,
        None,
        None,
        None,
        Arc::new(ClaudeCliSessionManager::new()),
    )
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

/// An agent def under `<tddyhome>/agents`, the only place a YAML-defined agent resolves from. The
/// fixture passes one temp dir as both the sessions base and the daemon's data dir, so this writes
/// into the directory the service actually reads.
fn an_agent_def_on_this_host(tddy_data_dir: &Path, name: &str) {
    let agents = tddy_data_dir.join("agents");
    std::fs::create_dir_all(&agents).expect("create agents dir");
    std::fs::write(
        agents.join(format!("{name}.yaml")),
        format!(
            "name: {name}\nmodel: qwen2.5-coder:7b\nbase_url: http://127.0.0.1:11434/v1\nreplaces:\n  - Grep\n"
        ),
    )
    .expect("write agent def");
}

// ---------------------------------------------------------------------------
// classify_codebase_placement — the pure validation core
// ---------------------------------------------------------------------------

#[test]
fn an_empty_codebase_daemon_is_co_located() {
    // When
    let placement = classify_codebase_placement(LOCAL_INSTANCE_ID, "", &[], true, "claude-cli");

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
    let error = classify_codebase_placement(
        LOCAL_INSTANCE_ID,
        UNKNOWN_PEER_ID,
        &eligible,
        true,
        "claude-cli",
    )
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
// What a split placement still refuses is work with no host-independent meaning: a workflow recipe
// and a sandbox both resolve a worktree on the daemon running the agent, which a split session does
// not have.
//
// The fixture holds no LiveKit room, so the two codes say everything: `InvalidArgument` means the
// daemon refused the combination outright, `FailedPrecondition` means it accepted it and got as far
// as looking for the codebase host.

#[tokio::test]
async fn a_split_start_seeding_an_agent_of_this_host_reaches_the_codebase_host() {
    // Given a split request seeding an agent this daemon defines
    let sessions_tmp = tempfile::tempdir().unwrap();
    an_agent_def_on_this_host(sessions_tmp.path(), "fastcontext");
    let service = service_with_known_codebase_peer(sessions_tmp.path().to_path_buf());
    let request = StartSessionRequest {
        specialized_agents: vec!["fastcontext".to_string()],
        ..a_split_claude_cli_request(CODEBASE_PEER_ID)
    };

    // When
    let status = service
        .start_session(Request::new(request))
        .await
        .expect_err("no room is connected here, so even an admissible split start cannot complete");

    // Then — the seed was admitted and the start went looking for the codebase host, which is the
    // only thing that reaches this failure; a refusal of the combination would be InvalidArgument
    assert_eq!(
        status.code(),
        tddy_rpc::Code::FailedPrecondition,
        "a seeded agent must not make a valid placement a bad request; got {:?}: {}",
        status.code(),
        status.message()
    );
}

#[tokio::test]
async fn a_split_start_seeding_an_agent_no_host_defines_is_refused_naming_that_agent() {
    // Given a split request seeding a name nothing resolves — no def on this host, and no daemon
    // qualifier pointing at one anywhere else
    let sessions_tmp = tempfile::tempdir().unwrap();
    let service = service_with_known_codebase_peer(sessions_tmp.path().to_path_buf());
    let request = StartSessionRequest {
        specialized_agents: vec!["ghost-agent".to_string()],
        ..a_split_claude_cli_request(CODEBASE_PEER_ID)
    };

    // When
    let status = service
        .start_session(Request::new(request))
        .await
        .expect_err("a seed that resolves to nothing must fail the start, not be dropped");

    // Then — a bad request rather than a precondition failure, which is also what says the codebase
    // host was never contacted: reaching it in this fixture is what produces FailedPrecondition
    assert_eq!(
        status.code(),
        tddy_rpc::Code::InvalidArgument,
        "expected InvalidArgument; got {:?}: {}",
        status.code(),
        status.message()
    );
    assert!(
        status.message().contains("ghost-agent"),
        "the refusal must name the agent it could not resolve; got '{}'",
        status.message()
    );
}

#[tokio::test]
async fn a_split_start_asking_for_a_semantic_index_reaches_the_codebase_host() {
    // Given a split request that also asks for a semantic index
    let sessions_tmp = tempfile::tempdir().unwrap();
    let service = service_with_known_codebase_peer(sessions_tmp.path().to_path_buf());
    let request = StartSessionRequest {
        semantic_index: true,
        ..a_split_claude_cli_request(CODEBASE_PEER_ID)
    };

    // When
    let status = service
        .start_session(Request::new(request))
        .await
        .expect_err("no room is connected here, so even an admissible split start cannot complete");

    // Then — an index is built where the worktree is, and on a split session that host is the
    // codebase host, so the request is admissible and the start goes looking for it
    assert_eq!(
        status.code(),
        tddy_rpc::Code::FailedPrecondition,
        "a semantic index must not make a valid placement a bad request; got {:?}: {}",
        status.code(),
        status.message()
    );
}

#[tokio::test]
async fn a_split_start_carrying_a_workflow_recipe_is_still_refused_naming_the_field() {
    // Given a split request that also names a workflow recipe
    let sessions_tmp = tempfile::tempdir().unwrap();
    let service = service_with_known_codebase_peer(sessions_tmp.path().to_path_buf());
    let request = StartSessionRequest {
        recipe: "plan-tdd-one-shot".to_string(),
        ..a_split_claude_cli_request(CODEBASE_PEER_ID)
    };

    // When
    let status = service
        .start_session(Request::new(request))
        .await
        .expect_err("a recipe needs a repository beside the agent, which a split session lacks");

    // Then
    assert_eq!(
        status.code(),
        tddy_rpc::Code::InvalidArgument,
        "expected InvalidArgument; got {:?}: {}",
        status.code(),
        status.message()
    );
    assert!(
        status.message().contains("recipe"),
        "the refusal must name the field it refused; got '{}'",
        status.message()
    );
}

#[tokio::test]
async fn a_split_start_asking_for_a_sandbox_is_still_refused_naming_the_field() {
    // Given a split request that also asks to be sandboxed
    let sessions_tmp = tempfile::tempdir().unwrap();
    let service = service_with_known_codebase_peer(sessions_tmp.path().to_path_buf());
    let request = StartSessionRequest {
        sandbox: true,
        ..a_split_claude_cli_request(CODEBASE_PEER_ID)
    };

    // When
    let status = service
        .start_session(Request::new(request))
        .await
        .expect_err("a sandbox resolves a worktree beside the agent, which a split session lacks");

    // Then
    assert_eq!(
        status.code(),
        tddy_rpc::Code::InvalidArgument,
        "expected InvalidArgument; got {:?}: {}",
        status.code(),
        status.message()
    );
    assert!(
        status.message().contains("sandbox"),
        "the refusal must name the field it refused; got '{}'",
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

#[tokio::test]
async fn the_split_forward_waits_out_the_worktree_budget_plus_one_ordinary_forward() {
    // Given a daemon configured to allow ten minutes for a worktree
    let sessions_tmp = tempfile::tempdir().unwrap();
    let service = service_with_known_codebase_peer_and_config(
        sessions_tmp.path().to_path_buf(),
        config_allowing_a_worktree_to_take_secs(600),
    );

    // When
    let deadline = service.split_forward_deadline();

    // Then — the peer's whole budget, plus one ordinary forward deadline of round-trip headroom.
    // This daemon can only assume the peer's budget matches its own, which is why the configured
    // value is what it waits out rather than a constant.
    assert_eq!(
        deadline,
        Duration::from_secs(600) + PEER_FORWARD_TIMEOUT,
        "the split forward must be derived from spawn_worker_request_timeout_secs; got {deadline:?}"
    );
}

#[tokio::test]
async fn the_split_forward_outlives_the_worktree_budget_the_codebase_daemon_gives_itself() {
    // Given a daemon on the default configuration
    let sessions_tmp = tempfile::tempdir().unwrap();
    let service = service_with_known_codebase_peer(sessions_tmp.path().to_path_buf());
    let peers_worktree_budget = DaemonConfig::default().spawn_worker_request_timeout();

    // When
    let deadline = service.split_forward_deadline();

    // Then — erroring while the peer is still building is the state that stranded a worktree, so
    // this must never be the shorter of the two. The cost is a vanished peer surfacing after this
    // wait rather than after 30 s, which the PRD accepts as the cheaper failure.
    assert!(
        deadline > peers_worktree_budget,
        "the split forward deadline ({deadline:?}) must outlast the worktree budget the codebase daemon gives itself ({peers_worktree_budget:?})"
    );
}

#[tokio::test]
async fn deleting_a_split_session_refuses_while_this_daemon_cannot_reach_the_common_room() {
    // Given a split session on disk, and a daemon with no live common-room connection
    let sessions_tmp = tempfile::tempdir().unwrap();
    let service = service_with_known_codebase_peer(sessions_tmp.path().to_path_buf());
    let session_id = "019d105b-ac0f-78d3-9a89-40973114aa01";
    write_split_session_metadata(sessions_tmp.path(), session_id);

    // When the session is deleted
    let status = service
        .delete_session(Request::new(DeleteSessionRequest {
            session_token: TEST_TOKEN.to_string(),
            session_id: session_id.to_string(),
        }))
        .await
        .expect_err("a delete that cannot reach the codebase daemon must refuse");

    // Then it refuses rather than continuing. Being unable to *ask* is not the same answer as the
    // peer saying it no longer has the session: the first leaves the worktree's fate unknown, and
    // treating unknown as "already torn down" strands a checkout on a host nobody is watching —
    // which is the very leak the paired teardown exists to prevent.
    assert_eq!(
        status.code(),
        tddy_rpc::Code::FailedPrecondition,
        "expected FailedPrecondition; got {:?}: {}",
        status.code(),
        status.message()
    );
    assert!(
        sessions_tmp
            .path()
            .join("sessions")
            .join(session_id)
            .exists(),
        "the local session must survive a refused delete, so a retry can still reach its worktree"
    );
}

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

/// A stopped split session: paired to a codebase daemon, with no repository of its own.
fn write_split_session_metadata(sessions_base: &std::path::Path, session_id: &str) {
    let session_dir =
        tddy_core::session_lifecycle::unified_session_dir_path(sessions_base, session_id);
    std::fs::create_dir_all(&session_dir).expect("session dir");
    let mut metadata = tddy_testing_commons::builders::a_session_metadata()
        .with_session_id(session_id)
        .with_status("exited")
        .build();
    metadata.session_type = Some("claude-cli".to_string());
    metadata.repo_path = None;
    metadata.codebase_daemon_instance_id = Some(CODEBASE_PEER_ID.to_string());
    metadata.codebase_session_id = Some("019d105b-ac0f-78d3-9a89-40973114bb02".to_string());
    tddy_core::write_session_metadata(&session_dir, &metadata).expect("write metadata");
}
