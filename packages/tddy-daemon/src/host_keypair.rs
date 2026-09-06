//! The RSA keypair a host publishes so answers can be encrypted end to end.
//!
//! An answer travelling to a host crosses the LiveKit common room and possibly a forwarding daemon.
//! `docs/ft/web/projects-screen-multi-host.md` is explicit that the room is "a trusted peer group,
//! **not a cryptographically authenticated one**", so a passphrase in plaintext there is readable by
//! any participant. Encrypting under the target host's public key removes that exposure.
//!
//! # What this does and does not defend against
//!
//! It defeats a **passive** relay: the room and any forwarding daemon stop seeing plaintext.
//!
//! It does **not**, by itself, defeat an **active** peer, because the client learns the public key
//! over that same unauthenticated channel — a hostile participant advertising itself as
//! `daemon-<instance_id>` could publish its own key. The mitigation is client-side **key continuity**
//! (pin on first sight, block loudly on change) plus a fingerprint the operator can verify out of
//! band. That is a deliberate, arguable choice; see the PRD.
//!
//! A passphrase is small, so plain **RSA-OAEP (SHA-256)** suffices — a 2048-bit key carries ~190
//! bytes, comfortably more than any passphrase. No hybrid AEAD envelope is needed.

use std::path::{Path, PathBuf};

/// A host's published identity for encrypted answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedKey {
    /// SPKI DER, which is what `SubtleCrypto.importKey("spki", …)` expects.
    pub spki_der: Vec<u8>,
    /// `SHA256:…` over the SPKI DER — shown in the dialog and pinned by the client.
    pub fingerprint: String,
}

/// Holds a host's keypair and decrypts answers addressed to it.
pub trait HostKeypair: Send + Sync {
    /// The public half, for publication with a prompt.
    fn published(&self) -> Result<PublishedKey, String>;

    /// Decrypt an RSA-OAEP(SHA-256) ciphertext produced against [`Self::published`].
    ///
    /// Returns the plaintext for immediate use. The caller must drop it as soon as the key is
    /// unlocked: forward-and-drop is the decision of record, and nothing here persists it.
    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, String>;
}

/// A keypair persisted under one directory, generated on first use.
pub struct FileHostKeypair {
    #[allow(dead_code)] // read by generation/loading once implemented (#hosts-screen 6/8 green)
    private_key_path: PathBuf,
}

impl FileHostKeypair {
    /// Keep the private half in `storage_dir`, owner-only.
    pub fn new(storage_dir: impl AsRef<Path>) -> Self {
        Self {
            private_key_path: storage_dir.as_ref().join("host-prompt-key.pem"),
        }
    }
}

impl HostKeypair for FileHostKeypair {
    fn published(&self) -> Result<PublishedKey, String> {
        // TODO(agent-add-key): implement
        unimplemented!("agent-add-key: published")
    }

    fn decrypt(&self, _ciphertext: &[u8]) -> Result<Vec<u8>, String> {
        // TODO(agent-add-key): implement
        unimplemented!("agent-add-key: decrypt")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fingerprint is shown in the dialog and pinned by the client, so it must be stable across
    /// reads — a fingerprint that changed per call would make every prompt look like a key rotation.
    #[test]
    fn derives_a_stable_fingerprint_for_a_published_public_key() {
        let dir = tempfile::tempdir().unwrap();
        let keypair = FileHostKeypair::new(dir.path());

        let first = keypair
            .published()
            .expect("a keypair is generated on first use");
        let second = keypair.published().expect("and reused on the next read");

        assert_eq!(first.fingerprint, second.fingerprint);
        assert!(
            first.fingerprint.starts_with("SHA256:"),
            "fingerprints use the form an operator already recognises, got {}",
            first.fingerprint
        );
        assert!(
            !first.spki_der.is_empty(),
            "the SPKI DER is what SubtleCrypto imports"
        );
    }

    /// The round trip the whole node rests on: what the browser encrypts under the published key,
    /// this host — and only this host — can read back.
    #[test]
    fn decrypts_a_payload_encrypted_against_its_published_public_key() {
        let dir = tempfile::tempdir().unwrap();
        let keypair = FileHostKeypair::new(dir.path());
        let published = keypair.published().expect("published key");

        let ciphertext = encrypt_for_test(&published.spki_der, b"correct horse battery staple");

        let plaintext = keypair.decrypt(&ciphertext).expect("its own ciphertext");

        assert_eq!(plaintext, b"correct horse battery staple");
    }

    /// A ciphertext meant for a different host must not decrypt here. Without this, "encrypted"
    /// would be decorative — the property that matters is that only the addressed host can read it.
    #[test]
    fn refuses_a_payload_encrypted_for_a_different_host() {
        let ours = tempfile::tempdir().unwrap();
        let theirs = tempfile::tempdir().unwrap();
        let our_keypair = FileHostKeypair::new(ours.path());
        let their_keypair = FileHostKeypair::new(theirs.path());

        let their_key = their_keypair.published().expect("their published key");
        let for_them = encrypt_for_test(&their_key.spki_der, b"not ours to read");

        assert!(
            our_keypair.decrypt(&for_them).is_err(),
            "a ciphertext addressed to another host must not decrypt here"
        );
    }

    /// Encrypt with RSA-OAEP(SHA-256) against an SPKI DER, standing in for the browser's
    /// `SubtleCrypto`.
    ///
    /// Deliberately a **separate implementation** from [`HostKeypair::decrypt`] — it goes through the
    /// `rsa` crate's public API directly rather than through anything in this module. A round trip
    /// where both halves share our code would prove only that we agree with ourselves; this pins the
    /// format against an independent encryptor, the same way node 5's fingerprint fixture is pinned
    /// against `ssh-keygen`.
    fn encrypt_for_test(spki_der: &[u8], plaintext: &[u8]) -> Vec<u8> {
        use rsa::pkcs8::DecodePublicKey;
        use rsa::{Oaep, RsaPublicKey};

        let public_key =
            RsaPublicKey::from_public_key_der(spki_der).expect("a published SPKI DER public key");
        let padding = Oaep::new::<sha2::Sha256>();
        public_key
            .encrypt(&mut rand::thread_rng(), padding, plaintext)
            .expect("a passphrase fits comfortably in an OAEP payload")
    }
}
