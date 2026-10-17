//! Which of ssh's askpass requests the helper may relay to an operator.
//!
//! ssh calls its helper for more than passphrases — host-key confirmation, account passwords — and
//! hands it only the question as one argument. Only a key passphrase request may be relayed.

/// What ssh asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AskpassPrompt {
    /// ssh wants the passphrase of the private key at `key`.
    KeyPassphrase { key: String },
    /// Anything else. Never relayed.
    Other,
}

const KEY_PASSPHRASE_PREFIX: &str = "Enter passphrase for key '";

/// Classify ssh's question.
///
/// ssh words the request `Enter passphrase for key '%.100s': `, so the path may itself contain
/// spaces or quotes and arrives cut at 100 characters. The match is on the **prefix**, and the
/// closing `':` must end the text: a server-controlled banner that merely contains the phrase
/// must not be taken for ssh asking about a local key.
#[must_use]
pub fn classify(prompt: &str) -> AskpassPrompt {
    let key = prompt
        .trim_end()
        .strip_prefix(KEY_PASSPHRASE_PREFIX)
        .and_then(|rest| rest.strip_suffix("':"));
    match key {
        Some(key) if !key.is_empty() && !key.chars().any(char::is_control) => {
            AskpassPrompt::KeyPassphrase {
                key: key.to_string(),
            }
        }
        _ => AskpassPrompt::Other,
    }
}
