//! The commit identity a session child is spawned under.
//!
//! The daemon resolves the project's account and hands the spawner four variables — the
//! `GIT_AUTHOR_*` / `GIT_COMMITTER_*` pairs — so a commit the child's agent makes is authored by
//! that account. **Nothing else rides along**: in particular never the account's token, which a
//! session's tools ask the daemon for per call instead (`github-token` over the toolcall socket).
//! The spawner enforces that itself, because the supervisor wire it feeds carries arbitrary `env`.
//!
//! Changeset: docs/dev/1-WIP/2026-09-19-keyring-github-identity.md

use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use tddy_spawn::spawner::{self, LiveKitCreds, SpawnOptions, StartupWatch};

const ADA: [(&str, &str); 4] = [
    ("GIT_AUTHOR_NAME", "ada"),
    ("GIT_AUTHOR_EMAIL", "101+ada@users.noreply.github.com"),
    ("GIT_COMMITTER_NAME", "ada"),
    ("GIT_COMMITTER_EMAIL", "101+ada@users.noreply.github.com"),
];

fn pairs(of: &[(&str, &str)]) -> Vec<(String, String)> {
    of.iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn current_username() -> String {
    std::env::var("USER").expect("USER must be set to resolve the target account")
}

fn a_livekit() -> LiveKitCreds {
    LiveKitCreds {
        url: "ws://127.0.0.1:7880".to_string(),
        api_key: "test-key".to_string(),
        api_secret: "test-secret".to_string(),
        common_room: None,
        daemon_instance_id: None,
    }
}

/// A stand-in for `tddy-coder` that writes the environment it was started with to `dump`, then
/// stays alive past the startup watch.
fn a_tool_that_dumps_its_environment(dir: &Path, dump: &Path) -> PathBuf {
    let path = dir.join("fake-tddy-coder.sh");
    std::fs::write(
        &path,
        format!("#!/bin/sh\nenv > \"{}\"\nsleep 5\n", dump.display()),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// The plan for a new session spawned with `git_environment`.
fn plan_with(
    git_environment: &[(String, String)],
) -> anyhow::Result<spawner::SessionChildPlan> {
    let repo = tempfile::tempdir().unwrap();
    let data_dir = tempfile::tempdir().unwrap();
    let tools = tempfile::tempdir().unwrap();
    let tool = a_tool_that_dumps_its_environment(tools.path(), &tools.path().join("env"));
    spawner::plan_session_child(
        &current_username(),
        tool.to_str().unwrap(),
        data_dir.path(),
        repo.path(),
        &a_livekit(),
        SpawnOptions {
            new_session_id: Some("session-a"),
            git_environment,
            ..Default::default()
        },
        "info",
        spawner::CHILD_LOG_FORMAT_FALLBACK,
        None,
    )
}

#[test]
fn a_planned_child_is_given_exactly_the_four_commit_identity_pairs() {
    // Given a project that acts as ada
    let identity = pairs(&ADA);

    // When the child is planned
    let plan = plan_with(&identity).expect("plan the session child");

    // Then its environment is her four pairs and nothing else
    assert_eq!(plan.env, identity);
}

#[test]
fn a_planned_child_with_no_commit_identity_is_given_no_environment_of_its_own() {
    // Given a project that resolves to no account
    let identity = Vec::new();

    // When the child is planned
    let plan = plan_with(&identity).expect("plan the session child");

    // Then the checkout's own identity stays in force
    assert_eq!(plan.env, Vec::<(String, String)>::new());
}

#[test]
fn a_github_token_is_refused_as_session_environment() {
    // Given ada's pairs with the account's token among them
    let mut identity = pairs(&ADA);
    identity.push(("GITHUB_TOKEN".to_string(), "ghp_ada_token".to_string()));

    // When the child is planned
    let refusal = plan_with(&identity)
        .map(|_| ())
        .expect_err("a token is not commit identity");

    // Then the key is named and the value is not
    let message = refusal.to_string();
    assert!(message.contains("GITHUB_TOKEN"), "got: {message}");
    assert!(!message.contains("ghp_ada_token"), "got: {message}");
}

#[test]
fn a_loader_variable_is_refused_as_session_environment() {
    // Given an identity carrying a variable that chooses what code the child loads
    let identity = pairs(&[("LD_PRELOAD", "/tmp/evil.so")]);

    // When the child is planned
    let refusal = plan_with(&identity).map(|_| ());

    // Then nothing but commit identity is accepted
    assert!(refusal.is_err());
}

#[test]
fn a_spawned_child_starts_with_the_commit_identity_in_its_environment() {
    // Given a tool that records the environment it starts with
    let repo = tempfile::tempdir().unwrap();
    let data_dir = tempfile::tempdir().unwrap();
    let tools = tempfile::tempdir().unwrap();
    let dump = tools.path().join("env");
    let tool = a_tool_that_dumps_its_environment(tools.path(), &dump);
    let identity = pairs(&ADA);

    // When a session is spawned for a project that acts as ada
    spawner::spawn_as_user(
        &current_username(),
        tool.to_str().unwrap(),
        data_dir.path(),
        repo.path(),
        &a_livekit(),
        SpawnOptions {
            new_session_id: Some("session-a"),
            git_environment: &identity,
            ..Default::default()
        },
        "info",
        spawner::CHILD_LOG_FORMAT_FALLBACK,
        None,
        StartupWatch::from_millis(300, 10),
    )
    .expect("spawn the session child");

    // Then the commit identity it started with is her four pairs, exactly
    let started_with = std::fs::read_to_string(&dump).expect("the child recorded its environment");
    let mut identity_lines: Vec<&str> = started_with
        .lines()
        .filter(|line| line.starts_with("GIT_AUTHOR_") || line.starts_with("GIT_COMMITTER_"))
        .collect();
    identity_lines.sort_unstable();
    assert_eq!(
        identity_lines,
        [
            "GIT_AUTHOR_EMAIL=101+ada@users.noreply.github.com",
            "GIT_AUTHOR_NAME=ada",
            "GIT_COMMITTER_EMAIL=101+ada@users.noreply.github.com",
            "GIT_COMMITTER_NAME=ada",
        ]
    );
}
