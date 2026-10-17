//! How a clone learns which agent to use and which environment to run in.
//!
//! The agent lookup keeps the resolver's own distinction between "looked, found none" and "could not
//! look" — the first sends an operator to start an agent, the second to their directory service, and
//! a clone that reported both as a missing agent would send them to the wrong one. A facilitator
//! clone, which fetches from another daemon and uses no SSH key here, takes none of the passphrase
//! machinery with it.
//!
//! Feature: docs/ft/daemon/clone-ssh-passphrase.md

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use tddy_agent_launch::clone_ssh_env::{clone_ssh_env_for, CloneAgentError, ClonePlan};
use tddy_host_service::askpass::CloneGrants;
use tddy_host_service::ssh_agent::AgentSocketResolver;

struct AgentAt(&'static str);

impl AgentSocketResolver for AgentAt {
    fn socket_for(&self, _os_user: &str) -> Result<Option<PathBuf>, String> {
        Ok(Some(PathBuf::from(self.0)))
    }
}

struct LookupThatFails(&'static str);

impl AgentSocketResolver for LookupThatFails {
    fn socket_for(&self, _os_user: &str) -> Result<Option<PathBuf>, String> {
        Err(self.0.to_string())
    }
}

fn environment_of(plan: ClonePlan) -> BTreeMap<String, String> {
    plan.environment()
}

#[test]
fn the_clone_runs_against_the_agent_the_resolver_found_for_the_os_user() {
    let grant = CloneGrants::new().register("ada");

    let env = clone_ssh_env_for(
        &AgentAt("/run/user/1000/ssh-agent.socket"),
        "mantas",
        Path::new("/usr/bin/tddy-askpass"),
        &grant,
    )
    .expect("an agent was found")
    .into_env();

    assert!(env.contains(&(
        "SSH_AUTH_SOCK".to_string(),
        "/run/user/1000/ssh-agent.socket".to_string()
    )));
}

#[test]
fn the_clone_is_given_the_grants_url_and_token_for_its_helper() {
    let grant = CloneGrants::new().register("ada");

    let env: BTreeMap<_, _> = clone_ssh_env_for(
        &AgentAt("/s"),
        "mantas",
        Path::new("/usr/bin/tddy-askpass"),
        &grant,
    )
    .expect("an agent was found")
    .into_env()
    .into_iter()
    .collect();

    assert_eq!(env["TDDY_ASKPASS_TOKEN"], grant.token());
    assert_eq!(env["TDDY_ASKPASS_URL"], grant.url());
    assert_eq!(env["SSH_ASKPASS"], "/usr/bin/tddy-askpass");
}

#[test]
fn a_lookup_that_could_not_be_made_is_not_reported_as_a_missing_agent() {
    let grant = CloneGrants::new().register("ada");

    let error = clone_ssh_env_for(
        &LookupThatFails("passwd database did not answer"),
        "mantas",
        Path::new("/usr/bin/tddy-askpass"),
        &grant,
    )
    .expect_err("nothing could be established");

    assert_eq!(
        error,
        CloneAgentError::LookupFailed {
            os_user: "mantas".to_string(),
            reason: "passwd database did not answer".to_string()
        }
    );
    assert!(error.to_string().contains("could not look"), "{error}");
    assert!(!error.to_string().contains("no ssh-agent"), "{error}");
}

#[test]
fn a_facilitator_clone_runs_with_only_its_transport_shim() {
    let plan = ClonePlan::Facilitator {
        ssh_command: "tddy-remote-git-repo --daemon-url http://d --session-token t".to_string(),
    };

    let env = environment_of(plan);

    assert_eq!(
        env,
        BTreeMap::from([(
            "GIT_SSH_COMMAND".to_string(),
            "tddy-remote-git-repo --daemon-url http://d --session-token t".to_string()
        )])
    );
}

#[test]
fn an_ssh_clone_runs_with_the_passphrase_machinery_and_no_facilitator_shim() {
    let grant = CloneGrants::new().register("ada");
    let ssh = clone_ssh_env_for(&AgentAt("/s"), "mantas", Path::new("/h"), &grant).expect("agent");

    let env = environment_of(ClonePlan::Ssh(ssh));

    assert_eq!(env["SSH_ASKPASS"], "/h");
    assert!(!env["GIT_SSH_COMMAND"].contains("tddy-remote-git-repo"));
}
