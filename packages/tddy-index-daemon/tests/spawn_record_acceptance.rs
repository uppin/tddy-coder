//! The daemon leaves a record of every language server it starts.
//!
//! The daemon starts rust-analyzer on demand and is itself started by a script, so when it dies
//! nothing says what it had started. `--spawn-record <path>` names an append-only file the daemon
//! writes one JSON object per line to. The warm index here is `fake_lsp`, as everywhere in this
//! crate's suites: a real rust-analyzer costs minutes and is a production test.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use prost::Message;
use serde_json::Value;
use tddy_code_restructuring::spawn_record::JsonlSpawnRecord;
use tddy_index_daemon::proto::code_index::{IndexProgress, WarmRequest};
use tddy_index_daemon::{build_code_index_entry, CodeIndexPorts};
use tddy_lsp::{Language, LaunchSpec, LspAllowList, LspRegistry};
use tddy_task::TaskRegistry;

/// A host whose warm servers are the fake, reporting every one it starts to `record`.
fn a_host_recording_to(record: &Path) -> tddy_rpc::ServiceEntry {
    let mut spec = LaunchSpec::new(env!("CARGO_BIN_EXE_fake_lsp"))
        .with_capabilities(tddy_code_restructuring::client_capabilities())
        .with_initialization_options(tddy_code_restructuring::server_settings());
    spec.args = vec!["--loads-crate-graph".to_string()];
    let mut allow = LspAllowList::new();
    allow.allow(Language::Rust, spec);
    let sink = JsonlSpawnRecord::open(record).expect("the record file opens for appending");
    build_code_index_entry(CodeIndexPorts {
        servers: LspRegistry::new(allow, TaskRegistry::new(), Duration::from_secs(60))
            .with_spawn_observer(Arc::new(sink)),
        wait_heartbeat: tddy_code_restructuring::backends::rust::WAIT_HEARTBEAT,
    })
}

fn a_message_carrying<Req: Message>(request: Req) -> tddy_rpc::RpcMessage {
    tddy_rpc::RpcMessage::new(
        request.encode_to_vec(),
        tddy_rpc::RequestMetadata::over(tddy_rpc::RequestTransport::Direct),
    )
}

/// Warm `root` through the registered coordinate and drain the progress stream.
async fn warming(entry: &tddy_rpc::ServiceEntry, root: &Path) -> Vec<IndexProgress> {
    let message = a_message_carrying(WarmRequest {
        workspace_root: root.to_string_lossy().to_string(),
    });
    let tddy_rpc::RpcResult::ServerStream(Ok(mut receiver)) =
        entry.service.handle_rpc(entry.name, "Warm", &message).await
    else {
        panic!("a warm answers with a stream");
    };
    let mut progress = Vec::new();
    while let Some(next) = receiver.recv().await {
        let payload = next.expect("a progress message");
        progress.push(IndexProgress::decode(payload.as_slice()).expect("progress decodes"));
    }
    progress
}

fn the_lines_of(record: &Path) -> Vec<Value> {
    std::fs::read_to_string(record)
        .expect("the record file")
        .lines()
        .map(|line| serde_json::from_str(line).expect("a line of the record is JSON"))
        .collect()
}

fn the_start_lines_of(record: &Path) -> Vec<Value> {
    the_lines_of(record)
        .into_iter()
        .filter(|line| line["event"] == "start")
        .collect()
}

fn a_workspace_root() -> tempfile::TempDir {
    let workspace = tempfile::tempdir().expect("a temporary workspace");
    std::fs::create_dir_all(workspace.path().join("src")).expect("src");
    std::fs::write(workspace.path().join("src/lib.rs"), "pub fn foo() {}\n").expect("a source");
    workspace
}

#[tokio::test(flavor = "multi_thread")]
async fn a_served_warm_appends_the_language_server_it_started_to_the_record_file() {
    // Given a service wired with a spawn record on a temporary file
    let home = tempfile::tempdir().expect("a temporary home");
    let record = home.path().join("spawns.jsonl");
    let entry = a_host_recording_to(&record);
    let workspace = a_workspace_root();

    // When a root is warmed, which starts the language server
    warming(&entry, workspace.path()).await;

    // Then the record holds one start for the fake server, written by the language-server layer
    let starts = the_start_lines_of(&record);
    assert_eq!(starts.len(), 1, "expected one start, found {starts:?}");
    assert_eq!(starts[0]["origin"], "lsp");
    assert_eq!(starts[0]["program"], env!("CARGO_BIN_EXE_fake_lsp"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_restarted_daemon_appends_after_the_previous_runs_lines() {
    // Given a record file one daemon wrote while warming a root
    let home = tempfile::tempdir().expect("a temporary home");
    let record = home.path().join("spawns.jsonl");
    let workspace = a_workspace_root();
    warming(&a_host_recording_to(&record), workspace.path()).await;
    let first_runs_bytes = std::fs::read(&record).expect("the record file");
    assert!(
        !first_runs_bytes.is_empty(),
        "the first daemon wrote nothing"
    );

    // When a second daemon, over the same file, warms a root
    warming(&a_host_recording_to(&record), workspace.path()).await;

    // Then the first daemon's lines are first, byte for byte, and a second start follows
    let both = std::fs::read(&record).expect("the record file");
    assert!(both.starts_with(&first_runs_bytes));
    assert_eq!(the_start_lines_of(&record).len(), 2);
}

/// The binary under test, resolved the way this repo resolves a sibling binary.
fn the_index_daemon() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_tddy-index-daemon"))
}

#[test]
fn the_spawn_record_flag_names_the_file_the_daemon_writes() {
    // Given the daemon's own help text
    let help = Command::new(the_index_daemon())
        .arg("--help")
        .output()
        .expect("run the help");

    // Then it documents the flag
    assert!(
        String::from_utf8_lossy(&help.stdout).contains("--spawn-record"),
        "--help does not document --spawn-record"
    );

    // And when a single-shot run is given the flag, it creates the file
    let home = tempfile::tempdir().expect("a temporary home");
    let record = home.path().join("spawns.jsonl");
    let workspace = a_workspace_root();
    let run = Command::new(the_index_daemon())
        .arg("--spawn-record")
        .arg(&record)
        .args(["restructure", "plans", "--workspace-root"])
        .arg(workspace.path())
        .stdin(Stdio::null())
        .output()
        .expect("run the daemon once");
    assert!(
        run.status.success(),
        "the single-shot run failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(record.exists(), "the run did not create the record file");
}
