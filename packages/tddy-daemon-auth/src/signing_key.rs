//! The daemon's own cryptographic identity, and the port that turns other daemons' key ids into
//! public keys.
//!
//! A daemon generates an Ed25519 keypair for itself the first time it boots and keeps it at
//! `<data_dir>/signing_key.pem`, mode `0600`. That key is what signs session tokens
//! ([`tddy_github::session_token_v2`]), so a daemon can authenticate its own users with no
//! `livekit.api_secret` configured and no secret shared with anybody.
//!
//! Verifying a token a *peer* minted needs that peer's public key, which is a lookup this crate
//! deliberately does not perform: [`KeyDirectory`] is a **port**, implemented by whoever owns a
//! transport the fleet can publish on. That keeps `tddy-daemon-livekit` off this crate's
//! dependency path — a rule its own `dependency_boundary_unit` asserts — and it keeps a desktop
//! daemon, which has no fleet at all, from needing one.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use ed25519_dalek::VerifyingKey;
use tddy_github::session_token_v2::{KeyId, SessionClaims, SessionTokenError, SessionTokenSigner};

/// The file name a daemon's keypair takes inside its data directory.
pub const SIGNING_KEY_FILE: &str = "signing_key.pem";

/// This daemon's Ed25519 identity: generated once, persisted, and never leaving the host.
pub struct DaemonSigningKey {
    // TODO(signing-key): implement
    _signing_key: ed25519_dalek::SigningKey,
}

impl DaemonSigningKey {
    /// Load the keypair at `path`, generating and writing one when the file is absent.
    ///
    /// Writes with mode `0600` and refuses to load a file that is readable by anyone else: a
    /// signing key that a second account can read is a signing key the fleet cannot attribute.
    /// There is **no** in-memory fallback for an unwritable path — a daemon that cannot persist
    /// its identity would mint a fresh one on every restart and invalidate every live session.
    pub fn load_or_generate(path: &Path) -> anyhow::Result<Self> {
        // TODO(signing-key): implement
        let _ = path;
        todo!("DaemonSigningKey::load_or_generate")
    }

    /// This daemon's key id — what its tokens stamp and what peers resolve.
    pub fn key_id(&self) -> KeyId {
        // TODO(signing-key): implement
        todo!("DaemonSigningKey::key_id")
    }

    /// The public half, for publishing to a [`KeyDirectory`].
    pub fn verifying_key(&self) -> VerifyingKey {
        // TODO(signing-key): implement
        todo!("DaemonSigningKey::verifying_key")
    }

    /// The public half in SPKI DER, which is what goes on the wire and what [`KeyId`] digests.
    pub fn public_spki_der(&self) -> Vec<u8> {
        // TODO(signing-key): implement
        todo!("DaemonSigningKey::public_spki_der")
    }

    /// A signer over this key. Cheap; the key is copied, not shared.
    pub fn signer(&self) -> SessionTokenSigner {
        // TODO(signing-key): implement
        todo!("DaemonSigningKey::signer")
    }
}

/// Where a daemon learns other daemons' public keys, and announces its own.
///
/// A port rather than a concrete type because the answer is deployment-shaped: a LiveKit fleet
/// resolves it off the common room, and a desktop install has no peers at all and answers `None`
/// to every id but its own.
#[async_trait]
pub trait KeyDirectory: Send + Sync {
    /// Announce this daemon's key so peers can verify tokens it mints.
    ///
    /// Called on every (re)connection, not once: a peer that joined after the first announcement
    /// would otherwise never learn the key.
    async fn publish(&self, key_id: &KeyId, public_key: &VerifyingKey) -> anyhow::Result<()>;

    /// The public key for `key_id`, or `None` when no peer has announced it.
    ///
    /// `None` is a real answer — an unknown daemon, or one that has not announced yet — and is
    /// distinct from `Err`, which means the lookup itself failed.
    async fn public_key_for(&self, key_id: &KeyId) -> anyhow::Result<Option<VerifyingKey>>;
}

/// Verifies any daemon's session token: reads the `kid`, resolves it, then checks the signature.
///
/// The local key is held directly rather than looked up, so a daemon can always verify the tokens
/// it minted itself — including a desktop daemon whose directory knows nobody.
pub struct DirectorySessionTokenVerifier {
    // TODO(signing-key): implement
    _local_key_id: KeyId,
    _local_public_key: VerifyingKey,
    _directory: Arc<dyn KeyDirectory>,
}

impl DirectorySessionTokenVerifier {
    pub fn new(local: &DaemonSigningKey, directory: Arc<dyn KeyDirectory>) -> Self {
        // TODO(signing-key): implement
        let _ = (local, directory);
        todo!("DirectorySessionTokenVerifier::new")
    }

    /// Verify `token`, resolving whichever daemon signed it.
    ///
    /// Returns [`SessionTokenError::UnknownKeyId`] when the token is well-formed but names a key
    /// this daemon has not learned — the one rejection a later retry can resolve.
    pub async fn verify(&self, token: &str) -> Result<SessionClaims, SessionTokenError> {
        // TODO(signing-key): implement
        let _ = token;
        todo!("DirectorySessionTokenVerifier::verify")
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[test]
    fn generates_a_keypair_on_first_use_and_reuses_it_across_restarts() {
        // Given a daemon that has booted once and generated its identity
        let home = a_data_directory();
        let first_boot = DaemonSigningKey::load_or_generate(&the_key_path(&home))
            .expect("a daemon generates a keypair on first use");

        // When it restarts and loads from the same data directory
        let after_restart = DaemonSigningKey::load_or_generate(&the_key_path(&home))
            .expect("a daemon reuses the keypair it already generated");

        // Then it is the same identity — a fresh key on every restart would invalidate every live
        // session and make the daemon a stranger to every peer that had learned it
        assert_eq!(after_restart.key_id(), first_boot.key_id());
    }

    #[test]
    fn two_daemons_generate_two_different_identities() {
        // Given two daemons with data directories of their own
        let one = a_data_directory();
        let other = a_data_directory();

        // When each generates its identity
        let one = DaemonSigningKey::load_or_generate(&the_key_path(&one)).expect("one generates");
        let other =
            DaemonSigningKey::load_or_generate(&the_key_path(&other)).expect("the other generates");

        // Then they are distinct, which is what lets a fleet attribute a token to its signer
        assert_ne!(one.key_id(), other.key_id());
    }

    #[test]
    fn writes_the_private_key_readable_only_by_the_account_that_owns_it() {
        // Given a daemon generating its identity for the first time
        let home = a_data_directory();
        let path = the_key_path(&home);
        DaemonSigningKey::load_or_generate(&path).expect("a daemon generates a keypair");

        // When the file it wrote is inspected
        let mode = std::fs::metadata(&path)
            .expect("the key file exists")
            .permissions()
            .mode()
            & 0o777;

        // Then no other account can read it — a signing key a second account can read is a signing
        // key the fleet cannot attribute to one daemon
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn refuses_a_key_file_other_accounts_can_read() {
        // Given a key file left world-readable
        let home = a_data_directory();
        let path = the_key_path(&home);
        DaemonSigningKey::load_or_generate(&path).expect("a daemon generates a keypair");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
            .expect("the permissions are loosened");

        // When the daemon restarts and loads it
        let refusal = DaemonSigningKey::load_or_generate(&path);

        // Then it refuses rather than signing with a key anybody on the host could have taken.
        // There is deliberately no repair-and-continue: a key that has been readable may already
        // have been read, and tightening the mode would hide that.
        assert!(
            refusal.is_err(),
            "a signing key readable by other accounts must be refused, not silently re-secured"
        );
    }

    #[test]
    fn the_key_id_is_the_one_its_own_tokens_carry() {
        // Given a daemon's identity
        let home = a_data_directory();
        let daemon = DaemonSigningKey::load_or_generate(&the_key_path(&home))
            .expect("a daemon generates a keypair");

        // When its signer stamps a token
        // Then the id the token names is the id the daemon publishes — otherwise a peer resolves
        // a key that cannot verify what it was fetched for
        assert_eq!(*daemon.signer().key_id(), daemon.key_id());
    }

    #[test]
    fn the_key_id_is_derived_from_the_public_half_it_publishes() {
        // Given a daemon's identity
        let home = a_data_directory();
        let daemon = DaemonSigningKey::load_or_generate(&the_key_path(&home))
            .expect("a daemon generates a keypair");

        // Then its id is exactly what a peer computes from the public key it receives, so no
        // separate announcement of the id can disagree with the key
        assert_eq!(daemon.key_id(), KeyId::of(&daemon.verifying_key()));
    }

    fn a_data_directory() -> tempfile::TempDir {
        tempfile::tempdir().expect("a temporary directory")
    }

    fn the_key_path(home: &tempfile::TempDir) -> std::path::PathBuf {
        home.path().join(SIGNING_KEY_FILE)
    }
}
