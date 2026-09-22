//! The daemon's [`KeyDirectory`]: peers' session-token signing keys, as they advertise them on
//! the LiveKit common room.
//!
//! It lives here, in the crate that depends on both halves, and not in either of them. The key
//! directory is `tddy-daemon-auth`'s port, and `tddy-daemon-livekit` may not reach that crate —
//! its own `dependency_boundary_unit` asserts it — so the LiveKit crate carries a key only as the
//! two opaque strings of [`AdvertisedSigningKey`] and this adapter is what turns them into keys.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use ed25519_dalek::pkcs8::DecodePublicKey;
use ed25519_dalek::VerifyingKey;
use tddy_daemon_auth::{DaemonSigningKey, KeyDirectory};
use tddy_daemon_livekit::livekit_peer_discovery::CommonRoomPeerRegistry;
use tddy_daemon_livekit::AdvertisedSigningKey;
use tddy_github::session_token_v2::KeyId;

/// This daemon's signing key in the form its common-room advertisement carries it — handed to the
/// discovery loop, which publishes it on every (re)connection to the common room.
pub fn advertised_signing_key(key: &DaemonSigningKey) -> AdvertisedSigningKey {
    AdvertisedSigningKey {
        key_id: key.key_id().to_string(),
        public_key: URL_SAFE_NO_PAD.encode(key.public_spki_der()),
    }
}

/// Resolves peers' keys from the common-room roster.
///
/// Every lookup is answered from memory — the registry's current snapshot, refreshed by the
/// discovery loop, or a key already learned — so it never waits, which is what [`KeyDirectory`]
/// asks of an implementation.
///
/// A key, once learned, is **kept** across `CommonRoomPeerRegistry::clear`, which the discovery
/// loop runs every time its connection to the room ends. That is safe because a key id is a digest
/// of its key: an id names exactly one key, forever, so a remembered answer can never become a
/// wrong one. Without it, every peer's token — a 24 h split-agent credential included — would be
/// refused for as long as this daemon is reconnecting. The cache holds one entry per peer identity
/// ever verified, which grows only when a daemon generates a new key.
pub struct CommonRoomKeyDirectory {
    registry: Arc<CommonRoomPeerRegistry>,
    learned: Mutex<HashMap<KeyId, VerifyingKey>>,
}

impl CommonRoomKeyDirectory {
    /// A directory over `registry`, which discovery fills from the common room.
    pub fn new(registry: Arc<CommonRoomPeerRegistry>) -> Self {
        Self {
            registry,
            learned: Mutex::new(HashMap::new()),
        }
    }

    fn remembered(&self, key_id: &KeyId) -> Option<VerifyingKey> {
        self.learned
            .lock()
            .expect("the learned-key cache is never poisoned")
            .get(key_id)
            .copied()
    }

    fn remember(&self, key_id: &KeyId, key: VerifyingKey) {
        self.learned
            .lock()
            .expect("the learned-key cache is never poisoned")
            .insert(key_id.clone(), key);
    }
}

#[async_trait]
impl KeyDirectory for CommonRoomKeyDirectory {
    /// The key advertised under `key_id` that really is that key.
    ///
    /// Every candidate the room holds is checked, because two participants can advertise one id
    /// and only the one whose bytes hash to it is telling the truth; an impostor re-advertising a
    /// genuine id is ignored rather than allowed to shadow the real key. `Err` only when candidates
    /// exist and none of them is the key — the error says why each was refused.
    async fn public_key_for(&self, key_id: &KeyId) -> anyhow::Result<Option<VerifyingKey>> {
        if let Some(key) = self.remembered(key_id) {
            return Ok(Some(key));
        }
        let candidates = self.registry.signing_public_keys_for(key_id.as_str());
        if candidates.is_empty() {
            return Ok(None);
        }
        let mut refusals = Vec::with_capacity(candidates.len());
        for advertised in &candidates {
            match decode_advertised_key(key_id, advertised) {
                Ok(key) => {
                    self.remember(key_id, key);
                    return Ok(Some(key));
                }
                Err(refusal) => refusals.push(refusal.to_string()),
            }
        }
        anyhow::bail!(
            "no peer advertising signing key {key_id} sent that key: {}",
            refusals.join("; ")
        )
    }
}

/// The key a peer advertised under `key_id`, if what it advertised really is that key.
///
/// A peer advertising bytes that do not hash to the id it names is refused here rather than handed
/// to the verifier: its tokens would fail anyway, and the error is the place to say why.
fn decode_advertised_key(key_id: &KeyId, advertised: &str) -> anyhow::Result<VerifyingKey> {
    let der = URL_SAFE_NO_PAD.decode(advertised).map_err(|e| {
        anyhow::anyhow!(
            "the peer advertising signing key {key_id} sent a key that is not base64url: {e}"
        )
    })?;
    let key = VerifyingKey::from_public_key_der(&der).map_err(|e| {
        anyhow::anyhow!(
            "the peer advertising signing key {key_id} sent a key that is not Ed25519 SPKI: {e}"
        )
    })?;
    let actual = KeyId::of(&key);
    if actual != *key_id {
        anyhow::bail!(
            "the peer advertising signing key {key_id} sent the key {actual}, which is not that key"
        );
    }
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_advertised_under_its_own_id_decodes_to_that_key() {
        // Given a daemon's key as its advertisement carries it
        let (daemon, _home) = a_daemon();
        let advertised = advertised_signing_key(&daemon);

        // When a peer decodes it for the id it was advertised under
        let decoded = decode_advertised_key(&daemon.key_id(), &advertised.public_key)
            .expect("a daemon's own advertisement decodes");

        // Then it is the daemon's public key
        assert_eq!(decoded, daemon.verifying_key());
    }

    #[test]
    fn a_key_advertised_under_another_daemons_id_is_refused() {
        // Given one daemon advertising its key under a second daemon's id
        let (impostor, _home) = a_daemon();
        let (victim, _other_home) = a_daemon();
        let advertised = advertised_signing_key(&impostor);

        // When a peer decodes it for the id it claims
        let refusal = decode_advertised_key(&victim.key_id(), &advertised.public_key)
            .expect_err("a key that does not hash to its id must be refused");

        // Then it is refused for exactly that reason — the id is a digest of the key, so the claim
        // is checkable — and not for a decoding failure
        assert!(
            refusal.to_string().contains("which is not that key"),
            "expected a key/id mismatch, got: {refusal}"
        );
    }

    #[tokio::test]
    async fn a_key_id_nobody_advertises_is_an_unknown_key() {
        // Given a directory over an empty roster
        let (stranger, _home) = a_daemon();
        let directory = CommonRoomKeyDirectory::new(Arc::new(CommonRoomPeerRegistry::new()));

        // When a key id nobody advertised is looked up
        let found = directory
            .public_key_for(&stranger.key_id())
            .await
            .expect("an empty roster is an answer, not a failure");

        // Then there is none — `None`, the verifier's cue to refuse as an unknown key
        assert_eq!(found, None);
    }

    fn a_daemon() -> (DaemonSigningKey, tempfile::TempDir) {
        let home = tempfile::tempdir().expect("a temporary directory");
        let key = DaemonSigningKey::load_or_generate(
            &home.path().join(tddy_daemon_auth::SIGNING_KEY_FILE),
        )
        .expect("a daemon generates a keypair");
        (key, home)
    }
}
