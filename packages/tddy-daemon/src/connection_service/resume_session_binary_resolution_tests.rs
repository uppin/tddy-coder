use super::*;

fn a_config_with_claude_binary(binary_path: &str) -> crate::config::DaemonConfig {
    let yaml = format!(
        "users:\n  - github_user: u\n    os_user: u\nclaude_cli:\n  binary_path: {binary_path}\n"
    );
    serde_yaml::from_str(&yaml).expect("daemon config should parse")
}

/// A daemon config that leaves `binary_path` at its default (the bare name `claude`). This is
/// the production case that broke ResumeSession: the bare name was spawned against the daemon's
/// minimal systemd `PATH` (which omits `~/.local/bin`) instead of being resolved to a host path.
fn a_config_with_default_claude_binary() -> crate::config::DaemonConfig {
    let yaml = "users:\n  - github_user: u\n    os_user: u\nclaude_cli: {}\n";
    serde_yaml::from_str(yaml).expect("daemon config should parse")
}

/// ResumeSession must resolve `claude` through the same host resolver as StartSession — an
/// explicitly configured absolute path is honored verbatim, never spawned by bare name.
#[test]
fn resume_session_honors_an_explicitly_configured_claude_binary_path() {
    // Given a daemon config naming an explicit claude binary path
    let config = a_config_with_claude_binary("/opt/custom/bin/claude");

    // When resolving the binary for a ResumeSession relaunch
    let resolved = resolve_resume_session_claude_binary(&config);

    // Then the configured absolute path is used verbatim
    assert_eq!(resolved, "/opt/custom/bin/claude");
}

/// The ResumeSession resolver must be the *same* resolution StartSession uses — resume was the
/// odd path out, spawning the bare config name while create resolved it to a host path.
#[test]
fn resume_session_resolves_the_same_binary_as_start_session() {
    // Given a daemon config that leaves the claude binary at its bare-name default
    let config = a_config_with_default_claude_binary();

    // When resolving via the ResumeSession path and the StartSession path
    let resume = resolve_resume_session_claude_binary(&config);
    let start_session = resolve_start_session_claude_binary(&config);

    // Then both paths pick the same binary — resume never diverges to the bare name
    assert_eq!(resume, start_session);
}
