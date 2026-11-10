//! A supervised clone is the daemon's work, done as ONE `SpawnStreamed` of the clone helper as the
//! session user (`#agent-tools` 32/32): the helper clones, renames into place and cleans up in its
//! own process, so the supervisor never renames anything as root and knows nothing about git. Its
//! SSH settings need no supervisor configuration.
//!
//! Feature docs: docs/ft/supervisor/1-WIP/PRD-2026-11-07-supervisor-agent-agnostic.md

use std::collections::BTreeMap;
use std::path::Path;

use tddy_spawn::clone_env::CloneSshEnv;
use tddy_spawn::supervisor_spawn::{git_for_supervised_clone, plan_supervised_clone};
use tddy_supervisor::policy::{resolve_spawn_env, SpawnKind};

const DESTINATION: &str = "/srv/repos/alice/project";
const URL: &str = "git@github.com:acme/project.git";
const CLONE_HELPER: &str = "/opt/tddy/bin/tddy-git-clone";
const GIT: &str = "/usr/bin/git";

fn an_ssh_clone_env() -> BTreeMap<String, String> {
    CloneSshEnv::new(
        "/opt/tddy/bin/tddy-askpass",
        "http://127.0.0.1:8899",
        "one-clone-token",
    )
    .agent_socket("/run/user/1000/ssh-agent.sock")
    .into_env()
    .into_iter()
    .collect()
}

#[test]
fn a_supervised_clone_is_one_streamed_spawn_of_the_clone_helper_as_the_session_user() {
    // When
    let request = plan_supervised_clone(
        "alice",
        Path::new(CLONE_HELPER),
        Path::new(GIT),
        URL,
        Path::new(DESTINATION),
        an_ssh_clone_env(),
    )
    .expect("a supervised clone plans");

    // Then — the helper, as alice, told which git and where, with the clone's env whole
    assert_eq!(
        (
            request.os_user.as_str(),
            request.tool_path.as_str(),
            request.args.clone(),
            request
                .env
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<BTreeMap<_, _>>(),
        ),
        (
            "alice",
            CLONE_HELPER,
            vec![
                "--git".to_string(),
                GIT.to_string(),
                "--".to_string(),
                URL.to_string(),
                DESTINATION.to_string(),
            ],
            an_ssh_clone_env(),
        )
    );
}

#[test]
fn an_ssh_clone_needs_no_supervisor_env_configuration() {
    // Given every variable an SSH clone with a passphrase prompt sets
    let env = an_ssh_clone_env();

    // When the supervisor's one env rule resolves it for a streamed spawn — no allow-list anywhere
    let resolved = resolve_spawn_env(SpawnKind::Streamed, &env);

    // Then it passes whole
    assert_eq!(resolved.map(|resolved| resolved.variables), Ok(env));
}

#[test]
fn a_supervised_clone_with_no_git_on_the_spawn_child_path_is_refused_naming_git() {
    // When no git was found where the clone helper would be told to look
    let refusal = git_for_supervised_clone(None)
        .expect_err("no bare `git` stands in")
        .to_string();

    // Then it is a refusal naming what is missing, not a bare name handed on
    assert!(
        refusal.contains("failed precondition") && refusal.contains("`git`"),
        "{refusal}"
    );
}

#[test]
fn the_git_found_on_the_spawn_child_path_is_the_one_the_helper_runs() {
    // When
    let git = git_for_supervised_clone(Some(GIT.into())).expect("a git was found");

    // Then
    assert_eq!(git, Path::new(GIT));
}

#[test]
fn a_supervised_clone_naming_a_relative_path_is_refused() {
    // Given a relative clone helper, git or destination, each in turn
    let cases = [
        ("tddy-git-clone", GIT, DESTINATION),
        (CLONE_HELPER, "git", DESTINATION),
        (CLONE_HELPER, GIT, "project"),
    ];

    // When
    let refusals: Vec<bool> = cases
        .iter()
        .map(|(clone_helper, git, destination)| {
            plan_supervised_clone(
                "alice",
                Path::new(clone_helper),
                Path::new(git),
                URL,
                Path::new(destination),
                BTreeMap::new(),
            )
            .is_err_and(|refusal| refusal.to_string().contains("absolute path"))
        })
        .collect();

    // Then — a relative path would be resolved by whoever runs it, so every one is refused
    assert_eq!(refusals, vec![true, true, true]);
}
