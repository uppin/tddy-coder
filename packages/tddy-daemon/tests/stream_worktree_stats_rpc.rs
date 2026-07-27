//! ConnectionService lazy worktree-size RPCs: `StreamWorktreeStats` + `CalculateWorktreeSize`.
//!
//! Feature: docs/ft/web/worktree-disk-usage-streaming.md
//! Changeset: docs/dev/1-WIP/worktree-disk-usage-streaming.md
//!
//! Mirrors the harness in `worktrees_rpc.rs` (service construction, valid/bad session tokens, a real
//! git repo with a secondary worktree) and the streaming-read style of the inline `stream_host_stats`
//! tests in `connection_service.rs`. The disk-size walk is injected via `with_worktree_size_calculator`
//! + `WorktreeSizeCalculator::with_sizer`, so every calculation is deterministic and instant.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{Stream, StreamExt};

use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_daemon::project_storage::{self, ProjectData};
use tddy_daemon::test_util::TEST_TOKEN;
use tddy_daemon::user_sessions_path::projects_path_for_user;
use tddy_daemon::worktrees::WorktreeSizeCalculator;
use tddy_rpc::{Code, Request, Status};
use tddy_service::proto::connection::{
    CalculateWorktreeSizeRequest, ConnectionService as ConnectionServiceTrait,
    ListWorktreesForProjectRequest, StreamWorktreeStatsRequest, WorktreeRow, WorktreeSizeStatus,
    WorktreeStatsEvent,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Fixed byte count reported by the injected sizer for every worktree.
const SIZE_BYTES: u64 = 4096;

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

/// Build a service wired to `sessions_base`, with an injected instant calculator persisting under
/// `calc_root`.
fn test_service(
    sessions_base: PathBuf,
    os_user: &str,
    calc_root: PathBuf,
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
    let sizer: Arc<dyn Fn(&Path) -> u64 + Send + Sync> = Arc::new(|_| SIZE_BYTES);
    let calculator = Arc::new(WorktreeSizeCalculator::with_sizer(calc_root, 2, sizer));
    ConnectionServiceImpl::new(
        config,
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        None,
        None,
        Arc::new(tddy_daemon::claude_cli_session::ClaudeCliSessionManager::new()),
    )
    .with_worktree_size_calculator(calculator)
}

fn require_git() {
    let ok = Command::new("git")
        .arg("--version")
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    assert!(ok, "git must be available for worktree RPC tests");
}

fn run_git(cwd: &std::path::Path, args: &[&str]) {
    let st = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .status()
        .unwrap_or_else(|e| panic!("git {:?} in {:?}: {e}", args, cwd));
    assert!(st.success(), "git {:?} failed in {:?}", args, cwd);
}

/// A registered project whose main repo is a real git repo with one secondary worktree, plus the
/// wired service. Owns every temp dir so they outlive the test body.
struct Fixture {
    service: ConnectionServiceImpl,
    project_id: String,
    secondary_wt: PathBuf,
    _data_dir: tempfile::TempDir,
    _repo_tmp: tempfile::TempDir,
    _calc_dir: tempfile::TempDir,
}

fn a_project_with_a_secondary_worktree() -> Fixture {
    require_git();
    let os_user = std::env::var("USER").expect("USER must be set");

    let data_dir = tempfile::tempdir().unwrap();
    let calc_dir = tempfile::tempdir().unwrap();
    let service = test_service(
        data_dir.path().to_path_buf(),
        &os_user,
        calc_dir.path().to_path_buf(),
    );
    let projects_dir =
        projects_path_for_user(&os_user, Some(data_dir.path())).expect("projects dir");

    let repo_tmp = tempfile::tempdir().unwrap();
    let repo = repo_tmp.path().join("main");
    std::fs::create_dir_all(&repo).unwrap();
    run_git(&repo, &["init"]);
    run_git(&repo, &["config", "user.email", "t@e.st"]);
    run_git(&repo, &["config", "user.name", "t"]);
    std::fs::write(repo.join("README.md"), "x\n").unwrap();
    run_git(&repo, &["add", "README.md"]);
    run_git(&repo, &["commit", "-m", "init"]);
    let wt = repo_tmp.path().join("wt-secondary");
    run_git(
        &repo,
        &["worktree", "add", wt.to_str().unwrap(), "-b", "size-branch"],
    );
    let secondary_wt = wt.canonicalize().unwrap();

    let main_repo_path = repo.canonicalize().unwrap();
    let project_id = uuid::Uuid::new_v4().to_string();
    project_storage::add_project(
        &projects_dir,
        ProjectData {
            project_id: project_id.clone(),
            name: "worktree-size-test".to_string(),
            git_url: "https://example.com/r.git".to_string(),
            main_repo_path: main_repo_path.display().to_string(),
            main_branch_ref: None,
            remote_name: None,
            host_repo_paths: std::collections::HashMap::new(),
        },
    )
    .unwrap();

    Fixture {
        service,
        project_id,
        secondary_wt,
        _data_dir: data_dir,
        _repo_tmp: repo_tmp,
        _calc_dir: calc_dir,
    }
}

/// Await the next stream event with a bounded timeout so a missing event fails loudly instead of
/// hanging the test.
async fn next_event(
    stream: &mut (impl Stream<Item = Result<WorktreeStatsEvent, Status>> + Unpin),
) -> WorktreeStatsEvent {
    tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .expect("no worktree-stats event arrived within the timeout")
        .expect("worktree-stats stream closed unexpectedly")
        .expect("worktree-stats stream yielded an error")
}

/// Drain `updated` events until one reports a `CACHED` row, returning it. Bounded so a stream that
/// never caches fails loudly. The fixture has two worktrees, each emitting a `Calculating` then a
/// `Cached` update, so 32 events is ample headroom.
async fn next_cached_update(
    stream: &mut (impl Stream<Item = Result<WorktreeStatsEvent, Status>> + Unpin),
) -> WorktreeRow {
    for _ in 0..32 {
        let event = next_event(stream).await;
        if let Some(row) = event.updated {
            if row.size_status == WorktreeSizeStatus::Cached as i32 {
                return row;
            }
        }
    }
    panic!("no CACHED worktree-size update arrived on the stream");
}

/// Poll `ListWorktreesForProject` until the worktree whose path contains `needle` reports `CACHED`,
/// returning that row. Bounded so a worktree that never caches fails loudly.
async fn cached_list_row(
    service: &ConnectionServiceImpl,
    project_id: &str,
    needle: &str,
) -> WorktreeRow {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        let resp = service
            .list_worktrees_for_project(Request::new(ListWorktreesForProjectRequest {
                session_token: TEST_TOKEN.to_string(),
                project_id: project_id.to_string(),
                refresh: true,
            }))
            .await
            .expect("ListWorktreesForProject")
            .into_inner();
        let row = resp.worktrees.into_iter().find(|r| r.path.contains(needle));
        if let Some(row) = row {
            if row.size_status == WorktreeSizeStatus::Cached as i32 {
                return row;
            }
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "worktree '{needle}' never reached CACHED in ListWorktreesForProject"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Invalid session token is rejected before any project/worktree work.
#[tokio::test]
async fn stream_worktree_stats_rejects_an_invalid_token() {
    // Given a wired service
    let fixture = a_project_with_a_secondary_worktree();

    // When an unauthenticated caller subscribes to the worktree-stats stream
    let err = fixture
        .service
        .stream_worktree_stats(Request::new(StreamWorktreeStatsRequest {
            session_token: "bad-token".to_string(),
            project_id: fixture.project_id.clone(),
            recalculate_all: false,
        }))
        .await
        .unwrap_err();

    // Then the subscription is rejected as unauthenticated
    assert_eq!(err.code, Code::Unauthenticated);
}

/// The first event is a full snapshot (each worktree NONE until sized); a later event carries the
/// secondary worktree flipped to CACHED with its byte count and a non-zero calculation timestamp.
#[tokio::test]
async fn stream_worktree_stats_emits_a_snapshot_then_a_cached_increment() {
    // Given a project with a secondary worktree and an instant sizer
    let fixture = a_project_with_a_secondary_worktree();

    // When an authenticated caller subscribes
    let mut stream = fixture
        .service
        .stream_worktree_stats(Request::new(StreamWorktreeStatsRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: fixture.project_id.clone(),
            recalculate_all: false,
        }))
        .await
        .expect("StreamWorktreeStats")
        .into_inner();

    // Then the first event is a snapshot listing every worktree, the secondary one not yet sized
    let snapshot_event = next_event(&mut stream).await;
    assert!(
        snapshot_event.updated.is_none(),
        "the first event must be a snapshot, not a single-row update"
    );
    let secondary_snapshot = snapshot_event
        .snapshot
        .iter()
        .find(|r| r.path.contains("wt-secondary"))
        .expect("snapshot must list the secondary worktree");
    assert_eq!(
        secondary_snapshot.size_status,
        WorktreeSizeStatus::None as i32
    );

    // And a later event carries the secondary worktree flipped to CACHED with its size
    let cached = next_cached_update(&mut stream).await;
    assert_eq!(cached.disk_bytes, SIZE_BYTES);
    assert_ne!(
        cached.size_calculated_at_unix_ms, 0,
        "a cached row must carry a non-zero calculation timestamp"
    );
}

/// `CalculateWorktreeSize` for a listed worktree returns ok and drives that worktree to CACHED,
/// observable through `ListWorktreesForProject`.
#[tokio::test]
async fn calculate_worktree_size_enqueues_a_listed_worktree() {
    // Given a project with a secondary worktree
    let fixture = a_project_with_a_secondary_worktree();

    // When the secondary worktree's size is (re)triggered
    let resp = fixture
        .service
        .calculate_worktree_size(Request::new(CalculateWorktreeSizeRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: fixture.project_id.clone(),
            worktree_path: fixture.secondary_wt.display().to_string(),
        }))
        .await
        .expect("CalculateWorktreeSize")
        .into_inner();

    // Then the call is accepted and the worktree becomes CACHED with its calculated size
    assert!(resp.ok, "calculate should be accepted: {}", resp.message);
    let row = cached_list_row(&fixture.service, &fixture.project_id, "wt-secondary").await;
    assert_eq!(row.disk_bytes, SIZE_BYTES);
}

/// A path that is not in `git worktree list` is refused.
#[tokio::test]
async fn calculate_worktree_size_rejects_a_path_not_in_the_worktree_list() {
    // Given a project with a secondary worktree
    let fixture = a_project_with_a_secondary_worktree();

    // When a bogus path (not a registered worktree) is (re)triggered
    let err = fixture
        .service
        .calculate_worktree_size(Request::new(CalculateWorktreeSizeRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: fixture.project_id.clone(),
            worktree_path: "/definitely/not/a/worktree".to_string(),
        }))
        .await
        .unwrap_err();

    // Then the call is refused because the path is not a listed worktree
    assert_eq!(err.code, Code::NotFound);
}
