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

impl Drop for SecretBytes {
    fn drop(&mut self) {
        wipe(&mut self.0);
    }
}

/// A secret that is text — a stored credential, a vault passphrase — zeroed when it is dropped.
///
/// It prints as `SecretString(<redacted>)` and implements neither `Serialize` nor `Deserialize`,
/// so it cannot reach a log line or an RPC response by being formatted or serialised as part of a
/// bigger value. The one way to the text is [`Self::expose`], which is a call a reviewer can find.
///
/// Equality is constant-time, so comparing two of them does not leak how long a shared prefix is.
/// A `Clone` is a second owned copy, wiped on its own drop.
#[derive(Clone)]
pub struct SecretString(String);

impl SecretString {
    /// Take ownership of secret text.
    #[must_use]
    pub fn new(secret: impl Into<String>) -> Self {
        Self(secret.into())
    }

    /// Borrow the text for the length of the operation that needs it. Never log it.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl PartialEq for SecretString {
    fn eq(&self, other: &Self) -> bool {
        use subtle::ConstantTimeEq;
        bool::from(self.0.as_bytes().ct_eq(other.0.as_bytes()))
    }
}

impl Eq for SecretString {}

impl std::fmt::Debug for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretString(<redacted>)")
    }
}

impl Drop for SecretString {
    fn drop(&mut self) {
        wipe_text(&mut self.0);
    }
}

/// Zero `text` in place, leaving it the same length.
fn wipe_text(text: &mut str) {
    // SAFETY: zero bytes are valid UTF-8, so the string stays well-formed for its last moment.
    wipe(unsafe { text.as_bytes_mut() });
}

/// Overwrite `bytes` with zeroes in a way the optimiser may not elide.
///
/// A plain `fill(0)` on memory that is about to be freed is a dead store, and removing dead stores
/// is exactly what an optimiser is for. Each byte is written volatile, and the fence stops the
/// writes being reordered past whatever releases the memory afterwards.
///
/// Also used on the transient plaintext buffers a seal or open produces — an unwrapped data key,
/// a serialised record — which live in a `Vec` rather than a [`SecretBytes`].
pub(crate) fn wipe(bytes: &mut [u8]) {
    for byte in bytes.iter_mut() {
        // SAFETY: `byte` is a valid, aligned, exclusive reference for the whole write.
        unsafe { std::ptr::write_volatile(byte, 0) };
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_strings_text_is_zeroed_in_place_before_its_buffer_is_released() {
        // Given secret text
        let mut text = String::from("correct horse battery staple");

        // When the wipe a `SecretString` runs on drop is applied to it — to a live buffer, since
        // reading one after it is freed would prove nothing an allocator did not decide
        wipe_text(&mut text);

        // Then every byte is zero
        assert_eq!(text.into_bytes(), vec![0u8; 28]);
    }
}
