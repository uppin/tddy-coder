//! What an askpass request ends as.

use std::fmt;

/// A decrypted passphrase. Zeroed when dropped, and never printed: `Debug` redacts it.
pub struct Passphrase(Vec<u8>);

impl Passphrase {
    pub(super) fn new(plaintext: Vec<u8>) -> Self {
        Self(plaintext)
    }

    /// The bytes the helper prints for ssh.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl Drop for Passphrase {
    fn drop(&mut self) {
        for byte in &mut self.0 {
            // SAFETY: `byte` is a valid, exclusive reference; volatile keeps the zeroing from being
            // optimised away as a dead store.
            unsafe { std::ptr::write_volatile(byte, 0) };
        }
        std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
    }
}

impl fmt::Debug for Passphrase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Passphrase(<redacted>)")
    }
}

impl PartialEq<&[u8]> for Passphrase {
    fn eq(&self, other: &&[u8]) -> bool {
        self.0 == *other
    }
}

/// How one helper request ended.
#[derive(Debug)]
pub enum AskpassOutcome {
    /// The operator answered; the helper prints this for ssh.
    Answer(Passphrase),
    /// Not a request the daemon relays — an unknown or ended clone's token, a question other than
    /// a key passphrase, or an answer that did not decrypt. The reason never carries a secret.
    Refused(String),
    /// Nobody answered in time.
    Expired,
}

/// An outcome with what else the daemon knows about the request.
#[derive(Debug)]
pub struct AskpassReply {
    pub outcome: AskpassOutcome,
    /// This clone already got an answer for the same key, so ssh asking again means that answer
    /// was rejected. Only ever set alongside an [`AskpassOutcome::Answer`].
    pub previous_answer_rejected: bool,
}

impl AskpassReply {
    pub(super) fn of(outcome: AskpassOutcome) -> Self {
        Self {
            outcome,
            previous_answer_rejected: false,
        }
    }
}
