//! Questions a host is waiting on an operator to answer.
//!
//! Nothing in tddy asked the UI a question before this. The one existing mechanism is the ACP bidi
//! stream (`AcpService.Session`), which was considered and not chosen: a server-stream plus a unary
//! reply mirrors `StreamWorktreeStats` + `CalculateWorktreeSize`, keeps correlation an explicit
//! `prompt_id` rather than an envelope sequence, and leaves the reply a unary call that can be
//! transport-restricted the way `mint_local_token` is.
//!
//! # Four properties this type exists to guarantee
//!
//! - **Ownership.** A prompt belongs to the operator whose session raised it. It is shown to
//!   nobody else and answerable by nobody else: replayed to every subscriber it would disclose the
//!   private-key path one operator named to every other operator watching, and hand each of them
//!   the chance to spend a single-use prompt that is not theirs.
//! - **Expiry.** An unanswered prompt must not pin an operation forever.
//! - **Single use.** Answering twice must not add a key twice, and must not turn the endpoint into a
//!   passphrase-guessing oracle that silently accepts repeated attempts against one prompt.
//! - **No plaintext at rest.** The registry passes the *ciphertext* straight to the operation
//!   waiting on the prompt and keeps no copy of it; it never stores a decrypted answer, and never
//!   logs either form.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::{broadcast, oneshot};

/// How long a prompt stays answerable.
pub const PROMPT_TTL: Duration = Duration::from_secs(120);

/// How many issued prompts the feed buffers for a subscriber that has not read yet.
///
/// A prompt is raised by an operator action, one at a time, so this is generous by orders of
/// magnitude — it exists so that a browser mid-reconnect cannot make the *issuing* side block or
/// fail, not because a burst is expected.
const PROMPT_FEED_CAPACITY: usize = 32;

/// The **ciphertext** answering one prompt, delivered once to the operation waiting on it.
///
/// A channel rather than a value the caller polls for: an operation waiting on somebody typing must
/// not wake on a timer to find out they have, and a poll interval is latency added to every answer.
pub type AnswerHandoff = oneshot::Receiver<Vec<u8>>;

/// What the host is asking for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    /// The passphrase unlocking a private key, so it can be added to this host's ssh-agent.
    SshKeyPassphrase,
}

/// A question awaiting an answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingPrompt {
    pub prompt_id: String,
    /// The operator this question belongs to, as the **GitHub user** their session resolves to.
    ///
    /// The GitHub user and not the mapped OS user, because that is the identity all three RPCs in
    /// this flow already have: `StreamHostPrompts` and `AnswerHostPrompt` resolve a session to a
    /// GitHub user and stop there, and only `AddHostKey` goes on to map it to an OS user. Keying
    /// on the OS user would also merge two operators who share one — they would see and be able to
    /// burn each other's prompts, which is the very thing this field exists to prevent.
    pub issued_for: String,
    pub kind: PromptKind,
    /// Human-readable subject — the key being unlocked. **Never a secret.**
    pub subject: String,
    pub expires_at_unix_ms: i64,
}

/// Why an answer was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnswerRejection {
    /// No prompt with that id — never issued, or already reaped.
    UnknownPrompt,
    /// The prompt's TTL elapsed before the answer arrived.
    Expired,
    /// Already answered. A prompt is single-use.
    AlreadyAnswered,
}

/// Issues prompts and accepts exactly one answer for each.
///
/// A trait so the RPC layer can be handed a deterministic double, and so time can be injected —
/// expiry asserted with a real sleep would be slow and flaky.
pub trait HostPromptRegistry: Send + Sync {
    /// Register a question `issued_for` is being asked, and return it stamped with its expiry.
    fn issue(
        &self,
        issued_for: &str,
        kind: PromptKind,
        subject: &str,
        now_unix_ms: i64,
    ) -> PendingPrompt;

    /// Every prompt of `issued_for`'s that is still answerable at `now_unix_ms`.
    ///
    /// Another operator's outstanding prompts are not merely uninteresting here: the subject of a
    /// prompt is a path on this host that somebody named, and this is the call a feed primes
    /// itself from.
    fn pending(&self, issued_for: &str, now_unix_ms: i64) -> Vec<PendingPrompt>;

    /// Submit `answered_by`'s **encrypted** answer for `prompt_id`.
    ///
    /// Takes ciphertext, not plaintext: decryption belongs to whoever holds the private key, and
    /// keeping the registry ignorant of the secret means it cannot leak one through a log or a
    /// debug impl.
    ///
    /// A prompt raised by somebody other than `answered_by` is refused as
    /// [`AnswerRejection::UnknownPrompt`] — the same refusal a prompt id that was never issued
    /// gets, so the endpoint cannot be used to find out which ids are live — and, crucially, is
    /// **not consumed**: a prompt answers once, and that one answer belongs to the operator who
    /// raised it.
    fn answer(
        &self,
        prompt_id: &str,
        answered_by: &str,
        encrypted_answer: Vec<u8>,
        now_unix_ms: i64,
    ) -> Result<(), AnswerRejection>;

    /// Claim the handoff carrying `prompt_id`'s answer, for the one operation waiting on it.
    ///
    /// The handoff is created with the prompt rather than here, so an answer that arrives between
    /// [`Self::issue`] and this call is still delivered — issuing puts the prompt on the feed, so a
    /// browser can answer before the operation that raised it has claimed anything.
    ///
    /// Expiry is not signalled through it. The waiter already holds the prompt's
    /// `expires_at_unix_ms` and bounds itself by it; a reaped prompt drops its sender, which ends
    /// the wait the same way.
    ///
    /// `None` when no such prompt is outstanding, or when its handoff has already been claimed — a
    /// prompt has one waiting operation, the same way it has one answer.
    fn awaited_answer(&self, prompt_id: &str) -> Option<AnswerHandoff>;

    /// A feed of prompts as they are issued, for `StreamHostPrompts` to forward.
    ///
    /// A feed rather than a poll: a prompt is worth nothing to the operator who raised it if it
    /// reaches their browser a poll interval later, and a subscription that woke on a timer would
    /// still be running when its subscriber had gone. Each subscriber gets every prompt issued
    /// after it subscribed; prompts already outstanding come from [`Self::pending`].
    fn subscribe(&self) -> broadcast::Receiver<PendingPrompt>;
}

/// The in-process registry behind `StreamHostPrompts` / `AnswerHostPrompt`.
pub struct InMemoryHostPromptRegistry {
    prompts: Mutex<HashMap<String, PromptRecord>>,
    issued: broadcast::Sender<PendingPrompt>,
}

/// A prompt and the two halves of the channel its answer travels down.
///
/// Holding the sender is what makes a prompt unanswered: single use is the presence of the half
/// that can still deliver an answer, not a flag beside a stored one that could disagree with it.
/// The ciphertext passes through — it is handed to the waiting operation and never kept here — and
/// the plaintext never enters this type at all.
struct PromptRecord {
    prompt: PendingPrompt,
    /// Taken by the first answer. `None` means this prompt has been answered.
    unanswered: Option<oneshot::Sender<Vec<u8>>>,
    /// Parked until the operation that raised this prompt claims it, so an answer arriving before
    /// the claim waits in the channel rather than being dropped.
    handoff: Option<oneshot::Receiver<Vec<u8>>>,
}

impl InMemoryHostPromptRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            prompts: Mutex::new(HashMap::new()),
            issued: broadcast::channel(PROMPT_FEED_CAPACITY).0,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, PromptRecord>> {
        // What this guards is a map of records with no cross-entry invariant: a thread that
        // panicked while holding it left nothing half-written, and a poisoned lock that refused
        // every later prompt would be a strictly worse outcome than reading what is there.
        self.prompts.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Default for InMemoryHostPromptRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl HostPromptRegistry for InMemoryHostPromptRegistry {
    fn issue(
        &self,
        issued_for: &str,
        kind: PromptKind,
        subject: &str,
        now_unix_ms: i64,
    ) -> PendingPrompt {
        let prompt = PendingPrompt {
            prompt_id: uuid::Uuid::now_v7().to_string(),
            issued_for: issued_for.to_string(),
            kind,
            subject: subject.to_string(),
            expires_at_unix_ms: now_unix_ms + PROMPT_TTL.as_millis() as i64,
        };
        {
            let mut prompts = self.lock();
            // Prompts that can no longer be answered are dead weight, and this is the only moment
            // the registry is guaranteed to be woken — nothing sweeps it on a timer. Without this a
            // long-lived daemon accumulates one entry per prompt ever raised.
            prompts.retain(|_, record| now_unix_ms < record.prompt.expires_at_unix_ms);
            let (unanswered, handoff) = oneshot::channel();
            prompts.insert(
                prompt.prompt_id.clone(),
                PromptRecord {
                    prompt: prompt.clone(),
                    unanswered: Some(unanswered),
                    handoff: Some(handoff),
                },
            );
        }
        // Nobody watching is the ordinary case for a host whose operator has no browser open: the
        // prompt is still registered, and `pending` hands it to whoever subscribes next.
        let _ = self.issued.send(prompt.clone());
        prompt
    }

    fn pending(&self, issued_for: &str, now_unix_ms: i64) -> Vec<PendingPrompt> {
        self.lock()
            .values()
            .filter(|record| {
                record.prompt.issued_for == issued_for
                    && record.unanswered.is_some()
                    && now_unix_ms < record.prompt.expires_at_unix_ms
            })
            .map(|record| record.prompt.clone())
            .collect()
    }

    fn answer(
        &self,
        prompt_id: &str,
        answered_by: &str,
        encrypted_answer: Vec<u8>,
        now_unix_ms: i64,
    ) -> Result<(), AnswerRejection> {
        let mut prompts = self.lock();
        let record = prompts
            .get_mut(prompt_id)
            .ok_or(AnswerRejection::UnknownPrompt)?;
        // Checked before anything else, and reported as a prompt that does not exist: an answer
        // from a session that did not raise this question is refused in terms that reveal nothing
        // about it — not that it exists, not whose it is, not whether it is still open. It returns
        // *before* the sender is taken, so a stranger cannot spend the one answer this prompt has.
        if record.prompt.issued_for != answered_by {
            return Err(AnswerRejection::UnknownPrompt);
        }
        // Checked before expiry, so a replayed answer is reported as the replay it is rather than
        // as a timeout — the two call for different responses from whoever sent it.
        if record.unanswered.is_none() {
            return Err(AnswerRejection::AlreadyAnswered);
        }
        if now_unix_ms >= record.prompt.expires_at_unix_ms {
            return Err(AnswerRejection::Expired);
        }
        let waiting = record
            .unanswered
            .take()
            .expect("the sender was present a line ago and this map is locked");
        // Nobody waiting is an ordinary outcome — an answer to a prompt whose operation has already
        // given up. The prompt still counts as answered, so it cannot be answered again, and the
        // ciphertext goes nowhere: it is dropped with the channel rather than kept here.
        let _ = waiting.send(encrypted_answer);
        Ok(())
    }

    fn awaited_answer(&self, prompt_id: &str) -> Option<AnswerHandoff> {
        self.lock()
            .get_mut(prompt_id)
            .and_then(|record| record.handoff.take())
    }

    fn subscribe(&self) -> broadcast::Receiver<PendingPrompt> {
        self.issued.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A registry double is not used here: these are the guarantees of the real type, so the real
    /// type is what must be exercised. Time is injected instead, so expiry needs no sleep.
    fn a_registry() -> impl HostPromptRegistry {
        InMemoryHostPromptRegistry::new()
    }

    const T0: i64 = 1_788_696_000_000;
    fn after(ms: i64) -> i64 {
        T0 + ms
    }

    /// The operator raising the prompts below, and one who is not.
    const RAISED_BY: &str = "ada";
    const SOMEBODY_ELSE: &str = "grace";

    /// An operator who walks away must not pin the operation waiting on them forever.
    #[test]
    fn an_unanswered_prompt_expires_and_releases_its_operation() {
        let registry = a_registry();
        let prompt = registry.issue(RAISED_BY, PromptKind::SshKeyPassphrase, "id_ed25519", T0);

        let past_ttl = after(PROMPT_TTL.as_millis() as i64 + 1);

        assert!(
            registry.pending(RAISED_BY, past_ttl).is_empty(),
            "an expired prompt is no longer pending"
        );
        assert_eq!(
            registry.answer(&prompt.prompt_id, RAISED_BY, vec![1, 2, 3], past_ttl),
            Err(AnswerRejection::Expired),
            "and it can no longer be answered"
        );
    }

    /// Single use. Beyond not adding a key twice, this stops one prompt becoming a guessing oracle
    /// that quietly accepts repeated attempts.
    #[test]
    fn a_prompt_accepts_exactly_one_answer() {
        let registry = a_registry();
        let prompt = registry.issue(RAISED_BY, PromptKind::SshKeyPassphrase, "id_ed25519", T0);

        assert_eq!(
            registry.answer(&prompt.prompt_id, RAISED_BY, vec![1], after(10)),
            Ok(())
        );
        assert_eq!(
            registry.answer(&prompt.prompt_id, RAISED_BY, vec![2], after(20)),
            Err(AnswerRejection::AlreadyAnswered),
            "a second answer to the same prompt is refused"
        );
    }

    #[test]
    fn an_unknown_prompt_id_is_rejected() {
        let registry = a_registry();

        assert_eq!(
            registry.answer("never-issued", RAISED_BY, vec![1], T0),
            Err(AnswerRejection::UnknownPrompt)
        );
    }

    /// A prompt's subject is shown in a dialog, so it must describe the key — never carry the secret
    /// that unlocks it.
    #[test]
    fn a_pending_prompt_names_its_subject_and_its_expiry() {
        let registry = a_registry();
        let prompt = registry.issue(RAISED_BY, PromptKind::SshKeyPassphrase, "id_ed25519", T0);

        let pending = registry.pending(RAISED_BY, after(1));
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].subject, "id_ed25519");
        assert_eq!(
            pending[0].expires_at_unix_ms,
            T0 + PROMPT_TTL.as_millis() as i64
        );
        assert_eq!(prompt.kind, PromptKind::SshKeyPassphrase);
    }

    /// A prompt's subject is a path on this host that one operator named. Listed to another, it is
    /// disclosed to them — and offers them a single-use prompt they can spend.
    #[test]
    fn a_prompt_is_not_pending_for_an_operator_who_did_not_raise_it() {
        let registry = a_registry();
        registry.issue(RAISED_BY, PromptKind::SshKeyPassphrase, "id_ed25519", T0);

        assert_eq!(
            registry.pending(SOMEBODY_ELSE, after(1)),
            vec![],
            "another operator was shown this prompt, and the private-key path it names"
        );
    }

    /// Refusing a stranger must not double as a lookup service: were the refusal distinguishable
    /// from an unknown id, any session could enumerate which prompt ids are live.
    #[test]
    fn an_answer_from_another_operator_is_refused_as_an_unknown_prompt_would_be() {
        let registry = a_registry();
        let prompt = registry.issue(RAISED_BY, PromptKind::SshKeyPassphrase, "id_ed25519", T0);

        assert_eq!(
            registry.answer(&prompt.prompt_id, SOMEBODY_ELSE, vec![1], after(10)),
            registry.answer("never-issued", SOMEBODY_ELSE, vec![1], after(10)),
            "a live prompt id must be indistinguishable from a fictional one"
        );
    }

    /// The single answer a prompt has belongs to the operator who raised it. Spent by anybody else,
    /// their add is dead for the whole of the TTL and there is nothing they can do about it.
    #[test]
    fn an_answer_from_another_operator_does_not_consume_the_prompt() {
        let registry = a_registry();
        let prompt = registry.issue(RAISED_BY, PromptKind::SshKeyPassphrase, "id_ed25519", T0);

        let _refused = registry.answer(&prompt.prompt_id, SOMEBODY_ELSE, vec![1], after(10));

        assert_eq!(
            registry.answer(&prompt.prompt_id, RAISED_BY, vec![2], after(20)),
            Ok(()),
            "a stranger's refused answer spent the one answer this prompt had"
        );
    }
}
