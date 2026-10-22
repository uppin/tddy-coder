//! What the CLI harness registry says about each terminal-pane harness, and the argument lists it
//! builds for OpenCode.
//!
//! Feature: docs/ft/daemon/opencode-cli-session.md

use tddy_agent_backend::cli_harness::{cli_harness, cli_harnesses, models_from_listing, CliLaunch};
use tddy_agent_backend::provider_route::Harness;

fn opencode() -> &'static tddy_agent_backend::cli_harness::CliHarness {
    cli_harness("opencode-cli").expect("opencode-cli is registered")
}

#[test]
fn the_registry_offers_claude_cursor_opencode_and_muse_in_order() {
    // When
    let offered: Vec<(&str, &str)> = cli_harnesses()
        .iter()
        .map(|harness| (harness.session_type, harness.label))
        .collect();

    // Then
    assert_eq!(
        offered,
        vec![
            ("claude-cli", "Claude CLI"),
            ("cursor-cli", "Cursor CLI"),
            ("opencode-cli", "OpenCode"),
            ("muse-cli", "Muse Code"),
        ]
    );
}

#[test]
fn a_tool_or_workspace_session_is_no_cli_harness() {
    // Then
    assert!(cli_harness("tool").is_none());
    assert!(cli_harness("").is_none());
    assert!(cli_harness("workspace").is_none());
}

#[test]
fn opencode_is_routed_as_opencode_and_reads_its_config_from_opencode_config() {
    // Then
    assert_eq!(opencode().route_harness, Harness::OpenCodeCli);
    assert_eq!(opencode().context_agent, "opencode");
    assert_eq!(opencode().default_binary, "opencode");
    assert_eq!(opencode().binary_env, "TDDY_OPENCODE_CLI");
    assert_eq!(opencode().route_config_env, Some("OPENCODE_CONFIG"));
}

#[test]
fn an_opencode_session_starts_with_its_model_and_first_prompt() {
    // When
    let argv = (opencode().argv)(&CliLaunch {
        binary: "/usr/local/bin/opencode",
        model: "fireworks/kimi-k2",
        session_id: "sess-1",
        initial_prompt: Some("  fix the flaky test  "),
        ..CliLaunch::default()
    });

    // Then
    assert_eq!(
        argv,
        vec![
            "/usr/local/bin/opencode",
            "--model",
            "fireworks/kimi-k2",
            "--prompt",
            "fix the flaky test",
        ]
    );
}

#[test]
fn a_resumed_opencode_session_continues_the_worktrees_last_session() {
    // When
    let argv = (opencode().argv)(&CliLaunch {
        binary: "opencode",
        model: "",
        session_id: "sess-1",
        resume: true,
        ..CliLaunch::default()
    });

    // Then: one session per worktree, so the last session there is this one
    assert_eq!(argv, vec!["opencode", "--continue"]);
}

#[test]
fn opencode_models_are_read_one_provider_model_per_line() {
    // When
    let catalog = models_from_listing("fireworks/kimi-k2\n\nopenai/gpt-5.6\n");

    // Then
    let ids: Vec<&str> = catalog.models.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, vec!["fireworks/kimi-k2", "openai/gpt-5.6"]);
    assert_eq!(catalog.default_model, "fireworks/kimi-k2");
}
