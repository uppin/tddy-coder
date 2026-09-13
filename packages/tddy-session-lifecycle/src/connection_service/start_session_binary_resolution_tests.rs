use super::*;

fn a_config_with_claude_binary(binary_path: &str) -> crate::config::DaemonConfig {
    let yaml = format!(
        "users:\n  - github_user: u\n    os_user: u\nclaude_cli:\n  binary_path: {binary_path}\n"
    );
    serde_yaml::from_str(&yaml).expect("daemon config should parse")
}

/// The interactive (non-sandboxed) StartSession path must resolve `claude` through the same
/// host resolver as the sandboxed path, so an explicitly configured absolute path is honored
/// instead of being spawned by bare name against the daemon's minimal systemd `PATH`.
#[test]
fn start_session_honors_an_explicitly_configured_claude_binary_path() {
    // Given a daemon config naming an explicit claude binary path
    let config = a_config_with_claude_binary("/opt/custom/bin/claude");

    // When resolving the binary for an interactive StartSession
    let resolved = resolve_start_session_claude_binary(&config);

    // Then the configured absolute path is used verbatim
    assert_eq!(resolved, "/opt/custom/bin/claude");
}

/// The StartSession resolver is the *same* resolution the sandboxed path uses — the two spawn
/// paths must never diverge on which `claude` they pick.
#[test]
fn start_session_resolves_the_same_binary_as_the_sandbox_path() {
    // Given a daemon config naming an explicit claude binary path
    let config = a_config_with_claude_binary("/opt/custom/bin/claude");

    // When resolving via the StartSession path and the shared sandbox resolver
    let start_session = resolve_start_session_claude_binary(&config);
    let sandbox = crate::config::resolve_claude_binary_path(&config);

    // Then both paths agree
    assert_eq!(start_session, sandbox);
}
