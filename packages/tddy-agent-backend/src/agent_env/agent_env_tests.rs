//! Unit tests: the allow-list an agent child's environment is built from (`#agent-tools 28/28`).

use std::ffi::{OsStr, OsString};

use pretty_assertions::assert_eq;

use super::*;

/// A base set as the registry hands it over: names and `PREFIX_*` patterns.
fn a_base_set() -> Vec<String> {
    ["PATH", "HOME", "LC_*", "XDG_*", "https_proxy"]
        .into_iter()
        .map(str::to_string)
        .collect()
}

/// A daemon environment holding the base variables, a session marker the agent must not see, an
/// inherited `TDDY_*` configuration variable, and a provider key nobody declared.
fn a_daemon_env() -> Vec<(OsString, OsString)> {
    [
        ("PATH", "/usr/bin:/bin"),
        ("HOME", "/home/dev"),
        ("LC_ALL", "en_US.UTF-8"),
        ("XDG_CONFIG_HOME", "/home/dev/.config"),
        ("https_proxy", "http://proxy:3128"),
        ("CLAUDE_CODE_CHILD_SESSION", "1"),
        ("TDDY_DAEMON_CONFIG", "/etc/tddy/daemon.yaml"),
        ("ANTHROPIC_API_KEY", "sk-undeclared"),
    ]
    .into_iter()
    .map(|(name, value)| (OsString::from(name), OsString::from(value)))
    .collect()
}

fn launch(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect()
}

fn names_of(env: &[(OsString, OsString)]) -> Vec<String> {
    let mut names: Vec<String> = env
        .iter()
        .map(|(name, _)| name.to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn value_of<'a>(env: &'a [(OsString, OsString)], name: &str) -> Option<&'a OsStr> {
    env.iter()
        .find(|(set, _)| set.as_os_str() == OsStr::new(name))
        .map(|(_, value)| value.as_os_str())
}

#[test]
fn the_agent_env_is_the_base_plus_pass_env_plus_launch_env() {
    // Given — a launch that sets a route key and the value a spec's `pass_env` named
    let launch = launch(&[
        ("BASE_URL", "https://route.example"),
        ("OPENAI_API_KEY", "sk-passed"),
    ]);

    // When
    let env = agent_child_env(&a_base_set(), a_daemon_env(), &launch);

    // Then — the base variables the daemon holds, and what the launch set; nothing else
    assert_eq!(
        names_of(&env),
        vec![
            "BASE_URL",
            "HOME",
            "LC_ALL",
            "OPENAI_API_KEY",
            "PATH",
            "XDG_CONFIG_HOME",
            "https_proxy",
        ]
    );
}

#[test]
fn a_daemon_variable_outside_the_allow_list_never_reaches_the_agent() {
    // When
    let env = agent_child_env(&a_base_set(), a_daemon_env(), &[]);

    // Then
    assert_eq!(value_of(&env, "CLAUDE_CODE_CHILD_SESSION"), None);
    assert_eq!(value_of(&env, "ANTHROPIC_API_KEY"), None);
}

#[test]
fn an_inherited_tddy_variable_never_reaches_the_agent() {
    // When
    let env = agent_child_env(&a_base_set(), a_daemon_env(), &[]);

    // Then
    assert_eq!(value_of(&env, "TDDY_DAEMON_CONFIG"), None);
}

#[test]
fn a_launch_variable_replaces_the_base_variable_of_the_same_name_once() {
    // Given — a launch run as another user, with that user's HOME
    let launch = launch(&[("HOME", "/home/session-user")]);

    // When
    let env = agent_child_env(&a_base_set(), a_daemon_env(), &launch);

    // Then
    assert_eq!(
        value_of(&env, "HOME"),
        Some(OsStr::new("/home/session-user"))
    );
    assert_eq!(names_of(&env).iter().filter(|n| *n == "HOME").count(), 1);
}

#[test]
fn a_base_pattern_admits_its_whole_prefix_and_an_exact_name_only_itself() {
    // Then
    assert!(matches_env_pattern("LC_*", OsStr::new("LC_CTYPE")));
    assert!(matches_env_pattern("XDG_*", OsStr::new("XDG_RUNTIME_DIR")));
    assert!(matches_env_pattern("PATH", OsStr::new("PATH")));
    assert!(!matches_env_pattern("PATH", OsStr::new("PATHEXT")));
    assert!(!matches_env_pattern("LC_*", OsStr::new("LC")));
}

#[test]
fn a_star_that_is_not_the_last_character_matches_nothing() {
    // Then
    assert!(!matches_env_pattern("*_KEY", OsStr::new("OPENAI_API_KEY")));
    assert!(!matches_env_pattern("A*B", OsStr::new("AXB")));
}

#[test]
fn a_base_variable_is_one_a_base_set_entry_names() {
    // Then
    assert!(is_base_variable(&a_base_set(), OsStr::new("LC_CTYPE")));
    assert!(!is_base_variable(
        &a_base_set(),
        OsStr::new("ANTHROPIC_API_KEY")
    ));
}

#[test]
fn a_command_given_the_agent_env_starts_with_exactly_that_env() {
    // Given
    std::env::set_var("ALLOW_LIST_UNIT_PROBE", "inherited");
    let mut command = std::process::Command::new("/usr/bin/env");
    apply_agent_child_env(&mut command, &launch(&[("SET_BY_LAUNCH", "yes")]));

    // When
    let output = command.output().expect("env runs");

    // Then
    let printed = String::from_utf8_lossy(&output.stdout);
    assert!(printed.contains("SET_BY_LAUNCH=yes"), "{printed}");
    assert!(!printed.contains("ALLOW_LIST_UNIT_PROBE"), "{printed}");
}
