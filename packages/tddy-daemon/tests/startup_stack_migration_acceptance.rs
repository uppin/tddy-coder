//! The daemon drops every pre-queue project stack record while it starts (`#workqueue` 5/14): a
//! work queue now belongs to its session, and the old records are deleted, not migrated.
//!
//! `tddy-changeset`'s `session_work_queue_acceptance` proves what the drop does to a projects
//! directory. This suite proves the daemon runs it: `runtime::build`, over a data directory left by
//! an older daemon, must leave no `stacks/` directory before anything is served.

use std::path::Path;

use tddy_daemon::config::DaemonConfig;
use tddy_daemon::runtime::{self, RuntimeOptions};
use tokio::net::TcpListener;

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

#[tokio::test]
async fn building_the_daemon_drops_the_old_project_stack_records() {
    // Given — a data directory holding a project's old stack record and its journal, beside other
    // project data
    let data_dir = tempfile::tempdir().expect("a data directory");
    let project = data_dir.path().join("projects/proj-legacy");
    std::fs::create_dir_all(project.join("stacks")).unwrap();
    std::fs::write(project.join("stacks/0190.yaml"), "stack_id: '0190'\n").unwrap();
    std::fs::write(project.join("stacks/0190.op.json"), "{}").unwrap();
    std::fs::write(project.join("session-watches.json"), "{}").unwrap();

    // When — the daemon is built over it
    let _runtime = runtime::build(
        a_daemon_config_over(data_dir.path(), a_free_tcp_port().await),
        RuntimeOptions::for_binary(tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker),
    )
    .await
    .expect("the binary runtime did not build");

    // Then — the old records are gone and nothing else is
    assert!(
        !project.join("stacks").exists(),
        "the old records are dropped"
    );
    assert!(project.join("session-watches.json").exists());
}
