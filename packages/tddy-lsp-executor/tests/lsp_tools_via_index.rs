//! What a session's `Lsp*` tools owe the agent when the daemon manages a warm index: ask that index
//! — rooted at the session's own worktree, bound on the host — and answer in exactly the JSON the
//! existing executor answers in; refuse a file outside the worktree before the index hears of it;
//! and, without an index, leave today's executor answering.
//!
//! The executor under test is the one a host registers, [`select_lsp_executor`], and every call
//! runs on a blocking thread the way `tddy_tool_engine`'s `Lsp*` dispatch runs it. The index is a
//! fake `code_index` server on an AF_UNIX socket, dialled through the same kind of channel the
//! daemon's `IndexDaemonRegistry::connect` hands out, which records every request it is asked — so
//! these tests pin this crate's translation, not the index's own answers.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use pretty_assertions::assert_eq;
use serde_json::{json, Value};
use tddy_core::toolcall::lsp::{LspExecutor, LspQuery};
use tddy_index_daemon::proto::code_index as index;
use tddy_index_daemon::proto::code_index::{CodeIndexService, CodeIndexServiceTonicAdapter};
use tddy_index_daemon::proto::tonic_code_index::code_index_service_server::CodeIndexServiceServer as TonicCodeIndexServiceServer;
use tddy_index_daemon::EventStream;
use tddy_lsp_executor::index_backed::{select_lsp_executor, IndexChannel};
use tempfile::TempDir;

// ---------------------------------------------------------------------------------------------
// Session worktrees

/// A session's worktree: `src/main.rs` calls `foo`, which `src/lib.rs` defines on line 11, and
/// `src/accents.rs` opens with a comment whose accents and emoji put `foo` at UTF-16 column 7 but
/// byte column 10.
struct ASessionWorktree {
    dir: TempDir,
}

fn a_session_worktree() -> ASessionWorktree {
    let dir = TempDir::new().expect("a session worktree");
    std::fs::create_dir_all(dir.path().join("src")).expect("src");
    std::fs::write(
        dir.path().join("src/main.rs"),
        "fn main() {\n    foo();\n}\n",
    )
    .expect("main.rs");
    std::fs::write(
        dir.path().join("src/lib.rs"),
        format!("{}fn foo() {{}}\n", "//\n".repeat(10)),
    )
    .expect("lib.rs");
    std::fs::write(dir.path().join("src/accents.rs"), "// é🎉 foo\n").expect("accents.rs");
    ASessionWorktree { dir }
}

impl ASessionWorktree {
    /// The worktree as the host resolved it from the session — canonical, so the root the index is
    /// asked about is exactly the one handed to the executor.
    fn root(&self) -> PathBuf {
        self.dir
            .path()
            .canonicalize()
            .expect("an existing worktree")
    }

    fn root_string(&self) -> String {
        self.root().display().to_string()
    }

    fn uri_of(&self, file: &str) -> String {
        format!("file://{}", self.root().join(file).display())
    }
}

// ---------------------------------------------------------------------------------------------
// The warm index: a fake `code_index` server that records what it is asked

/// Canned answers, and every navigation, symbols and diagnostics request the fake was asked.
#[derive(Clone, Default)]
struct AFakeIndex {
    definition_answer: index::DefinitionResponse,
    references_answer: index::ReferencesResponse,
    hover_answer: index::HoverResponse,
    symbols_answer: index::SymbolsResponse,
    diagnostics_answer: index::DiagnosticsResponse,
    refusal: Option<String>,
    asked: Arc<Mutex<Vec<AskedOfTheIndex>>>,
}

/// One request the fake index received, in arrival order.
#[derive(Debug, Clone, PartialEq)]
enum AskedOfTheIndex {
    Definition(index::DefinitionRequest),
    References(index::ReferencesRequest),
    Hover(index::HoverRequest),
    Symbols(index::SymbolsRequest),
    Diagnostics(index::DiagnosticsRequest),
}

fn a_fake_index() -> AFakeIndex {
    AFakeIndex::default()
}

impl AFakeIndex {
    fn answering_definitions_with(mut self, answer: index::DefinitionResponse) -> Self {
        self.definition_answer = answer;
        self
    }

    fn answering_references_with(mut self, answer: index::ReferencesResponse) -> Self {
        self.references_answer = answer;
        self
    }

    fn answering_hovers_with(mut self, answer: index::HoverResponse) -> Self {
        self.hover_answer = answer;
        self
    }

    /// Every request is answered with an error status carrying `message` instead of an answer.
    fn refusing_everything_with(mut self, message: &str) -> Self {
        self.refusal = Some(message.to_string());
        self
    }

    fn answering_symbols_with(mut self, answer: index::SymbolsResponse) -> Self {
        self.symbols_answer = answer;
        self
    }

    fn answering_diagnostics_with(mut self, answer: index::DiagnosticsResponse) -> Self {
        self.diagnostics_answer = answer;
        self
    }

    fn what_it_was_asked(&self) -> Vec<AskedOfTheIndex> {
        self.asked.lock().expect("the request log").clone()
    }

    /// Logs `asked`, then reports the configured refusal, if any.
    fn record(&self, asked: AskedOfTheIndex) -> Result<(), tddy_rpc::Status> {
        self.asked.lock().expect("the request log").push(asked);
        match &self.refusal {
            Some(message) => Err(tddy_rpc::Status::failed_precondition(message.clone())),
            None => Ok(()),
        }
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
        self.record(AskedOfTheIndex::Definition(request.into_inner()))?;
        Ok(tddy_rpc::Response::new(self.definition_answer.clone()))
    }

    async fn symbols(
        &self,
        request: tddy_rpc::Request<index::SymbolsRequest>,
    ) -> Result<tddy_rpc::Response<index::SymbolsResponse>, tddy_rpc::Status> {
        self.record(AskedOfTheIndex::Symbols(request.into_inner()))?;
        Ok(tddy_rpc::Response::new(self.symbols_answer.clone()))
    }

    async fn diagnostics(
        &self,
        request: tddy_rpc::Request<index::DiagnosticsRequest>,
    ) -> Result<tddy_rpc::Response<index::DiagnosticsResponse>, tddy_rpc::Status> {
        self.record(AskedOfTheIndex::Diagnostics(request.into_inner()))?;
        Ok(tddy_rpc::Response::new(self.diagnostics_answer.clone()))
    }

    async fn references(
        &self,
        request: tddy_rpc::Request<index::ReferencesRequest>,
    ) -> Result<tddy_rpc::Response<index::ReferencesResponse>, tddy_rpc::Status> {
        self.record(AskedOfTheIndex::References(request.into_inner()))?;
        Ok(tddy_rpc::Response::new(self.references_answer.clone()))
    }

    async fn hover(
        &self,
        request: tddy_rpc::Request<index::HoverRequest>,
    ) -> Result<tddy_rpc::Response<index::HoverResponse>, tddy_rpc::Status> {
        self.record(AskedOfTheIndex::Hover(request.into_inner()))?;
        Ok(tddy_rpc::Response::new(self.hover_answer.clone()))
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

/// `fake`, served on an AF_UNIX socket of its own and dialled per call, as the daemon's registry
/// dials the managed index daemon.
struct AnIndexOnASocket {
    dir: TempDir,
}

async fn an_index_serving(fake: AFakeIndex) -> AnIndexOnASocket {
    let host = AnIndexOnASocket {
        dir: TempDir::new().expect("a directory for the index socket"),
    };
    let listener =
        tokio::net::UnixListener::bind(host.socket_path()).expect("the fake index binds");
    tokio::spawn(
        tonic::transport::Server::builder()
            .add_service(TonicCodeIndexServiceServer::new(
                CodeIndexServiceTonicAdapter::new(Arc::new(fake)),
            ))
            .serve_with_incoming(tokio_stream::wrappers::UnixListenerStream::new(listener)),
    );
    host
}

impl AnIndexOnASocket {
    fn socket_path(&self) -> PathBuf {
        self.dir.path().join("index.sock")
    }
}

#[async_trait::async_trait]
impl IndexChannel for AnIndexOnASocket {
    async fn connect(&self) -> Result<tonic::transport::Channel, String> {
        let path = self.socket_path();
        tonic::transport::Endpoint::try_from("http://127.0.0.1:50051")
            .map_err(|err| err.to_string())?
            .connect_with_connector(tower::service_fn(move |_| {
                let path = path.clone();
                async move {
                    let stream = tokio::net::UnixStream::connect(&path).await?;
                    Ok::<_, std::io::Error>(hyper_util::rt::TokioIo::new(stream))
                }
            }))
            .await
            .map_err(|err| err.to_string())
    }
}

// ---------------------------------------------------------------------------------------------
// The executor a host has today

/// Stands in for `TddyLspExecutor`: answers every query with a payload naming itself, and records
/// that it was asked — so a test can tell which executor answered.
#[derive(Clone, Default)]
struct TheExistingExecutor {
    asked: Arc<Mutex<Vec<String>>>,
}

fn the_existing_executor() -> TheExistingExecutor {
    TheExistingExecutor::default()
}

impl TheExistingExecutor {
    fn answer(&self, op: &str) -> Result<Value, String> {
        self.asked
            .lock()
            .expect("the call log")
            .push(op.to_string());
        Ok(json!({ "answered_by": "the existing executor", "op": op }))
    }

    fn what_it_was_asked(&self) -> Vec<String> {
        self.asked.lock().expect("the call log").clone()
    }
}

impl LspExecutor for TheExistingExecutor {
    fn is_available(&self, _repo_dir: &Path) -> bool {
        true
    }
    fn diagnostics(&self, _repo_dir: &Path, _query: &LspQuery) -> Result<Value, String> {
        self.answer("diagnostics")
    }
    fn definition(&self, _repo_dir: &Path, _query: &LspQuery) -> Result<Value, String> {
        self.answer("definition")
    }
    fn references(&self, _repo_dir: &Path, _query: &LspQuery) -> Result<Value, String> {
        self.answer("references")
    }
    fn hover(&self, _repo_dir: &Path, _query: &LspQuery) -> Result<Value, String> {
        self.answer("hover")
    }
    fn symbols(&self, _repo_dir: &Path, _query: &LspQuery) -> Result<Value, String> {
        self.answer("symbols")
    }
    fn workspace_diagnostics(&self, _repo_dir: &Path) -> Result<Value, String> {
        self.answer("workspace_diagnostics")
    }
}

// ---------------------------------------------------------------------------------------------
// The agent's tool calls

/// The executor a host with `index` (or without one) registers, over `existing`.
fn the_registered_executor(
    index: Option<AnIndexOnASocket>,
    existing: &TheExistingExecutor,
) -> Arc<dyn LspExecutor> {
    select_lsp_executor(
        index.map(|index| Arc::new(index) as Arc<dyn IndexChannel>),
        Arc::new(existing.clone()),
    )
}

/// One of the five `Lsp*` tools.
#[derive(Debug, Clone, Copy)]
enum LspTool {
    Definition,
    References,
    Hover,
    Symbols,
    Diagnostics,
}

/// A position query at zero-based `line:character` of `file`, as the tool's arguments carry it.
fn a_query_at(file: &str, line: u32, character: u32) -> LspQuery {
    LspQuery {
        target: "packages/subject:library".to_string(),
        file: file.to_string(),
        line,
        character,
        symbol_query: None,
    }
}

/// A workspace symbol search for `symbol_query`, which names `file` only because the tool's schema
/// requires one.
fn a_workspace_symbol_search(symbol_query: &str, file: &str) -> LspQuery {
    LspQuery {
        symbol_query: Some(symbol_query.to_string()),
        ..a_query_at(file, 0, 0)
    }
}

/// What the agent's `tool` call against `worktree` answers, run on a blocking thread exactly as
/// `tddy_tool_engine`'s `Lsp*` dispatch runs it.
async fn the_agent_calls(
    executor: &Arc<dyn LspExecutor>,
    tool: LspTool,
    worktree: PathBuf,
    query: LspQuery,
) -> Result<Value, String> {
    let executor = Arc::clone(executor);
    tokio::task::spawn_blocking(move || match tool {
        LspTool::Definition => executor.definition(&worktree, &query),
        LspTool::References => executor.references(&worktree, &query),
        LspTool::Hover => executor.hover(&worktree, &query),
        LspTool::Symbols => executor.symbols(&worktree, &query),
        LspTool::Diagnostics => executor.diagnostics(&worktree, &query),
    })
    .await
    .expect("the tool call ran to completion")
}

/// What the agent's `ReadLints` call against `worktree` answers, run on a blocking thread exactly
/// as `tddy_tool_engine`'s `Lsp*` dispatch runs it.
async fn the_agent_reads_lints(
    executor: &Arc<dyn LspExecutor>,
    worktree: PathBuf,
) -> Result<Value, String> {
    let executor = Arc::clone(executor);
    tokio::task::spawn_blocking(move || executor.workspace_diagnostics(&worktree))
        .await
        .expect("the tool call ran to completion")
}

// ---------------------------------------------------------------------------------------------
// Coordinates

/// A one-based byte range on one line, as the index carries it.
fn an_index_range_on_line(
    line: u32,
    start_column: u32,
    end_column: u32,
) -> Option<index::SourceRange> {
    Some(index::SourceRange {
        start: Some(index::SourcePosition {
            line,
            column: start_column,
        }),
        end: Some(index::SourcePosition {
            line,
            column: end_column,
        }),
    })
}

/// A zero-based LSP range on one line, as the tools' JSON carries it.
fn an_lsp_range_on_line(line: u32, start: u32, end: u32) -> Value {
    json!({
        "start": { "line": line, "character": start },
        "end": { "line": line, "character": end },
    })
}

/// `fn foo() {}` on line 11 of `src/lib.rs`: the name at one-based bytes 4 to 7.
fn the_index_s_definition_of_foo() -> index::DefinitionResponse {
    index::DefinitionResponse {
        locations: vec![index::CodeLocation {
            file: "src/lib.rs".to_string(),
            range: an_index_range_on_line(11, 4, 7),
            outside_root: false,
        }],
    }
}

/// `foo` as the index locates it in `src/lib.rs`, on line 11 at one-based bytes 4 to 7.
fn the_index_s_location_of_foo() -> index::CodeLocation {
    index::CodeLocation {
        file: "src/lib.rs".to_string(),
        range: an_index_range_on_line(11, 4, 7),
        outside_root: false,
    }
}

/// The one symbol `src/lib.rs` declares: the function `foo`, the whole of line 11.
fn the_index_s_symbols_of_lib() -> index::SymbolsResponse {
    index::SymbolsResponse {
        symbols: vec![index::CodeSymbol {
            name: "foo".to_string(),
            kind: 12,
            location: Some(index::CodeLocation {
                file: "src/lib.rs".to_string(),
                range: an_index_range_on_line(11, 1, 12),
                outside_root: false,
            }),
            container: None,
        }],
    }
}

/// The error the index reports in `src/main.rs`: `foo` on line 2 is not in scope.
fn the_index_s_diagnostics_of_main() -> index::DiagnosticsResponse {
    index::DiagnosticsResponse {
        diagnostics: vec![index::CodeDiagnostic {
            range: an_index_range_on_line(2, 5, 8),
            severity: 1,
            message: "cannot find function `foo` in this scope".to_string(),
            source: Some("rustc".to_string()),
        }],
    }
}

// ---------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn lsp_definition_is_answered_by_the_index_for_the_session_worktree() {
    // Given a session worktree, a warm index that knows where `foo` is defined, and today's executor
    let worktree = a_session_worktree();
    let fake = a_fake_index().answering_definitions_with(the_index_s_definition_of_foo());
    let existing = the_existing_executor();
    let executor = the_registered_executor(Some(an_index_serving(fake.clone()).await), &existing);

    // When the agent asks for the definition of the call to `foo` (zero-based 1:4 of src/main.rs)
    let answer = the_agent_calls(
        &executor,
        LspTool::Definition,
        worktree.root(),
        a_query_at("src/main.rs", 1, 4),
    )
    .await;

    // Then the index was asked about that file, rooted at the session's worktree, one-based
    assert_eq!(
        fake.what_it_was_asked(),
        vec![AskedOfTheIndex::Definition(index::DefinitionRequest {
            workspace_root: worktree.root_string(),
            file: "src/main.rs".to_string(),
            position: Some(index::SourcePosition { line: 2, column: 5 }),
        })]
    );
    // And the agent gets the existing tool's JSON: a file URI and a zero-based LSP range
    assert_eq!(
        answer,
        Ok(json!({
            "locations": [{
                "uri": worktree.uri_of("src/lib.rs"),
                "range": an_lsp_range_on_line(10, 3, 6),
            }]
        }))
    );
    // And no language server of the existing executor's was asked
    assert_eq!(existing.what_it_was_asked(), Vec::<String>::new());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_path_outside_the_session_worktree_is_refused_on_the_host() {
    // Given a session worktree, a neighbouring session's worktree, and a warm index
    let worktree = a_session_worktree();
    let neighbour = a_session_worktree();
    let fake = a_fake_index().answering_definitions_with(the_index_s_definition_of_foo());
    let existing = the_existing_executor();
    let executor = the_registered_executor(Some(an_index_serving(fake.clone()).await), &existing);
    let neighbours_file = neighbour.root().join("src/lib.rs").display().to_string();

    // When the agent names a file in the neighbour's worktree
    let answer = the_agent_calls(
        &executor,
        LspTool::Definition,
        worktree.root(),
        a_query_at(&neighbours_file, 10, 3),
    )
    .await;

    // Then the host refuses it, naming the file
    assert_eq!(
        answer,
        Err(format!(
            "{neighbours_file} is outside the session's worktree"
        ))
    );
    // And neither the index nor the existing executor heard of it
    assert_eq!(fake.what_it_was_asked(), vec![]);
    assert_eq!(existing.what_it_was_asked(), Vec::<String>::new());
}

#[tokio::test(flavor = "multi_thread")]
async fn lsp_symbols_are_answered_by_the_index() {
    // Given a session worktree and a warm index that outlines src/lib.rs
    let worktree = a_session_worktree();
    let fake = a_fake_index().answering_symbols_with(the_index_s_symbols_of_lib());
    let existing = the_existing_executor();
    let executor = the_registered_executor(Some(an_index_serving(fake.clone()).await), &existing);

    // When the agent asks for the symbols of src/lib.rs
    let symbols = the_agent_calls(
        &executor,
        LspTool::Symbols,
        worktree.root(),
        a_query_at("src/lib.rs", 0, 0),
    )
    .await;

    // Then the index was asked for that file's symbols, rooted at the session's worktree
    assert_eq!(
        fake.what_it_was_asked(),
        vec![AskedOfTheIndex::Symbols(index::SymbolsRequest {
            workspace_root: worktree.root_string(),
            file: "src/lib.rs".to_string(),
            query: None,
        })]
    );
    // And the symbols are the existing tool's JSON, zero-based
    assert_eq!(
        symbols,
        Ok(json!({
            "symbols": [{
                "name": "foo",
                "kind": 12,
                "location": {
                    "uri": worktree.uri_of("src/lib.rs"),
                    "range": an_lsp_range_on_line(10, 0, 11),
                },
                "container": null,
            }]
        }))
    );
    // And the existing executor was not asked
    assert_eq!(existing.what_it_was_asked(), Vec::<String>::new());
}

#[tokio::test(flavor = "multi_thread")]
async fn lsp_diagnostics_are_answered_by_the_index() {
    // Given a session worktree and a warm index that diagnoses src/main.rs
    let worktree = a_session_worktree();
    let fake = a_fake_index().answering_diagnostics_with(the_index_s_diagnostics_of_main());
    let existing = the_existing_executor();
    let executor = the_registered_executor(Some(an_index_serving(fake.clone()).await), &existing);

    // When the agent asks for the diagnostics of src/main.rs
    let diagnostics = the_agent_calls(
        &executor,
        LspTool::Diagnostics,
        worktree.root(),
        a_query_at("src/main.rs", 0, 0),
    )
    .await;

    // Then the index was asked for that file's diagnostics, rooted at the session's worktree
    assert_eq!(
        fake.what_it_was_asked(),
        vec![AskedOfTheIndex::Diagnostics(index::DiagnosticsRequest {
            workspace_root: worktree.root_string(),
            file: "src/main.rs".to_string(),
        })]
    );
    // And the diagnostics are the existing tool's JSON, zero-based
    assert_eq!(
        diagnostics,
        Ok(json!({
            "diagnostics": [{
                "range": an_lsp_range_on_line(1, 4, 7),
                "severity": 1,
                "message": "cannot find function `foo` in this scope",
                "source": "rustc",
            }]
        }))
    );
    // And the existing executor was not asked
    assert_eq!(existing.what_it_was_asked(), Vec::<String>::new());
}

#[tokio::test(flavor = "multi_thread")]
async fn lsp_references_are_answered_by_the_index() {
    // Given a session worktree and a warm index that knows where `foo` is referenced
    let worktree = a_session_worktree();
    let fake = a_fake_index().answering_references_with(index::ReferencesResponse {
        locations: vec![the_index_s_location_of_foo()],
    });
    let existing = the_existing_executor();
    let executor = the_registered_executor(Some(an_index_serving(fake.clone()).await), &existing);

    // When the agent asks for the references of the call to `foo` (zero-based 1:4 of src/main.rs)
    let answer = the_agent_calls(
        &executor,
        LspTool::References,
        worktree.root(),
        a_query_at("src/main.rs", 1, 4),
    )
    .await;

    // Then the index was asked for references, not definitions, at the one-based position
    assert_eq!(
        fake.what_it_was_asked(),
        vec![AskedOfTheIndex::References(index::ReferencesRequest {
            workspace_root: worktree.root_string(),
            file: "src/main.rs".to_string(),
            position: Some(index::SourcePosition { line: 2, column: 5 }),
        })]
    );
    // And the agent gets the locations under the `references` key
    assert_eq!(
        answer,
        Ok(json!({
            "references": [{
                "uri": worktree.uri_of("src/lib.rs"),
                "range": an_lsp_range_on_line(10, 3, 6),
            }]
        }))
    );
    // And the existing executor was not asked
    assert_eq!(existing.what_it_was_asked(), Vec::<String>::new());
}

#[tokio::test(flavor = "multi_thread")]
async fn lsp_hover_is_answered_by_the_index() {
    // Given a session worktree and a warm index that can describe `foo`
    let worktree = a_session_worktree();
    let fake = a_fake_index().answering_hovers_with(index::HoverResponse {
        markdown: Some("fn foo()".to_string()),
    });
    let existing = the_existing_executor();
    let executor = the_registered_executor(Some(an_index_serving(fake.clone()).await), &existing);

    // When the agent hovers the call to `foo` (zero-based 1:4 of src/main.rs)
    let answer = the_agent_calls(
        &executor,
        LspTool::Hover,
        worktree.root(),
        a_query_at("src/main.rs", 1, 4),
    )
    .await;

    // Then the index was asked to hover at the one-based position
    assert_eq!(
        fake.what_it_was_asked(),
        vec![AskedOfTheIndex::Hover(index::HoverRequest {
            workspace_root: worktree.root_string(),
            file: "src/main.rs".to_string(),
            position: Some(index::SourcePosition { line: 2, column: 5 }),
        })]
    );
    // And the agent gets the markdown under the `hover` key
    assert_eq!(answer, Ok(json!({ "hover": "fn foo()" })));
    // And the existing executor was not asked
    assert_eq!(existing.what_it_was_asked(), Vec::<String>::new());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_workspace_symbol_search_asks_the_index_with_the_query_and_no_file() {
    // Given a session worktree and a warm index
    let worktree = a_session_worktree();
    let fake = a_fake_index().answering_symbols_with(the_index_s_symbols_of_lib());
    let existing = the_existing_executor();
    let executor = the_registered_executor(Some(an_index_serving(fake.clone()).await), &existing);

    // When the agent searches the workspace for `foo`, naming a file outside the worktree
    let answer = the_agent_calls(
        &executor,
        LspTool::Symbols,
        worktree.root(),
        a_workspace_symbol_search("foo", "/sessions/elsewhere/src/lib.rs"),
    )
    .await;

    // Then the index was asked for that query with an empty file, and nothing was refused
    assert_eq!(
        fake.what_it_was_asked(),
        vec![AskedOfTheIndex::Symbols(index::SymbolsRequest {
            workspace_root: worktree.root_string(),
            file: String::new(),
            query: Some("foo".to_string()),
        })]
    );
    assert!(answer.is_ok(), "the search was refused: {answer:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_zero_based_utf16_column_reaches_the_index_as_a_one_based_byte_column() {
    // Given a worktree whose src/accents.rs has `foo` after an accent and an emoji, at UTF-16
    // column 7 and byte offset 10
    let worktree = a_session_worktree();
    let fake = a_fake_index().answering_definitions_with(the_index_s_definition_of_foo());
    let existing = the_existing_executor();
    let executor = the_registered_executor(Some(an_index_serving(fake.clone()).await), &existing);

    // When the agent asks for the definition at zero-based 0:7
    let _ = the_agent_calls(
        &executor,
        LspTool::Definition,
        worktree.root(),
        a_query_at("src/accents.rs", 0, 7),
    )
    .await;

    // Then the index was asked at one-based line 1, byte column 11
    assert_eq!(
        fake.what_it_was_asked(),
        vec![AskedOfTheIndex::Definition(index::DefinitionRequest {
            workspace_root: worktree.root_string(),
            file: "src/accents.rs".to_string(),
            position: Some(index::SourcePosition {
                line: 1,
                column: 11
            }),
        })]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_one_based_byte_column_from_the_index_comes_back_as_a_zero_based_utf16_column() {
    // Given a worktree whose src/accents.rs has `foo` after an accent and an emoji, and an index
    // that locates `foo` there at one-based bytes 11 to 14
    let worktree = a_session_worktree();
    let fake = a_fake_index().answering_definitions_with(index::DefinitionResponse {
        locations: vec![index::CodeLocation {
            file: "src/accents.rs".to_string(),
            range: an_index_range_on_line(1, 11, 14),
            outside_root: false,
        }],
    });
    let existing = the_existing_executor();
    let executor = the_registered_executor(Some(an_index_serving(fake.clone()).await), &existing);

    // When the agent asks for a definition
    let answer = the_agent_calls(
        &executor,
        LspTool::Definition,
        worktree.root(),
        a_query_at("src/main.rs", 1, 4),
    )
    .await;

    // Then the range is zero-based line 0, UTF-16 columns 7 to 10
    assert_eq!(
        answer,
        Ok(json!({
            "locations": [{
                "uri": worktree.uri_of("src/accents.rs"),
                "range": an_lsp_range_on_line(0, 7, 10),
            }]
        }))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_location_outside_the_index_root_is_answered_by_its_own_file_uri() {
    // Given a session worktree and an index that locates the definition in a dependency's source,
    // outside its root
    let worktree = a_session_worktree();
    let dependency = TempDir::new().expect("a dependency's source directory");
    let dependency_file = dependency.path().join("registry_crate.rs");
    std::fs::write(&dependency_file, "pub fn foo() {}\n").expect("the dependency's source");
    let fake = a_fake_index().answering_definitions_with(index::DefinitionResponse {
        locations: vec![index::CodeLocation {
            file: dependency_file.display().to_string(),
            range: an_index_range_on_line(1, 8, 11),
            outside_root: true,
        }],
    });
    let existing = the_existing_executor();
    let executor = the_registered_executor(Some(an_index_serving(fake.clone()).await), &existing);

    // When the agent asks for the definition of the call to `foo`
    let answer = the_agent_calls(
        &executor,
        LspTool::Definition,
        worktree.root(),
        a_query_at("src/main.rs", 1, 4),
    )
    .await;

    // Then the location is that absolute path's file URI, not joined onto the worktree
    assert_eq!(
        answer,
        Ok(json!({
            "locations": [{
                "uri": format!("file://{}", dependency_file.display()),
                "range": an_lsp_range_on_line(0, 7, 10),
            }]
        }))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_error_from_the_index_is_the_tools_error_and_the_existing_executor_is_not_asked() {
    // Given a session worktree and an index that refuses every request
    let worktree = a_session_worktree();
    let fake = a_fake_index().refusing_everything_with("the crate graph is still loading");
    let existing = the_existing_executor();
    let executor = the_registered_executor(Some(an_index_serving(fake.clone()).await), &existing);

    // When the agent asks for a definition
    let answer = the_agent_calls(
        &executor,
        LspTool::Definition,
        worktree.root(),
        a_query_at("src/main.rs", 1, 4),
    )
    .await;

    // Then the tool fails with the index's message
    assert_eq!(answer, Err("the crate graph is still loading".to_string()));
    // And the existing executor was not asked in the index's place
    assert_eq!(existing.what_it_was_asked(), Vec::<String>::new());
}

#[tokio::test(flavor = "multi_thread")]
async fn without_an_index_daemon_the_existing_executor_answers() {
    // Given a host with no `index_daemon:` section, so no index to ask
    let worktree = a_session_worktree();
    let existing = the_existing_executor();
    let executor = the_registered_executor(None, &existing);

    // When the agent asks for a definition
    let answer = the_agent_calls(
        &executor,
        LspTool::Definition,
        worktree.root(),
        a_query_at("src/main.rs", 1, 4),
    )
    .await;

    // Then today's executor answers it, unchanged
    assert_eq!(
        answer,
        Ok(json!({ "answered_by": "the existing executor", "op": "definition" }))
    );
    assert_eq!(existing.what_it_was_asked(), vec!["definition".to_string()]);
}

#[tokio::test(flavor = "multi_thread")]
async fn read_lints_is_refused_through_the_index_and_not_answered_by_the_existing_executor() {
    // Given a session worktree and a warm index, which answers diagnostics one file at a time
    let worktree = a_session_worktree();
    let fake = a_fake_index().answering_diagnostics_with(the_index_s_diagnostics_of_main());
    let existing = the_existing_executor();
    let executor = the_registered_executor(Some(an_index_serving(fake.clone()).await), &existing);

    // When the agent reads the lints of the whole worktree
    let answer = the_agent_reads_lints(&executor, worktree.root()).await;

    // Then the call is refused, pointing the agent at `LspDiagnostics`
    let refusal = answer.expect_err("ReadLints is not answered through the index");
    assert!(
        refusal.contains("LspDiagnostics"),
        "the refusal should name LspDiagnostics, got: {refusal}"
    );
    // And the existing executor was not asked in the index's place
    assert_eq!(existing.what_it_was_asked(), Vec::<String>::new());
}
