//! Acceptance: an agent is added by dropping one YAML file into `<tddyhome>/tools/`, and an agent's
//! gates are its declared capabilities rather than its name (`#agent-tools 12/12`).
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-coding-agent-registry.md` § Acceptance Criteria.
//!
//! The daemon is the real `DaemonSessionHost` given the coding-agent registry over its
//! `<tddyhome>/tools` (see `agent_registry_support`); its daemon.yaml has no agent section. The
//! new agent, `acme`, names only adapters that already exist. The jailed start runs a real Seatbelt
//! jail with the real `tddy-sandbox-runner`, so it is macOS-only: CI (ubuntu) never runs the
//! `jailed` module.

mod agent_registry_support;

use std::collections::BTreeSet;

use agent_registry_support::{a_daemon_with_tools, ACME_MODEL, ACME_YAML};
use pretty_assertions::assert_eq;
use tddy_coding_agent_specs::{builtin_specs, Capability};
use tddy_rpc::Code;

/// The built-in claude.yaml with `hooks` struck from its capabilities — an operator's override.
fn claude_yaml_without_hooks() -> String {
    let (_, claude) = builtin_specs()
        .into_iter()
        .find(|(path, _)| path.ends_with("/claude.yaml"))
        .expect("claude.yaml is built in");
    let without = claude.replace("capabilities: [hooks, ", "capabilities: [");
    assert_ne!(
        without, claude,
        "the override strikes hooks from claude.yaml"
    );
    without
}

#[tokio::test]
#[serial_test::serial]
async fn a_new_agent_dropped_into_tddyhome_tools_appears_in_the_hosts_agent_list_starts_on_the_host_and_offers_its_declared_capabilities(
) {
    // Given — acme.yaml, and nothing else about acme anywhere
    let daemon = a_daemon_with_tools(&[("acme.yaml", ACME_YAML)]);

    // When
    let listed: Vec<String> = daemon
        .registry
        .all()
        .iter()
        .map(|spec| spec.id.clone())
        .filter(|id| id == "acme" || id == "claude")
        .collect();
    let declared: BTreeSet<Capability> = daemon
        .registry
        .for_session_type("acme-cli")
        .expect("acme-cli names acme")
        .capabilities()
        .clone();
    let (session_id, _host) = daemon
        .start(daemon.a_start("acme-cli", false))
        .await
        .expect("a host acme-cli start succeeds");
    let started = daemon.stub_report(&session_id).await;
    daemon.an_active_session("acme-hooked-1", "acme-cli");
    let hook_report = daemon.a_hook_reports_running("acme-hooked-1").await;

    // Then — listed beside the built-ins, started with acme.yaml's argv, and its declared `hooks`
    // admits the hook report a name gate would have refused
    assert_eq!(
        (
            listed,
            declared,
            started.argv(),
            daemon.metadata_of(&session_id).session_type,
            hook_report.map_err(|status| status.message),
            daemon.metadata_of("acme-hooked-1").activity_status,
        ),
        (
            vec!["acme".to_string(), "claude".to_string()],
            [Capability::Hooks, Capability::RequiresModel]
                .into_iter()
                .collect(),
            vec![
                daemon.stub_path(),
                "--model".to_string(),
                ACME_MODEL.to_string(),
            ],
            Some("acme-cli".to_string()),
            Ok(()),
            Some("Running".to_string()),
        )
    );
}

#[tokio::test]
#[serial_test::serial]
async fn removing_hooks_from_an_override_of_claude_yaml_turns_off_hook_based_activity_for_claude() {
    // Given — claude.yaml overridden without `hooks`; cursor.yaml as built in
    let claude = claude_yaml_without_hooks();
    let daemon = a_daemon_with_tools(&[("claude.yaml", claude.as_str())]);
    daemon.an_active_session("claude-unhooked-1", "claude-cli");
    daemon.an_active_session("cursor-hooked-1", "cursor-cli");

    // When
    let claude_report = daemon.a_hook_reports_running("claude-unhooked-1").await;
    let cursor_report = daemon.a_hook_reports_running("cursor-hooked-1").await;

    // Then — the claude-cli report is refused and records nothing; the cursor-cli one, whose spec
    // still declares `hooks`, is the positive control
    assert_eq!(
        (
            claude_report.map_err(|status| status.code),
            daemon.metadata_of("claude-unhooked-1").activity_status,
            cursor_report.map_err(|status| status.code),
            daemon.metadata_of("cursor-hooked-1").activity_status,
        ),
        (
            Err(Code::FailedPrecondition),
            None,
            Ok(()),
            Some("Running".to_string()),
        )
    );
}

/// The jailed half of the YAML-only claim: macOS only, through a real Seatbelt jail.
#[cfg(target_os = "macos")]
mod jailed {
    use pretty_assertions::assert_eq;

    use super::agent_registry_support::{a_daemon_with_tools, ACME_MODEL, ACME_YAML};

    #[tokio::test]
    #[serial_test::serial]
    async fn a_new_agent_dropped_into_tddyhome_tools_starts_in_a_jail() {
        // Given
        let daemon = a_daemon_with_tools(&[("acme.yaml", ACME_YAML)]);

        // When
        let (session_id, _jail) = daemon
            .start(daemon.a_start("acme-cli", true))
            .await
            .expect("a jailed acme-cli start succeeds");
        let started = daemon.stub_report(&session_id).await;

        // Then — acme.yaml's argv, run inside the jail
        let meta = daemon.metadata_of(&session_id);
        assert_eq!(
            (
                started.argv().into_iter().skip(1).collect::<Vec<_>>(),
                meta.session_type,
                meta.sandbox,
            ),
            (
                vec!["--model".to_string(), ACME_MODEL.to_string()],
                Some("acme-cli".to_string()),
                Some(true),
            )
        );
    }
}
