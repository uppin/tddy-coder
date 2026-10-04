//! What `code_navigation.CodeNavigationService` owes the code pane's restructure plan dialog: open a
//! plan file of the session's worktree as a plan (`OpenPlan`), follow its operations' status and
//! staleness (`WatchPlan`), and run it with each operation's outcome streamed as it lands
//! (`RunPlan`) — every call authorised exactly as the navigation calls are, and forwarded to the
//! index daemon's plan store with the listed worktree as its workspace root.
//!
//! The harness is `code_navigation_acceptance.rs`'s: the registry starts a shell-script stand-in
//! whose socket points at a `code_index` server this test hosts, so the lazy start and the dial are
//! the production ones. The fake index holds one plan's store state — its stale operations and its
//! journal counts — answers `LoadPlans`, `ListPlans` and `PlanStatus` from it, replays a scripted
//! `Apply`, and records what it was asked.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-10-03-plan-dialog.md

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use prost::Message;
use tddy_daemon::index_daemon::{IndexDaemonRegistry, IndexDaemonSpawn};
use tddy_daemon_kernel::user_paths::projects_path_for_user;
use tddy_daemon_rpc::code_navigation::{
    build_code_navigation_entry, CodeNavigationServiceImpl, IndexChannelSource,
};
use tddy_index_daemon::proto::code_index as index;
use tddy_index_daemon::proto::code_index::{CodeIndexService, CodeIndexServiceTonicAdapter};
use tddy_index_daemon::proto::tonic_code_index::code_index_service_server::CodeIndexServiceServer as TonicCodeIndexServiceServer;
use tddy_index_daemon::EventStream;
use tddy_service::proto::code_navigation::{
    plan_run_event, OpenPlanRequest, PlanOperation, PlanOperationApplied, PlanOperationStatus,
    PlanRunEvent, PlanRunOutcome, PlanSnapshot, RunPlanRequest, WatchPlanRequest,
};
use tddy_task::TaskRegistry;
use tddy_worktree_service::project_storage::{self, ProjectData};
use tddy_worktree_service::test_util::{current_os_user, require_git, test_service, TEST_TOKEN};
use tempfile::TempDir;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

/// Long enough that only a stand-in that never binds could exhaust it.
const A_READINESS_BUDGET_NOTHING_SHOULD_WAIT_OUT: Duration = Duration::from_secs(60);

/// How long a stream may take to deliver its next message before the test calls it stuck.
const A_MESSAGE_IS_DUE_WITHIN: Duration = Duration::from_secs(30);

/// The plan file every test opens, relative to the worktree.
const THE_PLAN: &str = "plans/split-geometry.jsonl";

/// Two operations: a rename standing alone, then a move in the transactional group `geometry`.
const THE_PLAN_JSONL: &str = concat!(
    r#"{"v":1,"snapshot":{}}"#,
    "\n",
    r#"{"id":"op-rename","op":"rename_symbol","anchor":{"kind":"symbol","file":"src/main.rs","path":"foo"},"name":"area"}"#,
    "\n",
    r#"{"id":"op-move","op":"move_symbol","anchor":{"kind":"symbol","file":"src/main.rs","path":"area"},"to":"src/geometry.rs","group":"geometry"}"#,
    "\n",
);

// ---------------------------------------------------------------------------------------------
// A project with a listed worktree holding a plan

/// A registered project whose main repo has one secondary worktree, `wt-feature`, holding a crate
/// and [`THE_PLAN`].
struct AProjectWithAPlan {
    project_id: String,
    worktree_path: String,
    worktree_service: tddy_worktree_service::WorktreeServiceImpl,
    _data_dir: TempDir,
    _repos: TempDir,
}

fn a_project_with_a_plan_in_its_worktree() -> AProjectWithAPlan {
    require_git();
    let os_user = current_os_user();
    let data_dir = TempDir::new().expect("a data directory");
    let worktree_service = test_service(data_dir.path().to_path_buf(), &os_user);
    let projects_dir =
        projects_path_for_user(&os_user, Some(data_dir.path())).expect("a projects directory");

    let repos = TempDir::new().expect("a directory for the repositories");
    let main_repo = repos.path().join("main");
    std::fs::create_dir_all(&main_repo).expect("the main repo directory");
    git(&main_repo, &["init", "-q"]);
    git(&main_repo, &["config", "user.email", "t@e.st"]);
    git(&main_repo, &["config", "user.name", "t"]);
    std::fs::write(
        main_repo.join("Cargo.toml"),
        "[package]\nname = \"subject\"\nversion = \"0.1.0\"\n",
    )
    .expect("a manifest");
    std::fs::create_dir_all(main_repo.join("src")).expect("src");
    std::fs::write(
        main_repo.join("src/main.rs"),
        "fn foo() -> u32 {\n    12\n}\n\nfn main() {\n    foo();\n}\n",
    )
    .expect("main.rs");
    std::fs::create_dir_all(main_repo.join("plans")).expect("plans");
    std::fs::write(main_repo.join(THE_PLAN), THE_PLAN_JSONL).expect("the plan");
    git(&main_repo, &["add", "."]);
    git(&main_repo, &["commit", "-q", "-m", "init"]);
    let worktree = repos.path().join("wt-feature");
    git(
        &main_repo,
        &[
            "worktree",
            "add",
            "-q",
            path_str(&worktree),
            "-b",
            "feature-x",
        ],
    );

    let project_id = "plan-dialog-project".to_string();
    project_storage::add_project(
        &projects_dir,
        ProjectData {
            project_id: project_id.clone(),
            name: "plan-dialog".to_string(),
            git_url: "https://example.com/plan-dialog.git".to_string(),
            main_repo_path: canonical(&main_repo),
            main_branch_ref: None,
            remote_name: None,
            host_repo_paths: std::collections::HashMap::new(),
            accounts: Vec::new(),
        },
    )
    .expect("the project is registered");

    AProjectWithAPlan {
        project_id,
        worktree_path: canonical(&worktree),
        worktree_service,
        _data_dir: data_dir,
        _repos: repos,
    }
}

/// A git repository nobody registered — a real checkout, just not one of the project's.
fn a_checkout_outside_the_project() -> TempDir {
    let elsewhere = TempDir::new().expect("an unrelated directory");
    git(elsewhere.path(), &["init", "-q"]);
    elsewhere
}

fn git(cwd: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .status()
        .unwrap_or_else(|err| panic!("git {args:?} in {cwd:?}: {err}"));
    assert!(status.success(), "git {args:?} failed in {cwd:?}");
}

fn path_str(path: &Path) -> &str {
    path.to_str().expect("a UTF-8 path")
}

fn canonical(path: &Path) -> String {
    path.canonicalize()
        .expect("an existing path")
        .display()
        .to_string()
}

// ---------------------------------------------------------------------------------------------
// The index daemon: a stand-in process in front of a fake `code_index` plan store

/// A `code_index` server holding one plan's store state. `LoadPlans`, `ListPlans` and `PlanStatus`
/// all answer from the same state, so a test pins what the dialog shows rather than which of them
/// the service chose to ask; `Apply` replays `apply_script`. Every plan request is recorded.
#[derive(Clone)]
struct AFakePlanStore {
    stale: Arc<Mutex<Vec<index::StaleOp>>>,
    apply_script: Arc<Mutex<Vec<index::RestructureEvent>>>,
    apply_refusal: Arc<Mutex<Option<tddy_rpc::Status>>>,
    loads_asked: Arc<Mutex<Vec<index::LoadPlansRequest>>>,
    applies_asked: Arc<Mutex<Vec<index::ApplyRequest>>>,
}

fn a_plan_store_with_nothing_stale() -> AFakePlanStore {
    AFakePlanStore {
        stale: Arc::new(Mutex::new(Vec::new())),
        apply_script: Arc::new(Mutex::new(Vec::new())),
        apply_refusal: Arc::new(Mutex::new(None)),
        loads_asked: Arc::new(Mutex::new(Vec::new())),
        applies_asked: Arc::new(Mutex::new(Vec::new())),
    }
}

impl AFakePlanStore {
    /// The same store, reporting `op` as no longer runnable for `reason`.
    fn with_stale(self, op: &str, reason: &str) -> Self {
        self.stale
            .lock()
            .expect("the stale list")
            .push(index::StaleOp {
                op: op.to_string(),
                reason: reason.to_string(),
            });
        self
    }

    /// The same store, whose `Apply` streams `events` and ends.
    fn applying_with(self, events: Vec<index::RestructureEvent>) -> Self {
        *self.apply_script.lock().expect("the apply script") = events;
        self
    }

    /// The same store, whose `Apply` stream ends in `refusal` after its scripted events.
    fn refusing_the_apply_with(self, refusal: tddy_rpc::Status) -> Self {
        *self.apply_refusal.lock().expect("the apply refusal") = Some(refusal);
        self
    }

    fn loads_it_was_asked(&self) -> Vec<index::LoadPlansRequest> {
        self.loads_asked.lock().expect("the request log").clone()
    }

    fn applies_it_was_asked(&self) -> Vec<index::ApplyRequest> {
        self.applies_asked.lock().expect("the request log").clone()
    }

    fn the_plan_as_held(&self) -> index::PlansResponse {
        index::PlansResponse {
            plans: vec![index::LoadedPlan {
                plan: THE_PLAN.to_string(),
                ops: 2,
                dirty: false,
                stale: self.stale.lock().expect("the stale list").clone(),
            }],
        }
    }
}

fn an_operation_applied(index: u32, op_id: &str, done: u32) -> index::RestructureEvent {
    index::RestructureEvent {
        event: Some(index::restructure_event::Event::Operation(
            index::OperationApplied {
                index,
                done,
                total: 2,
                kind: "rename_symbol".to_string(),
                files: vec!["src/main.rs".to_string()],
                visibility: Vec::new(),
                rehearsed_only: false,
                op_id: op_id.to_string(),
                group: String::new(),
            },
        )),
    }
}

/// The refusal the index gives when a transactional group did not compile at its end, worded as
/// `RestructureError::GroupDoesNotCompile` is and classed as `status_of` classes it.
fn a_group_that_does_not_compile(group: &str) -> tddy_rpc::Status {
    tddy_rpc::Status::failed_precondition(format!(
        "group `{group}` does not compile at its end, so it was rolled back: E0432 unresolved import"
    ))
}

fn a_run_outcome(applied: u32) -> index::RestructureEvent {
    index::RestructureEvent {
        event: Some(index::restructure_event::Event::Outcome(
            index::RunOutcome {
                applied,
                total: 2,
                stopped_early: false,
            },
        )),
    }
}

fn not_part_of_this_fake() -> tddy_rpc::Status {
    tddy_rpc::Status::unimplemented("not part of the fake plan store")
}

#[async_trait::async_trait]
impl CodeIndexService for AFakePlanStore {
    type WarmStream = EventStream<index::IndexProgress>;
    type CheckStream = EventStream<index::RestructureEvent>;
    type ApplyStream = EventStream<index::RestructureEvent>;
    type CoverageStream = EventStream<index::AnalyzeEvent>;
    type DuplicateTestsStream = EventStream<index::AnalyzeEvent>;

    async fn load_plans(
        &self,
        request: tddy_rpc::Request<index::LoadPlansRequest>,
    ) -> Result<tddy_rpc::Response<index::PlansResponse>, tddy_rpc::Status> {
        self.loads_asked
            .lock()
            .expect("the request log")
            .push(request.into_inner());
        Ok(tddy_rpc::Response::new(self.the_plan_as_held()))
    }

    async fn list_plans(
        &self,
        _request: tddy_rpc::Request<index::ListPlansRequest>,
    ) -> Result<tddy_rpc::Response<index::PlansResponse>, tddy_rpc::Status> {
        Ok(tddy_rpc::Response::new(self.the_plan_as_held()))
    }

    async fn plan_status(
        &self,
        _request: tddy_rpc::Request<index::PlanStatusRequest>,
    ) -> Result<tddy_rpc::Response<index::PlanStatusResponse>, tddy_rpc::Status> {
        Ok(tddy_rpc::Response::new(index::PlanStatusResponse {
            completed: 0,
            in_flight: 0,
            pending: 2,
            failed: 0,
            stale: self.stale.lock().expect("the stale list").clone(),
        }))
    }

    async fn apply(
        &self,
        request: tddy_rpc::Request<index::ApplyRequest>,
    ) -> Result<tddy_rpc::Response<Self::ApplyStream>, tddy_rpc::Status> {
        self.applies_asked
            .lock()
            .expect("the request log")
            .push(request.into_inner());
        let events = self.apply_script.lock().expect("the apply script").clone();
        let refusal = self
            .apply_refusal
            .lock()
            .expect("the apply refusal")
            .clone();
        let (tx, rx) = mpsc::channel(events.len() + 1);
        for event in events {
            tx.send(Ok(event)).await.expect("the apply stream is open");
        }
        if let Some(refusal) = refusal {
            tx.send(Err(refusal))
                .await
                .expect("the apply stream is open");
        }
        Ok(tddy_rpc::Response::new(ReceiverStream::new(rx)))
    }

    async fn unload_plans(
        &self,
        _request: tddy_rpc::Request<index::UnloadPlansRequest>,
    ) -> Result<tddy_rpc::Response<index::PlansResponse>, tddy_rpc::Status> {
        Err(not_part_of_this_fake())
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

    async fn warm(
        &self,
        _request: tddy_rpc::Request<index::WarmRequest>,
    ) -> Result<tddy_rpc::Response<Self::WarmStream>, tddy_rpc::Status> {
        Err(not_part_of_this_fake())
    }

    async fn check(
        &self,
        _request: tddy_rpc::Request<index::CheckRequest>,
    ) -> Result<tddy_rpc::Response<Self::CheckStream>, tddy_rpc::Status> {
        Err(not_part_of_this_fake())
    }

    async fn anchors(
        &self,
        _request: tddy_rpc::Request<index::AnchorsRequest>,
    ) -> Result<tddy_rpc::Response<index::AnchorsResponse>, tddy_rpc::Status> {
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
}

/// Where the managed index daemon lives: the stand-in program the registry starts, the socket it
/// is told to bind, and the socket the fake plan store actually serves on.
struct AnIndexDaemonHost {
    dir: TempDir,
}

/// A stand-in that points the socket it was told to bind at `fake` and stays up until stopped.
/// `fake` is already serving when this returns.
async fn an_index_daemon_host_serving(fake: AFakePlanStore) -> AnIndexDaemonHost {
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
    let program = host.program_path();
    std::fs::write(
        &program,
        format!(
            "#!/bin/sh\n\
             ln -s {fake} \"$2\"\n\
             exec sleep 86400\n",
            fake = host.fake_socket_path().display(),
        ),
    )
    .expect("write the stand-in index daemon");
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755))
        .expect("make the stand-in index daemon executable");
    host
}

impl AnIndexDaemonHost {
    fn program_path(&self) -> PathBuf {
        self.dir.path().join("tddy-index-daemon")
    }

    fn socket_path(&self) -> PathBuf {
        self.dir.path().join("index.sock")
    }

    fn fake_socket_path(&self) -> PathBuf {
        self.dir.path().join("fake-index.sock")
    }

    /// The registry an `index_daemon:` section pointing at this host would build.
    fn registry(&self) -> IndexDaemonRegistry {
        IndexDaemonRegistry::new(
            IndexDaemonSpawn {
                program: self.program_path(),
                socket_path: self.socket_path(),
                ready_timeout: A_READINESS_BUDGET_NOTHING_SHOULD_WAIT_OUT,
                idle_timeout: Duration::from_secs(600),
            },
            TaskRegistry::new(),
        )
    }
}

// ---------------------------------------------------------------------------------------------
// The service under test, and how a request reaches it

fn the_entry_for(
    project: &AProjectWithAPlan,
    index_daemon: Option<IndexDaemonRegistry>,
) -> tddy_rpc::ServiceEntry {
    build_code_navigation_entry(CodeNavigationServiceImpl::new(
        Arc::new(project.worktree_service.clone()),
        index_daemon.map(|registry| Arc::new(registry) as Arc<dyn IndexChannelSource>),
    ))
}

fn rpc_message(request: impl Message) -> tddy_rpc::RpcMessage {
    tddy_rpc::RpcMessage::new(
        request.encode_to_vec(),
        tddy_rpc::RequestMetadata::over(tddy_rpc::RequestTransport::Direct),
    )
}

/// Dispatch one unary request at the registered coordinate and decode its answer.
async fn unary_at<Req: Message, Res: Message + Default>(
    entry: &tddy_rpc::ServiceEntry,
    method: &str,
    request: Req,
) -> Result<Res, tddy_rpc::Status> {
    match entry
        .service
        .handle_rpc(entry.name, method, &rpc_message(request))
        .await
    {
        tddy_rpc::RpcResult::Unary(Ok(payload)) => {
            Ok(Res::decode(payload.as_slice()).expect("the answer decodes"))
        }
        tddy_rpc::RpcResult::Unary(Err(status)) => Err(status),
        tddy_rpc::RpcResult::ServerStream(_) => panic!("expected a unary answer from {method}"),
    }
}

/// Open a server stream at the registered coordinate.
async fn stream_at<Req: Message>(
    entry: &tddy_rpc::ServiceEntry,
    method: &str,
    request: Req,
) -> mpsc::Receiver<Result<Vec<u8>, tddy_rpc::Status>> {
    match entry
        .service
        .handle_rpc(entry.name, method, &rpc_message(request))
        .await
    {
        tddy_rpc::RpcResult::ServerStream(opened) => {
            opened.unwrap_or_else(|status| panic!("{method} opens a stream, got {status:?}"))
        }
        tddy_rpc::RpcResult::Unary(_) => panic!("{method} is a server stream"),
    }
}

/// The next message a stream delivers.
async fn the_next_message_of<Res: Message + Default>(
    stream: &mut mpsc::Receiver<Result<Vec<u8>, tddy_rpc::Status>>,
) -> Res {
    let item = tokio::time::timeout(A_MESSAGE_IS_DUE_WITHIN, stream.recv())
        .await
        .expect("the stream delivered nothing within the timeout")
        .expect("the stream ended before delivering a message")
        .expect("the stream delivered an error");
    Res::decode(item.as_slice()).expect("the message decodes")
}

/// Every message a stream delivers until it ends.
async fn everything_delivered_by<Res: Message + Default>(
    mut stream: mpsc::Receiver<Result<Vec<u8>, tddy_rpc::Status>>,
) -> Vec<Res> {
    let mut delivered = Vec::new();
    while let Some(item) = tokio::time::timeout(A_MESSAGE_IS_DUE_WITHIN, stream.recv())
        .await
        .expect("the stream neither delivered nor ended within the timeout")
    {
        let payload = item.expect("the stream delivered an error");
        delivered.push(Res::decode(payload.as_slice()).expect("the message decodes"));
    }
    delivered
}

fn an_open_plan_request(project: &AProjectWithAPlan, worktree_path: &str) -> OpenPlanRequest {
    OpenPlanRequest {
        session_token: TEST_TOKEN.to_string(),
        project_id: project.project_id.clone(),
        worktree_path: worktree_path.to_string(),
        rel_path: THE_PLAN.to_string(),
    }
}

/// [`THE_PLAN_JSONL`]'s operations as the dialog lists them, both pending, `op-move` stale for
/// `stale_reason` when one is given.
fn the_plan_s_operations(op_move_stale_reason: &str) -> Vec<PlanOperation> {
    vec![
        PlanOperation {
            id: "op-rename".to_string(),
            index: 0,
            op: "rename_symbol".to_string(),
            item: "foo".to_string(),
            file: "src/main.rs".to_string(),
            group: String::new(),
            status: PlanOperationStatus::Pending as i32,
            stale_reason: String::new(),
        },
        PlanOperation {
            id: "op-move".to_string(),
            index: 1,
            op: "move_symbol".to_string(),
            item: "area".to_string(),
            file: "src/main.rs".to_string(),
            group: "geometry".to_string(),
            status: PlanOperationStatus::Pending as i32,
            stale_reason: op_move_stale_reason.to_string(),
        },
    ]
}

// ---------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn open_plan_loads_the_plan_for_the_session_worktree() {
    // Given a session worktree holding a two-operation plan, and an index whose store holds it
    let project = a_project_with_a_plan_in_its_worktree();
    let store = a_plan_store_with_nothing_stale();
    let host = an_index_daemon_host_serving(store.clone()).await;
    let entry = the_entry_for(&project, Some(host.registry()));

    // When the pane opens the plan file as a plan
    let opened: PlanSnapshot = unary_at(
        &entry,
        "OpenPlan",
        an_open_plan_request(&project, &project.worktree_path),
    )
    .await
    .expect("a listed worktree's plan opens");

    // Then the index loaded that plan, rooted at the session's worktree
    assert_eq!(
        store.loads_it_was_asked(),
        vec![index::LoadPlansRequest {
            workspace_root: project.worktree_path.clone(),
            plans: vec![THE_PLAN.to_string()],
        }]
    );
    // And every operation is listed in plan order with its id, kind, item, file, group and status
    assert_eq!(
        opened,
        PlanSnapshot {
            rel_path: THE_PLAN.to_string(),
            operations: the_plan_s_operations(""),
        }
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_plan_reports_a_stale_operation_with_its_reason() {
    // Given a plan whose move no longer points at the item it was written against
    let project = a_project_with_a_plan_in_its_worktree();
    let store = a_plan_store_with_nothing_stale().with_stale("op-move", "item changed");
    let host = an_index_daemon_host_serving(store).await;
    let entry = the_entry_for(&project, Some(host.registry()));

    // When the dialog watches the plan
    let mut watch = stream_at(
        &entry,
        "WatchPlan",
        WatchPlanRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: project.project_id.clone(),
            worktree_path: project.worktree_path.clone(),
            rel_path: THE_PLAN.to_string(),
        },
    )
    .await;
    let first: PlanSnapshot = the_next_message_of(&mut watch).await;

    // Then the opening snapshot marks the move stale with the store's reason, and only the move
    assert_eq!(
        first,
        PlanSnapshot {
            rel_path: THE_PLAN.to_string(),
            operations: the_plan_s_operations("item changed"),
        }
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn run_plan_streams_each_operations_outcome() {
    // Given an index whose apply of the plan lands both operations and finishes
    let project = a_project_with_a_plan_in_its_worktree();
    let store = a_plan_store_with_nothing_stale().applying_with(vec![
        an_operation_applied(0, "op-rename", 1),
        an_operation_applied(1, "op-move", 2),
        a_run_outcome(2),
    ]);
    let host = an_index_daemon_host_serving(store.clone()).await;
    let entry = the_entry_for(&project, Some(host.registry()));

    // When the dialog runs the plan
    let run = stream_at(
        &entry,
        "RunPlan",
        RunPlanRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: project.project_id.clone(),
            worktree_path: project.worktree_path.clone(),
            rel_path: THE_PLAN.to_string(),
        },
    )
    .await;
    let events: Vec<PlanRunEvent> = everything_delivered_by(run).await;

    // Then the index applied that plan for real, rooted at the session's worktree
    let applies = store.applies_it_was_asked();
    assert_eq!(applies.len(), 1);
    assert_eq!(applies[0].workspace_root, project.worktree_path);
    assert_eq!(applies[0].plan, THE_PLAN);
    assert!(!applies[0].dry_run, "Run writes; it is not a rehearsal");
    // And each operation's outcome arrived as it landed, keyed by its id, then the run's outcome
    assert_eq!(
        events,
        vec![
            PlanRunEvent {
                event: Some(plan_run_event::Event::Operation(PlanOperationApplied {
                    op_id: "op-rename".to_string(),
                    index: 0,
                    done: 1,
                    total: 2,
                    files: vec!["src/main.rs".to_string()],
                })),
            },
            PlanRunEvent {
                event: Some(plan_run_event::Event::Operation(PlanOperationApplied {
                    op_id: "op-move".to_string(),
                    index: 1,
                    done: 2,
                    total: 2,
                    files: vec!["src/main.rs".to_string()],
                })),
            },
            PlanRunEvent {
                event: Some(plan_run_event::Event::Outcome(PlanRunOutcome {
                    applied: 2,
                    total: 2,
                })),
            },
        ]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn run_plan_reports_a_group_that_did_not_compile_as_rolled_back() {
    // Given an index that lands the ungrouped rename, then refuses the geometry group's end state
    let project = a_project_with_a_plan_in_its_worktree();
    let store = a_plan_store_with_nothing_stale()
        .applying_with(vec![an_operation_applied(0, "op-rename", 1)])
        .refusing_the_apply_with(a_group_that_does_not_compile("geometry"));
    let host = an_index_daemon_host_serving(store).await;
    let entry = the_entry_for(&project, Some(host.registry()));

    // When the dialog runs the plan
    let run = stream_at(
        &entry,
        "RunPlan",
        RunPlanRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: project.project_id.clone(),
            worktree_path: project.worktree_path.clone(),
            rel_path: THE_PLAN.to_string(),
        },
    )
    .await;
    let events: Vec<PlanRunEvent> = everything_delivered_by(run).await;

    // Then the run ends in a failure naming the geometry group, with the group's operations
    // rolled back and the ungrouped rename, which stands, left out
    let failure = match events.last().and_then(|event| event.event.clone()) {
        Some(plan_run_event::Event::Failure(failure)) => failure,
        other => panic!("the run should end in a failure, ended in {other:?}"),
    };
    assert_eq!(failure.group, "geometry");
    assert_eq!(failure.rolled_back, vec!["op-move".to_string()]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_worktree_not_listed_for_the_project_is_refused() {
    // Given a project, a checkout that is not one of its worktrees, and a managed index daemon
    let project = a_project_with_a_plan_in_its_worktree();
    let elsewhere = a_checkout_outside_the_project();
    let store = a_plan_store_with_nothing_stale();
    let host = an_index_daemon_host_serving(store.clone()).await;
    let registry = host.registry();
    let entry = the_entry_for(&project, Some(registry.clone()));

    // When the pane opens a plan in that checkout
    let refusal = unary_at::<_, PlanSnapshot>(
        &entry,
        "OpenPlan",
        an_open_plan_request(&project, &canonical(elsewhere.path())),
    )
    .await
    .expect_err("a plan outside the project's worktrees is refused");

    // Then it is refused exactly as the worktree service refuses it
    assert_eq!(refusal.code(), tddy_rpc::Code::FailedPrecondition);
    assert_eq!(
        refusal.message(),
        "worktree_path is not a worktree of this project"
    );
    // And nothing was loaded, and the index daemon was not even started for it
    assert_eq!(store.loads_it_was_asked(), vec![]);
    assert!(registry.running().await.is_none());
}
