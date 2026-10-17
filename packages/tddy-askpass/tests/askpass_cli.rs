//! The helper as ssh runs it: one argument in, the answer on stdout, an exit status out.
//!
//! Run against the daemon's real askpass endpoint rather than a stub, so the wire the helper speaks
//! is the one the daemon speaks and the two cannot drift. What ssh itself does with the helper is
//! covered where ssh really runs, in `tddy-agent-launch/tests/clone_askpass_acceptance.rs`.
//!
//! Feature: docs/ft/daemon/clone-ssh-passphrase.md

use std::process::{Command, Output};
use std::sync::Arc;
use std::time::Duration;

use tddy_host_service::askpass::{AskpassEndpoint, CloneGrant};
use tddy_host_service::host_keypair::{FileHostKeypair, HostKeypair};
use tddy_host_service::host_prompts::{HostPromptRegistry, InMemoryHostPromptRegistry};
use tddy_host_service::host_registry::now_unix_ms;
use tddy_host_service::test_util::encrypt_for_host;

const HELPER: &str = env!("CARGO_BIN_EXE_tddy-askpass");
const OPERATOR: &str = "ada";
const SECRET: &str = "correct horse battery staple";
const KEY_PROMPT: &str = "Enter passphrase for key '/home/ada/.ssh/id_rsa': ";

struct Daemon {
    /// Held so the server keeps serving: dropping the endpoint stops it.
    _endpoint: AskpassEndpoint,
    grant: CloneGrant,
    _keys: tempfile::TempDir,
}

/// A daemon whose operator types `answer` as soon as asked; `None` is an operator who never does.
async fn a_daemon_whose_operator(answer: Option<&'static str>, window: Duration) -> Daemon {
    let keys = tempfile::tempdir().expect("a directory for the host keypair");
    let keypair = Arc::new(FileHostKeypair::new(keys.path()));
    let prompts: Arc<dyn HostPromptRegistry> = Arc::new(InMemoryHostPromptRegistry::new());
    if let Some(answer) = answer {
        let (mut feed, prompts, keypair) =
            (prompts.subscribe(), Arc::clone(&prompts), keypair.clone());
        tokio::spawn(async move {
            while let Ok(prompt) = feed.recv().await {
                let spki = keypair.published().expect("a published key").spki_der;
                let _ = prompts.answer(
                    &prompt.prompt_id,
                    OPERATOR,
                    encrypt_for_host(&spki, answer.as_bytes()),
                    now_unix_ms(),
                );
            }
        });
    }
    let endpoint = AskpassEndpoint::start(Arc::clone(&prompts), keypair.clone())
        .await
        .expect("the endpoint binds a loopback port")
        .answer_window(window);
    let grant = endpoint.register_clone(OPERATOR);
    Daemon {
        _endpoint: endpoint,
        grant,
        _keys: keys,
    }
}

/// Run the helper the way ssh does: `SSH_ASKPASS` is a program that takes the prompt as `$1`.
async fn run_helper(prompt: &str, url: Option<&str>, token: Option<&str>) -> Output {
    let mut helper = Command::new(HELPER);
    helper.arg(prompt).env_clear();
    if let Some(url) = url {
        helper.env("TDDY_ASKPASS_URL", url);
    }
    if let Some(token) = token {
        helper.env("TDDY_ASKPASS_TOKEN", token);
    }
    tokio::task::spawn_blocking(move || helper.output().expect("the helper runs"))
        .await
        .expect("the helper task finished")
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("UTF-8 stdout")
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("UTF-8 stderr")
}

#[tokio::test(flavor = "multi_thread")]
async fn a_passphrase_request_prints_the_operators_answer_and_succeeds() {
    // Given an operator who will answer
    let daemon = a_daemon_whose_operator(Some(SECRET), Duration::from_secs(30)).await;

    // When ssh runs the helper with its passphrase question
    let output = run_helper(
        KEY_PROMPT,
        Some(&daemon.grant.url()),
        Some(&daemon.grant.token()),
    )
    .await;

    // Then the answer is on stdout, one line, for ssh to read — and nothing else is
    assert!(output.status.success(), "{}", stderr_of(&output));
    assert_eq!(stdout_of(&output), format!("{SECRET}\n"));
    assert_eq!(stderr_of(&output), "");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_host_key_question_fails_without_printing_anything() {
    let daemon = a_daemon_whose_operator(Some(SECRET), Duration::from_secs(30)).await;

    let output = run_helper(
        "Are you sure you want to continue connecting (yes/no/[fingerprint])? ",
        Some(&daemon.grant.url()),
        Some(&daemon.grant.token()),
    )
    .await;

    assert!(!output.status.success());
    assert_eq!(
        stdout_of(&output),
        "",
        "nothing ssh could mistake for an answer"
    );
    assert!(
        stderr_of(&output).contains("not a key passphrase"),
        "{}",
        stderr_of(&output)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unanswered_prompt_fails_saying_it_expired() {
    // Given an operator who never answers, and a short wait
    let daemon = a_daemon_whose_operator(None, Duration::from_millis(300)).await;

    // When the helper asks
    let output = run_helper(
        KEY_PROMPT,
        Some(&daemon.grant.url()),
        Some(&daemon.grant.token()),
    )
    .await;

    // Then it ends in failure naming the cause, with no answer on stdout
    assert!(!output.status.success());
    assert_eq!(stdout_of(&output), "");
    assert!(
        stderr_of(&output).contains("expired"),
        "{}",
        stderr_of(&output)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_token_the_daemon_does_not_know_fails_without_printing_anything() {
    let daemon = a_daemon_whose_operator(Some(SECRET), Duration::from_secs(30)).await;

    let output = run_helper(
        KEY_PROMPT,
        Some(&daemon.grant.url()),
        Some("not-a-clone-token"),
    )
    .await;

    assert!(!output.status.success());
    assert_eq!(stdout_of(&output), "");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_endpoint_variable_is_named_in_the_failure() {
    let output = run_helper(KEY_PROMPT, None, Some("token")).await;

    assert!(!output.status.success());
    assert!(
        stderr_of(&output).contains("TDDY_ASKPASS_URL"),
        "{}",
        stderr_of(&output)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_token_variable_is_named_in_the_failure() {
    let output = run_helper(KEY_PROMPT, Some("http://127.0.0.1:1"), None).await;

    assert!(!output.status.success());
    assert!(
        stderr_of(&output).contains("TDDY_ASKPASS_TOKEN"),
        "{}",
        stderr_of(&output)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unreachable_daemon_fails_naming_where_it_looked() {
    // Port 1 on loopback: nothing listens there
    let output = run_helper(KEY_PROMPT, Some("http://127.0.0.1:1"), Some("token")).await;

    assert!(!output.status.success());
    assert_eq!(stdout_of(&output), "");
    assert!(
        stderr_of(&output).contains("http://127.0.0.1:1"),
        "{}",
        stderr_of(&output)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_passphrase_reaches_stdout_and_no_other_channel() {
    let daemon = a_daemon_whose_operator(Some(SECRET), Duration::from_secs(30)).await;

    let output = run_helper(
        KEY_PROMPT,
        Some(&daemon.grant.url()),
        Some(&daemon.grant.token()),
    )
    .await;

    assert_eq!(stdout_of(&output), format!("{SECRET}\n"));
    assert!(
        !stderr_of(&output).contains(SECRET),
        "the helper's stderr is ssh's, and is logged"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_repeat_request_for_the_same_key_says_the_previous_passphrase_was_rejected() {
    // Given an operator who answers, and a clone whose ssh already got an answer for the key
    let daemon = a_daemon_whose_operator(Some(SECRET), Duration::from_secs(30)).await;
    let (url, token) = (daemon.grant.url(), daemon.grant.token());
    let first = run_helper(KEY_PROMPT, Some(&url), Some(&token)).await;

    // When ssh, finding that answer wrong, runs the helper again for the same key
    let second = run_helper(KEY_PROMPT, Some(&url), Some(&token)).await;

    // Then the first run said nothing, and the second names the rejected passphrase and the key
    assert_eq!(stderr_of(&first), "");
    let said = stderr_of(&second);
    assert!(said.contains("previous passphrase"), "{said}");
    assert!(said.contains("incorrect passphrase"), "{said}");
    assert!(said.contains("/home/ada/.ssh/id_rsa"), "{said}");
    // and the new answer is still on stdout, with the passphrase in no diagnostic
    assert_eq!(stdout_of(&second), format!("{SECRET}\n"));
    assert!(!said.contains(SECRET), "{said}");
}
