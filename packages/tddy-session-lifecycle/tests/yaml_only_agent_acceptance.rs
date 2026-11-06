//! Acceptance: an agent is added by dropping one YAML file into `<tddyhome>/tools/`, and an agent's
//! gates are its declared capabilities rather than its name (`#agent-tools 12/12`).
//!
//! Feature doc: `docs/ft/daemon/session-agent-roster.md`.
//!
//! The daemon is the real `DaemonSessionHost` given the coding-agent registry over its
//! `<tddyhome>/tools` (see `agent_registry_support`); its daemon.yaml has no agent section. The
//! new agent, `acme`, names only adapters that already exist. The routed starts (`#agent-tools
//! 13/19`) run on a Meta account in the owner's unlocked vault, and the stub reports the route
//! environment it was given. The jailed start runs a real Seatbelt jail with the real
//! `tddy-sandbox-runner`, so it is macOS-only: CI (ubuntu) never runs the
//! `jailed` module.

mod agent_registry_support;

use std::collections::BTreeSet;

use agent_registry_support::{
    a_daemon_with_tools, a_registry_daemon, acme_yaml_declaring, ACME_MODEL, ACME_YAML,
    META_ACCOUNT, META_ANTHROPIC_ROOT, META_KEY,
};
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

/// `acme.yaml` declaring `product: Acme Code` and a route to Anthropic Messages through its
/// environment — the shape and provider Claude Code's spec declares, under no name Rust knows.
fn acme_yaml_with_an_anthropic_env_route() -> String {
    acme_yaml_declaring(
        "    product: Acme Code\n    route:\n      providers: [anthropic]\n      shape: env\n",
    )
}

#[tokio::test]
#[serial_test::serial]
async fn a_new_agent_dropped_into_tddyhome_tools_with_an_anthropic_env_route_starts_on_a_provider_account_with_the_accounts_endpoint_in_its_environment(
) {
    // Given — acme.yaml with an anthropic env route, and the owner's Meta account, which serves
    // Anthropic Messages
    let daemon = a_registry_daemon()
        .with_tool("acme.yaml", &acme_yaml_with_an_anthropic_env_route())
        .with_a_meta_account()
        .build();

    // When
    let (session_id, _host) = daemon
        .start(daemon.a_start_on_account("acme-cli", META_ACCOUNT))
        .await
        .expect("a host acme-cli start on the Meta account succeeds");
    let started = daemon.stub_report(&session_id).await;

    // Then — the Anthropic-env body's environment: the account's root, its key and the model
    assert_eq!(
        (
            daemon.metadata_of(&session_id).session_type,
            started.env("ANTHROPIC_BASE_URL"),
            started.env("ANTHROPIC_AUTH_TOKEN"),
            started.env("ANTHROPIC_MODEL"),
        ),
        (
            Some("acme-cli".to_string()),
            META_ANTHROPIC_ROOT.to_string(),
            META_KEY.to_string(),
            ACME_MODEL.to_string(),
        )
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_new_agent_without_a_route_section_is_refused_on_a_provider_account_naming_the_agent_and_spawns_nothing(
) {
    // Given — acme.yaml declaring its product and no route, and the owner's Meta account
    let daemon = a_registry_daemon()
        .with_tool(
            "acme.yaml",
            &acme_yaml_declaring("    product: Acme Code\n"),
        )
        .with_a_meta_account()
        .build();

    // When
    let refusal = daemon
        .start(daemon.a_start_on_account("acme-cli", META_ACCOUNT))
        .await
        .map(|(session_id, _host)| session_id)
        .expect_err("an agent that declares no provider is not routed");

    // Then — refused naming the spec's product, and nothing is spawned
    assert_eq!(
        (refusal.code, refusal.message, daemon.spawn_marker.exists()),
        (
            Code::InvalidArgument,
            "Acme Code cannot be pointed at another provider; it always runs on its own \
             vendor"
                .to_string(),
            false,
        )
    );
}

#[tokio::test]
#[serial_test::serial]
async fn a_harness_profile_naming_a_dropped_in_agent_starts_its_session_type() {
    // Given — a profile running `acme-cli` on the Meta account, and acme.yaml routable to it
    let daemon = a_registry_daemon()
        .with_tool("acme.yaml", &acme_yaml_with_an_anthropic_env_route())
        .with_profile(
            "acme-on-meta",
            &format!(
                "name: acme-on-meta\nharness: acme-cli\nprovider_account: {META_ACCOUNT}\nmodel: {ACME_MODEL}\n"
            ),
        )
        .with_a_meta_account()
        .build();

    // When
    let (session_id, _host) = daemon
        .start(daemon.a_start_through_profile("acme-on-meta"))
        .await
        .expect("a start through the profile succeeds");
    let started = daemon.stub_report(&session_id).await;

    // Then — the profile's harness is the session type, run on the profile's account
    assert_eq!(
        (
            daemon.metadata_of(&session_id).session_type,
            started.env("ANTHROPIC_BASE_URL"),
            started.env("ANTHROPIC_AUTH_TOKEN"),
        ),
        (
            Some("acme-cli".to_string()),
            META_ANTHROPIC_ROOT.to_string(),
            META_KEY.to_string(),
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
