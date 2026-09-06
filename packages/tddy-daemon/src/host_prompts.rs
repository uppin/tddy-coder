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

use std::time::Duration;

/// How long a prompt stays answerable.
pub const PROMPT_TTL: Duration = Duration::from_secs(120);

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
}

/// The in-process registry behind `StreamHostPrompts` / `AnswerHostPrompt`.
pub struct InMemoryHostPromptRegistry;

impl InMemoryHostPromptRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for InMemoryHostPromptRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl HostPromptRegistry for InMemoryHostPromptRegistry {
    fn issue(&self, _kind: PromptKind, _subject: &str, _now_unix_ms: i64) -> PendingPrompt {
        // TODO(agent-add-key): implement
        unimplemented!("agent-add-key: issue")
    }

    fn pending(&self, _now_unix_ms: i64) -> Vec<PendingPrompt> {
        // TODO(agent-add-key): implement
        unimplemented!("agent-add-key: pending")
    }

    fn answer(
        &self,
        _prompt_id: &str,
        _encrypted_answer: Vec<u8>,
        _now_unix_ms: i64,
    ) -> Result<(), AnswerRejection> {
        // TODO(agent-add-key): implement
        unimplemented!("agent-add-key: answer")
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
