//! Acceptance: peer `ListProjects` fan-out is non-blocking end-to-end.
//!
//! The daemon must never park a worker thread to gather peer projects. That means:
//!   1. `EligibleDaemonSource::peer_project_entries` is an `async fn` (via `#[async_trait]`),
//!      so the `list_projects` RPC handler `.await`s it directly instead of bridging a sync
//!      trait method to the runtime with `block_in_place` + `Handle::block_on`.
//!   2. Because there is no `block_in_place`, the whole chain runs on a **current-thread**
//!      Tokio runtime (`#[tokio::test]` default) — `block_in_place` would panic there.
//!   3. The fan-out itself is concurrent, so wall-clock time is bounded by the slowest peer,
//!      not the serial sum across peers.
//!
//! These tests fail against the current sync/blocking design and pass once the trait and the
//! `merge_listed_projects_with_peers` -> `list_projects` chain are async.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_daemon::livekit_peer_discovery::{
    aggregate_peer_project_entries, LiveKitDiscoveryHandles,
};
use tddy_daemon::multi_host::{DaemonInstanceId, EligibleDaemonInfo, EligibleDaemonSource};
use tddy_daemon::test_util::TEST_TOKEN;
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ListProjectsRequest,
    ProjectEntry as ProtoProjectEntry,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

const PEER_MARKER_PROJECT_ID: &str = "bbbbbbbb-cccc-4ddd-8eee-ffffffffffff";

fn test_config_for_os_user(os_user: &str) -> DaemonConfig {
    let yaml = format!(
        r#"
users:
  - github_user: "testuser"
    os_user: "{os_user}"
"#
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).unwrap();
    DaemonConfig::load(&path).unwrap()
}

fn test_service(
    sessions_base: PathBuf,
    os_user: &str,
    eligible: Arc<dyn EligibleDaemonSource>,
) -> ConnectionServiceImpl {
    let config = test_config_for_os_user(os_user);
    let tddy_data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: UserResolver = Arc::new(|token| {
        if token == TEST_TOKEN {
            Some("testuser".to_string())
        } else {
            None
        }
    });
    ConnectionServiceImpl::new(
        config,
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        Some(LiveKitDiscoveryHandles {
            eligible_daemon_source: eligible,
            common_room_livekit_room: Arc::new(tokio::sync::RwLock::new(None)),
        }),
        None,
        Arc::new(tddy_daemon::claude_cli_session::ClaudeCliSessionManager::new()),
    )
}

/// A peer source whose `peer_project_entries` is genuinely asynchronous: it yields to the
/// runtime (proving it is a real awaited future, not a sync call bridged with `block_on`)
/// before returning one already-tagged peer row.
struct AsyncPeerProjectsSource;

#[async_trait::async_trait]
impl EligibleDaemonSource for AsyncPeerProjectsSource {
    fn list_eligible_daemons(&self) -> Vec<EligibleDaemonInfo> {
        vec![EligibleDaemonInfo {
            instance_id: DaemonInstanceId("workstation-1".to_string()),
            label: "workstation-1".to_string(),
        }]
    }

    async fn peer_project_entries(&self, session_token: &str) -> Vec<ProtoProjectEntry> {
        // Token forwarding is verified through the observable output: a mismatched token
        // yields no rows, so the merged `ListProjects` response would lack the peer marker.
        if session_token != TEST_TOKEN {
            return vec![];
        }
        // A cooperative yield only compiles inside an `async fn` — this is the point of the
        // change: the trait method is a future the handler awaits.
        tokio::task::yield_now().await;
        vec![ProtoProjectEntry {
            project_id: PEER_MARKER_PROJECT_ID.to_string(),
            name: "peer-a".to_string(),
            git_url: "https://example.com/a.git".to_string(),
            main_repo_path: "/peer/a".to_string(),
            daemon_instance_id: "workstation-1".to_string(),
            main_branch_ref: String::new(),
        }]
    }
}

#[tokio::test]
async fn peer_project_entries_is_an_awaitable_source_method() {
    // Given
    let source = AsyncPeerProjectsSource;

    // When
    let rows = source.peer_project_entries(TEST_TOKEN).await;

    // Then
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].project_id, PEER_MARKER_PROJECT_ID);
    assert_eq!(rows[0].daemon_instance_id, "workstation-1");
}

#[tokio::test]
async fn list_projects_awaits_an_async_peer_source_on_a_current_thread_runtime() {
    // Given — `#[tokio::test]` runs a current-thread runtime; a `block_in_place` bridge would
    // panic here, so a passing test proves the peer fan-out is awaited, not blocked on.
    let os_user = std::env::var("USER").expect("USER must be set for passwd-backed projects path");
    let service = test_service(
        tempfile::tempdir().unwrap().path().to_path_buf(),
        &os_user,
        Arc::new(AsyncPeerProjectsSource),
    );

    // When
    let response = service
        .list_projects(Request::new(ListProjectsRequest {
            session_token: TEST_TOKEN.to_string(),
            local_only: false,
        }))
        .await
        .expect("list_projects succeeds");

    // Then
    let peer_rows: Vec<_> = response
        .into_inner()
        .projects
        .into_iter()
        .filter(|p| p.project_id == PEER_MARKER_PROJECT_ID)
        .collect();
    assert_eq!(
        peer_rows.len(),
        1,
        "the async peer source's row must be merged into the aggregated ListProjects response"
    );
    assert_eq!(peer_rows[0].daemon_instance_id, "workstation-1");
}

fn a_project_entry(project_id: &str) -> ProtoProjectEntry {
    ProtoProjectEntry {
        project_id: project_id.to_string(),
        name: "Test Project".to_string(),
        git_url: String::new(),
        main_repo_path: "/repo".to_string(),
        daemon_instance_id: String::new(),
        main_branch_ref: String::new(),
    }
}

#[tokio::test]
async fn aggregation_fans_out_to_peers_concurrently_not_serially() {
    // Given — two peers that each take 300ms to answer.
    let forward = |peer_id: String| async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        Ok(vec![a_project_entry(&format!("proj-on-{peer_id}"))])
    };

    // When
    let started = Instant::now();
    let rows = aggregate_peer_project_entries(
        vec!["mac".to_string(), "laptop".to_string()],
        Duration::from_secs(5),
        forward,
    )
    .await;
    let elapsed = started.elapsed();

    // Then — both peers answer, and the fan-out overlaps: concurrent ~300ms, serial would be
    // ~600ms. The 450ms ceiling (> the 100ms unit budget) allows scheduling slack while still
    // failing a serial implementation.
    assert_eq!(rows.len(), 2);
    let mac = rows
        .iter()
        .find(|r| r.daemon_instance_id == "mac")
        .expect("mac peer row present");
    assert_eq!(mac.project_id, "proj-on-mac");
    let laptop = rows
        .iter()
        .find(|r| r.daemon_instance_id == "laptop")
        .expect("laptop peer row present");
    assert_eq!(laptop.project_id, "proj-on-laptop");
    assert!(
        elapsed < Duration::from_millis(450),
        "peer fan-out must be concurrent (bounded by the slowest peer), took {elapsed:?}"
    );
}
