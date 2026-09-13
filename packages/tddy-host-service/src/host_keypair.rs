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

use rsa::pkcs8::{DecodePrivateKey, EncodePrivateKey, EncodePublicKey, LineEnding};
use rsa::{Oaep, RsaPrivateKey, RsaPublicKey};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// 2048 bits: an OAEP-SHA256 payload of ~190 bytes, comfortably more than any passphrase, at the
/// size every `SubtleCrypto` implementation supports without argument.
const KEY_BITS: usize = 2048;

/// The private half is a secret at rest — readable by its owner and nobody else, from the moment
/// the file exists.
const OWNER_ONLY_FILE: u32 = 0o600;

/// What the private half is stored as, under the keypair's storage directory.
const PRIVATE_KEY_FILE: &str = "host-prompt-key.pem";

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
///
/// Generation is deferred to the first prompt rather than done at startup: a host whose operator
/// never adds a key never pays for an RSA keygen, and a daemon that cannot write its data directory
/// fails at the prompt that needs the key rather than at boot.
pub struct FileHostKeypair {
    private_key_path: PathBuf,
    /// The parsed key, so a stream forwarding prompts does not re-read and re-parse a PEM per
    /// event. `None` until the first [`Self::published`] or [`Self::decrypt`].
    loaded: Mutex<Option<RsaPrivateKey>>,
}

impl FileHostKeypair {
    /// Keep the private half in `storage_dir`, owner-only.
    pub fn new(storage_dir: impl AsRef<Path>) -> Self {
        Self {
            private_key_path: storage_dir.as_ref().join(PRIVATE_KEY_FILE),
            loaded: Mutex::new(None),
        }
    }

    /// This host's private key, generating and persisting one the first time it is asked for.
    fn private_key(&self) -> Result<RsaPrivateKey, String> {
        // Held across the generation so two concurrent first prompts cannot each generate a key and
        // race to publish a different one — the second would silently invalidate a fingerprint the
        // first had already shown to an operator.
        let mut loaded = self.loaded.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(key) = loaded.as_ref() {
            return Ok(key.clone());
        }
        let key = self.read_or_generate()?;
        *loaded = Some(key.clone());
        Ok(key)
    }

    fn read_or_generate(&self) -> Result<RsaPrivateKey, String> {
        match std::fs::read_to_string(&self.private_key_path) {
            Ok(pem) => RsaPrivateKey::from_pkcs8_pem(&pem).map_err(|e| {
                // Not regenerated on top of it: a key file that exists but does not parse is a
                // damaged secret, and overwriting it would destroy the only copy of the identity
                // clients have already pinned.
                format!(
                    "{} is not a usable host prompt key: {e}",
                    self.private_key_path.display()
                )
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => self.generate_and_persist(),
            Err(e) => Err(format!("reading {}: {e}", self.private_key_path.display())),
        }
    }

    fn generate_and_persist(&self) -> Result<RsaPrivateKey, String> {
        let key = RsaPrivateKey::new(&mut rand::thread_rng(), KEY_BITS)
            .map_err(|e| format!("generating this host's prompt key: {e}"))?;
        let pem = key
            .to_pkcs8_pem(LineEnding::LF)
            .map_err(|e| format!("encoding this host's prompt key: {e}"))?;
        // `write_atomic_with_mode`, not `write_atomic`: this is the *first* write to the path, and
        // `write_atomic` can only carry permissions over from a target that already exists — a
        // private key created at the process umask is a world-readable private key. The atomic half
        // matters too: a truncated key file reads as "no key", which would fail every prompt on this
        // host with nothing to say why.
        tddy_core::atomic_file::write_atomic_with_mode(
            &self.private_key_path,
            pem.as_bytes(),
            OWNER_ONLY_FILE,
        )
        .map_err(|e| format!("{}: {e}", self.private_key_path.display()))?;
        Ok(key)
    }
}

impl HostKeypair for FileHostKeypair {
    fn published(&self) -> Result<PublishedKey, String> {
        let public = RsaPublicKey::from(&self.private_key()?);
        let spki_der = public
            .to_public_key_der()
            .map_err(|e| format!("encoding this host's public key: {e}"))?
            .as_bytes()
            .to_vec();
        let fingerprint = spki_fingerprint(&spki_der);
        Ok(PublishedKey {
            spki_der,
            fingerprint,
        })
    }

    fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, String> {
        // Blinded: decryption timing that varies with the key is the classic RSA side channel, and
        // this key sits behind an endpoint anyone with a session can call.
        self.private_key()?
            .decrypt_blinded(
                &mut rand::thread_rng(),
                Oaep::new::<sha2::Sha256>(),
                ciphertext,
            )
            // Deliberately says nothing about the ciphertext beyond that it was not readable here:
            // the failure an operator sees is "this host cannot read that answer", and the reasons
            // (wrong host, corrupted payload) are indistinguishable to them anyway.
            .map_err(|e| format!("this host cannot read that answer: {e}"))
    }
}

/// The `SHA256:` fingerprint of an SPKI DER public key, in the form an operator already recognises
/// from `ssh-add -l`.
///
/// Hashes the published bytes themselves, so what the dialog shows is a fingerprint of exactly what
/// the browser imported — a digest over a re-encoding could differ from the key actually in use.
///
/// Named for the bytes it digests, because it is **not** the only `fingerprint_of` in this crate:
/// [`crate::ssh_agent`] has one that digests an SSH public-key blob. Two same-named functions over
/// different inputs are a trap, and the two are never interchangeable.
fn spki_fingerprint(spki_der: &[u8]) -> String {
    use base64::Engine;
    use sha2::Digest;

    let digest = sha2::Sha256::digest(spki_der);
    format!(
        "SHA256:{}",
        base64::engine::general_purpose::STANDARD_NO_PAD.encode(digest)
    )
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

    /// A restart must not rotate this host's identity: a client that pinned the fingerprint on
    /// first sight blocks the flow with a "the key changed" warning if it does, which is exactly the
    /// alarm a real substitution would raise. The in-memory cache cannot cover this, because a
    /// restarted daemon has none.
    #[test]
    fn republishes_the_key_it_persisted_when_the_host_starts_again() {
        let dir = tempfile::tempdir().unwrap();
        let before_restart = FileHostKeypair::new(dir.path())
            .published()
            .expect("a keypair is generated on first use");

        let after_restart = FileHostKeypair::new(dir.path())
            .published()
            .expect("and read back from disk by the next process");

        assert_eq!(after_restart.fingerprint, before_restart.fingerprint);
    }

    /// The private half is a secret at rest, and this is the file's **first** write — the one
    /// `write_atomic` alone would create at the process umask, i.e. world-readable.
    #[cfg(unix)]
    #[test]
    fn persists_the_private_half_readable_only_by_its_owner() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let keypair = FileHostKeypair::new(dir.path());

        keypair.published().expect("a keypair is generated");

        let mode = std::fs::metadata(dir.path().join(PRIVATE_KEY_FILE))
            .expect("the private half is on disk")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
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
