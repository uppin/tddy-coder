//! The claim the whole change exists for, against a real rust-analyzer.
//!
//! Every other suite in this crate runs against `fake_lsp`, which is what keeps them deterministic
//! and measured in milliseconds. But a fake cannot demonstrate the one thing this daemon is for: a
//! *second* request against a root whose crate graph is already loaded does not load it again. That
//! needs a real server, a real toolchain and real indexing, which is minutes — so by
//! [`docs/dev/guides/testing.md`] § Production Tests these are `#[ignore]`d and excluded from CI.
//!
//! Run them deliberately:
//!
//! ```bash
//! ./dev cargo test -p tddy-index-daemon --test warm_index_production -- --ignored --test-threads=1
//! ```
//!
//! `--test-threads=1` is not optional: two rust-analyzers indexing at once on one machine is how a
//! generous wait still expires, and it is the same rule `tddy-code-restructuring`'s live suites
//! enforce with a mutex.

use std::path::Path;
use std::time::{Duration, Instant};

use prost::Message;
use tddy_index_daemon::proto::code_index::{
    AnchorsRequest, AnchorsResponse, IndexProgress, WarmRequest, WorkspacesRequest,
    WorkspacesResponse,
};
use tddy_index_daemon::{build_code_index_entry, CodeIndexPorts};
use tddy_lsp::{Language, LaunchSpec, LspAllowList, LspRegistry};
use tddy_task::TaskRegistry;

/// How much faster a warm request must be than the cold one that preceded it.
///
/// A **ratio**, not an absolute budget, and the first draft of this test got that wrong: an
/// absolute three seconds passed trivially, because a cold load of this small graph is ~2.1s and so
/// would have passed it too. A ratio works here precisely because the test constructs the cold run
/// itself, so "the first one was slow" is guaranteed rather than hoped for — and it is
/// machine-independent, which an absolute budget never is.
///
/// Measured on this workspace: 2.15s cold, then 865µs warm (≈2,500×). After a *second* root's graph
/// loads in between, the first root's re-ask is 880ms against 2.10s cold — still comfortably past
/// this bar, and the gap is smaller because each request builds a fresh `RustBackend` whose
/// `indexed` flag starts false, so the readiness probe is re-paid even against a warm server. Two is
/// therefore a floor with real headroom, not a tight fit.
const A_WARM_REQUEST_SHOULD_BE_IMMEDIATE_FACTOR: u32 = 2;

/// A host whose language servers are real rust-analyzer, launched with this crate's own handshake —
/// a handshake that drifted from production's would be exercising a different client.
fn a_host_over_real_rust_analyzers() -> tddy_rpc::ServiceEntry {
    let mut allow = LspAllowList::new();
    allow.allow(
        Language::Rust,
        LaunchSpec::new("rust-analyzer")
            .with_capabilities(tddy_code_restructuring::client_capabilities())
            .with_initialization_options(tddy_code_restructuring::server_settings()),
    );
    build_code_index_entry(CodeIndexPorts {
        servers: LspRegistry::new(allow, TaskRegistry::new(), Duration::from_secs(900)),
    })
}

/// A real cargo workspace on disk. Small on purpose: the assertion is about the *second* request, so
/// paying for a large graph twice would only make the suite slower without making it stricter.
fn a_cargo_workspace() -> tempfile::TempDir {
    let workspace = tempfile::tempdir().expect("a temporary workspace");
    let root = workspace.path();
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nresolver = \"2\"\nmembers = [\"crates/subject\"]\n",
    )
    .expect("a workspace manifest");
    std::fs::create_dir_all(root.join("crates/subject/src")).expect("the crate directory");
    std::fs::write(
        root.join("crates/subject/Cargo.toml"),
        "[package]\nname = \"subject\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .expect("a crate manifest");
    std::fs::write(
        root.join("crates/subject/src/lib.rs"),
        "pub fn scored(value: u32) -> u32 {\n    if value > 1 {\n        value * 2\n    } else {\n        value\n    }\n}\n",
    )
    .expect("a source file");
    workspace
}

/// Ask for an anchor through the registered coordinate, returning how long it took.
///
/// **This, not `Warm`, is what demonstrates index reuse.** `Warm` reports ready as soon as
/// `get_or_spawn` hands back a live server — it does not wait for the crate graph, which is the
/// weakness recorded against `Warm.ready` in this change's `## Technical Debt`. An anchor is a
/// different matter: it goes through `RustBackend::ensure_indexed`, which polls until the server can
/// actually answer, so the first one on a root pays the graph load and a later one does not.
async fn anchoring(entry: &tddy_rpc::ServiceEntry, root: &Path) -> Duration {
    let request = AnchorsRequest {
        workspace_root: root.to_string_lossy().to_string(),
        file: "crates/subject/src/lib.rs".to_string(),
        items: vec!["scored".to_string()],
    };
    let message = tddy_rpc::RpcMessage::new(
        request.encode_to_vec(),
        tddy_rpc::RequestMetadata::default(),
    );
    let started = Instant::now();
    let answer = entry
        .service
        .handle_rpc(entry.name, "Anchors", &message)
        .await;
    let payload = match answer {
        tddy_rpc::RpcResult::Unary(Ok(payload)) => payload,
        tddy_rpc::RpcResult::Unary(Err(status)) => {
            panic!("Anchors was refused for an item the file declares: {status:?}")
        }
        tddy_rpc::RpcResult::ServerStream(_) => panic!("Anchors must answer unary"),
    };
    let anchored = AnchorsResponse::decode(payload.as_slice()).expect("it decodes");
    assert!(
        anchored.range.is_some(),
        "an anchor request that succeeded must carry a range"
    );
    started.elapsed()
}

/// Warm `root` through the registered coordinate, returning how long it took and every progress
/// message it streamed.
async fn warming(entry: &tddy_rpc::ServiceEntry, root: &Path) -> (Duration, Vec<IndexProgress>) {
    let request = WarmRequest {
        workspace_root: root.to_string_lossy().to_string(),
    };
    let message = tddy_rpc::RpcMessage::new(
        request.encode_to_vec(),
        tddy_rpc::RequestMetadata::default(),
    );
    let started = Instant::now();
    let answer = entry.service.handle_rpc(entry.name, "Warm", &message).await;
    let tddy_rpc::RpcResult::ServerStream(Ok(mut receiver)) = answer else {
        panic!("Warm must answer with a stream");
    };
    let mut progress = Vec::new();
    while let Some(next) = receiver.recv().await {
        let payload = next.expect("a progress message rather than a refusal");
        progress.push(IndexProgress::decode(payload.as_slice()).expect("it decodes"));
    }
    (started.elapsed(), progress)
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "boots a real rust-analyzer and loads a real crate graph — minutes, and a toolchain"]
async fn a_second_request_against_a_warm_root_does_not_load_the_crate_graph_again() {
    // Given a workspace whose crate graph this process has already loaded, by answering a request
    // that could not have been answered without it
    let workspace = a_cargo_workspace();
    let entry = a_host_over_real_rust_analyzers();
    let cold = anchoring(&entry, workspace.path()).await;

    // When a second request needing the same graph arrives
    let warm = anchoring(&entry, workspace.path()).await;

    // Then it is answered from the index already held, rather than loading it a second time
    assert!(
        warm * A_WARM_REQUEST_SHOULD_BE_IMMEDIATE_FACTOR < cold,
        "the second request took {warm:?} against the first's {cold:?}, so it re-loaded the graph \
         rather than reusing it"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "boots a real rust-analyzer and loads a real crate graph — minutes, and a toolchain"]
async fn warming_a_root_reports_its_index_ready() {
    // Given a workspace this process holds no index for
    let workspace = a_cargo_workspace();
    let entry = a_host_over_real_rust_analyzers();

    // When it is warmed
    let (_, progress) = warming(&entry, workspace.path()).await;

    // Then the stream ends by reporting the index ready. Note this says a live server holds the
    // root, not that its graph is loaded — see `Warm.ready` in this change's `## Technical Debt`.
    // The reuse claim is pinned by the anchor test above, which needs the graph to answer at all.
    assert!(
        progress.last().expect("progress").ready,
        "a completed warm must end by reporting the index ready"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "boots two real rust-analyzers — minutes, and a toolchain"]
async fn two_roots_in_one_process_each_answer_from_their_own_index() {
    // Given two separate cargo workspaces and one host
    let one = a_cargo_workspace();
    let other = a_cargo_workspace();
    let entry = a_host_over_real_rust_analyzers();

    // When each is asked a question only its own loaded graph can answer, and then the first is
    // asked again
    anchoring(&entry, one.path()).await;
    anchoring(&entry, other.path()).await;
    anchoring(&entry, one.path()).await;

    // Then both roots are still held, each with its own index — `anchoring` asserts a range came
    // back, so a root whose graph had been evicted would have failed above rather than here.
    //
    // Deliberately **not** a timing assertion. Measured on this machine, re-asking the first root
    // after the second's graph loaded ranged from 880ms to 3.6s — the upper end *exceeding* its own
    // 2.19s cold load. Two resident rust-analyzers contend for CPU, and a ratio cannot tell "the
    // index was evicted" from "the machine was busy", so a timing claim here would be measuring the
    // host rather than the feature. Index reuse itself is pinned by the test above, whose margin is
    // ~2,500× on one root and leaves no room for that ambiguity.
    let held: WorkspacesResponse = {
        let message = tddy_rpc::RpcMessage::new(
            WorkspacesRequest {}.encode_to_vec(),
            tddy_rpc::RequestMetadata::default(),
        );
        let answer = entry
            .service
            .handle_rpc(entry.name, "Workspaces", &message)
            .await;
        let tddy_rpc::RpcResult::Unary(Ok(payload)) = answer else {
            panic!("Workspaces must answer unary");
        };
        WorkspacesResponse::decode(payload.as_slice()).expect("it decodes")
    };
    let mut roots: Vec<String> = held
        .workspaces
        .iter()
        .map(|workspace| workspace.workspace_root.clone())
        .collect();
    roots.sort();
    let mut expected = vec![
        one.path().to_string_lossy().to_string(),
        other.path().to_string_lossy().to_string(),
    ];
    expected.sort();
    assert_eq!(roots, expected);
}
