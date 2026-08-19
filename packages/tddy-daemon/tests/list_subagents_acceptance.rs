//! Acceptance tests: `ListSubagents` RPC — resolved specialized-agent defs
//! (`<tddyhome>/agents/*.yaml` and registry assistants) exposed to the web new-session picker.
//!
//! Feature: docs/ft/coder/specialized-subagents.md (criterion 16)
//! Changeset: docs/dev/1-WIP/specialized-subagents.md

use std::path::PathBuf;
use std::sync::Arc;

use tddy_daemon::config::DaemonConfig;
use tddy_daemon::connection_service::ConnectionServiceImpl;
use tddy_rpc::Request;
use tddy_service::proto::connection::{
    ConnectionService as ConnectionServiceTrait, ListSubagentsRequest,
};

type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;
type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

fn write_config(yaml: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("daemon.yaml");
    std::fs::write(&path, yaml).unwrap();
    (dir, path)
}

/// `tddy_data_dir` doubles as `<tddyhome>` for `ListSubagents` — mirrors
/// `list_agents_allowlist_acceptance.rs::service_with_config`.
fn service_with_config(config: DaemonConfig, tddy_data_dir: PathBuf) -> ConnectionServiceImpl {
    let sessions_base = tddy_data_dir.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: UserResolver = Arc::new(|token| {
        if token == "valid-token" {
            Some("testuser".to_string())
        } else {
            None
        }
    });
    ConnectionServiceImpl::new(
        config,
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        None,
        None,
        Arc::new(tddy_daemon::claude_cli_session::ClaudeCliSessionManager::new()),
    )
}

fn minimal_daemon_config() -> DaemonConfig {
    let yaml = r#"
users:
  - github_user: "u"
    os_user: "u"
"#;
    let (_dir, path) = write_config(yaml);
    DaemonConfig::load(&path).unwrap()
}

/// With no `<tddyhome>/agents` directory at all (a fresh install), `ListSubagents` succeeds and
/// offers nothing. A host with no def sources has no agents to offer — there is no builtin to
/// surface, and a fresh install is not an error case.
#[tokio::test]
async fn list_subagents_offers_nothing_with_no_user_agents_dir() {
    // Given — a fresh tddy_data_dir with no agents/ subdirectory at all
    let tddy_home = tempfile::tempdir().unwrap();
    let service = service_with_config(minimal_daemon_config(), tddy_home.path().to_path_buf());

    // When
    let response = service
        .list_subagents(Request::new(ListSubagentsRequest {}))
        .await
        .expect("ListSubagents must succeed even with no <tddyhome>/agents directory");

    // Then
    let subagents = response.into_inner().subagents;
    assert!(
        subagents.is_empty(),
        "a host with no def sources must offer no subagents; got: {subagents:?}"
    );
}

/// A YAML file written into `<tddyhome>/agents/` must appear in the `ListSubagents` response,
/// carrying its own `label`/`model` — proving the RPC actually reads the user's agents directory.
#[tokio::test]
async fn list_subagents_includes_a_def_written_to_tddyhome_agents_dir() {
    // Given — a <tddyhome>/agents/my-explorer.yaml def
    let tddy_home = tempfile::tempdir().unwrap();
    let agents_dir = tddy_home.path().join("agents");
    std::fs::create_dir_all(&agents_dir).unwrap();
    std::fs::write(
        agents_dir.join("my-explorer.yaml"),
        "name: my-explorer\nlabel: \"My Explorer\"\nmodel: qwen2.5-coder:7b\nbase_url: http://localhost:11434\n",
    )
    .unwrap();
    let service = service_with_config(minimal_daemon_config(), tddy_home.path().to_path_buf());

    // When
    let response = service
        .list_subagents(Request::new(ListSubagentsRequest {}))
        .await
        .expect("ListSubagents must succeed");

    // Then
    let subagents = response.into_inner().subagents;
    let my_explorer = subagents
        .iter()
        .find(|s| s.name == "my-explorer")
        .unwrap_or_else(|| panic!("expected a 'my-explorer' row; got: {subagents:?}"));
    assert_eq!(my_explorer.label, "My Explorer");
    assert_eq!(my_explorer.model, "qwen2.5-coder:7b");
}
