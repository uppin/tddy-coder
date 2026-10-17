//! Acceptance: a clone whose key needs a passphrase asks the operator for it, hands it to ssh, and
//! leaves the unlocked key in the OS user's agent.
//!
//! Runs real `git`, `ssh`, a user-mode `sshd` on a loopback port and a real `ssh-agent`: the
//! behaviours under test are ssh's own — which prompt text it emits, when it calls its askpass
//! helper, whether `AddKeysToAgent` fires — and a fake `ssh` would only encode our assumptions
//! about them. Only the operator is scripted, answering the way the browser does (RSA-OAEP under the
//! host's published key).
//!
//! Needs `sshd`, `ssh-keygen`, `ssh-agent` and a built `tddy-askpass`; `./test` provides them.
//!
//! Feature: docs/ft/daemon/clone-ssh-passphrase.md

mod support;

use std::path::PathBuf;
use std::time::Duration;

use support::rig::{a_clone_rig, OPERATOR};
use support::sshd::an_sshd;
use tddy_agent_launch::clone_ssh_env::{agent_socket_for, CloneAgentError};
use tddy_host_service::host_prompts::PromptKind;
use tddy_host_service::ssh_agent::AgentSocketResolver;

const PASSPHRASE: &str = "correct horse battery staple";

// -- the passphrase flow ---------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn a_passphrase_protected_key_is_asked_for_once_and_the_clone_completes() {
    // Given a server that only admits a passphrase-protected key, and an operator who knows it
    let sshd = an_sshd()
        .authorizing_a_passphrase_protected_key(PASSPHRASE)
        .started();
    let rig = a_clone_rig(sshd, PASSPHRASE).await;

    // When the repository is cloned
    let run = rig.clone_the_repository("first").await;

    // Then the clone is the served repository at its head, and the operator was asked exactly once
    assert!(
        run.is_checked_out_at(&rig.sshd.repo_head()),
        "the clone did not complete: {:?}",
        run.outcome
    );
    assert_eq!(
        rig.operator.prompts_raised().len(),
        1,
        "one passphrase, one prompt"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_prompt_is_a_passphrase_request_for_the_key_in_question_and_is_the_operators() {
    // Given a clone started by an operator, against a server that wants a passphrase-protected key
    let sshd = an_sshd()
        .authorizing_a_passphrase_protected_key(PASSPHRASE)
        .started();
    let rig = a_clone_rig(sshd, PASSPHRASE).await;

    // When the repository is cloned
    rig.clone_the_repository("first").await;

    // Then the one prompt asks for a key passphrase, names the key, and belongs to that operator
    let raised = rig.operator.prompts_raised();
    assert_eq!(raised.len(), 1, "{raised:?}");
    assert_eq!(raised[0].kind, PromptKind::SshKeyPassphrase);
    assert!(
        raised[0].subject.contains(rig.sshd.key_file_name()),
        "the prompt should name the key, not {:?}",
        raised[0].subject
    );
    assert_eq!(raised[0].issued_for, OPERATOR);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_key_path_longer_than_ssh_displays_still_raises_the_prompt() {
    // Given a key whose path is longer than the 100 characters ssh shows in its passphrase prompt
    let sshd = an_sshd()
        .authorizing_a_passphrase_protected_key(PASSPHRASE)
        .with_the_key_deep_under_the_home(6)
        .started();
    let rig = a_clone_rig(sshd, PASSPHRASE).await;

    // When the repository is cloned
    let run = rig.clone_the_repository("deep").await;

    // Then the truncated prompt is still recognised and answered
    assert!(
        run.is_checked_out_at(&rig.sshd.repo_head()),
        "a truncated prompt was not recognised: {:?}",
        run.outcome
    );
    assert_eq!(rig.operator.prompts_raised().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_unlocked_key_is_left_in_the_users_agent() {
    // Given an agent holding nothing
    let sshd = an_sshd()
        .authorizing_a_passphrase_protected_key(PASSPHRASE)
        .started();
    let rig = a_clone_rig(sshd, PASSPHRASE).await;
    assert!(
        rig.agent.fingerprints().is_empty(),
        "the agent starts empty"
    );

    // When the repository is cloned
    rig.clone_the_repository("first").await;

    // Then the agent holds exactly the key that was unlocked
    assert_eq!(rig.agent.fingerprints(), vec![rig.sshd.key_fingerprint()]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_second_clone_with_the_key_in_the_agent_raises_no_prompt() {
    // Given a first clone that left the key in the agent
    let sshd = an_sshd()
        .authorizing_a_passphrase_protected_key(PASSPHRASE)
        .started();
    let rig = a_clone_rig(sshd, PASSPHRASE).await;
    rig.clone_the_repository("first").await;

    // When the repository is cloned again
    let second = rig.clone_the_repository("second").await;

    // Then it completes, and the operator was not asked a second time
    assert!(
        second.is_checked_out_at(&rig.sshd.repo_head()),
        "{:?}",
        second.outcome
    );
    assert_eq!(rig.operator.prompts_raised().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_wrong_passphrase_fails_the_clone_and_is_not_echoed_anywhere() {
    // Given an operator who types the wrong passphrase
    const WRONG: &str = "definitely not the passphrase";
    let sshd = an_sshd()
        .authorizing_a_passphrase_protected_key(PASSPHRASE)
        .started();
    let rig = a_clone_rig(sshd, WRONG).await;

    // When the repository is cloned
    let run = rig.clone_the_repository("first").await;

    // Then the clone failed saying the passphrase was wrong, without repeating what was typed
    let failure = run.failure();
    assert!(
        failure.contains("incorrect passphrase"),
        "the failure should say the passphrase was rejected: {failure}"
    );
    assert!(
        !failure.contains(WRONG),
        "the typed passphrase leaked into the error: {failure}"
    );
    // and ssh's own retry policy bounded the asking (NumberOfPasswordPrompts is 3)
    let asked = rig.operator.prompts_raised().len();
    assert!((1..=3).contains(&asked), "asked {asked} times");
    assert!(
        !run.destination().exists(),
        "a failed clone left {} behind",
        run.destination().display()
    );
}

// -- everything else fails fast ----------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn a_host_key_confirmation_request_is_refused_and_the_clone_fails_in_seconds() {
    // Given a server whose host key the client has never seen, and is told to ask about
    let sshd = an_sshd()
        .authorizing_a_passphrase_protected_key(PASSPHRASE)
        .unknown_to_the_client()
        .started();
    let rig = a_clone_rig(sshd, PASSPHRASE).await;

    // When the repository is cloned
    let run = rig.clone_the_repository("first").await;

    // Then ssh's question about the host key was refused rather than put to the operator
    assert!(
        run.failure().contains("Host key verification failed"),
        "{}",
        run.failure()
    );
    assert!(
        rig.operator.prompts_raised().is_empty(),
        "a host-key question must not become a passphrase prompt"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unreachable_host_fails_within_the_connect_timeout() {
    // Given a server that accepts connections and never speaks SSH, and a 2 s connect timeout
    let silent = std::net::TcpListener::bind("127.0.0.1:0").expect("a silent listener");
    let port = silent.local_addr().expect("its address").port();
    let sshd = an_sshd()
        .authorizing_a_passphrase_protected_key(PASSPHRASE)
        .started();
    let rig = a_clone_rig(sshd, PASSPHRASE)
        .await
        .with_a_connect_timeout_of(Duration::from_secs(2));

    // When the repository is cloned from it
    let started = std::time::Instant::now();
    let run = rig
        .clone_from(&format!("ssh://ada@127.0.0.1:{port}/served.git"), "first")
        .await;

    // Then the clone gave up by itself, naming the timeout
    assert!(run.failure().contains("timed out"), "{}", run.failure());
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "gave up after {:?}, which is the old hang, not a connect timeout",
        started.elapsed()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_key_with_no_passphrase_that_the_server_rejects_fails_with_permission_denied() {
    // Given a client key the server does not authorize, and no passphrase to ask for
    let sshd = an_sshd().rejecting_the_clients_unprotected_key().started();
    let rig = a_clone_rig(sshd, PASSPHRASE).await;

    // When the repository is cloned
    let run = rig.clone_the_repository("first").await;

    // Then it fails as ssh fails, and nobody was asked anything
    assert!(
        run.failure().contains("Permission denied (publickey)"),
        "{}",
        run.failure()
    );
    assert!(rig.operator.prompts_raised().is_empty());
}

// -- the agent ---------------------------------------------------------------------------------

/// A resolver that looked and found no agent for anyone.
struct NoAgentAnywhere;

impl AgentSocketResolver for NoAgentAnywhere {
    fn socket_for(&self, _os_user: &str) -> Result<Option<PathBuf>, String> {
        Ok(None)
    }
}

#[test]
fn the_agent_lookup_names_the_os_user_when_it_finds_no_agent() {
    // Given an OS user with no ssh-agent anywhere the daemon can find

    // When the clone asks where the agent is — the planner then runs it without prompting
    // (see `clone_planner`'s tests); what it must not do is pass ssh an empty socket path
    let error = agent_socket_for(&NoAgentAnywhere, "ada").expect_err("there is nothing to name");

    // Then it is told so, by name — not handed an empty socket path for ssh to fail on later
    assert_eq!(
        error,
        CloneAgentError::NoAgentReachable {
            os_user: "ada".to_string()
        }
    );
    assert_eq!(error.to_string(), "no ssh-agent is reachable for ada");
}
