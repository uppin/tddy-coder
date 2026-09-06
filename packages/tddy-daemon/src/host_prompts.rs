//! Questions a host is waiting on an operator to answer.
//!
//! Nothing in tddy asked the UI a question before this. The one existing mechanism is the ACP bidi
//! stream (`AcpService.Session`), which was considered and not chosen: a server-stream plus a unary
//! reply mirrors `StreamWorktreeStats` + `CalculateWorktreeSize`, keeps correlation an explicit
//! `prompt_id` rather than an envelope sequence, and leaves the reply a unary call that can be
//! transport-restricted the way `mint_local_token` is.
//!
//! # Three properties this type exists to guarantee
//!
//! - **Expiry.** An unanswered prompt must not pin an operation forever.
//! - **Single use.** Answering twice must not add a key twice, and must not turn the endpoint into a
//!   passphrase-guessing oracle that silently accepts repeated attempts against one prompt.
//! - **No plaintext at rest.** The registry holds the *ciphertext* only until the waiting operation
//!   takes it; it never stores a decrypted answer, and never logs either form.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::broadcast;

/// How long a prompt stays answerable.
pub const PROMPT_TTL: Duration = Duration::from_secs(120);

/// How many issued prompts the feed buffers for a subscriber that has not read yet.
///
/// A prompt is raised by an operator action, one at a time, so this is generous by orders of
/// magnitude — it exists so that a browser mid-reconnect cannot make the *issuing* side block or
/// fail, not because a burst is expected.
const PROMPT_FEED_CAPACITY: usize = 32;

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
    /// Register a question and return it, stamped with its expiry.
    fn issue(&self, kind: PromptKind, subject: &str, now_unix_ms: i64) -> PendingPrompt;

    /// Every prompt still answerable at `now_unix_ms`.
    fn pending(&self, now_unix_ms: i64) -> Vec<PendingPrompt>;

    /// Submit the **encrypted** answer for `prompt_id`.
    ///
    /// Takes ciphertext, not plaintext: decryption belongs to whoever holds the private key, and
    /// keeping the registry ignorant of the secret means it cannot leak one through a log or a
    /// debug impl.
    fn answer(
        &self,
        prompt_id: &str,
        encrypted_answer: Vec<u8>,
        now_unix_ms: i64,
    ) -> Result<(), AnswerRejection>;

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

/// A prompt and the answer it has been given, if any.
///
/// The ciphertext is what makes the prompt answered: single use is the absence of an answer, not a
/// separate flag that could disagree with one. It stays here only until the operation waiting on
/// this prompt takes it — the plaintext never enters this type at all.
struct PromptRecord {
    prompt: PendingPrompt,
    encrypted_answer: Option<Vec<u8>>,
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
    fn issue(&self, kind: PromptKind, subject: &str, now_unix_ms: i64) -> PendingPrompt {
        let prompt = PendingPrompt {
            prompt_id: uuid::Uuid::now_v7().to_string(),
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
            prompts.insert(
                prompt.prompt_id.clone(),
                PromptRecord {
                    prompt: prompt.clone(),
                    encrypted_answer: None,
                },
            );
        }
        // Nobody watching is the ordinary case for a host whose operator has no browser open: the
        // prompt is still registered, and `pending` hands it to whoever subscribes next.
        let _ = self.issued.send(prompt.clone());
        prompt
    }

    fn pending(&self, now_unix_ms: i64) -> Vec<PendingPrompt> {
        self.lock()
            .values()
            .filter(|record| {
                record.encrypted_answer.is_none() && now_unix_ms < record.prompt.expires_at_unix_ms
            })
            .map(|record| record.prompt.clone())
            .collect()
    }

    fn answer(
        &self,
        prompt_id: &str,
        encrypted_answer: Vec<u8>,
        now_unix_ms: i64,
    ) -> Result<(), AnswerRejection> {
        let mut prompts = self.lock();
        let record = prompts
            .get_mut(prompt_id)
            .ok_or(AnswerRejection::UnknownPrompt)?;
        // Checked before expiry, so a replayed answer is reported as the replay it is rather than
        // as a timeout — the two call for different responses from whoever sent it.
        if record.encrypted_answer.is_some() {
            return Err(AnswerRejection::AlreadyAnswered);
        }
        if now_unix_ms >= record.prompt.expires_at_unix_ms {
            return Err(AnswerRejection::Expired);
        }
        record.encrypted_answer = Some(encrypted_answer);
        Ok(())
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

    /// An operator who walks away must not pin the operation waiting on them forever.
    #[test]
    fn an_unanswered_prompt_expires_and_releases_its_operation() {
        let registry = a_registry();
        let prompt = registry.issue(PromptKind::SshKeyPassphrase, "id_ed25519", T0);

        let past_ttl = after(PROMPT_TTL.as_millis() as i64 + 1);

        assert!(
            registry.pending(past_ttl).is_empty(),
            "an expired prompt is no longer pending"
        );
        assert_eq!(
            registry.answer(&prompt.prompt_id, vec![1, 2, 3], past_ttl),
            Err(AnswerRejection::Expired),
            "and it can no longer be answered"
        );
    }

    /// Single use. Beyond not adding a key twice, this stops one prompt becoming a guessing oracle
    /// that quietly accepts repeated attempts.
    #[test]
    fn a_prompt_accepts_exactly_one_answer() {
        let registry = a_registry();
        let prompt = registry.issue(PromptKind::SshKeyPassphrase, "id_ed25519", T0);

        assert_eq!(
            registry.answer(&prompt.prompt_id, vec![1], after(10)),
            Ok(())
        );
        assert_eq!(
            registry.answer(&prompt.prompt_id, vec![2], after(20)),
            Err(AnswerRejection::AlreadyAnswered),
            "a second answer to the same prompt is refused"
        );
    }

    #[test]
    fn an_unknown_prompt_id_is_rejected() {
        let registry = a_registry();

        assert_eq!(
            registry.answer("never-issued", vec![1], T0),
            Err(AnswerRejection::UnknownPrompt)
        );
    }

    /// A prompt's subject is shown in a dialog, so it must describe the key — never carry the secret
    /// that unlocks it.
    #[test]
    fn a_pending_prompt_names_its_subject_and_its_expiry() {
        let registry = a_registry();
        let prompt = registry.issue(PromptKind::SshKeyPassphrase, "id_ed25519", T0);

        let pending = registry.pending(after(1));
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].subject, "id_ed25519");
        assert_eq!(
            pending[0].expires_at_unix_ms,
            T0 + PROMPT_TTL.as_millis() as i64
        );
        assert_eq!(prompt.kind, PromptKind::SshKeyPassphrase);
    }
}
