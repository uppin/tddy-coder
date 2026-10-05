//! What the daemon says while a run waits — for an index that never finishes loading, or for its
//! turn on a root another run holds.
//!
//! A run waits until the server is ready or until its caller stops it, and a stream that went
//! silent while a server did is the one thing the daemon could not tell from a stuck run. Its
//! progress sink is also its only way to learn a caller has hung up: a send that fails cancels the
//! run. So a heartbeat is two things at once, a line for the person waiting and the probe that
//! releases a root whose caller is gone.
//!
//! The warm index is `fake_lsp` in its `--never-quiescent` mode — a server that says it is busy on
//! a build script and never says anything else — and the cadence is shortened to 300 ms through
//! `CodeIndexPorts`, so nothing here waits thirty seconds. **No deadline is added:** the guard
//! lives in `tddy-code-restructuring`'s own suite and a run here still ends only when its caller
//! does.

use std::path::Path;
use std::time::{Duration, Instant};

use prost::Message;
use tddy_index_daemon::proto::code_index::{
    restructure_event, ApplyRequest, IndexProgress, RestructureEvent, WarmRequest,
};
use tddy_index_daemon::{build_code_index_entry, CodeIndexPorts};
use tddy_lsp::{Language, LaunchSpec, LspAllowList, LspRegistry};
use tddy_task::TaskRegistry;

/// The cadence every test here asks the daemon to beat at.
const THE_CADENCE: Duration = Duration::from_millis(300);

/// How long a test lets a stream go on for a line it needs before it gives up on one, so a daemon
/// that never says it fails by assertion instead of hanging the suite.
const LONG_ENOUGH_FOR_A_LINE_THE_TEST_NEEDS: Duration = Duration::from_secs(8);

/// Three cadences and room for a cancelled wait to unwind and hand a root on.
const THREE_BEATS_AND_AN_UNWIND: Duration = Duration::from_millis(3 * 300 + 1500);

type Events = tokio::sync::mpsc::Receiver<Result<Vec<u8>, tddy_rpc::Status>>;

/// A host whose warm servers are a fake that is busy for ever, beating every [`THE_CADENCE`].
fn a_host_over_a_server_that_never_becomes_ready() -> tddy_rpc::ServiceEntry {
    let mut spec = LaunchSpec::new(env!("CARGO_BIN_EXE_fake_lsp"))
        .with_capabilities(tddy_code_restructuring::client_capabilities())
        .with_initialization_options(tddy_code_restructuring::server_settings());
    spec.args = vec!["--never-quiescent".to_string()];
    let mut allow = LspAllowList::new();
    allow.allow(Language::Rust, spec);
    build_code_index_entry(CodeIndexPorts {
        servers: LspRegistry::new(allow, TaskRegistry::new(), Duration::from_secs(60)),
        wait_heartbeat: THE_CADENCE,
    })
}

/// A git worktree holding one source file and a plan that renames the function in it.
fn a_workspace_with_a_plan_renaming_foo() -> tempfile::TempDir {
    let workspace = tempfile::tempdir().expect("a temporary workspace");
    std::process::Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(workspace.path())
        .status()
        .expect("git init");
    std::fs::create_dir_all(workspace.path().join("src")).expect("src");
    std::fs::write(
        workspace.path().join("src/lib.rs"),
        "pub fn foo() -> u32 {\n    1\n}\n",
    )
    .expect("a source file");
    std::fs::write(
        workspace.path().join("plan.jsonl"),
        "{\"v\":1,\"snapshot\":{}}\n{\"op\":\"rename_symbol\",\"anchor\":{\"kind\":\"symbol\",\
         \"file\":\"src/lib.rs\",\"path\":\"foo\"},\"name\":\"bar\"}\n",
    )
    .expect("write the plan");
    workspace
}

fn an_apply_of_the_plan_in(workspace: &Path) -> ApplyRequest {
    ApplyRequest {
        workspace_root: workspace.to_string_lossy().to_string(),
        plan: workspace.join("plan.jsonl").to_string_lossy().to_string(),
        dry_run: false,
        resume: false,
        from: None,
        stop_after: None,
    }
}

/// Open a server-streaming request at the registered coordinate and hand back the stream undrained:
/// every run here ends only when somebody hangs up, so draining it would never return.
async fn open_stream_at<Req: Message>(
    entry: &tddy_rpc::ServiceEntry,
    method: &str,
    request: Req,
) -> Events {
    let message = tddy_rpc::RpcMessage::new(
        request.encode_to_vec(),
        tddy_rpc::RequestMetadata::over(tddy_rpc::RequestTransport::Direct),
    );
    match entry.service.handle_rpc(entry.name, method, &message).await {
        tddy_rpc::RpcResult::ServerStream(Ok(events)) => events,
        _ => panic!("expected {method} to open a stream"),
    }
}

/// The next item on `events` within `within`, or `None` when the stream ended or none came.
async fn next_item(events: &mut Events, within: Duration) -> Option<Vec<u8>> {
    tokio::time::timeout(within, events.recv())
        .await
        .ok()
        .flatten()
        .map(|item| item.expect("the stream carries items, not a refusal"))
}

/// The progress lines of an apply's stream, in order, up to and including the first that contains
/// `needle` — or up to the time a test will wait for one.
async fn indexing_lines_until_one_says(events: &mut Events, needle: &str) -> Vec<String> {
    let deadline = Instant::now() + LONG_ENOUGH_FOR_A_LINE_THE_TEST_NEEDS;
    let mut lines = Vec::new();
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let Some(item) = next_item(events, remaining).await else {
            break;
        };
        let event = RestructureEvent::decode(item.as_slice()).expect("an event decodes");
        let Some(restructure_event::Event::Indexing(IndexProgress { line, .. })) = event.event
        else {
            continue;
        };
        let found = line.contains(needle);
        lines.push(line);
        if found {
            break;
        }
    }
    lines
}

/// Read `events` for `within` and keep every message, decoded as the progress a warm sends.
async fn warm_messages_within(events: &mut Events, within: Duration) -> Vec<IndexProgress> {
    let deadline = Instant::now() + within;
    let mut messages = Vec::new();
    while let Some(item) =
        next_item(events, deadline.saturating_duration_since(Instant::now())).await
    {
        messages.push(IndexProgress::decode(item.as_slice()).expect("progress decodes"));
    }
    messages
}

/// Read a stream in the background for as long as the test lives, so that a run writing to it is
/// never held up by a client that stopped reading — which would hold its root without being a wait.
fn kept_listening_to(mut events: Events) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move { while events.recv().await.is_some() {} })
}

#[tokio::test(flavor = "multi_thread")]
async fn a_busy_servers_beats_arrive_on_the_stream_as_indexing_events() {
    // Given an apply of a one-operation plan over a server that never becomes ready
    let workspace = a_workspace_with_a_plan_renaming_foo();
    let entry = a_host_over_a_server_that_never_becomes_ready();

    // When the stream is read until the run says it is still waiting
    let mut events =
        open_stream_at(&entry, "Apply", an_apply_of_the_plan_in(workspace.path())).await;
    let lines = indexing_lines_until_one_says(&mut events, "still waiting").await;

    // Then an `Indexing` event began `still waiting` and named the stage
    let beat = lines.last().expect("at least one indexing line");
    assert!(
        beat.starts_with("still waiting") && beat.contains("warming the crate index"),
        "no beat among {lines:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_caller_that_hangs_up_during_a_silent_wait_releases_the_root_within_one_beat() {
    // Given a first apply waiting on a server that never becomes ready, which has beaten once
    let workspace = a_workspace_with_a_plan_renaming_foo();
    let entry = a_host_over_a_server_that_never_becomes_ready();
    let mut first =
        open_stream_at(&entry, "Apply", an_apply_of_the_plan_in(workspace.path())).await;
    indexing_lines_until_one_says(&mut first, "still waiting").await;

    // When its caller hangs up, and a second apply asks for the same root
    drop(first);
    let mut second =
        open_stream_at(&entry, "Apply", an_apply_of_the_plan_in(workspace.path())).await;
    let started = tokio::time::timeout(
        THREE_BEATS_AND_AN_UNWIND,
        indexing_lines_until_one_says(&mut second, "starting rust-analyzer session"),
    )
    .await
    .expect("the second apply is still queued: the first was never released");

    // Then the second reached its own wait, instead of queueing behind a caller who had gone
    assert_eq!(
        started.last().map(String::as_str),
        Some("starting rust-analyzer session"),
        "the second apply said {started:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_queued_behind_another_on_its_root_says_so_and_for_how_long() {
    // Given a first apply holding the root, its caller still listening
    let workspace = a_workspace_with_a_plan_renaming_foo();
    let entry = a_host_over_a_server_that_never_becomes_ready();
    let mut first =
        open_stream_at(&entry, "Apply", an_apply_of_the_plan_in(workspace.path())).await;
    indexing_lines_until_one_says(&mut first, "still waiting").await;
    let _listening = kept_listening_to(first);

    // When a second apply asks for the same root
    let mut second =
        open_stream_at(&entry, "Apply", an_apply_of_the_plan_in(workspace.path())).await;
    let lines = indexing_lines_until_one_says(&mut second, "queued behind another operation").await;

    // Then its stream says it is queued, with how long it has been
    assert!(
        lines.iter().any(|line| line.starts_with("still waiting (")
            && line.contains("queued behind another operation")),
        "the second apply never said it was queued: {lines:?}"
    );
}

/// A guard, green before and after: a failure or a hang-up must never leave a root looking ready.
/// It is observable only through the stream — the root's latch is crate-private and a backend's
/// `indexed` dies with its request.
#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_wait_leaves_the_root_not_ready_and_the_next_warm_still_waits_for_the_graph() {
    // Given a first apply that its caller hung up on after waiting on a server that never loads
    let workspace = a_workspace_with_a_plan_renaming_foo();
    let entry = a_host_over_a_server_that_never_becomes_ready();
    let mut first =
        open_stream_at(&entry, "Apply", an_apply_of_the_plan_in(workspace.path())).await;
    indexing_lines_until_one_says(&mut first, "still waiting").await;
    drop(first);

    // When the root is warmed, and the stream is read for three cadences
    let mut warm = open_stream_at(
        &entry,
        "Warm",
        WarmRequest {
            workspace_root: workspace.path().to_string_lossy().to_string(),
        },
    )
    .await;
    let messages = warm_messages_within(&mut warm, 3 * THE_CADENCE).await;

    // Then no message claims the index is ready
    assert!(
        messages.iter().all(|message| !message.ready),
        "a root whose server never became quiescent was reported ready: {messages:?}"
    );
}
