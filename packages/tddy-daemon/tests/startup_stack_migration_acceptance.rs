//! The daemon moves every session-held PR stack into its project while it starts
//! (`#orchestrators` 2/11).
//!
//! `tddy-pr-stack`'s `stack_migration_sweep_acceptance` proves what the sweep does to a sessions
//! directory. This suite proves the daemon runs it: `runtime::build`, over a data directory left by
//! an older daemon, must leave that directory migrated before anything is served.

use std::path::Path;

use tddy_core::changeset::{read_changeset, write_changeset, Changeset, Stack, StackNode};
use tddy_core::project_stack_store::ProjectStackStore;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_core::session_metadata::{
    write_initial_tool_session_metadata, InitialToolSessionMetadataOpts,
};
use tddy_daemon::config::DaemonConfig;
use tddy_daemon::runtime::{self, RuntimeOptions};
use tokio::net::TcpListener;

const A_PROJECT: &str = "proj-legacy";
const A_LEGACY_ORCHESTRATOR: &str = "orch-legacy";

async fn a_free_tcp_port() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("no loopback port was available");
    listener
        .local_addr()
        .expect("the bound listener has no address")
        .port()
}

/// GitHub auth is stubbed: the session services — and the session host whose build runs the
/// sweep — are mounted only when the runtime has a user resolver.
fn a_daemon_config_over(data_dir: &Path, web_port: u16) -> DaemonConfig {
    let yaml = format!(
        r#"
listen:
  web_port: {web_port}
  web_host: 127.0.0.1
tddy_data_dir: {data_dir}
github:
  stub: true
"#,
        data_dir = data_dir.display(),
    );
    serde_yaml::from_str(&yaml).expect("the config fixture did not parse")
}

fn a_two_node_stack() -> Stack {
    Stack {
        version: 1,
        nodes: vec![
            StackNode {
                node_id: "n1".to_string(),
                title: "Add token store".to_string(),
                ..StackNode::default()
            },
            StackNode {
                node_id: "n2".to_string(),
                title: "Rotate tokens".to_string(),
                parents: vec!["n1".to_string()],
                ..StackNode::default()
            },
        ],
    }
}

/// A pr-stack session as an older daemon left it: the stack inside its `changeset.yaml`.
fn a_legacy_orchestrator_in(data_dir: &Path, stack: Stack) -> std::path::PathBuf {
    let dir = unified_session_dir_path(data_dir, A_LEGACY_ORCHESTRATOR);
    std::fs::create_dir_all(&dir).expect("session dir");
    write_initial_tool_session_metadata(
        &dir,
        InitialToolSessionMetadataOpts {
            project_id: A_PROJECT.to_string(),
            ..InitialToolSessionMetadataOpts::default()
        },
    )
    .expect("write .session.yaml");
    write_changeset(
        &dir,
        &Changeset {
            recipe: Some("pr-stack".to_string()),
            stack: Some(stack),
            ..Changeset::default()
        },
    )
    .expect("write the legacy changeset.yaml");
    dir
}

#[tokio::test]
async fn building_the_daemon_moves_a_session_held_stack_into_its_project() {
    // Given — a data directory holding one pr-stack session with its stack in its changeset
    let data_dir = tempfile::tempdir().expect("a data directory");
    let session_dir = a_legacy_orchestrator_in(data_dir.path(), a_two_node_stack());

    // When — the daemon is built over it
    let _runtime = runtime::build(
        a_daemon_config_over(data_dir.path(), a_free_tcp_port().await),
        RuntimeOptions::for_binary(tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker),
    )
    .await
    .expect("the binary runtime did not build");

    // Then — the changeset names a project record holding that stack, and carries no copy
    let migrated = read_changeset(&session_dir).expect("read the migrated changeset");
    assert_eq!(migrated.stack, None, "the legacy copy must be dropped");
    let stack_id = migrated
        .stack_id
        .expect("the changeset must name its project stack");
    let record = ProjectStackStore::new(data_dir.path().join("projects"))
        .read(A_PROJECT, &stack_id)
        .expect("the project must hold the named stack");
    assert_eq!(
        (record.created_by_session_id.as_str(), record.stack),
        (A_LEGACY_ORCHESTRATOR, a_two_node_stack())
    );
}
