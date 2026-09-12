//! Unit tests: `DaemonSessionHost::specialized_subagent_env` — resolving
//! `StartSessionRequest.specialized_agents` names into the `TDDY_SUBAGENT`/
//! `TDDY_SUBAGENTS_JSON` jail env pair.
//!
//! Feature: docs/ft/coder/specialized-subagents.md (criteria 17-18)
//! Changeset: docs/dev/1-WIP/specialized-subagents.md
//!
//! The full sandboxed spawn (`start_sandboxed_claude_cli_session`) requires a real git
//! repo/project/platform sandbox (darwin Seatbelt / Linux cgroups) — see
//! `sandboxed_claude_cli_acceptance.rs` for that heavier end-to-end harness. This module
//! isolates the new, platform-independent resolution logic this changeset adds.

use super::*;
use tddy_daemon_kernel::{SessionUserResolver, SessionsBaseResolver};

fn make_unit_config() -> crate::config::DaemonConfig {
    let yaml = "users:\n  - github_user: \"u\"\n    os_user: \"u\"\n";
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).unwrap();
    crate::config::DaemonConfig::load(&path).unwrap()
}

fn make_unit_service(tddy_data_dir: std::path::PathBuf) -> DaemonSessionHost {
    let config = make_unit_config();
    let base = tddy_data_dir.clone();
    let sessions_base_resolver: SessionsBaseResolver = Arc::new(move |_| Some(base.clone()));
    let user_resolver: SessionUserResolver = Arc::new(|token| {
        if token == "valid" {
            Some("u".to_string())
        } else {
            None
        }
    });
    DaemonSessionHost::new(
        config,
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        None,
        None,
        Arc::new(CliSessionManager::new()),
    )
}

/// A def under `<tddyhome>/agents`, which is where every YAML-defined agent comes from.
fn an_agent_def(tddy_home: &std::path::Path, name: &str) {
    let agents = tddy_home.join("agents");
    std::fs::create_dir_all(&agents).expect("create agents dir");
    std::fs::write(
        agents.join(format!("{name}.yaml")),
        format!("name: {name}\nmodel: qwen2.5-coder:7b\nbase_url: http://localhost:11434\n"),
    )
    .expect("write agent def");
}

/// An empty `specialized_agents` list is never consulted by the caller (see the `if
/// !specialized_defs.is_empty()` guard in `start_sandboxed_claude_cli_session`) — this test
/// documents that `specialized_subagent_env` itself, when called directly with an empty def
/// list, still resolves cleanly (an empty env pair list), matching "no subagents requested =
/// no subagent env vars" rather than an error.
#[test]
fn specialized_subagent_env_with_no_defs_produces_no_env_pairs() {
    // Given
    let tddy_home = tempfile::tempdir().unwrap();
    let service = make_unit_service(tddy_home.path().to_path_buf());

    // When
    let result = service.specialized_subagent_env(&[]);

    // Then
    assert_eq!(
        result.unwrap(),
        Vec::<(String, String)>::new(),
        "an empty defs list must resolve to no env pairs, not an error"
    );
}

/// An empty `specialized_agents` name list resolves to an empty defs list, not an error.
#[tokio::test]
async fn resolve_specialized_agent_defs_with_no_names_produces_no_defs() {
    // Given
    let tddy_home = tempfile::tempdir().unwrap();
    let service = make_unit_service(tddy_home.path().to_path_buf());

    // When
    let result = service.resolve_specialized_agent_defs(&[]).await;

    // Then
    assert_eq!(
        result.unwrap(),
        Vec::<tddy_discovery::agent_def::SpecializedAgentDef>::new(),
        "an empty specialized_agents list must resolve to no defs, not an error"
    );
}

/// A `<tddyhome>/agents` def resolves by name. That directory is the only place a YAML-defined
/// agent can come from — nothing resolves out of the binary.
#[tokio::test]
async fn resolve_specialized_agent_defs_resolves_a_def_from_the_agents_dir() {
    // Given
    let tddy_home = tempfile::tempdir().unwrap();
    an_agent_def(tddy_home.path(), "explorer");
    let service = make_unit_service(tddy_home.path().to_path_buf());

    // When
    let result = service
        .resolve_specialized_agent_defs(&["explorer".to_string()])
        .await;

    // Then
    let defs = result.expect("a def under <tddyhome>/agents must resolve by name");
    assert_eq!(defs.len(), 1);
    assert_eq!(defs[0].name, "explorer");
}

/// A name that resolves against no def source must reject the whole request — no partial
/// resolution for the names that *did* resolve.
#[tokio::test]
async fn resolve_specialized_agent_defs_rejects_unknown_name() {
    // Given
    let tddy_home = tempfile::tempdir().unwrap();
    an_agent_def(tddy_home.path(), "explorer");
    let service = make_unit_service(tddy_home.path().to_path_buf());

    // When
    let result = service
        .resolve_specialized_agent_defs(&["explorer".to_string(), "ghost-agent".to_string()])
        .await;

    // Then
    let err = result.expect_err("an unresolvable name must reject the whole request");
    assert_eq!(err.code(), tddy_rpc::Code::InvalidArgument);
    assert!(
        err.message().contains("ghost-agent"),
        "the error must name the unresolvable subagent; got: {}",
        err.message()
    );
}

/// A resolved def produces both `TDDY_SUBAGENT` (comma names) and `TDDY_SUBAGENTS_JSON` (the
/// serialized def) — the exact env shape `tddy-tools --mcp` (see `subagents_from_env` in
/// `tddy-tools/src/server.rs`) expects.
#[tokio::test]
async fn specialized_subagent_env_builds_env_pairs_for_a_resolved_def() {
    // Given
    let tddy_home = tempfile::tempdir().unwrap();
    an_agent_def(tddy_home.path(), "explorer");
    let service = make_unit_service(tddy_home.path().to_path_buf());
    let defs = service
        .resolve_specialized_agent_defs(&["explorer".to_string()])
        .await
        .expect("a def under <tddyhome>/agents must resolve by name");

    // When
    let result = service.specialized_subagent_env(&defs);

    // Then
    let env = result.expect("a resolved def must build env pairs without error");
    let names = env
        .iter()
        .find(|(k, _)| k == "TDDY_SUBAGENT")
        .map(|(_, v)| v.clone());
    assert_eq!(names.as_deref(), Some("explorer"));
    let defs_json = env
        .iter()
        .find(|(k, _)| k == "TDDY_SUBAGENTS_JSON")
        .map(|(_, v)| v.clone())
        .expect("TDDY_SUBAGENTS_JSON must be present");
    assert!(
        defs_json.contains("explorer"),
        "TDDY_SUBAGENTS_JSON must serialize the resolved def; got: {defs_json}"
    );
}
