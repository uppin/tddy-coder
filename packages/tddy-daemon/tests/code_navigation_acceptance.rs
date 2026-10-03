//! What `code_navigation.CodeNavigationService` owes the web's code pane: authorise a request the
//! way `worktree.WorktreeService` does, forward it to the index daemon this daemon manages with the
//! listed worktree as the workspace root, start that index daemon on the first request, and refuse
//! — rather than fall back — when no `index_daemon:` section configured one.
//!
//! Every test dispatches at the registered coordinate (`entry.service.handle_rpc(…)`), so a handler
//! that compiles but is unreachable fails here rather than in the browser.
//!
//! The index daemon is real process management over a stand-in: the registry starts a shell script
//! (the `index_daemon_lifecycle_acceptance.rs` pattern, for the reasons that suite gives), and the
//! script points the socket it was told to bind at a `code_index` server this test hosts. So the
//! lazy start, the readiness wait and the dial are the production ones, and what answers the dial
//! is a fake that records what it was asked.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use prost::Message;
use tddy_daemon::code_navigation::{build_code_navigation_entry, CodeNavigationServiceImpl};
use tddy_daemon::index_daemon::{IndexDaemonRegistry, IndexDaemonSpawn};
use tddy_daemon_kernel::user_paths::projects_path_for_user;
use tddy_index_daemon::proto::code_index as index;
use tddy_index_daemon::proto::code_index::{CodeIndexService, CodeIndexServiceTonicAdapter};
use tddy_index_daemon::proto::tonic_code_index::code_index_service_server::CodeIndexServiceServer as TonicCodeIndexServiceServer;
use tddy_index_daemon::EventStream;
use tddy_service::proto::code_navigation::{
    CodeLocation, DefinitionRequest, DefinitionResponse, HoverRequest, HoverResponse,
    ReferencesRequest, ReferencesResponse, SourcePosition, SourceRange,
};
use tddy_task::TaskRegistry;
use tddy_worktree_service::project_storage::{self, ProjectData};
use tddy_worktree_service::test_util::{current_os_user, require_git, test_service, TEST_TOKEN};
use tempfile::TempDir;

/// Long enough that only a stand-in that never binds could exhaust it.
const A_READINESS_BUDGET_NOTHING_SHOULD_WAIT_OUT: Duration = Duration::from_secs(60);

// ---------------------------------------------------------------------------------------------
// A project with a listed worktree

/// A registered project whose main repo has one secondary worktree, `wt-feature`, holding a crate.
struct AProjectWithAWorktree {
    project_id: String,
    worktree_path: String,
    worktree_service: tddy_worktree_service::WorktreeServiceImpl,
    _data_dir: TempDir,
    _repos: TempDir,
}

fn a_project_with_a_worktree() -> AProjectWithAWorktree {
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
            path_str(&worktree),
            "-b",
            "feature-x",
        ],
    );

    let project_id = "code-navigation-project".to_string();
    project_storage::add_project(
        &projects_dir,
        ProjectData {
            project_id: project_id.clone(),
            name: "code-navigation".to_string(),
            git_url: "https://example.com/code-navigation.git".to_string(),
            main_repo_path: canonical(&main_repo),
            main_branch_ref: None,
            remote_name: None,
            host_repo_paths: std::collections::HashMap::new(),
        },
    )
    .expect("the project is registered");

    AProjectWithAWorktree {
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
// The index daemon: a stand-in process in front of a fake `code_index` server

/// A `code_index` server that answers `Definition` and `References` with one canned location list
/// and `Hover` with one canned text, and records every request it was asked. Nothing else on the
/// service is part of what this suite exercises.
#[derive(Clone)]
struct AFakeIndex {
    definition_answer: index::DefinitionResponse,
    definitions_asked: Arc<Mutex<Vec<index::DefinitionRequest>>>,
    references_asked: Arc<Mutex<Vec<index::ReferencesRequest>>>,
    hovers_asked: Arc<Mutex<Vec<index::HoverRequest>>>,
}

fn a_fake_index_answering_definitions_with(answer: index::DefinitionResponse) -> AFakeIndex {
    AFakeIndex {
        definition_answer: answer,
        definitions_asked: Arc::new(Mutex::new(Vec::new())),
        references_asked: Arc::new(Mutex::new(Vec::new())),
        hovers_asked: Arc::new(Mutex::new(Vec::new())),
    }
}

/// The hover text the fake index answers every hover with.
const THE_INDEX_S_HOVER_OF_FOO: &str = "```rust\nfn foo()\n```";

impl AFakeIndex {
    fn definitions_it_was_asked(&self) -> Vec<index::DefinitionRequest> {
        self.definitions_asked
            .lock()
            .expect("the request log")
            .clone()
    }

    fn references_it_was_asked(&self) -> Vec<index::ReferencesRequest> {
        self.references_asked
            .lock()
            .expect("the request log")
            .clone()
    }

    fn hovers_it_was_asked(&self) -> Vec<index::HoverRequest> {
        self.hovers_asked.lock().expect("the request log").clone()
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

    async fn definition(
        &self,
        request: tddy_rpc::Request<index::DefinitionRequest>,
    ) -> Result<tddy_rpc::Response<index::DefinitionResponse>, tddy_rpc::Status> {
        self.definitions_asked
            .lock()
            .expect("the request log")
            .push(request.into_inner());
        Ok(tddy_rpc::Response::new(self.definition_answer.clone()))
    }

    async fn references(
        &self,
        request: tddy_rpc::Request<index::ReferencesRequest>,
    ) -> Result<tddy_rpc::Response<index::ReferencesResponse>, tddy_rpc::Status> {
        self.references_asked
            .lock()
            .expect("the request log")
            .push(request.into_inner());
        Ok(tddy_rpc::Response::new(index::ReferencesResponse {
            locations: self.definition_answer.locations.clone(),
        }))
    }

    async fn hover(
        &self,
        request: tddy_rpc::Request<index::HoverRequest>,
    ) -> Result<tddy_rpc::Response<index::HoverResponse>, tddy_rpc::Status> {
        self.hovers_asked
            .lock()
            .expect("the request log")
            .push(request.into_inner());
        Ok(tddy_rpc::Response::new(index::HoverResponse {
            markdown: Some(THE_INDEX_S_HOVER_OF_FOO.to_string()),
        }))
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
/// is told to bind, and the socket the fake index actually serves on.
struct AnIndexDaemonHost {
    dir: TempDir,
}

/// A stand-in that records its argv, points the socket it was told to bind at `fake`, and stays up
/// until stopped. `fake` is already serving when this returns.
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
    let program = host.program_path();
    std::fs::write(
        &program,
        format!(
            "#!/bin/sh\n\
             printf '%s' \"$*\" > {argv}\n\
             ln -s {fake} \"$2\"\n\
             exec sleep 86400\n",
            argv = host.recorded_argv_path().display(),
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

    fn recorded_argv_path(&self) -> PathBuf {
        self.dir.path().join("argv")
    }

    /// The command line the stand-in was started with, or `None` if nothing started it.
    fn argv_it_was_started_with(&self) -> Option<String> {
        std::fs::read_to_string(self.recorded_argv_path()).ok()
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
    project: &AProjectWithAWorktree,
    index_daemon: Option<IndexDaemonRegistry>,
) -> tddy_rpc::ServiceEntry {
    build_code_navigation_entry(CodeNavigationServiceImpl::new(
        Arc::new(project.worktree_service.clone()),
        index_daemon,
    ))
}

/// Dispatch one unary request at the registered coordinate and decode its answer.
async fn unary_at<Req: Message, Res: Message + Default>(
    entry: &tddy_rpc::ServiceEntry,
    method: &str,
    request: Req,
) -> Result<Res, tddy_rpc::Status> {
    let message = tddy_rpc::RpcMessage::new(
        request.encode_to_vec(),
        tddy_rpc::RequestMetadata::over(tddy_rpc::RequestTransport::Direct),
    );
    match entry.service.handle_rpc(entry.name, method, &message).await {
        tddy_rpc::RpcResult::Unary(Ok(payload)) => {
            Ok(Res::decode(payload.as_slice()).expect("the answer decodes"))
        }
        tddy_rpc::RpcResult::Unary(Err(status)) => Err(status),
        tddy_rpc::RpcResult::ServerStream(_) => panic!("expected a unary answer from {method}"),
    }
}

/// The call to `foo` on line 2 of `src/main.rs` — one-based line, one-based byte column.
fn the_call_to_foo() -> Option<SourcePosition> {
    Some(SourcePosition { line: 2, column: 5 })
}

fn a_definition_request(project: &AProjectWithAWorktree, worktree_path: &str) -> DefinitionRequest {
    DefinitionRequest {
        session_token: TEST_TOKEN.to_string(),
        project_id: project.project_id.clone(),
        worktree_path: worktree_path.to_string(),
        rel_path: "src/main.rs".to_string(),
        position: the_call_to_foo(),
    }
}

/// What the fake index answers every definition with: `src/lib.rs`, line 11, bytes 4 to 7.
fn the_index_s_definition_of_foo() -> index::DefinitionResponse {
    index::DefinitionResponse {
        locations: vec![index::CodeLocation {
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
        }],
    }
}

// ---------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn a_definition_request_is_forwarded_to_the_index_daemon_for_the_session_worktree() {
    // Given a session worktree and an index daemon that knows where `foo` is defined
    let project = a_project_with_a_worktree();
    let fake = a_fake_index_answering_definitions_with(the_index_s_definition_of_foo());
    let host = an_index_daemon_host_serving(fake.clone()).await;
    let entry = the_entry_for(&project, Some(host.registry()));

    // When the pane asks for the definition of the call to `foo`
    let answer: DefinitionResponse = unary_at(
        &entry,
        "Definition",
        a_definition_request(&project, &project.worktree_path),
    )
    .await
    .expect("a listed worktree's definition is answered");

    // Then the index daemon was asked about that file, rooted at the session's worktree
    assert_eq!(
        fake.definitions_it_was_asked(),
        vec![index::DefinitionRequest {
            workspace_root: project.worktree_path.clone(),
            file: "src/main.rs".to_string(),
            position: Some(index::SourcePosition { line: 2, column: 5 }),
        }]
    );
    // And its answer reaches the pane as a worktree-relative location
    assert_eq!(
        answer,
        DefinitionResponse {
            locations: vec![CodeLocation {
                rel_path: "src/lib.rs".to_string(),
                range: Some(SourceRange {
                    start: Some(SourcePosition {
                        line: 11,
                        column: 4
                    }),
                    end: Some(SourcePosition {
                        line: 11,
                        column: 7
                    }),
                }),
                outside_worktree: false,
            }],
        }
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_worktree_not_listed_for_the_project_is_refused() {
    // Given a project, a checkout that is not one of its worktrees, and a managed index daemon
    let project = a_project_with_a_worktree();
    let elsewhere = a_checkout_outside_the_project();
    let fake = a_fake_index_answering_definitions_with(the_index_s_definition_of_foo());
    let host = an_index_daemon_host_serving(fake.clone()).await;
    let registry = host.registry();
    let entry = the_entry_for(&project, Some(registry.clone()));

    // When the pane asks about a file in that checkout
    let refusal = unary_at::<_, DefinitionResponse>(
        &entry,
        "Definition",
        a_definition_request(&project, &canonical(elsewhere.path())),
    )
    .await
    .expect_err("a path outside the project's worktrees is refused");

    // Then it is refused exactly as the worktree service refuses it
    assert_eq!(refusal.code(), tddy_rpc::Code::FailedPrecondition);
    assert_eq!(
        refusal.message(),
        "worktree_path is not a worktree of this project"
    );
    // And nothing was asked of the index, which was not even started for it
    assert_eq!(fake.definitions_it_was_asked(), vec![]);
    assert!(registry.running().await.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn without_an_index_daemon_section_the_service_answers_failed_precondition() {
    // Given a daemon configured without an `index_daemon:` section
    let project = a_project_with_a_worktree();
    let entry = the_entry_for(&project, None);

    // When the pane asks for a definition in a listed worktree
    let refusal = unary_at::<_, DefinitionResponse>(
        &entry,
        "Definition",
        a_definition_request(&project, &project.worktree_path),
    )
    .await
    .expect_err("there is no index to answer from");

    // Then the refusal names the missing configuration rather than falling back to another server
    assert_eq!(refusal.code(), tddy_rpc::Code::FailedPrecondition);
    assert!(
        refusal.message().contains("index_daemon"),
        "the refusal should name the `index_daemon:` section, got {:?}",
        refusal.message()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_first_request_starts_the_index_daemon() {
    // Given a managed index daemon nothing has needed yet
    let project = a_project_with_a_worktree();
    let fake = a_fake_index_answering_definitions_with(the_index_s_definition_of_foo());
    let host = an_index_daemon_host_serving(fake).await;
    let registry = host.registry();
    let entry = the_entry_for(&project, Some(registry.clone()));

    // When the pane's first navigation request arrives
    let _: DefinitionResponse = unary_at(
        &entry,
        "Definition",
        a_definition_request(&project, &project.worktree_path),
    )
    .await
    .expect("the first request is answered");

    // Then that request is what started the index daemon, on the socket the registry dials
    assert_eq!(
        host.argv_it_was_started_with(),
        Some(format!("--grpc-uds {}", host.socket_path().display()))
    );
    assert_eq!(
        registry
            .running()
            .await
            .map(|running| running.socket_path.clone()),
        Some(host.socket_path())
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_references_request_is_forwarded_and_answered_as_worktree_locations() {
    // Given a session worktree and an index daemon that knows where `foo` is used
    let project = a_project_with_a_worktree();
    let fake = a_fake_index_answering_definitions_with(the_index_s_definition_of_foo());
    let host = an_index_daemon_host_serving(fake.clone()).await;
    let entry = the_entry_for(&project, Some(host.registry()));

    // When the pane asks for the references to `foo`
    let answer: ReferencesResponse = unary_at(
        &entry,
        "References",
        ReferencesRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: project.project_id.clone(),
            worktree_path: project.worktree_path.clone(),
            rel_path: "src/main.rs".to_string(),
            position: the_call_to_foo(),
        },
    )
    .await
    .expect("a listed worktree's references are answered");

    // Then the index was asked about that file, rooted at the worktree, and its locations arrive
    assert_eq!(
        fake.references_it_was_asked(),
        vec![index::ReferencesRequest {
            workspace_root: project.worktree_path.clone(),
            file: "src/main.rs".to_string(),
            position: Some(index::SourcePosition { line: 2, column: 5 }),
        }]
    );
    assert_eq!(
        answer
            .locations
            .iter()
            .map(|l| l.rel_path.as_str())
            .collect::<Vec<_>>(),
        vec!["src/lib.rs"]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_hover_request_is_forwarded_and_its_markdown_returned() {
    // Given a session worktree and an index daemon with something to say about `foo`
    let project = a_project_with_a_worktree();
    let fake = a_fake_index_answering_definitions_with(the_index_s_definition_of_foo());
    let host = an_index_daemon_host_serving(fake.clone()).await;
    let entry = the_entry_for(&project, Some(host.registry()));

    // When the pane hovers the call to `foo`
    let answer: HoverResponse = unary_at(
        &entry,
        "Hover",
        HoverRequest {
            session_token: TEST_TOKEN.to_string(),
            project_id: project.project_id.clone(),
            worktree_path: project.worktree_path.clone(),
            rel_path: "src/main.rs".to_string(),
            position: the_call_to_foo(),
        },
    )
    .await
    .expect("a listed worktree's hover is answered");

    // Then the index was asked about that file, rooted at the worktree, and its text arrives
    assert_eq!(
        fake.hovers_it_was_asked(),
        vec![index::HoverRequest {
            workspace_root: project.worktree_path.clone(),
            file: "src/main.rs".to_string(),
            position: Some(index::SourcePosition { line: 2, column: 5 }),
        }]
    );
    assert_eq!(answer.markdown.as_deref(), Some(THE_INDEX_S_HOVER_OF_FOO));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_rel_path_leaving_the_worktree_is_refused_without_asking_the_index() {
    // Given a listed worktree and a managed index daemon
    let project = a_project_with_a_worktree();
    let fake = a_fake_index_answering_definitions_with(the_index_s_definition_of_foo());
    let host = an_index_daemon_host_serving(fake.clone()).await;
    let registry = host.registry();
    let entry = the_entry_for(&project, Some(registry.clone()));

    for escaping in ["../secret.rs", "/etc/passwd"] {
        // When the pane asks about a file outside the worktree
        let mut request = a_definition_request(&project, &project.worktree_path);
        request.rel_path = escaping.to_string();
        let refusal = unary_at::<_, DefinitionResponse>(&entry, "Definition", request)
            .await
            .expect_err("a path leaving the worktree is refused");

        // Then it is refused as invalid, and the index was neither asked nor started
        assert_eq!(
            refusal.code(),
            tddy_rpc::Code::InvalidArgument,
            "{escaping}"
        );
    }
    assert_eq!(fake.definitions_it_was_asked(), vec![]);
    assert!(registry.running().await.is_none());
}
