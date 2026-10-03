//! What the daemon owes the session header's indexing indicator: once a session's worktree exists,
//! warm its code index in the background through the index daemon this daemon manages, keep the
//! latest progress per session, and deliver it over `code_navigation.WatchCodeIndex` until the index
//! is ready — or until the warm fails, which is reported and never takes the session with it.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-10-03-indexing-indicators.md
//!
//! The index daemon is real process management over a stand-in, exactly as in
//! `code_navigation_acceptance.rs`: the registry starts a shell script that points the socket it
//! was told to bind at a `code_index` server this test hosts. So the lazy start, the readiness wait
//! and the dial are the production ones, and what answers `Warm` is a fake whose stream the test
//! drives.
//!
//! The warm is started by calling `code_index_warmup::warm_for_session` directly — the seam a
//! started session's worktree reaches — rather than by starting a session: the session start lives
//! in `tddy-session-lifecycle`, and what that crate streams while it starts is pinned there
//! (`start_phase_acceptance.rs`).

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use prost::Message;
use tddy_daemon::index_daemon::{IndexDaemonRegistry, IndexDaemonSpawn};
use tddy_daemon_rpc::code_index_warmup::{warm_for_session, SessionIndexProgress};
use tddy_daemon_rpc::code_navigation::{
    build_code_navigation_entry, CodeNavigationServiceImpl, IndexChannelSource,
};
use tddy_index_daemon::proto::code_index as index;
use tddy_index_daemon::proto::code_index::{CodeIndexService, CodeIndexServiceTonicAdapter};
use tddy_index_daemon::proto::tonic_code_index::code_index_service_server::CodeIndexServiceServer as TonicCodeIndexServiceServer;
use tddy_index_daemon::EventStream;
use tddy_service::proto::code_navigation::{CodeIndexProgress, WatchCodeIndexRequest};
use tddy_task::TaskRegistry;
use tddy_worktree_service::test_util::{current_os_user, require_git, test_service, TEST_TOKEN};
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Long enough that only a stand-in that never binds could exhaust it.
const A_READINESS_BUDGET_NOTHING_SHOULD_WAIT_OUT: Duration = Duration::from_secs(60);

/// Per awaited event: a warm here is a local socket round-trip to a fake, not an index load.
const AN_EVENT_IS_DUE_WITHIN: Duration = Duration::from_secs(30);

const THE_SESSION: &str = "019d105b-ac0f-78d3-9a89-409731145a01";

// ---------------------------------------------------------------------------------------------
// A session's worktree

/// A session's worktree holding a crate — a Rust workspace root — and the worktree service this
/// daemon authorises its code-navigation requests through.
struct ASessionWorktree {
    path: PathBuf,
    worktree_service: tddy_worktree_service::WorktreeServiceImpl,
    _data_dir: TempDir,
    _dir: TempDir,
}

fn a_rust_session_worktree() -> ASessionWorktree {
    require_git();
    let data_dir = TempDir::new().expect("a data directory");
    let worktree_service = test_service(data_dir.path().to_path_buf(), &current_os_user());
    let dir = TempDir::new().expect("a directory for the worktree");
    let path = dir.path().join("wt-session");
    std::fs::create_dir_all(path.join("src")).expect("the worktree's src");
    std::fs::write(
        path.join("Cargo.toml"),
        "[package]\nname = \"subject\"\nversion = \"0.1.0\"\n",
    )
    .expect("a manifest");
    std::fs::write(path.join("src/main.rs"), "fn main() {}\n").expect("main.rs");
    git(&path, &["init", "-q"]);
    ASessionWorktree {
        path: path.canonicalize().expect("an existing worktree"),
        worktree_service,
        _data_dir: data_dir,
        _dir: dir,
    }
}

fn git(cwd: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .status()
        .unwrap_or_else(|err| panic!("git {args:?} in {cwd:?}: {err}"));
    assert!(status.success(), "git {args:?} failed in {cwd:?}");
}

// ---------------------------------------------------------------------------------------------
// The index daemon: a stand-in process in front of a fake `code_index` server

/// What the fake's `Warm` streams: whatever the test sends through the matching [`AWarmDriver`],
/// ending when the driver is dropped.
type WarmScript = mpsc::Receiver<Result<index::IndexProgress, tddy_rpc::Status>>;

/// A `code_index` server whose `Warm` the test drives and which records every warm it was asked
/// for. Nothing else on the service is part of what this suite exercises.
#[derive(Clone)]
struct AFakeIndex {
    warm_script: Arc<Mutex<Option<WarmScript>>>,
    warms_asked: Arc<Mutex<Vec<index::WarmRequest>>>,
}

/// The test's end of the fake's `Warm` stream.
struct AWarmDriver {
    tx: mpsc::Sender<Result<index::IndexProgress, tddy_rpc::Status>>,
}

fn a_fake_index_whose_warm_the_test_drives() -> (AFakeIndex, AWarmDriver) {
    let (tx, rx) = mpsc::channel(16);
    let fake = AFakeIndex {
        warm_script: Arc::new(Mutex::new(Some(rx))),
        warms_asked: Arc::new(Mutex::new(Vec::new())),
    };
    (fake, AWarmDriver { tx })
}

impl AFakeIndex {
    fn warms_it_was_asked_for(&self) -> Vec<index::WarmRequest> {
        self.warms_asked.lock().expect("the request log").clone()
    }
}

impl AWarmDriver {
    async fn reports(&self, progress: index::IndexProgress) {
        self.tx
            .send(Ok(progress))
            .await
            .expect("the fake's warm stream is open");
    }

    async fn fails_with(self, status: tddy_rpc::Status) {
        self.tx
            .send(Err(status))
            .await
            .expect("the fake's warm stream is open");
    }
}

fn indexing_at(percentage: u32) -> index::IndexProgress {
    index::IndexProgress {
        line: format!("Indexing {percentage}%"),
        phase: "Indexing".to_string(),
        percentage,
        furthest: format!("{percentage}%"),
        ready: false,
    }
}

fn the_index_is_ready() -> index::IndexProgress {
    index::IndexProgress {
        line: "ready".to_string(),
        phase: "Indexing".to_string(),
        percentage: 100,
        furthest: "100%".to_string(),
        ready: true,
    }
}

fn not_part_of_this_fake() -> tddy_rpc::Status {
    tddy_rpc::Status::unimplemented("not part of the fake index")
}

#[async_trait::async_trait]
impl CodeIndexService for AFakeIndex {
    type WarmStream = EventStream<index::IndexProgress>;
    type CheckStream = EventStream<index::RestructureEvent>;
    type ApplyStream = EventStream<index::RestructureEvent>;
    type CoverageStream = EventStream<index::AnalyzeEvent>;
    type DuplicateTestsStream = EventStream<index::AnalyzeEvent>;

    async fn warm(
        &self,
        request: tddy_rpc::Request<index::WarmRequest>,
    ) -> Result<tddy_rpc::Response<Self::WarmStream>, tddy_rpc::Status> {
        self.warms_asked
            .lock()
            .expect("the request log")
            .push(request.into_inner());
        let script = self
            .warm_script
            .lock()
            .expect("the warm script")
            .take()
            .ok_or_else(|| tddy_rpc::Status::internal("this fake warms once"))?;
        Ok(tddy_rpc::Response::new(EventStream::new(script)))
    }

    async fn definition(
        &self,
        _request: tddy_rpc::Request<index::DefinitionRequest>,
    ) -> Result<tddy_rpc::Response<index::DefinitionResponse>, tddy_rpc::Status> {
        Err(not_part_of_this_fake())
    }

    async fn references(
        &self,
        _request: tddy_rpc::Request<index::ReferencesRequest>,
    ) -> Result<tddy_rpc::Response<index::ReferencesResponse>, tddy_rpc::Status> {
        Err(not_part_of_this_fake())
    }

    async fn hover(
        &self,
        _request: tddy_rpc::Request<index::HoverRequest>,
    ) -> Result<tddy_rpc::Response<index::HoverResponse>, tddy_rpc::Status> {
        Err(not_part_of_this_fake())
    }

    async fn check(
        &self,
        _request: tddy_rpc::Request<index::CheckRequest>,
    ) -> Result<tddy_rpc::Response<Self::CheckStream>, tddy_rpc::Status> {
        Err(not_part_of_this_fake())
    }

    async fn apply(
        &self,
        _request: tddy_rpc::Request<index::ApplyRequest>,
    ) -> Result<tddy_rpc::Response<Self::ApplyStream>, tddy_rpc::Status> {
        Err(not_part_of_this_fake())
    }

    async fn anchors(
        &self,
        _request: tddy_rpc::Request<index::AnchorsRequest>,
    ) -> Result<tddy_rpc::Response<index::AnchorsResponse>, tddy_rpc::Status> {
        Err(not_part_of_this_fake())
    }

    async fn plan_status(
        &self,
        _request: tddy_rpc::Request<index::PlanStatusRequest>,
    ) -> Result<tddy_rpc::Response<index::PlanStatusResponse>, tddy_rpc::Status> {
        Err(not_part_of_this_fake())
    }

    async fn load_plans(
        &self,
        _request: tddy_rpc::Request<index::LoadPlansRequest>,
    ) -> Result<tddy_rpc::Response<index::PlansResponse>, tddy_rpc::Status> {
        Err(not_part_of_this_fake())
    }

    async fn unload_plans(
        &self,
        _request: tddy_rpc::Request<index::UnloadPlansRequest>,
    ) -> Result<tddy_rpc::Response<index::PlansResponse>, tddy_rpc::Status> {
        Err(not_part_of_this_fake())
    }

    async fn list_plans(
        &self,
        _request: tddy_rpc::Request<index::ListPlansRequest>,
    ) -> Result<tddy_rpc::Response<index::PlansResponse>, tddy_rpc::Status> {
        Err(not_part_of_this_fake())
    }

    async fn verify(
        &self,
        _request: tddy_rpc::Request<index::VerifyRequest>,
    ) -> Result<tddy_rpc::Response<index::VerifyResponse>, tddy_rpc::Status> {
        Err(not_part_of_this_fake())
    }

    async fn coverage(
        &self,
        _request: tddy_rpc::Request<index::CoverageRequest>,
    ) -> Result<tddy_rpc::Response<Self::CoverageStream>, tddy_rpc::Status> {
        Err(not_part_of_this_fake())
    }

    async fn report(
        &self,
        _request: tddy_rpc::Request<index::ReportRequest>,
    ) -> Result<tddy_rpc::Response<index::ReportResponse>, tddy_rpc::Status> {
        Err(not_part_of_this_fake())
    }

    async fn duplicate_tests(
        &self,
        _request: tddy_rpc::Request<index::DuplicateTestsRequest>,
    ) -> Result<tddy_rpc::Response<Self::DuplicateTestsStream>, tddy_rpc::Status> {
        Err(not_part_of_this_fake())
    }

    async fn complexity(
        &self,
        _request: tddy_rpc::Request<index::ComplexityRequest>,
    ) -> Result<tddy_rpc::Response<index::ComplexityResponse>, tddy_rpc::Status> {
        Err(not_part_of_this_fake())
    }

    async fn workspaces(
        &self,
        _request: tddy_rpc::Request<index::WorkspacesRequest>,
    ) -> Result<tddy_rpc::Response<index::WorkspacesResponse>, tddy_rpc::Status> {
        Err(not_part_of_this_fake())
    }
}

/// Where the managed index daemon lives: the stand-in program the registry starts, the socket it
/// is told to bind, and the socket the fake index actually serves on.
struct AnIndexDaemonHost {
    dir: TempDir,
}

/// A stand-in that points the socket it was told to bind at `fake` and stays up until stopped.
/// `fake` is already serving when this returns.
async fn an_index_daemon_host_serving(fake: AFakeIndex) -> AnIndexDaemonHost {
    let host = AnIndexDaemonHost {
        dir: TempDir::new().expect("a scratch directory for the index daemon"),
    };
    let listener = tokio::net::UnixListener::bind(host.fake_socket_path())
        .expect("the fake index binds its socket");
    tokio::spawn(
        tonic::transport::Server::builder()
            .add_service(TonicCodeIndexServiceServer::new(
                CodeIndexServiceTonicAdapter::new(Arc::new(fake)),
            ))
            .serve_with_incoming(tokio_stream::wrappers::UnixListenerStream::new(listener)),
    );
    let program = host.dir.path().join("tddy-index-daemon");
    std::fs::write(
        &program,
        format!(
            "#!/bin/sh\nln -s {fake} \"$2\"\nexec sleep 86400\n",
            fake = host.fake_socket_path().display(),
        ),
    )
    .expect("write the stand-in index daemon");
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755))
        .expect("make the stand-in index daemon executable");
    host
}

impl AnIndexDaemonHost {
    fn fake_socket_path(&self) -> PathBuf {
        self.dir.path().join("fake-index.sock")
    }

    /// The registry an `index_daemon:` section pointing at this host would build.
    fn registry(&self) -> IndexDaemonRegistry {
        IndexDaemonRegistry::new(
            IndexDaemonSpawn {
                program: self.dir.path().join("tddy-index-daemon"),
                socket_path: self.dir.path().join("index.sock"),
                ready_timeout: A_READINESS_BUDGET_NOTHING_SHOULD_WAIT_OUT,
                idle_timeout: Duration::from_secs(600),
            },
            TaskRegistry::new(),
        )
    }
}

// ---------------------------------------------------------------------------------------------
// Starting a warm, and watching it

/// Warm `worktree` for [`THE_SESSION`] and expect a warm to have started.
fn a_warm_of(
    worktree: &ASessionWorktree,
    registry: &IndexDaemonRegistry,
    progress: &SessionIndexProgress,
) -> tokio::task::JoinHandle<()> {
    let port = Arc::new(registry.clone()) as Arc<dyn IndexChannelSource>;
    warm_for_session(Some(&port), progress, THE_SESSION, &worktree.path)
        .expect("a session on a Rust worktree starts a warm")
}

/// Wait until [`THE_SESSION`]'s latest progress is `expected`.
async fn until_the_latest_progress_is(
    progress: &SessionIndexProgress,
    expected: CodeIndexProgress,
) {
    let mut watching = progress.watch(THE_SESSION);
    tokio::time::timeout(
        AN_EVENT_IS_DUE_WITHIN,
        watching.wait_for(|latest| latest.as_ref() == Some(&expected)),
    )
    .await
    .unwrap_or_else(|_| panic!("the session's progress never became {expected:?}"))
    .expect("the progress holder outlives the wait");
}

fn the_navigation_entry(
    worktree: &ASessionWorktree,
    index_daemon: Option<IndexDaemonRegistry>,
    progress: &SessionIndexProgress,
) -> tddy_rpc::ServiceEntry {
    build_code_navigation_entry(
        CodeNavigationServiceImpl::new(
            Arc::new(worktree.worktree_service.clone()),
            index_daemon.map(|registry| Arc::new(registry) as Arc<dyn IndexChannelSource>),
        )
        .with_index_progress(progress.clone()),
    )
}

/// Open `WatchCodeIndex` for [`THE_SESSION`] at the registered coordinate.
async fn watch_code_index_at(
    entry: &tddy_rpc::ServiceEntry,
) -> mpsc::Receiver<Result<Vec<u8>, tddy_rpc::Status>> {
    let request = WatchCodeIndexRequest {
        session_token: TEST_TOKEN.to_string(),
        session_id: THE_SESSION.to_string(),
    };
    let message = tddy_rpc::RpcMessage::new(
        request.encode_to_vec(),
        tddy_rpc::RequestMetadata::over(tddy_rpc::RequestTransport::Direct),
    );
    match entry
        .service
        .handle_rpc(entry.name, "WatchCodeIndex", &message)
        .await
    {
        tddy_rpc::RpcResult::ServerStream(opened) => {
            opened.expect("WatchCodeIndex opens a stream for the session")
        }
        tddy_rpc::RpcResult::Unary(_) => panic!("WatchCodeIndex is a server stream"),
    }
}

/// Every message a `WatchCodeIndex` stream delivers until it ends.
async fn everything_delivered_by(
    mut stream: mpsc::Receiver<Result<Vec<u8>, tddy_rpc::Status>>,
) -> Vec<CodeIndexProgress> {
    let mut delivered = Vec::new();
    while let Some(item) = tokio::time::timeout(AN_EVENT_IS_DUE_WITHIN, stream.recv())
        .await
        .expect("WatchCodeIndex neither delivered nor ended within the timeout")
    {
        let payload = item.expect("WatchCodeIndex delivers progress, not an error");
        delivered.push(CodeIndexProgress::decode(payload.as_slice()).expect("progress decodes"));
    }
    delivered
}

fn the_header_sees_indexing_at(percentage: u32) -> CodeIndexProgress {
    CodeIndexProgress {
        line: format!("Indexing {percentage}%"),
        phase: "Indexing".to_string(),
        percentage,
        furthest: format!("{percentage}%"),
        ready: false,
        error: String::new(),
    }
}

fn the_header_sees_ready() -> CodeIndexProgress {
    CodeIndexProgress {
        line: "ready".to_string(),
        phase: "Indexing".to_string(),
        percentage: 100,
        furthest: "100%".to_string(),
        ready: true,
        error: String::new(),
    }
}

// ---------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn a_session_on_a_rust_worktree_starts_warm_once_its_worktree_exists() {
    // Given a session's Rust worktree and a managed index daemon whose warm finishes at once
    let worktree = a_rust_session_worktree();
    let (fake, warm) = a_fake_index_whose_warm_the_test_drives();
    let host = an_index_daemon_host_serving(fake.clone()).await;
    let progress = SessionIndexProgress::new();
    warm.reports(the_index_is_ready()).await;
    drop(warm);

    // When the session's worktree is handed to the warm-up
    let warming = a_warm_of(&worktree, &host.registry(), &progress);
    tokio::time::timeout(AN_EVENT_IS_DUE_WITHIN, warming)
        .await
        .expect("the warm finishes within the timeout")
        .expect("the warm's task does not panic");

    // Then the index daemon was asked to warm exactly that worktree
    assert_eq!(
        fake.warms_it_was_asked_for(),
        vec![index::WarmRequest {
            workspace_root: worktree.path.display().to_string(),
        }]
    );
    // And the session's latest progress is the index being ready
    assert_eq!(progress.latest(THE_SESSION), Some(the_header_sees_ready()));
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_code_index_delivers_phase_percentage_and_ready() {
    // Given a session whose warm has reached 40% of indexing
    let worktree = a_rust_session_worktree();
    let (fake, warm) = a_fake_index_whose_warm_the_test_drives();
    let host = an_index_daemon_host_serving(fake).await;
    let registry = host.registry();
    let progress = SessionIndexProgress::new();
    let _warming = a_warm_of(&worktree, &registry, &progress);
    warm.reports(indexing_at(40)).await;
    until_the_latest_progress_is(&progress, the_header_sees_indexing_at(40)).await;

    // When the session header watches the session's code index, and the index then finishes
    let entry = the_navigation_entry(&worktree, Some(registry), &progress);
    let watching = watch_code_index_at(&entry).await;
    warm.reports(the_index_is_ready()).await;
    drop(warm);

    // Then it is told the phase and percentage it joined at, then that the index is ready, and the
    // stream ends there
    assert_eq!(
        everything_delivered_by(watching).await,
        vec![the_header_sees_indexing_at(40), the_header_sees_ready()]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn without_an_index_daemon_no_warm_starts() {
    // Given a daemon configured without an `index_daemon:` section
    let worktree = a_rust_session_worktree();
    let progress = SessionIndexProgress::new();

    // When the session's worktree is handed to the warm-up
    let warming = warm_for_session(None, &progress, THE_SESSION, &worktree.path);

    // Then nothing is started and the session has no progress
    assert!(warming.is_none(), "no warm starts without an index daemon");
    assert_eq!(progress.latest(THE_SESSION), None);
    // And watching its code index ends at once with nothing to show, so no indicator shows
    let entry = the_navigation_entry(&worktree, None, &progress);
    assert_eq!(
        everything_delivered_by(watch_code_index_at(&entry).await).await,
        vec![]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_warm_failure_is_reported_and_the_session_stays_usable() {
    // Given a managed index daemon whose warm of the session's worktree fails
    let worktree = a_rust_session_worktree();
    let (fake, warm) = a_fake_index_whose_warm_the_test_drives();
    let host = an_index_daemon_host_serving(fake).await;
    let registry = host.registry();
    let progress = SessionIndexProgress::new();
    warm.fails_with(tddy_rpc::Status::internal(
        "rust-analyzer exited during startup",
    ))
    .await;

    // When the session's worktree is handed to the warm-up and the warm runs its course
    let warming = a_warm_of(&worktree, &registry, &progress);
    tokio::time::timeout(AN_EVENT_IS_DUE_WITHIN, warming)
        .await
        .expect("a failed warm still finishes within the timeout")
        .expect("a failed warm is recorded, not raised: the warm's task does not panic");

    // Then the session's indicator ends on that failure, naming its reason
    let entry = the_navigation_entry(&worktree, Some(registry), &progress);
    let delivered = everything_delivered_by(watch_code_index_at(&entry).await).await;
    assert_eq!(delivered.len(), 1, "one final message: {delivered:?}");
    assert!(!delivered[0].ready, "a failed warm is never ready");
    // How the reason is framed (a prefix naming the warm, the status code) is the implementation's
    // to choose; what the operator must see is the index daemon's own reason.
    assert!(
        delivered[0]
            .error
            .contains("rust-analyzer exited during startup"),
        "the indicator names the failure's reason, was {:?}",
        delivered[0].error
    );
}
