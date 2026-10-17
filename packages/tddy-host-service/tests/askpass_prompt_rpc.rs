//! Integration tests for the daemon's askpass endpoint: what happens between "the clone's ssh asked
//! its helper for a passphrase" and "the helper got one".
//!
//! Everything ssh-specific is covered where ssh really runs, in
//! `tddy-agent-launch/tests/clone_askpass_acceptance.rs`. What is pinned here needs no ssh: whose
//! prompt it is, how long it waits, what it refuses, and — above all — that the decrypted answer
//! reaches nothing but the helper.
//!
//! Feature: docs/ft/daemon/clone-ssh-passphrase.md

use std::sync::{Arc, Mutex, Once};
use std::time::Duration;

use log::{Level, LevelFilter, Log, Metadata, Record};
use tddy_host_service::askpass::{AskpassEndpoint, AskpassOutcome, CloneGrant};
use tddy_host_service::host_keypair::{FileHostKeypair, HostKeypair};
use tddy_host_service::host_prompts::{
    AnswerRejection, HostPromptRegistry, InMemoryHostPromptRegistry, PromptKind,
};
use tddy_host_service::host_registry::now_unix_ms;
use tddy_host_service::test_util::encrypt_for_host;
use tddy_testing_commons::wait::eventually_awaiting;

const OPERATOR: &str = "ada";
const SECRET: &str = "correct horse battery staple";
const KEY_PROMPT: &str = "Enter passphrase for key '/home/ada/.ssh/id_rsa': ";

struct Host {
    prompts: Arc<InMemoryHostPromptRegistry>,
    keypair: Arc<FileHostKeypair>,
    endpoint: Arc<AskpassEndpoint>,
    _keys: tempfile::TempDir,
}

async fn a_host() -> Host {
    a_host_whose_prompts_wait(None).await
}

async fn a_host_whose_prompts_wait(window: Option<Duration>) -> Host {
    let keys = tempfile::tempdir().expect("a directory for the host keypair");
    let keypair = Arc::new(FileHostKeypair::new(keys.path()));
    let prompts = Arc::new(InMemoryHostPromptRegistry::new());
    let mut endpoint = AskpassEndpoint::start(prompts.clone(), keypair.clone())
        .await
        .expect("the endpoint binds a loopback port");
    if let Some(window) = window {
        endpoint = endpoint.answer_window(window);
    }
    Host {
        prompts,
        keypair,
        endpoint: Arc::new(endpoint),
        _keys: keys,
    }
}

impl Host {
    /// The clone's helper asks `prompt_text`, on a task, the way ssh's helper would.
    fn the_helper_asks(
        &self,
        grant: &CloneGrant,
        prompt_text: &'static str,
    ) -> tokio::task::JoinHandle<AskpassOutcome> {
        let (endpoint, token) = (Arc::clone(&self.endpoint), grant.token());
        tokio::spawn(async move { endpoint.ask(&token, prompt_text).await })
    }

    /// The prompt `user` is being shown, once there is one.
    async fn the_prompt_shown_to(
        &self,
        user: &str,
    ) -> tddy_host_service::host_prompts::PendingPrompt {
        eventually_awaiting("a prompt for the operator", Duration::from_secs(5), || {
            std::future::ready(
                self.prompts
                    .pending(user, now_unix_ms())
                    .into_iter()
                    .next()
                    .ok_or_else(|| "no prompt yet".to_string()),
            )
        })
        .await
    }

    /// `user` types `answer`, encrypted the way the browser encrypts it.
    fn answers(&self, prompt_id: &str, user: &str, answer: &str) -> Result<(), AnswerRejection> {
        let spki = self.keypair.published().expect("a published key").spki_der;
        self.prompts.answer(
            prompt_id,
            user,
            encrypt_for_host(&spki, answer.as_bytes()),
            now_unix_ms(),
        )
    }
}

// -- whose prompt it is ------------------------------------------------------------------------

#[tokio::test]
async fn a_helper_request_raises_one_passphrase_prompt_for_the_clones_operator() {
    // Given a clone started by ada
    let host = a_host().await;
    let grant = host.endpoint.register_clone(OPERATOR);

    // When its ssh asks the helper for a key's passphrase
    let asking = host.the_helper_asks(&grant, KEY_PROMPT);

    // Then ada is shown one passphrase prompt, naming the key ssh named
    let prompt = host.the_prompt_shown_to(OPERATOR).await;
    assert_eq!(prompt.kind, PromptKind::SshKeyPassphrase);
    assert_eq!(prompt.subject, "/home/ada/.ssh/id_rsa");
    // and what she types comes back to the helper, decrypted
    host.answers(&prompt.prompt_id, OPERATOR, SECRET)
        .expect("her own prompt takes her answer");
    match asking.await.expect("the request finished") {
        AskpassOutcome::Answer(answer) => assert_eq!(answer, SECRET.as_bytes()),
        other => panic!("the helper should have been handed the passphrase, got {other:?}"),
    }
}

#[tokio::test]
async fn another_operator_never_sees_the_prompt_and_cannot_answer_it() {
    // Given ada's clone has raised a prompt
    let host = a_host().await;
    let grant = host.endpoint.register_clone(OPERATOR);
    let asking = host.the_helper_asks(&grant, KEY_PROMPT);
    let prompt = host.the_prompt_shown_to(OPERATOR).await;

    // When grace — another operator on the same host — looks for it and tries to answer it
    let grace_sees = host.prompts.pending("grace", now_unix_ms());
    let grace_answers = host.answers(&prompt.prompt_id, "grace", "grace's guess");

    // Then she sees nothing, is refused as if it never existed, and ada's answer still counts
    assert!(grace_sees.is_empty());
    assert_eq!(grace_answers, Err(AnswerRejection::UnknownPrompt));
    host.answers(&prompt.prompt_id, OPERATOR, SECRET)
        .expect("ada's answer is unspent");
    assert!(matches!(
        asking.await.expect("finished"),
        AskpassOutcome::Answer(_)
    ));
}

// -- time and refusals -------------------------------------------------------------------------

#[tokio::test]
async fn an_unanswered_prompt_ends_the_request_as_expired_at_its_deadline() {
    // Given a host whose prompts wait 300 ms, and a clone nobody answers for
    let host = a_host_whose_prompts_wait(Some(Duration::from_millis(300))).await;
    let grant = host.endpoint.register_clone(OPERATOR);

    // When the helper asks
    let started = std::time::Instant::now();
    let outcome = host.endpoint.ask(&grant.token(), KEY_PROMPT).await;

    // Then it is told the prompt expired — promptly, not after the prompt's whole lifetime
    assert!(matches!(outcome, AskpassOutcome::Expired), "{outcome:?}");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "waited {:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn a_non_passphrase_prompt_text_is_refused_without_raising_a_prompt() {
    // Given a clone
    let host = a_host().await;
    let grant = host.endpoint.register_clone(OPERATOR);

    // When ssh's helper relays a host-key question instead of a passphrase request
    let outcome = host
        .endpoint
        .ask(
            &grant.token(),
            "Are you sure you want to continue connecting (yes/no/[fingerprint])? ",
        )
        .await;

    // Then it is refused, and ada was never shown anything
    assert!(matches!(outcome, AskpassOutcome::Refused(_)), "{outcome:?}");
    assert!(host.prompts.pending(OPERATOR, now_unix_ms()).is_empty());
}

#[tokio::test]
async fn a_token_the_daemon_never_issued_is_refused_without_raising_a_prompt() {
    // Given a host with a clone in flight
    let host = a_host().await;
    let _grant = host.endpoint.register_clone(OPERATOR);

    // When something that holds no clone's token asks
    let outcome = host.endpoint.ask("not-a-clone-token", KEY_PROMPT).await;

    // Then it is refused, and no operator is bothered on its behalf
    assert!(matches!(outcome, AskpassOutcome::Refused(_)), "{outcome:?}");
    assert!(host.prompts.pending(OPERATOR, now_unix_ms()).is_empty());
}

#[tokio::test]
async fn a_token_is_refused_once_its_clone_has_ended() {
    // Given a token that worked while its clone ran — ssh may ask more than once, e.g. after a
    // wrong passphrase, so the token is the clone's, not each question's
    let host = a_host().await;
    let grant = host.endpoint.register_clone(OPERATOR);
    let token = grant.token();
    let first = host.the_helper_asks(&grant, KEY_PROMPT);
    let prompt = host.the_prompt_shown_to(OPERATOR).await;
    host.answers(&prompt.prompt_id, OPERATOR, SECRET)
        .expect("answered");
    assert!(matches!(
        first.await.expect("finished"),
        AskpassOutcome::Answer(_)
    ));

    // When the clone ends
    drop(grant);
    let late = host.endpoint.ask(&token, KEY_PROMPT).await;

    // Then a helper that outlived it is refused
    assert!(matches!(late, AskpassOutcome::Refused(_)), "{late:?}");
}

// -- the secret --------------------------------------------------------------------------------

#[tokio::test]
async fn the_decrypted_answer_appears_in_no_log_line_or_error() {
    // Given every log line this process emits being recorded
    let recording = LogRecording::started();
    let host = a_host().await;
    let grant = host.endpoint.register_clone(OPERATOR);

    // When a passphrase goes through the endpoint end to end
    let asking = host.the_helper_asks(&grant, KEY_PROMPT);
    let prompt = host.the_prompt_shown_to(OPERATOR).await;
    host.answers(&prompt.prompt_id, OPERATOR, SECRET)
        .expect("answered");
    let outcome = asking.await.expect("finished");

    // Then it is in the outcome the helper receives, and nowhere that prints
    assert!(matches!(outcome, AskpassOutcome::Answer(_)));
    assert!(
        !format!("{outcome:?}").contains(SECRET),
        "the outcome's Debug output prints the passphrase"
    );
    recording.assert_nothing_recorded_contains(SECRET);
}

// -- telling the operator a passphrase was wrong ----------------------------------------------

impl Host {
    /// The helper asks about `prompt_text` and ada types `SECRET`; returns whether the daemon
    /// flagged the request as following a rejected answer.
    async fn asked_and_answered(&self, grant: &CloneGrant, prompt_text: &'static str) -> bool {
        let (endpoint, token) = (Arc::clone(&self.endpoint), grant.token());
        let asking =
            tokio::spawn(
                async move { endpoint.ask_reporting_rejection(&token, prompt_text).await },
            );
        let prompt = self.the_prompt_shown_to(OPERATOR).await;
        self.answers(&prompt.prompt_id, OPERATOR, SECRET)
            .expect("her own prompt takes her answer");
        let reply = asking.await.expect("the request finished");
        assert!(matches!(reply.outcome, AskpassOutcome::Answer(_)));
        reply.previous_answer_rejected
    }
}

#[tokio::test]
async fn a_second_request_for_the_same_key_is_flagged_as_following_a_rejected_answer() {
    // Given a clone whose ssh was already answered for a key
    let host = a_host().await;
    let grant = host.endpoint.register_clone(OPERATOR);
    host.asked_and_answered(&grant, KEY_PROMPT).await;

    // When ssh asks about that same key again, as it does after a wrong passphrase
    let flagged = host.asked_and_answered(&grant, KEY_PROMPT).await;

    // Then the reply says the previous answer was rejected
    assert!(flagged);
}

#[tokio::test]
async fn the_first_request_for_a_key_is_not_flagged() {
    let host = a_host().await;
    let grant = host.endpoint.register_clone(OPERATOR);

    let flagged = host.asked_and_answered(&grant, KEY_PROMPT).await;

    assert!(!flagged);
}

#[tokio::test]
async fn a_request_for_a_different_key_is_not_flagged() {
    // Given a clone whose ssh was already answered for one key
    let host = a_host().await;
    let grant = host.endpoint.register_clone(OPERATOR);
    host.asked_and_answered(&grant, KEY_PROMPT).await;

    // When ssh asks about another key
    let flagged = host
        .asked_and_answered(
            &grant,
            "Enter passphrase for key '/home/ada/.ssh/id_ed25519': ",
        )
        .await;

    // Then that is no retry
    assert!(!flagged);
}

#[tokio::test]
async fn another_clone_asking_about_the_same_key_is_not_flagged() {
    // Given one clone answered for a key
    let host = a_host().await;
    host.asked_and_answered(&host.endpoint.register_clone(OPERATOR), KEY_PROMPT)
        .await;

    // When a different clone asks about the same key
    let flagged = host
        .asked_and_answered(&host.endpoint.register_clone(OPERATOR), KEY_PROMPT)
        .await;

    // Then it is a first ask for that clone
    assert!(!flagged);
}

// -- a log recorder: nothing in this binary installs one, and the secret must reach none -------

static RECORDED: Mutex<Option<Arc<Mutex<Vec<String>>>>> = Mutex::new(None);
static INSTALL: Once = Once::new();
const MARKER: &str = "askpass-recording-live";

struct Recorder;

impl Log for Recorder {
    fn enabled(&self, _: &Metadata<'_>) -> bool {
        true
    }
    fn log(&self, record: &Record<'_>) {
        if let Some(buffer) = RECORDED.lock().expect("slot").as_ref() {
            buffer.lock().expect("buffer").push(format!(
                "{} {} {}",
                record.level(),
                record.target(),
                record.args()
            ));
        }
    }
    fn flush(&self) {}
}

struct LogRecording {
    lines: Arc<Mutex<Vec<String>>>,
}

impl LogRecording {
    fn started() -> Self {
        let lines = Arc::new(Mutex::new(Vec::new()));
        *RECORDED.lock().expect("slot") = Some(Arc::clone(&lines));
        INSTALL.call_once(|| {
            let _ = log::set_boxed_logger(Box::new(Recorder))
                .map(|()| log::set_max_level(LevelFilter::Trace));
        });
        log::log!(Level::Info, "{MARKER}");
        Self { lines }
    }

    /// Asserts `secret` reached no line — and that lines were recorded at all, so a recorder that
    /// failed to install cannot make this pass by capturing nothing.
    fn assert_nothing_recorded_contains(&self, secret: &str) {
        let lines = self.lines.lock().expect("buffer").clone();
        assert!(
            lines.iter().any(|l| l.contains(MARKER)),
            "nothing was recorded at all"
        );
        let leaked: Vec<_> = lines.iter().filter(|l| l.contains(secret)).collect();
        assert!(
            leaked.is_empty(),
            "the passphrase reached the logs: {leaked:?}"
        );
    }
}

impl Drop for LogRecording {
    fn drop(&mut self) {
        *RECORDED.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}
