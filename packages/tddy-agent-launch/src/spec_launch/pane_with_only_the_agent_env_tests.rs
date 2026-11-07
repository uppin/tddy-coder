//! Unit tests: a tmux pane runs its agent with only the agent's environment (`#agent-tools 28/28`).
//!
//! A pane inherits the tmux *server's* environment, which the daemon cannot read; the pane file the
//! launch writes is sourced on top of it. The pane command must therefore drop, inside the pane,
//! everything but the base set and what the launch set.

use super::in_a_pane_with_only_the_agent_env;

/// The names a pane command prints from its own environment, given `pane_env` as what the pane
/// holds before it runs (the server's environment plus the sourced pane file).
fn names_seen_in_a_pane_holding(argv: &[String], pane_env: &[(&str, &str)]) -> Vec<String> {
    let output = std::process::Command::new(&argv[0])
        .args(&argv[1..])
        .env_clear()
        .envs(pane_env.iter().copied())
        .output()
        .expect("the pane command runs");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_once('=').map(|(name, _)| name.to_string()))
        .collect()
}

fn launch_env(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect()
}

#[test]
fn a_pane_keeps_only_the_variables_of_the_agent_env() {
    // Given — the agent's whole environment, as the daemon computed it; a pane also holding a
    // session marker, an inherited TDDY_ variable and an undeclared provider key from the tmux server
    let launch = launch_env(&[
        ("PATH", "/usr/bin:/bin"),
        ("HOME", "/home/dev"),
        ("LC_ALL", "C"),
        ("BASE_URL", "https://route.example"),
    ]);
    let argv = in_a_pane_with_only_the_agent_env(vec!["/usr/bin/env".to_string()], &launch);

    // When
    let seen = names_seen_in_a_pane_holding(
        &argv,
        &[
            ("PATH", "/usr/bin:/bin"),
            ("HOME", "/home/dev"),
            ("LC_ALL", "C"),
            ("BASE_URL", "https://route.example"),
            ("CLAUDE_CODE_CHILD_SESSION", "1"),
            ("TDDY_LEAK_PROBE", "1"),
            ("ANTHROPIC_API_KEY", "sk-undeclared"),
        ],
    );

    // Then
    for kept in ["PATH", "HOME", "LC_ALL", "BASE_URL"] {
        assert!(
            seen.iter().any(|name| name == kept),
            "{kept} kept: {seen:?}"
        );
    }
    for dropped in [
        "CLAUDE_CODE_CHILD_SESSION",
        "TDDY_LEAK_PROBE",
        "ANTHROPIC_API_KEY",
    ] {
        assert!(
            !seen.iter().any(|name| name == dropped),
            "{dropped} dropped: {seen:?}"
        );
    }
}

#[test]
fn a_pane_puts_no_launch_value_on_its_argv() {
    // Given — a secret the launch sets, which reaches the pane only through its environment file
    let launch = launch_env(&[("ROUTE_API_KEY", "sk-secret-value")]);

    // When
    let argv = in_a_pane_with_only_the_agent_env(vec!["/usr/bin/env".to_string()], &launch);

    // Then — guard: values never go on an argv, readable through `ps`
    assert!(
        argv.iter().all(|arg| !arg.contains("sk-secret-value")),
        "{argv:?}"
    );
}
