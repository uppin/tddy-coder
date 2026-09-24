//! Key material that does not outlive its owner.
//!
//! Wiped with `zeroize`, the same crate the RustCrypto ciphers and MACs this crate builds from wipe
//! their own key schedules with: a volatile write the optimiser is not allowed to elide, followed by
//! a fence so it is not reordered past the drop. Both types here are [`ZeroizeOnDrop`], so a bound
//! can ask for — and a test can prove — that a value wipes itself.

use zeroize::{Zeroize, ZeroizeOnDrop};

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

impl Zeroize for SecretBytes {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

impl Drop for SecretBytes {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl ZeroizeOnDrop for SecretBytes {}

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

impl Zeroize for SecretString {
    /// Zero the text's whole buffer — its spare capacity too — and leave it empty.
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

impl Drop for SecretString {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl ZeroizeOnDrop for SecretString {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compiles only for a type that wipes itself when it is dropped.
    fn wipes_itself_on_drop<T: ZeroizeOnDrop>() {}

    #[test]
    fn every_holder_of_key_material_wipes_itself_on_drop() {
        // Given / When / Then — the bound is the proof: a type that dropped its bytes unwiped
        // would not compile here. The cipher is the one every seal and open builds from a key; the
        // MAC is the one every HKDF step keys.
        wipes_itself_on_drop::<SecretBytes>();
        wipes_itself_on_drop::<SecretString>();
        wipes_itself_on_drop::<chacha20poly1305::ChaCha20Poly1305>();
        // `hmac` marks no `Hmac` as wiping itself, but everything one holds does: the two keyed
        // SHA-256 states (inner and outer pad) and the block buffer beside them.
        wipes_itself_on_drop::<<sha2::Sha256 as hmac::EagerHash>::Core>();
        wipes_itself_on_drop::<
            hmac::digest::block_api::Buffer<hmac::block_api::HmacCore<sha2::Sha256>>,
        >();
    }

    #[test]
    fn a_secret_keys_bytes_are_zeroed_by_the_wipe_its_drop_runs() {
        // Given key material
        let mut key = SecretBytes::new([0x5a; 32]);

        // When the wipe its drop runs is applied to it — to a live value, since reading one after
        // it is freed would prove nothing an allocator did not decide
        key.zeroize();

        // Then every byte is zero
        assert_eq!(key.expose(), &[0u8; 32]);
    }

    #[test]
    fn a_secret_strings_text_is_gone_after_the_wipe_its_drop_runs() {
        // Given secret text
        let mut text = SecretString::new("correct horse battery staple");

        // When the wipe its drop runs is applied to it, while it is still alive
        text.zeroize();

        // Then no text is left to expose
        assert_eq!(text.expose(), "");
    }
}
