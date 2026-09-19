//! Key material that does not outlive its owner.
//!
//! Hand-rolled rather than taken from `zeroize`, which would be tidier and needs CLAUDE.md § ASK
//! approval before it is added. The mechanism is the same one that crate uses: a volatile write the
//! optimiser is not allowed to elide, followed by a fence so it is not reordered past the drop.

/// Thirty-two bytes of key material, zeroed when it is dropped.
///
/// Everything in this crate that holds a key holds it as one of these — the derived KEK, the
/// unwrapped data key, the intermediate HKDF output. A `[u8; 32]` would be copied wherever it went
/// and leave each copy behind in freed memory.
pub struct SecretBytes([u8; 32]);

impl SecretBytes {
    /// Take ownership of key material.
    #[must_use]
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Borrow the bytes for the length of a cipher operation. Never copy them out.
    #[must_use]
    pub fn expose(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Debug for SecretBytes {
    /// Prints the type and nothing else. A derived `Debug` would put key material in every log line
    /// that formats a struct containing one.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretBytes(<redacted>)")
    }
}

// TODO(keyring 3/9): implement — a volatile zeroing write plus a compiler fence.
