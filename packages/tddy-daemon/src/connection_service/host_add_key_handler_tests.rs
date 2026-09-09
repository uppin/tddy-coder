use super::*;
use crate::host_keypair::FileHostKeypair;
use crate::host_prompts::{InMemoryHostPromptRegistry, PendingPrompt};
use crate::ssh_agent_add::{AgentAddFailure, SshAgentKeyAdder};
use crate::test_util::{test_service, TEST_TOKEN, TEST_USER};
use log::{Level, LevelFilter, Log, Metadata, Record};
use ssh_key::PrivateKey;
use std::sync::{Mutex, Once};
use std::time::Duration;
use tddy_service::proto::connection::{AddHostKeyOutcome, HostKeyCandidate};

/// Distinctive, and used nowhere else in the workspace, so
/// [`the_passphrase_never_appears_in_captured_logs`] cannot be fooled by another test's output
/// and cannot accidentally match an unrelated log line.
const PASSPHRASE: &str = "orbital-thistle-9-quay";

/// What an operator types when they get it wrong. Also distinctive, because the refusal must
/// not quote it back either.
const WRONG_PASSPHRASE: &str = "meridian-hollow-4-tarn";

/// How long the whole flow gets before a test calls it hung. Generous: the only slow step is an
/// RSA keygen on first use.
const ADD_KEY_WINDOW: Duration = Duration::from_secs(10);

/// How long a test waits for `AddHostKey` to raise the prompt it answers.
const PROMPT_WINDOW: Duration = Duration::from_secs(5);
const PROMPT_POLL: Duration = Duration::from_millis(5);

/// How long a feed is watched before a test concludes nothing is coming down it.
///
/// Short on purpose, and safe to keep short: every prompt a test looks for is already
/// outstanding before the feed is opened, so a feed that would deliver it delivers it at once —
/// and this host's keypair is generated in the fixture, not inside the window.
const FEED_WINDOW: Duration = Duration::from_millis(500);

/// A second operator, with their own session on the same host.
///
/// A different GitHub user mapped to a different OS user, because that is what makes them
/// another operator rather than the same one with a second browser tab.
const OTHER_TOKEN: &str = "another-operators-token";
const OTHER_GITHUB_USER: &str = "otheroperator";

const TWO_OPERATOR_CONFIG: &str = r#"
users:
  - github_user: "testuser"
    os_user: "testdev"
  - github_user: "otheroperator"
    os_user: "otherdev"
"#;

/// Emitted the moment log recording starts, so an assertion about what is *absent* from the
/// logs can first prove the recorder was recording at all.
const RECORDING_MARKER: &str = "host-add-key log recording is live";

// -- the agent, and only the agent, is a double ---------------------------------------------

/// An ssh-agent that remembers every identity it was handed.
///
/// The fingerprint is what it records: it is what an operator sees in `ssh-add -l`, so
/// "the right key reached the agent" is stated in the terms the operator would check it in.
#[derive(Default)]
struct RecordingAgent {
    added: Mutex<Vec<String>>,
}

impl RecordingAgent {
    fn fingerprints_added(&self) -> Vec<String> {
        self.added
            .lock()
            .expect("the recording lock is only held to push a fingerprint")
            .clone()
    }
}

impl SshAgentKeyAdder for RecordingAgent {
    fn add_identity(&self, _os_user: &str, identity: &PrivateKey) -> Result<(), AgentAddFailure> {
        self.added
            .lock()
            .expect("the recording lock is only held to push a fingerprint")
            .push(identity.fingerprint(ssh_key::HashAlg::Sha256).to_string());
        Ok(())
    }
}

// -- the host under test --------------------------------------------------------------------

/// A host with one passphrase-protected key on disk, an agent that records what it is given,
/// and an operator standing by to answer the prompt.
struct HostWithAnEncryptedKey {
    service: ConnectionServiceImpl,
    prompts: Arc<InMemoryHostPromptRegistry>,
    keypair: Arc<FileHostKeypair>,
    agent: Arc<RecordingAgent>,
    /// The home directory of the OS user [`TEST_TOKEN`]'s operator is mapped to. Their key
    /// lives inside it, because that is the only place a key of theirs can live.
    home: PathBuf,
    key_path: PathBuf,
    /// The fingerprint the agent must end up holding.
    key_fingerprint: String,
    /// The key of the OS user [`OTHER_TOKEN`]'s operator is mapped to, in their own home — what
    /// makes "whose keys am I offered?" a question this host can answer wrongly.
    other_key_path: PathBuf,
    /// Owns the key file and the daemon's data directory for the life of the test.
    _storage: tempfile::TempDir,
}

/// A service on which two different GitHub users each have a session.
///
/// `test_service` knows one operator, which cannot express the question these tests ask: whose
/// prompt is this? Everything else about it is the same wiring.
fn a_service_serving_two_operators(storage: &Path) -> ConnectionServiceImpl {
    let config_path = storage.join("two-operator-config.yaml");
    std::fs::write(&config_path, TWO_OPERATOR_CONFIG).expect("a config for this host");
    let config = DaemonConfig::load(&config_path).expect("a loadable config");
    let sessions_base = storage.to_path_buf();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let user_resolver: SessionUserResolver = Arc::new(|token| match token {
        TEST_TOKEN => Some(TEST_USER.to_string()),
        OTHER_TOKEN => Some(OTHER_GITHUB_USER.to_string()),
        _ => None,
    });
    ConnectionServiceImpl::new(
        config,
        sessions_base_resolver,
        storage.to_path_buf(),
        user_resolver,
        None,
        None,
        None,
        Arc::new(CliSessionManager::new()),
    )
}

fn a_host_with_an_encrypted_key(passphrase: &str) -> HostWithAnEncryptedKey {
    a_host_where(passphrase, |files| files)
}

/// The same host, with its one un-stageable seam adjusted: `adjust` receives the OS-file reader
/// the fixture would have used and returns the one this host gets.
///
/// Parameterised for exactly one reason — impersonating an OS user, and failing to, is the step
/// a test process cannot perform for real without depending on who it happens to be running
/// as. Everything else about this host is the real thing.
fn a_host_where(
    passphrase: &str,
    adjust: impl FnOnce(
        crate::host_private_key::UserFilesUnder,
    ) -> crate::host_private_key::UserFilesUnder,
) -> HostWithAnEncryptedKey {
    let storage = tempfile::tempdir().expect("a temp directory for this host");
    // A home directory for the mapped OS user, with the key inside it: an operator's private
    // key is a file in their own home, and a fixture that put it anywhere else would be
    // exercising a path no real add-key flow can take.
    let home = storage.path().join("home").join("testdev");
    std::fs::create_dir_all(home.join(".ssh")).expect("this operator has a ~/.ssh");
    let key_path = home.join(".ssh").join("id_ed25519");
    let key_fingerprint = an_encrypted_private_key_at(&key_path, passphrase);
    // The second operator's own key, in their own home. Two operators with a key each is the
    // only arrangement in which offering the wrong one is visible.
    let other_home = storage.path().join("home").join("otherdev");
    std::fs::create_dir_all(other_home.join(".ssh")).expect("they have a ~/.ssh too");
    let other_key_path = other_home.join(".ssh").join("id_ed25519");
    an_encrypted_private_key_at(&other_key_path, passphrase);

    let prompts = Arc::new(InMemoryHostPromptRegistry::new());
    let keypair = Arc::new(FileHostKeypair::new(storage.path()));
    let agent = Arc::new(RecordingAgent::default());
    let service = a_service_serving_two_operators(storage.path())
        .with_host_prompts(Arc::clone(&prompts) as Arc<dyn HostPromptRegistry>)
        .with_host_keypair(Arc::clone(&keypair) as Arc<dyn HostKeypair>)
        .with_ssh_agent_key_adder(Arc::clone(&agent) as Arc<dyn SshAgentKeyAdder>)
        // Impersonating an OS user is not something a test can do; the home directory it
        // reports is the real confinement's own input.
        .with_host_user_files(Arc::new(adjust(
            crate::host_private_key::UserFilesUnder::home(&home)
                .and_the_home_of("otherdev", &other_home),
        )));
    // Generated up front so no test's timing window has to cover an RSA keygen: the prompt feed
    // reads the published key per event, and the first read is the one that makes the key.
    keypair
        .published()
        .expect("this host can publish a key for its prompts");

    HostWithAnEncryptedKey {
        service,
        prompts,
        keypair,
        agent,
        home,
        key_path,
        key_fingerprint,
        other_key_path,
        _storage: storage,
    }
}

impl HostWithAnEncryptedKey {
    /// The request an add makes for this operator's own key, so each test states only the field
    /// it is about.
    fn an_add_of_their_key(&self) -> AddHostKeyRequest {
        self.an_add_of(&self.key_path)
    }

    /// The request an add makes for a key the operator names themselves, which is what the
    /// field is: free text from a browser.
    fn an_add_of(&self, subject: &Path) -> AddHostKeyRequest {
        AddHostKeyRequest {
            session_token: TEST_TOKEN.to_string(),
            daemon_instance_id: String::new(),
            subject: subject.display().to_string(),
        }
    }

    /// Run the whole flow: start the add, answer the prompt it raises with `passphrase`, and
    /// report what the add concluded.
    async fn add_key_answering_with(&self, passphrase: &str) -> AddHostKeyResponse {
        self.add_key_answered_with(self.an_answer_carrying(passphrase))
            .await
    }

    /// [`Self::add_key_answering_with`] for an answer that is not this host's to read — the
    /// only way to reach the decrypt failure, which no passphrase can produce.
    async fn add_key_answered_with(&self, ciphertext: Vec<u8>) -> AddHostKeyResponse {
        self.add_key_answering(self.an_add_of_their_key(), ciphertext)
            .await
    }

    /// [`Self::add_key_answering_with`] for a key at a path of the operator's choosing.
    async fn add_key_at(&self, subject: &Path, passphrase: &str) -> AddHostKeyResponse {
        self.add_key_answering(self.an_add_of(subject), self.an_answer_carrying(passphrase))
            .await
    }

    /// Run `request`, answering the prompt it raises with `ciphertext`.
    ///
    /// The two halves run concurrently because that is the shape of the real flow — `AddHostKey`
    /// is still in flight when the operator's answer arrives on a different call.
    async fn add_key_answering(
        &self,
        request: AddHostKeyRequest,
        ciphertext: Vec<u8>,
    ) -> AddHostKeyResponse {
        let started = self.service.add_host_key(Request::new(request));

        let (added, _answered) = tokio::time::timeout(ADD_KEY_WINDOW, async {
            tokio::join!(started, self.answer_the_prompt_with(ciphertext))
        })
        .await
        .expect("the add settles once its prompt has been answered");

        added.expect("a valid session may add a key").into_inner()
    }

    /// Answer whatever prompt the add raises with `ciphertext`, on the session that raised it.
    async fn answer_the_prompt_with(&self, ciphertext: Vec<u8>) -> AnswerHostPromptResponse {
        let prompt = self.prompt_awaiting_an_answer().await;
        self.answer_as(TEST_TOKEN, &prompt.prompt_id, ciphertext)
            .await
    }

    /// Put this host in the state of waiting on a question `github_user` raised.
    ///
    /// Straight through the registry rather than through `AddHostKey`, so the prompt is
    /// outstanding *before* a feed is opened — which is what makes every assertion below about
    /// what does or does not come down that feed a deterministic one.
    fn raise_a_prompt_for(&self, github_user: &str) -> PendingPrompt {
        self.prompts.issue(
            github_user,
            PromptKind::SshKeyPassphrase,
            &self.key_path.display().to_string(),
            crate::host_registry::now_unix_ms(),
        )
    }

    /// The first prompt this host puts on `token`'s feed, or `None` if it stays silent.
    async fn first_prompt_on_the_feed_of(&self, token: &str) -> Option<HostPromptEvent> {
        let mut feed = self
            .service
            .stream_host_prompts(Request::new(StreamHostPromptsRequest {
                session_token: token.to_string(),
                daemon_instance_id: String::new(),
            }))
            .await
            .expect("a valid session subscribes to this host's prompts")
            .into_inner();
        tokio::time::timeout(FEED_WINDOW, feed.next())
            .await
            .ok()
            .flatten()
            .map(|frame| frame.expect("a prompt frame rather than a mid-stream error"))
    }

    /// Submit `ciphertext` as the answer to `prompt_id`, on `token`'s session.
    async fn answer_as(
        &self,
        token: &str,
        prompt_id: &str,
        ciphertext: Vec<u8>,
    ) -> AnswerHostPromptResponse {
        self.service
            .answer_host_prompt(Request::new(AnswerHostPromptRequest {
                session_token: token.to_string(),
                daemon_instance_id: String::new(),
                prompt_id: prompt_id.to_string(),
                encrypted_answer: ciphertext,
            }))
            .await
            .expect("a valid session may answer a prompt")
            .into_inner()
    }

    /// The passphrase, encrypted under this host's published key the way the browser would.
    fn an_answer_carrying(&self, passphrase: &str) -> Vec<u8> {
        let published = self
            .keypair
            .published()
            .expect("this host publishes a key with its prompts");
        encrypted_for(&published.spki_der, passphrase.as_bytes())
    }

    /// The prompt `AddHostKey` raises for [`TEST_USER`], once it has raised one.
    async fn prompt_awaiting_an_answer(&self) -> PendingPrompt {
        let deadline = tokio::time::Instant::now() + PROMPT_WINDOW;
        while tokio::time::Instant::now() < deadline {
            let outstanding = self
                .prompts
                .pending(TEST_USER, crate::host_registry::now_unix_ms());
            if let Some(prompt) = outstanding.into_iter().next() {
                return prompt;
            }
            tokio::time::sleep(PROMPT_POLL).await;
        }
        panic!(
            "AddHostKey never raised a prompt, so there is no way for an operator to supply \
                 the passphrase it needs"
        );
    }

    /// Assert `secret` reached no byte of any file this host has — and that the walk found
    /// something to inspect, so a walk that read nothing cannot make this pass.
    ///
    /// One temp directory holds both the operator's key file and the daemon's own data
    /// directory, so a single walk covers every place this flow could spill a passphrase to.
    fn assert_nothing_on_disk_contains(&self, secret: &str) {
        let files = every_file_under(self._storage.path());
        assert!(
            !files.is_empty(),
            "the walk inspected no files at all, so this assertion would hold for a host \
                 that wrote the passphrase into every one of them"
        );
        let leaked: Vec<&PathBuf> = files
            .iter()
            .filter(|(_, bytes)| contains_bytes(bytes, secret.as_bytes()))
            .map(|(path, _)| path)
            .collect();
        assert!(
            leaked.is_empty(),
            "the passphrase was written to this host's disk: {leaked:?}"
        );
    }
}

// -- fixtures -------------------------------------------------------------------------------

/// A freshly generated ed25519 key, locked with `passphrase` and written to `path`. Returns the
/// `SHA256:` fingerprint the agent must end up holding.
///
/// Generated rather than committed: key material checked into a repository is key material in
/// every clone of it, and a fixture that never changes is a fixture someone eventually trusts.
fn an_encrypted_private_key_at(path: &Path, passphrase: &str) -> String {
    let key = PrivateKey::random(&mut rand::thread_rng(), ssh_key::Algorithm::Ed25519)
        .expect("an ed25519 private key");
    let fingerprint = key.fingerprint(ssh_key::HashAlg::Sha256).to_string();
    let locked = key
        .encrypt(&mut rand::thread_rng(), passphrase)
        .expect("a passphrase-protected copy of it");
    let openssh = locked
        .to_openssh(ssh_key::LineEnding::LF)
        .expect("in the format ssh-keygen writes");
    std::fs::write(path, openssh.as_bytes()).expect("the key file is written");
    // The public half beside it, because that is what `ssh-keygen` leaves on disk and what a
    // listing describes a candidate from. A fixture without it would make every key on this
    // host invisible to the picker while still addable by path — a state no real host is in.
    let public_line = key
        .public_key()
        .to_openssh()
        .expect("in the one-line format ssh-keygen writes a .pub in");
    let mut pub_name = path.file_name().expect("a key file name").to_os_string();
    pub_name.push(".pub");
    std::fs::write(path.with_file_name(pub_name), format!("{public_line}\n"))
        .expect("the public half is written beside it");
    fingerprint
}

/// A well-formed RSA-OAEP answer addressed to a **different** host, which this one therefore
/// cannot decrypt. The only way to reach the decrypt failure: no passphrase, right or wrong,
/// produces it.
fn a_ciphertext_for_another_host() -> Vec<u8> {
    let elsewhere = tempfile::tempdir().expect("a temp directory for another host");
    let published = FileHostKeypair::new(elsewhere.path())
        .published()
        .expect("another host's published key");
    encrypted_for(&published.spki_der, PASSPHRASE.as_bytes())
}

/// A payload that is not an answer to anything — what somebody who only wants to spend another
/// operator's single-use prompt would send, having no passphrase to offer.
fn a_meaningless_answer() -> Vec<u8> {
    vec![0xde, 0xad, 0xbe, 0xef]
}

/// Encrypt with RSA-OAEP(SHA-256) against an SPKI DER, standing in for the browser's
/// `SubtleCrypto`.
///
/// Goes through the `rsa` crate's public API rather than through anything in this workspace, so
/// what the handler decrypts is pinned to the *format* the browser produces rather than to our
/// own agreement with ourselves.
fn encrypted_for(spki_der: &[u8], plaintext: &[u8]) -> Vec<u8> {
    use rsa::pkcs8::DecodePublicKey;
    use rsa::{Oaep, RsaPublicKey};

    RsaPublicKey::from_public_key_der(spki_der)
        .expect("a published SPKI DER public key")
        .encrypt(
            &mut rand::thread_rng(),
            Oaep::new::<sha2::Sha256>(),
            plaintext,
        )
        .expect("a passphrase fits comfortably in an OAEP payload")
}

/// Every file under `root`, recursively, paired with its bytes.
///
/// Symlinks are stepped over rather than followed: what is being inspected is the bytes this
/// host wrote, and a link would either re-read a file already in the list or lead outside the
/// directory the host owns.
fn every_file_under(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut found = Vec::new();
    let entries = std::fs::read_dir(root).expect("a readable directory on this host");
    for entry in entries {
        let entry = entry.expect("a readable entry in this host's directory");
        let kind = entry.file_type().expect("the entry's kind is readable");
        let path = entry.path();
        if kind.is_dir() {
            found.extend(every_file_under(&path));
        } else if kind.is_file() {
            let bytes = std::fs::read(&path).expect("a readable file on this host");
            found.push((path, bytes));
        }
    }
    found
}

/// Whether `needle` appears anywhere in `haystack`, byte for byte.
///
/// Over bytes rather than text, because a file that is not valid UTF-8 is exactly where a
/// secret would hide from a search that decoded first.
fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

// -- assertions -----------------------------------------------------------------------------

trait AddHostKeyAssertions {
    fn assert_added_the_key(&self, fingerprint: &str) -> &Self;
    fn assert_refused_with(&self, outcome: AddHostKeyOutcome) -> &Self;
    fn assert_says_nothing_about(&self, secret: &str) -> &Self;
}

impl AddHostKeyAssertions for AddHostKeyResponse {
    fn assert_added_the_key(&self, fingerprint: &str) -> &Self {
        assert!(
            self.added,
            "the add reported failure: {}",
            self.failure_reason
        );
        assert_eq!(
            self.outcome,
            AddHostKeyOutcome::Added as i32,
            "a successful add reports ADDED"
        );
        assert_eq!(
            self.fingerprint, fingerprint,
            "the response names the key that was loaded"
        );
        self
    }

    fn assert_refused_with(&self, outcome: AddHostKeyOutcome) -> &Self {
        assert!(!self.added, "no key should have been added");
        assert_eq!(
            self.outcome, outcome as i32,
            "the operator is told which of the failures this was, got: {}",
            self.failure_reason
        );
        self
    }

    fn assert_says_nothing_about(&self, secret: &str) -> &Self {
        assert!(
            !self.failure_reason.contains(secret),
            "the failure reason quoted the passphrase back: {}",
            self.failure_reason
        );
        self
    }
}

// -- log recording --------------------------------------------------------------------------

static INSTALL_RECORDER: Once = Once::new();
static RECORDED: Mutex<Option<Arc<Mutex<Vec<String>>>>> = Mutex::new(None);

/// Records every log line this process emits, whatever its target or level.
///
/// Deliberately unfiltered: a passphrase reaching a log is a leak wherever it surfaces, and a
/// recorder scoped to the modules we expected to be careless would miss the ones we did not.
struct EverythingRecorder;

impl Log for EverythingRecorder {
    fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &Record<'_>) {
        let line = format!("{} {} {}", record.level(), record.target(), record.args());
        let slot = RECORDED
            .lock()
            .expect("the recorder slot is never poisoned");
        if let Some(buffer) = slot.as_ref() {
            buffer
                .lock()
                .expect("the recording buffer is only held to push a line")
                .push(line);
        }
    }

    fn flush(&self) {}
}

/// One test's log recording, live for as long as the guard is held.
///
/// A guard, because the recorder itself can never be uninstalled — `log` takes one logger per
/// process, for the life of it. A recording left in the slot afterwards would go on collecting
/// every other test in this binary into a leaked buffer, behind one global mutex, for the rest
/// of the run; dropping the guard empties the slot, and the installed recorder then has nothing
/// to push to.
struct LogRecording {
    lines: Arc<Mutex<Vec<String>>>,
}

impl LogRecording {
    /// Start recording every log line this process emits, and prove the recording is live by
    /// putting a marker through it.
    fn started() -> Self {
        let lines = Arc::new(Mutex::new(Vec::new()));
        let displaced = RECORDED
            .lock()
            .expect("the recorder slot is never poisoned")
            .replace(Arc::clone(&lines));
        // One slot, so two recordings at once would each capture a fraction of the other's
        // output — and these tests run in parallel with the rest of the binary. Stated as a
        // failure rather than left to whichever assertion happens to lose its lines.
        assert!(
            displaced.is_none(),
            "a log recording was already live, so neither it nor this one records reliably; \
                 the recorder holds a single slot and cannot serve two tests at once"
        );
        INSTALL_RECORDER.call_once(|| {
            let _ = log::set_boxed_logger(Box::new(EverythingRecorder))
                .map(|()| log::set_max_level(LevelFilter::Trace));
        });
        log::log!(Level::Info, "{RECORDING_MARKER}");
        Self { lines }
    }

    /// Assert `secret` reached no recorded line — and that anything at all was recorded, so a
    /// recorder that failed to install cannot make this pass by capturing nothing.
    fn assert_nothing_recorded_contains(&self, secret: &str) {
        let lines = self
            .lines
            .lock()
            .expect("the recording buffer is only held to read it")
            .clone();
        assert!(
            lines.iter().any(|line| line.contains(RECORDING_MARKER)),
            "nothing was recorded at all, so this assertion would hold for a daemon that \
                 logged the passphrase on every line"
        );
        let leaked: Vec<&String> = lines.iter().filter(|line| line.contains(secret)).collect();
        assert!(
            leaked.is_empty(),
            "the passphrase reached the daemon's logs: {leaked:?}"
        );
    }
}

impl Drop for LogRecording {
    fn drop(&mut self) {
        // Poison is stepped over rather than unwrapped: this drop runs while a failing test is
        // already unwinding, and a second panic here would abort the whole test binary —
        // taking the failure that was being reported with it.
        *RECORDED
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }
}

// -- the tests ------------------------------------------------------------------------------

/// The reply carries the ciphertext of a secret, so an unauthenticated caller must not be able
/// to submit one — nor to discover, by the shape of the answer, whether a prompt id exists.
#[tokio::test]
async fn answer_host_prompt_rejects_an_invalid_token() {
    // Given a host serving the add-key endpoints
    let dir = tempfile::tempdir().expect("a temp directory for this host");
    let service = test_service(dir.path().to_path_buf());

    // When an answer arrives on a session this host does not know
    let refused = service
        .answer_host_prompt(Request::new(AnswerHostPromptRequest {
            session_token: "not-a-token".to_string(),
            daemon_instance_id: String::new(),
            prompt_id: "any-prompt".to_string(),
            encrypted_answer: vec![1, 2, 3],
        }))
        .await;

    // Then
    assert_eq!(
        refused
            .expect_err("an invalid session must be refused")
            .code,
        tddy_rpc::Code::Unauthenticated
    );
}

/// The round trip this node exists for: a prompt is raised, the answer travels back encrypted
/// under this host's published key, the host decrypts it, unlocks a real passphrase-protected
/// key with it, and the identity reaches the agent.
#[tokio::test]
async fn a_correct_passphrase_adds_the_key_to_the_agent() {
    // Given a host holding a key locked with a passphrase the operator knows
    let host = a_host_with_an_encrypted_key(PASSPHRASE);

    // When the operator answers the prompt with it
    let reported = host.add_key_answering_with(PASSPHRASE).await;

    // Then
    reported.assert_added_the_key(&host.key_fingerprint);
    assert_eq!(
        host.agent.fingerprints_added(),
        vec![host.key_fingerprint.clone()],
        "the unlocked identity must actually reach the agent — an add that reports success \
             while handing the agent nothing is exactly the failure this test exists to catch"
    );
}

/// A wrong passphrase is an ordinary mistake, not an error: the operator is told which failure
/// it was so they can retry, and the agent is left exactly as it was.
#[tokio::test]
async fn an_incorrect_passphrase_reports_failure_and_adds_nothing() {
    // Given a host holding a key locked with a passphrase the operator mistypes
    let host = a_host_with_an_encrypted_key(PASSPHRASE);

    // When they answer with the wrong one
    let reported = host.add_key_answering_with(WRONG_PASSPHRASE).await;

    // Then
    reported
        .assert_refused_with(AddHostKeyOutcome::WrongPassphrase)
        .assert_says_nothing_about(WRONG_PASSPHRASE);
    assert_eq!(
        host.agent.fingerprints_added(),
        Vec::<String>::new(),
        "a passphrase that did not unlock the key must leave the agent holding nothing new"
    );
}

/// The whole point of encrypting the answer is undone if the host then writes the plaintext to
/// its own log. A stray `debug!` on the decrypt path is precisely how such a secret escapes,
/// and it is invisible to every other test here.
#[tokio::test]
async fn the_passphrase_never_appears_in_captured_logs() {
    // Given every log line this process emits being recorded
    let recording = LogRecording::started();
    let host = a_host_with_an_encrypted_key(PASSPHRASE);

    // When the whole flow runs, from raising the prompt to loading the key
    let reported = host.add_key_answering_with(PASSPHRASE).await;

    // Then
    reported.assert_added_the_key(&host.key_fingerprint);
    recording.assert_nothing_recorded_contains(PASSPHRASE);
}

/// The other half of the same promise. Encrypting the answer buys nothing if the host then
/// leaves the plaintext in a scratch file, a cache or a log file under its own data directory:
/// a passphrase on disk outlives the session that used it, and the operator has no way to know
/// it is there.
#[tokio::test]
async fn the_passphrase_is_never_written_to_disk() {
    // Given a host holding a key locked with a passphrase the operator knows
    let host = a_host_with_an_encrypted_key(PASSPHRASE);

    // When the whole flow runs, from raising the prompt to loading the key
    let reported = host.add_key_answering_with(PASSPHRASE).await;

    // Then
    reported.assert_added_the_key(&host.key_fingerprint);
    host.assert_nothing_on_disk_contains(PASSPHRASE);
}

// -- whose prompt is this? ------------------------------------------------------------------

/// The feed must actually carry the prompt to the operator waiting on it — the guard that stops
/// every "another operator sees nothing" assertion below from holding for a feed that carries
/// nothing to anyone.
#[tokio::test]
async fn streams_an_outstanding_prompt_to_the_operator_who_raised_it() {
    // Given a host waiting on a question this operator raised
    let host = a_host_with_an_encrypted_key(PASSPHRASE);
    let raised = host.raise_a_prompt_for(TEST_USER);

    // When they open this host's prompt feed
    let seen = host.first_prompt_on_the_feed_of(TEST_TOKEN).await;

    // Then
    assert_eq!(
        seen.map(|event| event.prompt_id),
        Some(raised.prompt_id),
        "the operator who raised the prompt must be shown it, or nothing can ever answer it"
    );
}

/// A prompt belongs to the session that raised it. Replayed to every subscriber, it shows
/// operator B the dialog A opened — disclosing the private-key path A named — and hands B a
/// prompt they can burn.
#[tokio::test]
async fn does_not_stream_a_prompt_to_an_operator_who_did_not_raise_it() {
    // Given a host waiting on a question one operator raised
    let host = a_host_with_an_encrypted_key(PASSPHRASE);
    host.raise_a_prompt_for(TEST_USER);

    // When a different operator opens this host's prompt feed
    let seen = host.first_prompt_on_the_feed_of(OTHER_TOKEN).await;

    // Then
    assert_eq!(
        seen, None,
        "another operator was shown this prompt — with it the private-key path it names, and \
             the chance to burn a single-use prompt that is not theirs"
    );
}

/// Refusing a stranger's answer must not double as a lookup service: "that prompt is not
/// yours" and "there is no such prompt" have to read identically, or the endpoint tells any
/// authenticated caller which prompt ids are live.
#[tokio::test]
async fn an_answer_from_another_operator_is_refused_as_an_unknown_prompt_would_be() {
    // Given a host waiting on a question one operator raised
    let host = a_host_with_an_encrypted_key(PASSPHRASE);
    let raised = host.raise_a_prompt_for(TEST_USER);

    // When a different operator answers it, and answers a prompt id that was never issued
    let for_someone_elses = host
        .answer_as(OTHER_TOKEN, &raised.prompt_id, a_meaningless_answer())
        .await;
    let for_a_prompt_that_never_existed = host
        .answer_as(OTHER_TOKEN, "never-issued", a_meaningless_answer())
        .await;

    // Then
    assert!(
        !for_someone_elses.accepted,
        "no key was being unlocked here"
    );
    assert_eq!(
        for_someone_elses, for_a_prompt_that_never_existed,
        "the two refusals differ, so a caller can tell a live prompt id from a fictional one"
    );
}

/// A prompt answers exactly once. A stranger who can spend that one answer denies the operator
/// who raised it their add for the whole of the prompt's TTL.
#[tokio::test]
async fn an_answer_from_another_operator_leaves_the_prompt_answerable_by_its_owner() {
    // Given a host waiting on a question one operator raised
    let host = a_host_with_an_encrypted_key(PASSPHRASE);
    let raised = host.raise_a_prompt_for(TEST_USER);

    // When a different operator answers it first, and then its owner does
    host.answer_as(OTHER_TOKEN, &raised.prompt_id, a_meaningless_answer())
        .await;
    let by_its_owner = host
        .answer_as(
            TEST_TOKEN,
            &raised.prompt_id,
            host.an_answer_carrying(PASSPHRASE),
        )
        .await;

    // Then
    assert!(
        by_its_owner.accepted,
        "a stranger consumed this operator's single-use prompt: {}",
        by_its_owner.rejection_reason
    );
}

/// **An adaptive decryption oracle.** A response that distinguishes "this host could not
/// decrypt your answer" from "your passphrase did not unlock the key" gives any authenticated
/// session one clean bit per chosen ciphertext against this host's long-lived RSA key — the
/// input a Manger-style attack on RSA-OAEP runs on, and `AddHostKey`→`AnswerHostPrompt` is a
/// loop anyone with a session can drive. The two outcomes must be indistinguishable from
/// outside.
#[tokio::test]
async fn an_answer_this_host_cannot_decrypt_is_refused_exactly_as_a_wrong_passphrase_is() {
    // Given a host holding a key locked with a passphrase
    let host = a_host_with_an_encrypted_key(PASSPHRASE);

    // When one add is answered with the wrong passphrase, and another with a ciphertext this
    // host has no key for
    let mistyped = host.add_key_answering_with(WRONG_PASSPHRASE).await;
    let undecryptable = host
        .add_key_answered_with(a_ciphertext_for_another_host())
        .await;

    // Then
    assert_eq!(
        undecryptable, mistyped,
        "the response says whether the ciphertext decrypted, which is one bit per query \
             against this host's RSA key"
    );
}

/// Which arm the indistinguishable refusal actually *is*.
///
/// [`an_answer_this_host_cannot_decrypt_is_refused_exactly_as_a_wrong_passphrase_is`] pins that
/// the two responses match; identical to each other would hold just as well if both reported
/// `UNSPECIFIED`. The arm is what the browser turns into a sentence, and the only sentence that
/// is any use here is the one that says "type it again" — an operator sent anywhere else is
/// hunting for a fault in a passphrase that was never the problem.
#[tokio::test]
async fn an_answer_this_host_cannot_decrypt_reports_the_wrong_passphrase_arm() {
    // Given a host holding a key locked with a passphrase
    let host = a_host_with_an_encrypted_key(PASSPHRASE);

    // When the answer is a well-formed ciphertext addressed to a different host
    let refused = host
        .add_key_answered_with(a_ciphertext_for_another_host())
        .await;

    // Then
    refused.assert_refused_with(AddHostKeyOutcome::WrongPassphrase);
}

// -- whose key is this, and where does it live? ---------------------------------------------

/// **A file oracle.** `subject` is free text from a browser, and the two failures the read can
/// produce — nothing at that path, and something that is not a private key — used to come back
/// as two different sentences in `failure_reason`. Distinguishable, they answer "does this file
/// exist on your host?" for any path the caller cares to name.
#[tokio::test]
async fn a_key_that_is_absent_and_a_key_that_is_malformed_are_refused_in_the_same_words() {
    // Given a path in this operator's home with nothing at it
    let host = a_host_with_an_encrypted_key(PASSPHRASE);
    let named = host.home.join(".ssh").join("maybe-a-key");

    // When they name it while nothing is there, and again once something that is not a key is
    let while_absent = host.add_key_at(&named, PASSPHRASE).await;
    std::fs::write(&named, "ssh-ed25519 AAAA... not a private key\n")
        .expect("something that is not a private key");
    let while_malformed = host.add_key_at(&named, PASSPHRASE).await;

    // Then
    assert_eq!(
        while_malformed, while_absent,
        "the refusal says whether the file exists, which answers that question for any path \
             on this host"
    );
}

/// The arm behind that shared wording, for the absent case.
///
/// The test above pins the two refusals as identical, which says nothing about what they say:
/// two responses that both reported `WRONG_PASSPHRASE` would satisfy it, and would send an
/// operator who mistyped a *path* off to retype a passphrase that was correct.
#[tokio::test]
async fn reports_the_key_as_unreadable_when_nothing_is_at_the_path_the_operator_named() {
    // Given a path inside this operator's home with no file at it
    let host = a_host_with_an_encrypted_key(PASSPHRASE);
    let named = host.home.join(".ssh").join("id_ed25519.bak");

    // When they name it
    let refused = host.add_key_at(&named, PASSPHRASE).await;

    // Then
    refused.assert_refused_with(AddHostKeyOutcome::KeyUnreadable);
}

/// The same arm for the other half of the pair: a file that is there and is not a private key.
///
/// A public key beside its private half is the file an operator reaches for by mistake, and the
/// remedy is to name the other one — not to type anything again.
#[tokio::test]
async fn reports_the_key_as_unreadable_when_the_named_file_is_not_a_private_key() {
    // Given a file in this operator's home that is not an OpenSSH private key
    let host = a_host_with_an_encrypted_key(PASSPHRASE);
    let named = host.home.join(".ssh").join("id_ed25519.pub");
    std::fs::write(&named, "ssh-ed25519 AAAA... not a private key\n")
        .expect("something that is not a private key");

    // When they name it
    let refused = host.add_key_at(&named, PASSPHRASE).await;

    // Then
    refused.assert_refused_with(AddHostKeyOutcome::KeyUnreadable);
}

/// The key is read with the **mapped OS user's** privileges and only from inside their own
/// home. Read as the daemon from a path the client chose, a session mapped to one user can name
/// another user's `~/.ssh/id_rsa` and have the host open it on their behalf.
#[tokio::test]
async fn refuses_a_key_outside_the_mapped_users_home() {
    // Given a real, unlocked-with-this-passphrase private key that is not in this operator's
    // home directory
    let host = a_host_with_an_encrypted_key(PASSPHRASE);
    let somebody_elses = host.home.parent().expect("a /home").join("otherdev");
    std::fs::create_dir_all(&somebody_elses).expect("another user's home");
    let their_key = somebody_elses.join("id_ed25519");
    an_encrypted_private_key_at(&their_key, PASSPHRASE);

    // When this operator names it
    let refused = host.add_key_at(&their_key, PASSPHRASE).await;

    // Then
    refused.assert_refused_with(AddHostKeyOutcome::KeyUnreadable);
    assert_eq!(
        host.agent.fingerprints_added(),
        Vec::<String>::new(),
        "this host read a key from outside the caller's own home and loaded it into an agent"
    );
}

// -- which host is this for? ----------------------------------------------------------------

/// How long a call addressed to another host gets to be refused. Generous for a decision that
/// reads a list in memory, and far short of the 120s a prompt would wait for an answer.
const ROUTING_WINDOW: Duration = Duration::from_secs(2);

/// A host nobody has ever heard of. `AddHostKey` names the machine whose agent the key is
/// loaded into, so a name this daemon cannot place is not a call it may answer for itself.
const AN_UNKNOWN_HOST: &str = "daemon-on-some-other-machine";

/// `daemon_instance_id` documents itself as "the host whose agent the key is loaded into".
/// Unread, an operator's key is loaded into the agent of whichever daemon happened to serve the
/// call — a private key in the wrong machine's agent, which no later request can take back.
#[tokio::test]
async fn refuses_an_add_addressed_to_a_host_this_daemon_does_not_know() {
    // Given an operator's key on this host
    let host = a_host_with_an_encrypted_key(PASSPHRASE);

    // When they address the add to a different host
    let refused = tokio::time::timeout(
        ROUTING_WINDOW,
        host.service.add_host_key(Request::new(AddHostKeyRequest {
            daemon_instance_id: AN_UNKNOWN_HOST.to_string(),
            ..host.an_add_of_their_key()
        })),
    )
    .await
    .expect(
        "an add addressed to another host must be refused at once — this one raised a prompt \
             and set about loading the key into its own agent",
    );

    // Then
    assert_eq!(
        refused.err().map(|status| status.code),
        Some(tddy_rpc::Code::InvalidArgument)
    );
    assert_eq!(
        host.agent.fingerprints_added(),
        Vec::<String>::new(),
        "a key addressed to another host reached this host's agent"
    );
}

/// The other half of honouring the field: a host named by its own id serves the call itself,
/// rather than trying to forward it to a peer.
#[tokio::test]
async fn serves_an_add_addressed_to_this_daemon_by_its_own_instance_id() {
    // Given an operator's key on this host
    let host = a_host_with_an_encrypted_key(PASSPHRASE);

    // When they address the add to this host by name
    let added = host
        .add_key_answering(
            AddHostKeyRequest {
                daemon_instance_id: local_instance_id_for_config(&host.service.config),
                ..host.an_add_of_their_key()
            },
            host.an_answer_carrying(PASSPHRASE),
        )
        .await;

    // Then
    added.assert_added_the_key(&host.key_fingerprint);
}

/// An answer is only ever answering a prompt on the host that raised it. Served locally, it is
/// matched against this daemon's own prompts — where the id belongs to nothing.
#[tokio::test]
async fn refuses_an_answer_addressed_to_a_host_this_daemon_does_not_know() {
    // Given a host serving the add-key endpoints
    let host = a_host_with_an_encrypted_key(PASSPHRASE);

    // When an answer is addressed to a different host
    let refused = host
        .service
        .answer_host_prompt(Request::new(AnswerHostPromptRequest {
            session_token: TEST_TOKEN.to_string(),
            daemon_instance_id: AN_UNKNOWN_HOST.to_string(),
            prompt_id: "a-prompt-on-another-host".to_string(),
            encrypted_answer: a_meaningless_answer(),
        }))
        .await;

    // Then
    assert_eq!(
        refused.err().map(|status| status.code),
        Some(tddy_rpc::Code::InvalidArgument)
    );
}

/// A feed addressed elsewhere must not be answered with this host's own questions: the browser
/// asked one host what it is waiting on, and would take another's prompt for that host's.
#[tokio::test]
async fn refuses_a_prompt_feed_addressed_to_a_host_this_daemon_does_not_know() {
    // Given a host serving the add-key endpoints
    let host = a_host_with_an_encrypted_key(PASSPHRASE);

    // When a feed is opened against a different host
    let refused = host
        .service
        .stream_host_prompts(Request::new(StreamHostPromptsRequest {
            session_token: TEST_TOKEN.to_string(),
            daemon_instance_id: AN_UNKNOWN_HOST.to_string(),
        }))
        .await;

    // Then
    assert_eq!(
        refused.err().map(|status| status.code),
        Some(tddy_rpc::Code::InvalidArgument)
    );
}

// -- what the operator can pick from --------------------------------------------------------

/// How long a listing gets. It reads one directory and a handful of small files, and unlike an
/// add it waits on no operator, so a listing that takes seconds is a listing that is wrong.
const LISTING_WINDOW: Duration = Duration::from_secs(2);

impl HostWithAnEncryptedKey {
    /// The keys an operator holding `token` is offered on this host.
    async fn keys_offered_to(&self, token: &str) -> Vec<HostKeyCandidate> {
        self.keys_offered(ListHostKeyCandidatesRequest {
            session_token: token.to_string(),
            daemon_instance_id: String::new(),
        })
        .await
        .expect("a listing for a valid session")
    }

    async fn keys_offered(
        &self,
        request: ListHostKeyCandidatesRequest,
    ) -> Result<Vec<HostKeyCandidate>, Status> {
        tokio::time::timeout(
            LISTING_WINDOW,
            self.service.list_host_key_candidates(Request::new(request)),
        )
        .await
        .expect("a listing reads one directory and must not hang")
        .map(|listed| listed.into_inner().candidates)
    }
}

fn paths_of(candidates: &[HostKeyCandidate]) -> Vec<String> {
    candidates
        .iter()
        .map(|candidate| candidate.path.clone())
        .collect()
}

/// The ordinary case: the operator's own key, described from its public half, ready to be
/// picked. Also the guard that keeps the refusals below from passing for an endpoint that
/// offers nothing to anyone.
#[tokio::test]
async fn offers_the_operator_the_key_in_their_own_home() {
    // Given a host where this operator has a key
    let host = a_host_with_an_encrypted_key(PASSPHRASE);

    // When they ask what they could add
    let offered = host.keys_offered_to(TEST_TOKEN).await;

    // Then
    assert_eq!(
        paths_of(&offered),
        vec![host.key_path.display().to_string()]
    );
    assert_eq!(
        offered
            .first()
            .map(|candidate| candidate.fingerprint.clone()),
        Some(host.key_fingerprint.clone()),
        "the key was offered under a fingerprint that is not its own"
    );
}

/// **Whose keys are these?** The listing is resolved through the same GitHub-to-OS-user mapping
/// the add is, so a session is offered its own operator's keys and no one else's. Resolved
/// wrongly, this endpoint enumerates another operator's `~/.ssh` for anyone with a session.
#[tokio::test]
async fn offers_each_operator_only_the_keys_of_their_own_os_user() {
    // Given a host where two operators each have a key of their own
    let host = a_host_with_an_encrypted_key(PASSPHRASE);

    // When each of them asks what they could add
    let theirs = host.keys_offered_to(TEST_TOKEN).await;
    let the_others = host.keys_offered_to(OTHER_TOKEN).await;

    // Then
    assert_eq!(paths_of(&theirs), vec![host.key_path.display().to_string()]);
    assert_eq!(
        paths_of(&the_others),
        vec![host.other_key_path.display().to_string()],
        "an operator was not offered the keys of their own OS user"
    );
}

/// **The two halves must agree.** A picked key goes straight back as `AddHostKeyRequest.subject`
/// and is read under a confinement this endpoint does not share by construction — only by
/// being built out of the same parts. A path this offers that the add refuses is a choice that
/// does not work.
#[tokio::test]
async fn offers_a_key_that_an_add_of_the_very_same_path_then_loads() {
    // Given a host where this operator has a key
    let host = a_host_with_an_encrypted_key(PASSPHRASE);

    // When they pick the key they are offered and add it
    let offered = host.keys_offered_to(TEST_TOKEN).await;
    let picked = offered.first().expect("a key to pick").path.clone();
    let added = host.add_key_at(Path::new(&picked), PASSPHRASE).await;

    // Then
    added.assert_added_the_key(&host.key_fingerprint);
}

/// A listing is not a public directory of a host's keys. Unauthenticated, it enumerates one.
#[tokio::test]
async fn rejects_a_listing_for_an_invalid_session_token() {
    // Given a host where this operator has a key
    let host = a_host_with_an_encrypted_key(PASSPHRASE);

    // When a listing arrives with a token this host never issued
    let refused = host
        .keys_offered(ListHostKeyCandidatesRequest {
            session_token: "not-a-session".to_string(),
            daemon_instance_id: String::new(),
        })
        .await;

    // Then
    assert_eq!(
        refused.err().map(|status| status.code),
        Some(tddy_rpc::Code::Unauthenticated)
    );
}

/// The same reason [`refuses_an_add_addressed_to_a_host_this_daemon_does_not_know`] gives. A
/// key is a file on one machine: answered locally, the browser is shown this daemon's keys as
/// though they were the other host's, and the path it then picks does not exist over there.
#[tokio::test]
async fn refuses_a_listing_addressed_to_a_host_this_daemon_does_not_know() {
    // Given a host where this operator has a key
    let host = a_host_with_an_encrypted_key(PASSPHRASE);

    // When the listing is addressed to a different host
    let refused = host
        .keys_offered(ListHostKeyCandidatesRequest {
            session_token: TEST_TOKEN.to_string(),
            daemon_instance_id: AN_UNKNOWN_HOST.to_string(),
        })
        .await;

    // Then
    assert_eq!(
        refused.err().map(|status| status.code),
        Some(tddy_rpc::Code::InvalidArgument)
    );
}

/// The other half of honouring the field: a host named by its own id answers for itself rather
/// than trying to forward the call to a peer.
#[tokio::test]
async fn serves_a_listing_addressed_to_this_daemon_by_its_own_instance_id() {
    // Given a host where this operator has a key
    let host = a_host_with_an_encrypted_key(PASSPHRASE);

    // When the listing names this host by its own id
    let offered = host
        .keys_offered(ListHostKeyCandidatesRequest {
            session_token: TEST_TOKEN.to_string(),
            daemon_instance_id: local_instance_id_for_config(&host.service.config),
        })
        .await
        .expect("a listing addressed to this host by name");

    // Then
    assert_eq!(
        paths_of(&offered),
        vec![host.key_path.display().to_string()]
    );
}

/// The listing's own version of the file oracle. An operator with no `~/.ssh` and one whose
/// `~/.ssh` this host cannot read must be indistinguishable from one who simply has no keys —
/// otherwise a session probes the host's filesystem one directory at a time.
#[tokio::test]
async fn offers_nothing_and_fails_nothing_when_the_ssh_directory_is_unreadable() {
    // Given a host where this operator's ~/.ssh cannot be read
    let host = a_host_where(PASSPHRASE, |files| {
        let denied = crate::host_private_key::HostUserFiles::home_dir(&files, "testdev")
            .expect("this fixture's reader knows where testdev lives")
            .join(".ssh");
        files.and_a_directory_it_cannot_read(denied)
    });

    // When they ask what they could add
    let offered = host
        .keys_offered(ListHostKeyCandidatesRequest {
            session_token: TEST_TOKEN.to_string(),
            daemon_instance_id: String::new(),
        })
        .await
        .expect("an unreadable ~/.ssh must read as an empty list, not as a failure");

    // Then
    assert_eq!(
        paths_of(&offered),
        Vec::<String>::new(),
        "an unreadable ~/.ssh was reported as something other than an empty list"
    );
}
