//! Scaffolding shared by the suites that follow a worktree's code index through this daemon:
//! `code_index_warmup_acceptance.rs` (the session header's `WatchCodeIndex`) and
//! `code_session_acceptance.rs` (the code pane's `CodeSession`).
//!
//! - [`AScriptedIndex`] — an in-memory `code_index` server holding **one root's progress**, which the
//!   test moves (`IDLE` → `LOADING` → `READY`, or `FAILED`) and which every progress RPC follows:
//!   `WatchRoot` forwards it until the caller leaves (or the test stops it following), `Warm`
//!   forwards it until `READY`. Its navigation answers follow the same state — empty and `LOADING`
//!   until the root is ready, empty and `FAILED` once it failed, and once it is ready the canned
//!   definition, references and hover of the call to `foo` (nothing anywhere else) — so "held until
//!   ready" is behaviour of the fake, not a script of canned replies.
//! - [`AnIndexDaemonHost`] — real process management over a stand-in, as in
//!   `code_navigation_acceptance.rs`: the registry starts a shell script that points the socket it
//!   was told to bind at the fake.
//! - [`a_project_with_a_session_on_its_worktree`] — a registered project, one listed worktree
//!   holding a crate, and a session whose `.session.yaml` records that worktree, under the user
//!   [`TEST_TOKEN`] authenticates.
//!
//! Each suite uses a subset of what is here; an item only one suite uses carries a targeted
//! `#[allow(dead_code)]` naming that suite, so a helper neither uses surfaces as dead.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tddy_daemon::index_daemon::{IndexDaemonRegistry, IndexDaemonSpawn};
use tddy_daemon_kernel::user_paths::projects_path_for_user;
use tddy_index_daemon::proto::code_index as index;
use tddy_index_daemon::proto::code_index::{CodeIndexService, CodeIndexServiceTonicAdapter};
use tddy_index_daemon::proto::tonic_code_index::code_index_service_server::CodeIndexServiceServer as TonicCodeIndexServiceServer;
use tddy_index_daemon::EventStream;
use tddy_service::proto::code_navigation::{
    CodeIndexPhase, CodeIndexProgress, CodeIndexState, CodeLocation, SourcePosition, SourceRange,
};
use tddy_task::TaskRegistry;
use tddy_testing_commons::builders::a_session_metadata;
use tddy_worktree_service::project_storage::{self, ProjectData};
use tddy_worktree_service::test_util::{current_os_user, require_git, test_service};
use tempfile::TempDir;
use tokio::sync::{mpsc, watch};

pub use tddy_worktree_service::test_util::TEST_TOKEN;

/// Long enough that only a stand-in that never binds could exhaust it.
pub const A_READINESS_BUDGET_NOTHING_SHOULD_WAIT_OUT: Duration = Duration::from_secs(60);

/// Per awaited event: everything here is a local socket round-trip to a fake, not an index load —
/// except the first, which waits on the registry starting the stand-in index daemon (a `sh`
/// process). The figure is headroom for a loaded CI machine, not an expected wait: at 5 s the
/// code-session suite timed out 2–9 of 17 tests on a host at load ~100, and a bound only delays a
/// failure's report, never a pass.
pub const AN_EVENT_IS_DUE_WITHIN: Duration = Duration::from_secs(15);

/// The session the header watches.
pub const THE_SESSION: &str = "019d105b-ac0f-78d3-9a89-409731145a01";

// ---------------------------------------------------------------------------------------------
// A project, its worktree, and a session on it

/// A registered project whose main repo has one secondary worktree, `wt-feature`, holding a crate
/// whose `src/main.rs` calls `foo` on line 2 — and [`THE_SESSION`], whose `.session.yaml` records
/// that worktree as its repo path, the way a started session does.
pub struct AProjectWithASession {
    pub project_id: String,
    /// The listed worktree, canonical.
    pub worktree_path: String,
    pub worktree_service: tddy_worktree_service::WorktreeServiceImpl,
    /// [`THE_SESSION`]'s directory, holding its `.session.yaml`.
    session_dir: PathBuf,
    _data_dir: TempDir,
    _repos: TempDir,
}

pub fn a_project_with_a_session_on_its_worktree() -> AProjectWithASession {
    require_git();
    let os_user = current_os_user();
    let data_dir = TempDir::new().expect("a data directory");
    let worktree_service = test_service(data_dir.path().to_path_buf(), &os_user);
    let projects_dir =
        projects_path_for_user(&os_user, Some(data_dir.path())).expect("a projects directory");

    let repos = TempDir::new().expect("a directory for the repositories");
    let main_repo = repos.path().join("main");
    std::fs::create_dir_all(main_repo.join("src")).expect("the main repo's src");
    git(&main_repo, &["init", "-q"]);
    git(&main_repo, &["config", "user.email", "t@e.st"]);
    git(&main_repo, &["config", "user.name", "t"]);
    std::fs::write(
        main_repo.join("Cargo.toml"),
        "[package]\nname = \"subject\"\nversion = \"0.1.0\"\n",
    )
    .expect("a manifest");
    std::fs::write(
        main_repo.join("src/main.rs"),
        "fn main() {\n    foo();\n}\n",
    )
    .expect("main.rs");
    git(&main_repo, &["add", "."]);
    git(&main_repo, &["commit", "-q", "-m", "init"]);
    let worktree = repos.path().join("wt-feature");
    git(
        &main_repo,
        &[
            "worktree",
            "add",
            "-q",
            worktree.to_str().expect("a UTF-8 path"),
            "-b",
            "feature-x",
        ],
    );

    let project_id = "code-index-project".to_string();
    project_storage::add_project(
        &projects_dir,
        ProjectData {
            project_id: project_id.clone(),
            name: "code-index".to_string(),
            git_url: "https://example.com/code-index.git".to_string(),
            main_repo_path: canonical(&main_repo),
            main_branch_ref: None,
            remote_name: None,
            host_repo_paths: std::collections::HashMap::new(),
            accounts: Vec::new(),
            local_gh: false,
        },
    )
    .expect("the project is registered");

    let worktree_path = canonical(&worktree);
    // The session exists under the user it belongs to — its directory in their sessions base —
    // and its record names the worktree, which is where a daemon reads it back from.
    let session_dir = data_dir
        .path()
        .join(tddy_core::output::SESSIONS_SUBDIR)
        .join(THE_SESSION);
    std::fs::create_dir_all(&session_dir).expect("the session's directory");
    tddy_core::write_session_metadata(
        &session_dir,
        &a_session_metadata()
            .with_session_id(THE_SESSION)
            .with_project_id(&project_id)
            .with_repo_path(&worktree_path)
            .build(),
    )
    .expect("the session's .session.yaml");

    AProjectWithASession {
        project_id,
        worktree_path,
        worktree_service,
        session_dir,
        _data_dir: data_dir,
        _repos: repos,
    }
}

impl AProjectWithASession {
    pub fn worktree(&self) -> PathBuf {
        PathBuf::from(&self.worktree_path)
    }

    /// The worktree no longer holds a crate: its `Cargo.toml` is gone, so it is not a Rust
    /// worktree.
    pub fn without_a_cargo_manifest(self) -> Self {
        std::fs::remove_file(self.worktree().join("Cargo.toml")).expect("remove the manifest");
        self
    }

    /// [`THE_SESSION`]'s record names no worktree — as for a session whose worktree is still being
    /// created.
    #[allow(dead_code)] // code_index_warmup_acceptance only
    pub fn whose_session_has_no_worktree_yet(self) -> Self {
        tddy_core::write_session_metadata(
            &self.session_dir,
            &a_session_metadata()
                .with_session_id(THE_SESSION)
                .with_project_id(&self.project_id)
                .build(),
        )
        .expect("the session's .session.yaml");
        self
    }

    /// [`THE_SESSION`]'s `.session.yaml` holds something that is not a session record.
    #[allow(dead_code)] // code_index_warmup_acceptance only
    pub fn whose_session_record_is_unreadable(self) -> Self {
        std::fs::write(
            self.session_dir.join(".session.yaml"),
            "this: [is not a session record",
        )
        .expect("overwrite the session's .session.yaml");
        self
    }
}

/// A git repository nobody registered — a real checkout, just not one of the project's.
#[allow(dead_code)] // code_session_acceptance only
pub fn a_checkout_outside_the_project() -> TempDir {
    let elsewhere = TempDir::new().expect("an unrelated directory");
    git(elsewhere.path(), &["init", "-q"]);
    elsewhere
}

#[allow(dead_code)] // used by the fixture, and directly by code_session_acceptance only
pub fn canonical(path: &Path) -> String {
    path.canonicalize()
        .expect("an existing path")
        .display()
        .to_string()
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
// The root's progress, as the index reports it and as the web receives it

/// The root nothing holds.
pub fn the_root_is_idle() -> index::IndexProgress {
    index::IndexProgress {
        state: index::IndexState::Idle as i32,
        ..Default::default()
    }
}

/// rust-analyzer `percentage`% through indexing, fetching metadata alongside it.
pub fn indexing_at(percentage: u32) -> index::IndexProgress {
    index::IndexProgress {
        line: format!("Indexing {percentage}%"),
        phase: "Indexing".to_string(),
        percentage,
        furthest: format!("{percentage}%"),
        ready: false,
        state: index::IndexState::Loading as i32,
        phases: vec![
            index::ActivePhase {
                title: "Fetching".to_string(),
                message: "cargo metadata".to_string(),
                percentage: None,
            },
            index::ActivePhase {
                title: "Indexing".to_string(),
                message: format!("{percentage}/100 (subject)"),
                percentage: Some(percentage),
            },
        ],
        error: String::new(),
    }
}

/// The crate graph is loaded.
pub fn the_index_is_ready() -> index::IndexProgress {
    index::IndexProgress {
        line: "ready".to_string(),
        phase: "Indexing".to_string(),
        percentage: 100,
        furthest: "100%".to_string(),
        ready: true,
        state: index::IndexState::Ready as i32,
        phases: vec![],
        error: String::new(),
    }
}

/// The root's language server went away mid-load, for `reason`.
pub fn the_index_failed(reason: &str) -> index::IndexProgress {
    index::IndexProgress {
        state: index::IndexState::Failed as i32,
        error: reason.to_string(),
        ..Default::default()
    }
}

/// [`indexing_at`] as the web receives it.
pub fn the_web_sees_indexing_at(percentage: u32) -> CodeIndexProgress {
    CodeIndexProgress {
        line: format!("Indexing {percentage}%"),
        phase: "Indexing".to_string(),
        percentage,
        furthest: format!("{percentage}%"),
        ready: false,
        error: String::new(),
        state: CodeIndexState::Loading as i32,
        phases: vec![
            CodeIndexPhase {
                title: "Fetching".to_string(),
                message: "cargo metadata".to_string(),
                percentage: None,
            },
            CodeIndexPhase {
                title: "Indexing".to_string(),
                message: format!("{percentage}/100 (subject)"),
                percentage: Some(percentage),
            },
        ],
    }
}

/// [`the_index_is_ready`] as the web receives it.
pub fn the_web_sees_ready() -> CodeIndexProgress {
    CodeIndexProgress {
        line: "ready".to_string(),
        phase: "Indexing".to_string(),
        percentage: 100,
        furthest: "100%".to_string(),
        ready: true,
        error: String::new(),
        state: CodeIndexState::Ready as i32,
        phases: vec![],
    }
}

/// [`the_index_failed`] as the web receives it.
#[allow(dead_code)] // code_index_warmup_acceptance only
pub fn the_web_sees_failed(reason: &str) -> CodeIndexProgress {
    CodeIndexProgress {
        state: CodeIndexState::Failed as i32,
        error: reason.to_string(),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------------------------
// The answers a loaded index gives about the call to `foo`

/// The call to `foo` on line 2 of `src/main.rs` — one-based line, one-based byte column.
#[allow(dead_code)] // code_session_acceptance only
pub fn the_call_to_foo() -> SourcePosition {
    SourcePosition { line: 2, column: 5 }
}

/// [`the_call_to_foo`] in the index's terms.
pub fn the_index_s_call_to_foo() -> index::SourcePosition {
    index::SourcePosition { line: 2, column: 5 }
}

/// The `fn` keyword on line 1 of `src/main.rs`: nothing is defined there.
#[allow(dead_code)] // code_session_acceptance only
pub fn the_fn_keyword() -> SourcePosition {
    SourcePosition { line: 1, column: 1 }
}

/// Where a loaded index finds `foo` referenced: the call on line 2 of `src/main.rs`, bytes 5 to 8.
fn the_index_s_reference_to_foo() -> index::CodeLocation {
    index::CodeLocation {
        file: "src/main.rs".to_string(),
        range: Some(index::SourceRange {
            start: Some(index::SourcePosition { line: 2, column: 5 }),
            end: Some(index::SourcePosition { line: 2, column: 8 }),
        }),
        outside_root: false,
    }
}

/// [`the_index_s_reference_to_foo`] as the web receives it.
#[allow(dead_code)] // code_session_acceptance only
pub fn the_web_s_reference_to_foo() -> CodeLocation {
    CodeLocation {
        rel_path: "src/main.rs".to_string(),
        range: Some(SourceRange {
            start: Some(SourcePosition { line: 2, column: 5 }),
            end: Some(SourcePosition { line: 2, column: 8 }),
        }),
        outside_worktree: false,
    }
}

/// Where a loaded index says `foo` is defined: `src/lib.rs`, line 11, bytes 4 to 7.
pub fn the_index_s_location_of_foo() -> index::CodeLocation {
    index::CodeLocation {
        file: "src/lib.rs".to_string(),
        range: Some(index::SourceRange {
            start: Some(index::SourcePosition {
                line: 11,
                column: 4,
            }),
            end: Some(index::SourcePosition {
                line: 11,
                column: 7,
            }),
        }),
        outside_root: false,
    }
}

/// [`the_index_s_location_of_foo`] as the web receives it.
#[allow(dead_code)] // code_session_acceptance only
pub fn the_web_s_location_of_foo() -> CodeLocation {
    CodeLocation {
        rel_path: "src/lib.rs".to_string(),
        range: Some(SourceRange {
            start: Some(SourcePosition {
                line: 11,
                column: 4,
            }),
            end: Some(SourcePosition {
                line: 11,
                column: 7,
            }),
        }),
        outside_worktree: false,
    }
}

/// The hover text a loaded index gives for `foo`.
pub const THE_INDEX_S_HOVER_OF_FOO: &str = "```rust\nfn foo()\n```";

// ---------------------------------------------------------------------------------------------
// The scripted index

/// A `code_index` server holding one root's progress, which the test moves with
/// [`AScriptedIndex::reports`]. Every request is recorded.
#[derive(Clone)]
pub struct AScriptedIndex {
    root: Arc<watch::Sender<index::IndexProgress>>,
    /// Whether `WatchRoot` streams keep following the root; [`AScriptedIndex::stops_following`]
    /// ends them all.
    following: Arc<watch::Sender<bool>>,
    warms_asked: Arc<Mutex<Vec<index::WarmRequest>>>,
    watches_asked: Arc<Mutex<Vec<index::WatchRootRequest>>>,
    definitions_asked: Arc<Mutex<Vec<index::DefinitionRequest>>>,
    hovers_asked: Arc<Mutex<Vec<index::HoverRequest>>>,
}

/// An index whose root nothing holds yet.
pub fn a_scripted_index() -> AScriptedIndex {
    AScriptedIndex {
        root: Arc::new(watch::channel(the_root_is_idle()).0),
        following: Arc::new(watch::channel(true).0),
        warms_asked: Arc::default(),
        watches_asked: Arc::default(),
        definitions_asked: Arc::default(),
        hovers_asked: Arc::default(),
    }
}

impl AScriptedIndex {
    /// The root's progress is now `progress` — as when whatever holds it moves on.
    pub fn reports(&self, progress: index::IndexProgress) {
        self.root.send_replace(progress);
    }

    /// Every `WatchRoot` stream ends — as when the index daemon stops following the root.
    #[allow(dead_code)] // code_session_acceptance only
    pub fn stops_following(&self) {
        self.following.send_replace(false);
    }

    pub fn warms_it_was_asked_for(&self) -> Vec<index::WarmRequest> {
        self.warms_asked.lock().expect("the request log").clone()
    }

    #[allow(dead_code)] // code_index_warmup_acceptance only
    pub fn roots_it_was_asked_to_watch(&self) -> Vec<index::WatchRootRequest> {
        self.watches_asked.lock().expect("the request log").clone()
    }

    #[allow(dead_code)] // code_session_acceptance only
    pub fn hovers_it_was_asked(&self) -> Vec<index::HoverRequest> {
        self.hovers_asked.lock().expect("the request log").clone()
    }

    #[allow(dead_code)] // code_session_acceptance only
    pub fn definitions_it_was_asked(&self) -> Vec<index::DefinitionRequest> {
        self.definitions_asked
            .lock()
            .expect("the request log")
            .clone()
    }

    /// The state a navigation answer carries: the root's own once it is `READY` or `FAILED`;
    /// otherwise `LOADING`, since asking about a root nothing holds starts its server, which then
    /// loads.
    fn state(&self) -> index::IndexState {
        match self.root.borrow().state() {
            index::IndexState::Ready => index::IndexState::Ready,
            index::IndexState::Failed => index::IndexState::Failed,
            _ => index::IndexState::Loading,
        }
    }

    /// Whether a loaded index knows the symbol at `position` of `file`: only the call to `foo`.
    fn knows(&self, file: &str, position: Option<index::SourcePosition>) -> bool {
        self.state() == index::IndexState::Ready
            && file == "src/main.rs"
            && position == Some(the_index_s_call_to_foo())
    }

    /// The root's progress from its current value on: every change, until the caller leaves —
    /// or, `until_ready`, until a `READY` message has been sent. A `WatchRoot` stream
    /// (`until_ready` false) also ends when the test stops the index following the root.
    fn follow_the_root(&self, until_ready: bool) -> EventStream<index::IndexProgress> {
        let mut root = self.root.subscribe();
        let mut following = self.following.subscribe();
        let (tx, rx) = mpsc::channel(16);
        tokio::spawn(async move {
            loop {
                let current = root.borrow_and_update().clone();
                let ready = current.state() == index::IndexState::Ready;
                if tx.send(Ok(current)).await.is_err() || (until_ready && ready) {
                    return;
                }
                tokio::select! {
                    changed = root.changed() => {
                        if changed.is_err() {
                            return;
                        }
                    }
                    _ = following.wait_for(|following| !following), if !until_ready => return,
                    () = tx.closed() => return,
                }
            }
        });
        EventStream::new(rx)
    }
}

fn not_part_of_this_fake() -> tddy_rpc::Status {
    tddy_rpc::Status::unimplemented("not part of the scripted index")
}

#[async_trait::async_trait]
impl CodeIndexService for AScriptedIndex {
    type WarmStream = EventStream<index::IndexProgress>;
    type WatchRootStream = EventStream<index::IndexProgress>;
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
        Ok(tddy_rpc::Response::new(self.follow_the_root(true)))
    }

    async fn watch_root(
        &self,
        request: tddy_rpc::Request<index::WatchRootRequest>,
    ) -> Result<tddy_rpc::Response<Self::WatchRootStream>, tddy_rpc::Status> {
        self.watches_asked
            .lock()
            .expect("the request log")
            .push(request.into_inner());
        Ok(tddy_rpc::Response::new(self.follow_the_root(false)))
    }

    async fn definition(
        &self,
        request: tddy_rpc::Request<index::DefinitionRequest>,
    ) -> Result<tddy_rpc::Response<index::DefinitionResponse>, tddy_rpc::Status> {
        let request = request.into_inner();
        let locations = if self.knows(&request.file, request.position) {
            vec![the_index_s_location_of_foo()]
        } else {
            vec![]
        };
        self.definitions_asked
            .lock()
            .expect("the request log")
            .push(request);
        Ok(tddy_rpc::Response::new(index::DefinitionResponse {
            locations,
            state: self.state() as i32,
        }))
    }

    async fn references(
        &self,
        request: tddy_rpc::Request<index::ReferencesRequest>,
    ) -> Result<tddy_rpc::Response<index::ReferencesResponse>, tddy_rpc::Status> {
        let request = request.into_inner();
        let locations = if self.knows(&request.file, request.position) {
            vec![the_index_s_reference_to_foo()]
        } else {
            vec![]
        };
        Ok(tddy_rpc::Response::new(index::ReferencesResponse {
            locations,
            state: self.state() as i32,
        }))
    }

    async fn hover(
        &self,
        request: tddy_rpc::Request<index::HoverRequest>,
    ) -> Result<tddy_rpc::Response<index::HoverResponse>, tddy_rpc::Status> {
        let request = request.into_inner();
        let markdown = self
            .knows(&request.file, request.position)
            .then(|| THE_INDEX_S_HOVER_OF_FOO.to_string());
        self.hovers_asked
            .lock()
            .expect("the request log")
            .push(request);
        Ok(tddy_rpc::Response::new(index::HoverResponse {
            markdown,
            state: self.state() as i32,
        }))
    }

    async fn symbols(
        &self,
        _request: tddy_rpc::Request<index::SymbolsRequest>,
    ) -> Result<tddy_rpc::Response<index::SymbolsResponse>, tddy_rpc::Status> {
        Err(not_part_of_this_fake())
    }

    async fn diagnostics(
        &self,
        _request: tddy_rpc::Request<index::DiagnosticsRequest>,
    ) -> Result<tddy_rpc::Response<index::DiagnosticsResponse>, tddy_rpc::Status> {
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

    async fn snapshot(
        &self,
        _request: tddy_rpc::Request<index::SnapshotRequest>,
    ) -> Result<tddy_rpc::Response<index::SnapshotResponse>, tddy_rpc::Status> {
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

// ---------------------------------------------------------------------------------------------
// The index daemon: a stand-in process in front of the scripted index

/// Where the managed index daemon lives: the stand-in program the registry starts, the socket it
/// is told to bind, and the socket the fake index actually serves on.
pub struct AnIndexDaemonHost {
    dir: TempDir,
}

/// A stand-in that points the socket it was told to bind at `fake` and stays up until stopped.
/// `fake` is already serving when this returns.
pub async fn an_index_daemon_host_serving(fake: AScriptedIndex) -> AnIndexDaemonHost {
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

    /// The registry an `index_daemon:` section pointing at this host would build, with its index
    /// daemon already started — as once anything has asked it something.
    #[allow(dead_code)] // code_index_warmup_acceptance only
    pub async fn a_running_registry(&self) -> IndexDaemonRegistry {
        let registry = self.registry();
        registry
            .get_or_spawn()
            .await
            .expect("the stand-in index daemon starts");
        registry
    }

    /// The registry an `index_daemon:` section pointing at this host would build. Starts nothing.
    pub fn registry(&self) -> IndexDaemonRegistry {
        IndexDaemonRegistry::new(
            IndexDaemonSpawn {
                program: self.dir.path().join("tddy-index-daemon"),
                socket_path: self.dir.path().join("index.sock"),
                ready_timeout: A_READINESS_BUDGET_NOTHING_SHOULD_WAIT_OUT,
                idle_timeout: Duration::from_secs(600),
                rust_analyzer: None,
                env: Default::default(),
            },
            TaskRegistry::new(),
        )
    }
}
