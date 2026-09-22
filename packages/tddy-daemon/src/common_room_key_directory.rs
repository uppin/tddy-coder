//! The daemon's [`KeyDirectory`]: peers' session-token signing keys, as they advertise them on
//! the LiveKit common room.
//!
//! It lives here, in the crate that depends on both halves, and not in either of them. The key
//! directory is `tddy-daemon-auth`'s port, and `tddy-daemon-livekit` may not reach that crate —
//! its own `dependency_boundary_unit` asserts it — so the LiveKit crate carries a key only as the
//! two opaque strings of [`AdvertisedSigningKey`] and this adapter is what turns them into keys.

use std::sync::Arc;

use async_trait::async_trait;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use ed25519_dalek::pkcs8::DecodePublicKey;
use ed25519_dalek::VerifyingKey;
use tddy_daemon_auth::{DaemonSigningKey, KeyDirectory};
use tddy_daemon_livekit::livekit_peer_discovery::CommonRoomPeerRegistry;
use tddy_daemon_livekit::AdvertisedSigningKey;
use tddy_github::session_token_v2::{spki_der, KeyId};

/// This daemon's signing key in the form its common-room advertisement carries it.
pub fn advertised_signing_key(key: &DaemonSigningKey) -> AdvertisedSigningKey {
    AdvertisedSigningKey {
        key_id: key.key_id().to_string(),
        public_key: URL_SAFE_NO_PAD.encode(key.public_spki_der()),
    }
}

/// Resolves peers' keys from the common-room roster and announces this daemon's through its
/// advertisement.
///
/// Every lookup reads the registry's current snapshot — held in memory and refreshed by the
/// discovery loop — so it answers without waiting, which is what [`KeyDirectory`] asks of an
/// implementation.
pub struct CommonRoomKeyDirectory {
    registry: Arc<CommonRoomPeerRegistry>,
    advertised: AdvertisedSigningKey,
}

impl CommonRoomKeyDirectory {
    /// A directory over `registry` for the daemon that holds `local`.
    pub fn new(registry: Arc<CommonRoomPeerRegistry>, local: &DaemonSigningKey) -> Self {
        Self {
            registry,
            advertised: advertised_signing_key(local),
        }
    }

    /// What this daemon's advertisement carries — handed to the discovery loop, which publishes
    /// it on every (re)connection to the common room.
    pub fn advertised(&self) -> AdvertisedSigningKey {
        self.advertised.clone()
    }
}

#[async_trait]
impl KeyDirectory for CommonRoomKeyDirectory {
    /// The room learns a daemon's key from the advertisement the discovery loop publishes on each
    /// connection, and that advertisement is fixed when the loop is started. So there is nothing to
    /// send here; what can be checked is that the key being announced is the one the room is told.
    /// A different one would mean this daemon signs with a key no peer can find.
    async fn publish(&self, key_id: &KeyId, public_key: &VerifyingKey) -> anyhow::Result<()> {
        let announcing = AdvertisedSigningKey {
            key_id: key_id.to_string(),
            public_key: URL_SAFE_NO_PAD.encode(spki_der(public_key)),
        };
        if announcing != self.advertised {
            anyhow::bail!(
                "the common room announces signing key {} for this daemon, not {key_id}",
                self.advertised.key_id
            );
        }
        Ok(())
    }

    async fn public_key_for(&self, key_id: &KeyId) -> anyhow::Result<Option<VerifyingKey>> {
        self.registry
            .signing_public_key_for(key_id.as_str())
            .map(|advertised| decode_advertised_key(key_id, &advertised))
            .transpose()
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
    if KeyId::of(&key) != *key_id {
        anyhow::bail!(
            "the peer advertising signing key {key_id} sent the key {}, which is not that key",
            KeyId::of(&key)
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
        let refusal = decode_advertised_key(&victim.key_id(), &advertised.public_key);

        // Then it is refused — the id is a digest of the key, so the claim is checkable
        assert!(
            refusal.is_err(),
            "a key that does not hash to the id it is advertised under must be refused"
        );
    }

    #[tokio::test]
    async fn announces_only_the_key_its_advertisement_carries() {
        // Given a directory advertising one daemon's key
        let (daemon, _home) = a_daemon();
        let (stranger, _other_home) = a_daemon();
        let directory =
            CommonRoomKeyDirectory::new(Arc::new(CommonRoomPeerRegistry::new()), &daemon);

        // When it is asked to announce that key, and then a different one
        let own = directory
            .publish(&daemon.key_id(), &daemon.verifying_key())
            .await;
        let other = directory
            .publish(&stranger.key_id(), &stranger.verifying_key())
            .await;

        // Then only the advertised key is announceable
        assert_eq!((own.is_ok(), other.is_ok()), (true, false));
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
