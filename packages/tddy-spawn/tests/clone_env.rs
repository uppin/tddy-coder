//! The environment a daemon-run SSH clone is given.
//!
//! Composed by a pure function so that what reaches `git` is a value a test can read, the same
//! value whichever backend carries it, and — for the supervised backend — a set of keys an operator
//! can be told to allow. Every assertion here is about a variable a real `git` and `ssh` act on;
//! that they act on them as claimed is `clone_askpass_acceptance`'s to prove.
//!
//! Feature: docs/ft/daemon/clone-ssh-passphrase.md

use std::collections::BTreeMap;
use std::time::Duration;

use tddy_spawn::clone_env::CloneSshEnv;

fn a_clone_env() -> CloneSshEnv {
    CloneSshEnv::new(
        "/usr/local/bin/tddy-askpass",
        "http://127.0.0.1:41234",
        "clone-token-1",
    )
    .agent_socket("/run/user/1000/ssh-agent.socket")
}

fn env_of(clone: CloneSshEnv) -> BTreeMap<String, String> {
    clone.into_env().into_iter().collect()
}

#[test]
fn ssh_is_pointed_at_the_helper_and_told_to_use_it_without_a_terminal() {
    let env = env_of(a_clone_env());

    assert_eq!(env["SSH_ASKPASS"], "/usr/local/bin/tddy-askpass");
    // Without `force`, ssh calls its helper only when DISPLAY is set, and a daemon has none
    assert_eq!(env["SSH_ASKPASS_REQUIRE"], "force");
}

#[test]
fn git_is_forbidden_to_prompt_on_its_own() {
    assert_eq!(env_of(a_clone_env())["GIT_TERMINAL_PROMPT"], "0");
}

#[test]
fn the_helper_is_told_where_the_daemon_is_and_which_clone_it_serves() {
    let env = env_of(a_clone_env());

    assert_eq!(env["TDDY_ASKPASS_URL"], "http://127.0.0.1:41234");
    assert_eq!(env["TDDY_ASKPASS_TOKEN"], "clone-token-1");
}

#[test]
fn ssh_reaches_the_users_agent_through_the_socket_named() {
    assert_eq!(
        env_of(a_clone_env())["SSH_AUTH_SOCK"],
        "/run/user/1000/ssh-agent.socket"
    );
}

#[test]
fn ssh_adds_an_unlocked_key_to_the_agent_and_gives_up_on_a_silent_host() {
    let command = env_of(a_clone_env())["GIT_SSH_COMMAND"].clone();

    assert!(command.starts_with("ssh "), "{command}");
    assert!(command.contains("-o AddKeysToAgent=yes"), "{command}");
    assert!(command.contains("-o ConnectTimeout=15"), "{command}");
}

#[test]
fn the_connect_timeout_can_be_chosen() {
    let command =
        env_of(a_clone_env().connect_timeout(Duration::from_secs(2)))["GIT_SSH_COMMAND"].clone();

    assert!(command.contains("-o ConnectTimeout=2"), "{command}");
    assert!(!command.contains("ConnectTimeout=15"), "{command}");
}

#[test]
fn extra_ssh_arguments_follow_the_ones_every_clone_gets() {
    let command = env_of(a_clone_env().ssh_args(["-p", "2222"]))["GIT_SSH_COMMAND"].clone();

    assert!(command.ends_with("-p 2222"), "{command}");
    assert!(command.contains("AddKeysToAgent=yes"), "{command}");
}

#[test]
fn no_agent_socket_leaves_ssh_auth_sock_unset_rather_than_empty() {
    // An empty SSH_AUTH_SOCK makes ssh try a socket named "" — a different failure from "no agent"
    let env = env_of(CloneSshEnv::new("/h", "http://127.0.0.1:1", "t"));

    assert!(!env.contains_key("SSH_AUTH_SOCK"));
}

#[test]
fn every_variable_a_clone_sets_is_one_a_supervisor_can_be_told_to_allow() {
    // The supervisor refuses any env key not on its allowlist, so the allowlist an operator is
    // handed must be exactly what a clone can set — no more to over-grant, none missing to refuse.
    let set: Vec<String> = env_of(a_clone_env()).into_keys().collect();
    let mut allowed: Vec<String> = CloneSshEnv::ALLOWED_ENV_KEYS
        .iter()
        .map(|k| k.to_string())
        .collect();
    allowed.sort();

    assert_eq!(set, allowed);
}

#[test]
fn an_ssh_argument_containing_a_space_stays_one_word_for_the_shell_git_runs_it_through() {
    let command = env_of(a_clone_env().ssh_args(["-i", "/home/ada/my keys/id_rsa"]))
        ["GIT_SSH_COMMAND"]
        .clone();

    assert!(
        command.ends_with("-i '/home/ada/my keys/id_rsa'"),
        "{command}"
    );
}

#[test]
fn an_ssh_argument_containing_a_quote_is_escaped_rather_than_ending_its_word() {
    let command = env_of(a_clone_env().ssh_args(["-i", "/home/ada/ada's keys/id_rsa"]))
        ["GIT_SSH_COMMAND"]
        .clone();

    assert!(
        command.ends_with(r"-i '/home/ada/ada'\''s keys/id_rsa'"),
        "{command}"
    );
}
