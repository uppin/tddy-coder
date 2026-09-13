//! Unit tests: `ConnectionServiceImpl::seeded_roster_records` — what a session's
//! `specialized_agents` seed resolves to, before anything is started for it.
//!
//! Feature: docs/ft/daemon/session-agent-roster.md § Seeding at start, § Remote agents.
//!
//! A seed resolves to the same thing an attach does — roster records, not defs — because an
//! agent is placeable on any host and a def can only describe one. The record is what carries
//! the placement (`daemon_instance_id`) and the withdrawal (`replaces`) into the spawn, so a
//! reference that cannot be turned into one has to fail the start rather than be dropped.

use super::*;
use tddy_daemon_kernel::{SessionUserResolver, SessionsBaseResolver};

/// A daemon with no peers: the whole common room is this host, so a reference naming any other
/// daemon is one nothing here can resolve.
fn a_daemon_with_no_peers(tddy_data_dir: std::path::PathBuf) -> ConnectionServiceImpl {
    let yaml = "users:\n  - github_user: \"u\"\n    os_user: \"u\"\n";
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).unwrap();
    let config = crate::config::DaemonConfig::load(&path).unwrap();
    let base = tddy_data_dir.clone();
    let sessions_base_resolver: SessionsBaseResolver = Arc::new(move |_| Some(base.clone()));
    let user_resolver: SessionUserResolver = Arc::new(|_| Some("u".to_string()));
    ConnectionServiceImpl::new(
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

/// An agent def under `<tddyhome>/agents` that takes `Grep` away from the main agent.
fn an_agent_def_replacing_grep(tddy_data_dir: &std::path::Path, name: &str) {
    let agents = tddy_data_dir.join("agents");
    std::fs::create_dir_all(&agents).expect("create agents dir");
    std::fs::write(
        agents.join(format!("{name}.yaml")),
        format!(
            "name: {name}\nmodel: qwen2.5-coder:7b\nbase_url: http://127.0.0.1:11434/v1\nreplaces:\n  - Grep\n"
        ),
    )
    .expect("write agent def");
}

#[tokio::test]
async fn an_empty_seed_resolves_to_an_empty_roster() {
    // Given a daemon asked to start a session with no agents
    let tddy_home = tempfile::tempdir().unwrap();
    let service = a_daemon_with_no_peers(tddy_home.path().to_path_buf());

    // When
    let records = service.seeded_roster_records(&[]).await;

    // Then — an empty seed is the ordinary case, not a request error
    assert_eq!(
        records.expect("an empty seed must resolve, not fail"),
        Vec::<tddy_core::SessionAgentRecord>::new()
    );
}

#[tokio::test]
async fn a_bare_seed_reference_resolves_to_this_daemons_own_agent() {
    // Given a def this host holds, named without a daemon qualifier
    let tddy_home = tempfile::tempdir().unwrap();
    an_agent_def_replacing_grep(tddy_home.path(), "explorer");
    let service = a_daemon_with_no_peers(tddy_home.path().to_path_buf());
    let local = local_instance_id_for_config(&service.config);

    // When
    let records = service
        .seeded_roster_records(&["explorer".to_string()])
        .await
        .expect("a def this host holds must resolve");

    // Then — a bare name means the daemon the start was addressed to, and an agent placed there
    // reads the authoritative worktree itself, so it names no clone
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].agent_id, format!("explorer@{local}"));
    assert_eq!(records[0].daemon_instance_id, local);
    assert_eq!(records[0].codebase_session_id, None);
}

#[tokio::test]
async fn a_seeded_record_carries_the_tools_its_def_takes_from_the_main_agent() {
    // Given a def that replaces `Grep`
    let tddy_home = tempfile::tempdir().unwrap();
    an_agent_def_replacing_grep(tddy_home.path(), "explorer");
    let service = a_daemon_with_no_peers(tddy_home.path().to_path_buf());

    // When
    let records = service
        .seeded_roster_records(&["explorer".to_string()])
        .await
        .expect("a def this host holds must resolve");

    // Then — the record is what the spawn derives its allowlist from, so the withdrawal has to
    // be on it: a record without it launches the agent still holding the tool it gave away
    assert_eq!(records[0].replaces, vec!["Grep".to_string()]);
}

#[tokio::test]
async fn a_seed_reference_that_resolves_to_no_def_is_a_request_error_naming_it() {
    // Given one reference this host holds and one nothing does
    let tddy_home = tempfile::tempdir().unwrap();
    an_agent_def_replacing_grep(tddy_home.path(), "explorer");
    let service = a_daemon_with_no_peers(tddy_home.path().to_path_buf());

    // When
    let error = service
        .seeded_roster_records(&["explorer".to_string(), "ghost-agent".to_string()])
        .await
        .expect_err("an unresolvable reference must fail the whole seed");

    // Then — the whole seed fails, and the message names the reference the operator typed
    assert_eq!(error.code(), tddy_rpc::Code::InvalidArgument);
    assert!(
        error.message().contains("ghost-agent"),
        "the refusal must name the reference it could not resolve; got: {}",
        error.message()
    );
}

#[tokio::test]
async fn a_seed_reference_naming_a_daemon_this_host_cannot_see_is_a_request_error_naming_it() {
    // Given a reference qualified with a daemon that is in no common room this host can see
    let tddy_home = tempfile::tempdir().unwrap();
    an_agent_def_replacing_grep(tddy_home.path(), "explorer");
    let service = a_daemon_with_no_peers(tddy_home.path().to_path_buf());

    // When
    let error = service
        .seeded_roster_records(&["explorer@workstation-b".to_string()])
        .await
        .expect_err("a daemon this host cannot reach must fail the seed");

    // Then — a bad request naming the daemon, decided from the eligible list rather than by
    // waiting out a forward. Never read as the local `explorer`: a def of the same name on
    // another host is a different agent, which is what qualified ids exist to keep apart
    assert_eq!(error.code(), tddy_rpc::Code::InvalidArgument);
    assert!(
        error.message().contains("workstation-b"),
        "the refusal must name the daemon it could not reach; got: {}",
        error.message()
    );
}

#[tokio::test]
async fn a_seed_reference_naming_two_daemons_is_a_request_error_naming_the_field() {
    // Given a reference that splits into no single pair
    let tddy_home = tempfile::tempdir().unwrap();
    let service = a_daemon_with_no_peers(tddy_home.path().to_path_buf());

    // When
    let error = service
        .seeded_roster_records(&["explorer@a@b".to_string()])
        .await
        .expect_err("a reference naming no single daemon must fail the seed");

    // Then — refused for its shape, before any def source is consulted
    assert_eq!(error.code(), tddy_rpc::Code::InvalidArgument);
    assert!(
        error.message().contains("specialized_agents"),
        "the refusal must name the field it read; got: {}",
        error.message()
    );
}
