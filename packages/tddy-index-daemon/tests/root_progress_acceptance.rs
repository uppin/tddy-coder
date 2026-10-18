//! A workspace root's index progress: what anyone watching a root sees, what a warm reports, and
//! what a navigation answer says about the index it came from.
//!
//! Every request is dispatched at the registered coordinate, as `code_index_service_acceptance`
//! does. The language server is `fake_lsp --loads-crate-graph-on-signal <file>`: it begins two
//! overlapping phases (`Fetching`, `Indexing`) and holds its crate-graph load until the test creates
//! that file — so "mid-load" and "once loaded" are moments the test chooses, not ones it sleeps
//! towards. Every await on a stream is bounded, so a missing message fails rather than hangs.

use std::path::{Path, PathBuf};
use std::time::Duration;

use pretty_assertions::assert_eq;
use prost::Message;
use tddy_index_daemon::proto::code_index::{
    ActivePhase, DefinitionRequest, DefinitionResponse, IndexProgress, IndexState, SourcePosition,
    WarmRequest, WatchRootRequest, WorkspacesRequest, WorkspacesResponse,
};
use tddy_index_daemon::{build_code_index_entry, CodeIndexPorts};
use tddy_lsp::{Language, LaunchSpec, LspAllowList, LspRegistry};
use tddy_task::TaskRegistry;
use tokio::sync::mpsc;

/// The longest any one progress message may take to arrive. It bounds a failure rather than paces
/// a success: every step here is answered in milliseconds, the slowest being the first message after
/// a cold spawn of `fake_lsp` (process start plus the `initialize` handshake) and a load the fake
/// notices within its 20 ms signal poll. Five seconds leaves room for a loaded CI runner without
/// letting a missing message hold the suite for long.
const A_PROGRESS_STEP: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------------------------------------
// Fixtures

/// A host whose language server is `fake_lsp` started with `args`.
fn a_host_running_fake_lsp(args: Vec<String>) -> tddy_rpc::ServiceEntry {
    let mut spec = LaunchSpec::new(env!("CARGO_BIN_EXE_fake_lsp"))
        .with_capabilities(tddy_code_restructuring::client_capabilities())
        .with_initialization_options(tddy_code_restructuring::server_settings());
    spec.args = args;
    let mut allow = LspAllowList::new();
    allow.allow(Language::Rust, spec);
    build_code_index_entry(CodeIndexPorts {
        servers: LspRegistry::new(allow, TaskRegistry::new(), Duration::from_secs(60)),
        wait_heartbeat: tddy_code_restructuring::backends::rust::WAIT_HEARTBEAT,
    })
}

/// A host whose language servers hold their crate-graph load until `signal` exists, answering every
/// request in their own workspace meanwhile.
fn a_host_whose_graph_loads_once(signal: &TheGraphLoadSignal) -> tddy_rpc::ServiceEntry {
    a_host_running_fake_lsp(vec![
        "--answers-in-its-workspace".to_string(),
        "--loads-crate-graph-on-signal".to_string(),
        signal.path().to_string_lossy().to_string(),
    ])
}

/// A host whose language server exits as soon as it is spawned.
fn a_host_whose_server_cannot_start() -> tddy_rpc::ServiceEntry {
    a_host_running_fake_lsp(vec!["--exit-immediately".to_string()])
}

/// The file whose appearance tells the fake server its crate graph has finished loading.
struct TheGraphLoadSignal {
    directory: tempfile::TempDir,
}

fn a_graph_load_signal() -> TheGraphLoadSignal {
    TheGraphLoadSignal {
        directory: tempfile::tempdir().expect("a directory for the load signal"),
    }
}

impl TheGraphLoadSignal {
    fn path(&self) -> PathBuf {
        self.directory.path().join("graph-loaded")
    }

    /// Let every server waiting on this signal finish loading its crate graph.
    fn the_graph_finishes_loading(&self) {
        std::fs::write(self.path(), b"").expect("the load signal is written");
    }
}

/// A git worktree holding a package called `subject` whose `src/lib.rs` calls `foo`.
fn a_workspace_calling_foo() -> tempfile::TempDir {
    let workspace = tempfile::tempdir().expect("a temporary workspace");
    std::process::Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(workspace.path())
        .status()
        .expect("git init");
    std::fs::create_dir_all(workspace.path().join("src")).expect("src");
    std::fs::write(
        workspace.path().join("src/lib.rs"),
        "pub fn caller() -> u32 {\n    foo()\n}\n\nfn foo() -> u32 {\n    42\n}\n",
    )
    .expect("a source file");
    std::fs::write(
        workspace.path().join("Cargo.toml"),
        "[package]\nname = \"subject\"\nversion = \"0.1.0\"\n",
    )
    .expect("a package manifest");
    workspace
}

fn root_of(workspace: &Path) -> String {
    workspace.to_string_lossy().to_string()
}

/// The two phases the fake server runs while its crate graph loads, as a watcher should name them.
fn the_two_loading_phases() -> Vec<ActivePhase> {
    vec![
        ActivePhase {
            title: "Fetching".to_string(),
            message: "metadata".to_string(),
            percentage: Some(40),
        },
        ActivePhase {
            title: "Indexing".to_string(),
            message: "120/310 (subject)".to_string(),
            percentage: Some(38),
        },
    ]
}

// ---------------------------------------------------------------------------------------------
// Dispatch

/// An incoming request with no transport metadata — nothing here dispatches on the sender.
fn a_message_carrying<Req: Message>(request: Req) -> tddy_rpc::RpcMessage {
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
        .handle_rpc(entry.name, method, &a_message_carrying(request))
        .await
    {
        tddy_rpc::RpcResult::Unary(Ok(payload)) => {
            Ok(Res::decode(payload.as_slice()).expect("the answer decodes"))
        }
        tddy_rpc::RpcResult::Unary(Err(status)) => Err(status),
        tddy_rpc::RpcResult::ServerStream(_) => panic!("expected a unary answer from {method}"),
    }
}

/// Open a progress stream (`Warm` or `WatchRoot`) at the registered coordinate, left open so the
/// test reads it as things happen.
async fn progress_stream_at<Req: Message>(
    entry: &tddy_rpc::ServiceEntry,
    method: &str,
    request: Req,
) -> Result<ProgressStream, tddy_rpc::Status> {
    match entry
        .service
        .handle_rpc(entry.name, method, &a_message_carrying(request))
        .await
    {
        tddy_rpc::RpcResult::ServerStream(Ok(receiver)) => Ok(ProgressStream {
            method: method.to_string(),
            receiver,
        }),
        tddy_rpc::RpcResult::ServerStream(Err(status)) => Err(status),
        tddy_rpc::RpcResult::Unary(Err(status)) => Err(status),
        tddy_rpc::RpcResult::Unary(Ok(_)) => panic!("expected a stream from {method}"),
    }
}

/// Start watching `workspace`'s root.
async fn a_watch_on(entry: &tddy_rpc::ServiceEntry, workspace: &Path) -> ProgressStream {
    progress_stream_at(
        entry,
        "WatchRoot",
        WatchRootRequest {
            workspace_root: root_of(workspace),
        },
    )
    .await
    .expect("a watch on the root opens")
}

/// Start warming `workspace`'s root.
async fn a_warm_of(entry: &tddy_rpc::ServiceEntry, workspace: &Path) -> ProgressStream {
    progress_stream_at(
        entry,
        "Warm",
        WarmRequest {
            workspace_root: root_of(workspace),
        },
    )
    .await
    .expect("a warm of the root opens")
}

/// Ask where the call to `foo` is defined.
async fn the_definition_of_the_call_to_foo(
    entry: &tddy_rpc::ServiceEntry,
    workspace: &Path,
) -> DefinitionResponse {
    unary_at(
        entry,
        "Definition",
        DefinitionRequest {
            workspace_root: root_of(workspace),
            file: "src/lib.rs".to_string(),
            position: Some(SourcePosition { line: 2, column: 5 }),
        },
    )
    .await
    .expect("a call inside the workspace is answered")
}

/// The roots this host holds an index for.
async fn the_workspaces_held_by(entry: &tddy_rpc::ServiceEntry) -> WorkspacesResponse {
    unary_at(entry, "Workspaces", WorkspacesRequest {})
        .await
        .expect("the host lists its workspaces")
}

/// One open `IndexProgress` stream, read message by message with a bound on each.
struct ProgressStream {
    method: String,
    receiver: mpsc::Receiver<Result<Vec<u8>, tddy_rpc::Status>>,
}

impl ProgressStream {
    /// The next message; panics if none arrives within [`A_PROGRESS_STEP`] or the stream ends.
    async fn next(&mut self) -> IndexProgress {
        let next = tokio::time::timeout(A_PROGRESS_STEP, self.receiver.recv())
            .await
            .unwrap_or_else(|_| panic!("{} sent nothing for {A_PROGRESS_STEP:?}", self.method));
        let payload = next
            .unwrap_or_else(|| panic!("{} ended while a message was awaited", self.method))
            .unwrap_or_else(|status| panic!("{} failed: {status:?}", self.method));
        IndexProgress::decode(payload.as_slice()).expect("a progress message decodes")
    }

    /// Every message up to and including the first that `reached` accepts.
    async fn until(
        &mut self,
        awaited: &str,
        reached: impl Fn(&IndexProgress) -> bool,
    ) -> Vec<IndexProgress> {
        let mut seen = Vec::new();
        loop {
            let next = tokio::time::timeout(A_PROGRESS_STEP, self.receiver.recv())
                .await
                .unwrap_or_else(|_| {
                    panic!(
                        "{} never reported {awaited} within {A_PROGRESS_STEP:?} of its last \
                         message; it reported: {seen:#?}",
                        self.method
                    )
                });
            let Some(payload) = next else {
                panic!(
                    "{} ended without reporting {awaited}; it reported: {seen:#?}",
                    self.method
                );
            };
            let payload = payload.unwrap_or_else(|status| {
                panic!(
                    "{} failed before reporting {awaited}: {status:?}; it reported: {seen:#?}",
                    self.method
                )
            });
            let progress =
                IndexProgress::decode(payload.as_slice()).expect("a progress message decodes");
            let done = reached(&progress);
            seen.push(progress);
            if done {
                return seen;
            }
        }
    }

    /// Every message until both loading phases are named.
    async fn until_both_phases_are_running(&mut self) -> Vec<IndexProgress> {
        self.until("both loading phases", |progress| {
            progress.phases == the_two_loading_phases()
        })
        .await
    }

    /// Every message until the root is reported failed.
    async fn until_failed(&mut self) -> Vec<IndexProgress> {
        self.until("the root failed", |progress| {
            progress.state == IndexState::Failed as i32
        })
        .await
    }

    /// Every message until the index is reported ready.
    async fn until_ready(&mut self) -> Vec<IndexProgress> {
        self.until("the index ready", |progress| {
            progress.state == IndexState::Ready as i32
        })
        .await
    }

    /// Every remaining message, until the stream ends.
    async fn until_it_ends(&mut self) -> Vec<IndexProgress> {
        let mut seen = Vec::new();
        loop {
            let next = tokio::time::timeout(A_PROGRESS_STEP, self.receiver.recv())
                .await
                .unwrap_or_else(|_| {
                    panic!(
                        "{} did not end within {A_PROGRESS_STEP:?} of its last message; it \
                         reported: {seen:#?}",
                        self.method
                    )
                });
            let Some(payload) = next else {
                return seen;
            };
            let payload = payload.unwrap_or_else(|status| {
                panic!("{} failed: {status:?}; it reported: {seen:#?}", self.method)
            });
            seen.push(
                IndexProgress::decode(payload.as_slice()).expect("a progress message decodes"),
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Reading progress

/// The state a wire value names, so a failure prints `Loading` rather than `2`.
fn state_named(wire: i32) -> IndexState {
    IndexState::try_from(wire).expect("a known index state")
}

/// The states `progress` passed through, each once, in the order it reached them.
fn the_states_passed_through(progress: &[IndexProgress]) -> Vec<IndexState> {
    let mut states: Vec<IndexState> = progress.iter().map(|p| state_named(p.state)).collect();
    states.dedup();
    states
}

/// What a progress message says about the index, in the terms a watcher reads it in.
#[derive(Debug, PartialEq)]
struct Reading {
    state: IndexState,
    phases: Vec<ActivePhase>,
    ready: bool,
}

fn reading_of(progress: &IndexProgress) -> Reading {
    Reading {
        state: state_named(progress.state),
        phases: progress.phases.clone(),
        ready: progress.ready,
    }
}

fn idle() -> Reading {
    Reading {
        state: IndexState::Idle,
        phases: Vec::new(),
        ready: false,
    }
}

fn loading_through_both_phases() -> Reading {
    Reading {
        state: IndexState::Loading,
        phases: the_two_loading_phases(),
        ready: false,
    }
}

fn ready_with_nothing_running() -> Reading {
    Reading {
        state: IndexState::Ready,
        phases: Vec::new(),
        ready: true,
    }
}

// ---------------------------------------------------------------------------------------------
// Watching a root

#[tokio::test(flavor = "multi_thread")]
async fn watching_a_root_no_server_holds_reports_idle_and_starts_none() {
    // Given a workspace root no language server holds
    let signal = a_graph_load_signal();
    let entry = a_host_whose_graph_loads_once(&signal);
    let workspace = a_workspace_calling_foo();

    // When the root is watched
    let mut watch = a_watch_on(&entry, workspace.path()).await;
    let opening = watch.next().await;

    // Then the watch opens by reporting the root idle
    assert_eq!(reading_of(&opening), idle());
    // and watching it started no server for it
    assert_eq!(
        the_workspaces_held_by(&entry).await,
        WorkspacesResponse { workspaces: vec![] }
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn watching_a_root_reports_the_load_a_navigation_request_started() {
    // Given a watch on a root no language server holds yet
    let signal = a_graph_load_signal();
    let entry = a_host_whose_graph_loads_once(&signal);
    let workspace = a_workspace_calling_foo();
    let mut watch = a_watch_on(&entry, workspace.path()).await;

    // When a navigation request starts the root's server, and its graph later finishes loading
    the_definition_of_the_call_to_foo(&entry, workspace.path()).await;
    let mut seen = watch.until_both_phases_are_running().await;
    signal.the_graph_finishes_loading();
    seen.extend(watch.until_ready().await);

    // Then the watch followed the root from idle, through the load, to ready
    assert_eq!(
        the_states_passed_through(&seen),
        vec![IndexState::Idle, IndexState::Loading, IndexState::Ready]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn watching_a_root_whose_server_cannot_start_reports_it_failed_with_the_reason() {
    // Given a watch on a root whose language server exits as soon as it is spawned
    let entry = a_host_whose_server_cannot_start();
    let workspace = a_workspace_calling_foo();
    let mut watch = a_watch_on(&entry, workspace.path()).await;

    // When a navigation request tries to start the root's server
    let refusal = unary_at::<_, DefinitionResponse>(
        &entry,
        "Definition",
        DefinitionRequest {
            workspace_root: root_of(workspace.path()),
            file: "src/lib.rs".to_string(),
            position: Some(SourcePosition { line: 2, column: 5 }),
        },
    )
    .await
    .expect_err("a root whose server cannot start cannot answer");
    let failed = watch
        .until_failed()
        .await
        .pop()
        .expect("the message reporting the failure");

    // Then the request is refused as unavailable
    assert_eq!(
        (refusal.code(), refusal.message()),
        (tddy_rpc::Code::Unavailable, A_SERVER_THAT_EXITS_AT_ONCE)
    );
    // and the watch reports the root failed, giving the refusal's reason
    assert_eq!(failed.error, A_SERVER_THAT_EXITS_AT_ONCE);
}

/// Why a root whose server exits as soon as it is spawned has no index.
const A_SERVER_THAT_EXITS_AT_ONCE: &str = "lsp server exited";

#[tokio::test(flavor = "multi_thread")]
async fn a_watch_that_joins_mid_load_names_every_phase_already_running() {
    // Given a root whose server is part-way through its load, with both phases running
    let signal = a_graph_load_signal();
    let entry = a_host_whose_graph_loads_once(&signal);
    let workspace = a_workspace_calling_foo();
    let mut first_watch = a_watch_on(&entry, workspace.path()).await;
    the_definition_of_the_call_to_foo(&entry, workspace.path()).await;
    first_watch.until_both_phases_are_running().await;

    // When a second watch joins
    let mut joining_watch = a_watch_on(&entry, workspace.path()).await;
    let opening = joining_watch.next().await;

    // Then its first message already names every running phase — title, latest message, percentage
    assert_eq!(reading_of(&opening), loading_through_both_phases());
}

// ---------------------------------------------------------------------------------------------
// Warming a root

#[tokio::test(flavor = "multi_thread")]
async fn a_warm_reports_ready_only_once_the_graph_is_loaded_and_carries_the_active_phases() {
    // Given a root no language server holds, whose graph loads only when signalled
    let signal = a_graph_load_signal();
    let entry = a_host_whose_graph_loads_once(&signal);
    let workspace = a_workspace_calling_foo();

    // When it is warmed, and its graph finishes loading once both phases are running
    let mut warm = a_warm_of(&entry, workspace.path()).await;
    let while_loading = warm.until_both_phases_are_running().await;
    signal.the_graph_finishes_loading();
    let after_loading = warm.until_it_ends().await;

    // Then the warm named both phases while the graph loaded, without claiming it ready
    assert_eq!(
        while_loading.last().map(reading_of),
        Some(loading_through_both_phases())
    );
    assert_eq!(
        while_loading
            .iter()
            .filter(|progress| progress.ready)
            .count(),
        0,
        "a warm claimed the index ready before its graph loaded: {while_loading:#?}"
    );
    // and it ended by reporting the index ready, with nothing left running
    assert_eq!(
        after_loading.last().map(reading_of),
        Some(ready_with_nothing_running())
    );
}

// ---------------------------------------------------------------------------------------------
// Navigation answers

#[tokio::test(flavor = "multi_thread")]
async fn a_definition_asked_while_the_graph_loads_says_loading() {
    // Given a root whose graph never finishes loading during this test
    let signal = a_graph_load_signal();
    let entry = a_host_whose_graph_loads_once(&signal);
    let workspace = a_workspace_calling_foo();

    // When a definition is asked
    let answer = the_definition_of_the_call_to_foo(&entry, workspace.path()).await;

    // Then the answer says the index was still loading, so it is not final
    assert_eq!(state_named(answer.state), IndexState::Loading);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_definition_asked_once_the_graph_is_loaded_says_ready() {
    // Given a root warmed until its graph is loaded
    let signal = a_graph_load_signal();
    signal.the_graph_finishes_loading();
    let entry = a_host_whose_graph_loads_once(&signal);
    let workspace = a_workspace_calling_foo();
    a_warm_of(&entry, workspace.path())
        .await
        .until_it_ends()
        .await;

    // When a definition is asked
    let answer = the_definition_of_the_call_to_foo(&entry, workspace.path()).await;

    // Then the answer says the index was ready
    assert_eq!(state_named(answer.state), IndexState::Ready);
}
