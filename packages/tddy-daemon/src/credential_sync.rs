//! Wires `#keyring` 6/9's [`tddy_credential_sync::SyncEngine`] into this daemon: its identity
//! verifier, its signed advertisement, its persisted transport key, and the trigger that calls
//! `SyncEngine::publish` when a peer joins the common room.
//!
//! Lives here, in the crate that depends on both halves — `tddy-credential-sync` (the engine) and
//! `tddy-daemon-auth`/`tddy-daemon-livekit` (the identity and the transport) — for the same reason
//! [`crate::common_room_key_directory`] does: neither of those crates may depend on the other.
//!
//! # The single-subject limitation
//!
//! `tddy_credential_sync`'s wire format carries no subject identifier — a received record cannot
//! be routed to the right person's vault when more than one is open on this daemon. Publishing is
//! therefore only ever attempted while **exactly one** subject has an open vault; see
//! `docs/dev/todo/2026-10-05-keyring-sync-single-subject-only.md`.

use std::future::Future;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ed25519_dalek::{Signature, VerifyingKey};
use livekit::Room;
use tokio::sync::RwLock;

use tddy_credential_sync::{
    Ack, GroupSecret, IdentityVerifier, PeerAdvertisement, PeerId, PeerTransport, RecordKey,
    RefusalReason, SignedAdvertisement, SyncEngine, TransportError, WrappedRecords,
};
use tddy_daemon_auth::{DaemonSigningKey, KeyDirectory, SessionVaults};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_daemon_kernel::peer_forwarding::CommonRoom;
use tddy_daemon_livekit::livekit_peer_discovery::CommonRoomPeerRegistry;
use tddy_daemon_livekit::LiveKitPeerTransport;
use tddy_github::session_token_v2::KeyId;

/// The file [`tddy_credential_sync::VaultTransportKey`] persists at, beside this daemon's signing
/// key. Not `.pem` like [`tddy_daemon_auth::SIGNING_KEY_FILE`]: `VaultTransportKey::load_or_generate`
/// writes the raw 32-byte secret, not a PEM envelope, so a `.pem` name here would misdescribe it.
const TRANSPORT_KEY_FILE: &str = "credential_sync_transport_key.bin";

/// How often the peer-join watch checks the common-room roster for an arrival worth syncing to.
///
/// A poll, not `RoomEvent::ParticipantConnected` itself: that event fires inside
/// `livekit_peer_discovery`'s connect loop, already named in its own
/// `docs/code-issues/heavy-params-connect-common-room-publish-metadata.md` as carrying more
/// parameters than it should, and three acceptance suites call it by its current signature. Polling
/// [`CommonRoomPeerRegistry::snapshot_remotes`] — already public, already kept current by that same
/// loop — reaches the same trigger ("a peer that was not here is now") without widening it. Fires
/// `publish` only on a genuine arrival, never on a bare tick, so this remains "on join", not "on a
/// timer" (`SyncEngine::publish`'s own doc comment).
const PEER_JOIN_POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Everything `runtime::build` needs once [`build`] has assembled the engine.
pub struct CredentialSyncWiring {
    /// Reads `#keyring` 6/9's journal for the Accounts screen's sync-status badge.
    pub sync_status_source: Arc<dyn tddy_accounts::SyncStatusSource>,
    /// Held by the peer-join watch `run_publish_watch` runs; `runtime::build` keeps no other
    /// reference, but a future caller (a vault-change hook, say) reaches the same engine through
    /// this `Arc`.
    pub engine: Arc<Mutex<SyncEngine>>,
}

/// Assemble this daemon's sync engine: its persisted transport key, its identity verifier over
/// `key_directory`, and its transport — this daemon's own advertisements signed by `signing_key`,
/// wrapping `registry`/`room_slot`/`common_room` exactly as `tddy_daemon_livekit::LiveKitPeerTransport`
/// is already built elsewhere in `runtime::build`.
///
/// Constructed whenever a common room exists, regardless of whether `keyring.group_secret` is
/// set — an unconfigured secret still produces a real engine, one whose `publish` refuses every
/// peer's record through (`SyncError::NotConfigured`) rather than one that was never built.
pub fn build(
    config: &DaemonConfig,
    signing_key: Arc<DaemonSigningKey>,
    registry: Arc<CommonRoomPeerRegistry>,
    room_slot: Arc<RwLock<Option<Arc<Room>>>>,
    common_room: CommonRoom,
    local_instance_id: &str,
) -> anyhow::Result<CredentialSyncWiring> {
    let transport_key_path = transport_key_path(config)?;
    let transport_key = tddy_credential_sync::VaultTransportKey::load_or_generate(
        &transport_key_path,
    )
    .map_err(|e| {
        anyhow::anyhow!(
            "could not persist this daemon's credential-sync transport key at {}: {e}",
            transport_key_path.display()
        )
    })?;

    let key_directory: Arc<dyn KeyDirectory> = Arc::new(
        crate::common_room_key_directory::CommonRoomKeyDirectory::new(Arc::clone(&registry)),
    );
    let verifier: Arc<dyn IdentityVerifier> =
        Arc::new(KeyDirectoryIdentityVerifier::new(key_directory));

    let inner_transport: Arc<dyn PeerTransport> = Arc::new(LiveKitPeerTransport::new(
        registry,
        local_instance_id.to_string(),
        room_slot,
        common_room,
    ));
    let signing_key_id = signing_key.key_id().to_string();
    let sign: SignFn = Arc::new(move |message: &[u8]| signing_key.sign(message));
    let transport: Arc<dyn PeerTransport> = Arc::new(SigningPeerTransport::new(
        inner_transport,
        PeerId::new(local_instance_id.to_string()),
        signing_key_id,
        sign,
    ));

    let group_secret = config
        .keyring
        .as_ref()
        .and_then(|k| k.group_secret.clone())
        .map(GroupSecret::new);

    let engine = Arc::new(Mutex::new(SyncEngine::new(
        group_secret,
        transport,
        verifier,
        transport_key,
    )));

    Ok(CredentialSyncWiring {
        sync_status_source: Arc::new(EngineSyncStatusSource::new(Arc::clone(&engine))),
        engine,
    })
}

/// Where this daemon's transport key is persisted: beside its signing key.
fn transport_key_path(config: &DaemonConfig) -> anyhow::Result<std::path::PathBuf> {
    let signing_key_path = tddy_daemon_auth::signing_key_path(config)?;
    Ok(signing_key_path.with_file_name(TRANSPORT_KEY_FILE))
}

/// Resolves `#keyring` 1/9's [`KeyDirectory`] into [`IdentityVerifier`]'s synchronous answer.
///
/// [`KeyDirectory::public_key_for`] is `async`; [`IdentityVerifier::verify`] is not, by that trait's
/// own design (see its doc comment) — so this bridges the two with `block_on` below, the exact
/// pattern `tddy_daemon_livekit::LiveKitPeerTransport::block_on` already uses for the same kind of
/// call.
struct KeyDirectoryIdentityVerifier {
    directory: Arc<dyn KeyDirectory>,
}

impl KeyDirectoryIdentityVerifier {
    fn new(directory: Arc<dyn KeyDirectory>) -> Self {
        Self { directory }
    }

    fn block_on<F: Future>(&self, fut: F) -> F::Output {
        tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(fut))
    }
}

impl IdentityVerifier for KeyDirectoryIdentityVerifier {
    fn verify(
        &self,
        signing_key_id: &str,
        message: &[u8],
        signature: &[u8],
    ) -> Result<bool, RefusalReason> {
        let key_id = KeyId::parse(signing_key_id).map_err(|_| RefusalReason::UnknownIdentity)?;
        let key = match self.block_on(self.directory.public_key_for(&key_id)) {
            Ok(Some(key)) => key,
            Ok(None) => return Err(RefusalReason::UnknownIdentity),
            Err(e) => {
                log::warn!(
                    target: "tddy_daemon::credential_sync",
                    "could not look up the public key for credential-sync key id {key_id}: {e}"
                );
                return Err(RefusalReason::UnknownIdentity);
            }
        };
        let Ok(signature_bytes): Result<[u8; 64], _> = signature.try_into() else {
            return Ok(false);
        };
        Ok(verifies(&key, message, &signature_bytes))
    }
}

fn verifies(key: &VerifyingKey, message: &[u8], signature: &[u8; 64]) -> bool {
    key.verify_strict(message, &Signature::from_bytes(signature))
        .is_ok()
}

/// A function that signs arbitrary bytes with this daemon's identity key — [`DaemonSigningKey::sign`]
/// captured as a closure, so [`SigningPeerTransport`] need not depend on `tddy-daemon-auth`'s type
/// directly and a test can hand it a fake signer.
type SignFn = Arc<dyn Fn(&[u8]) -> Vec<u8> + Send + Sync>;

/// Fills in the identity `SyncEngine::publish` leaves blank on its own self-advertisement — its
/// private `self_advertisement` helper's doc comment names exactly this as the daemon runtime's
/// job — and signs the result, before handing it to the real transport.
///
/// The engine produces `peer`, `signing_key_id` and `signature` empty because it holds no identity
/// of its own ([`IdentityVerifier`] is verify-only, by design). This wrapper is that identity: it
/// overwrites those three fields with this daemon's own and re-derives the signature over the
/// corrected advertisement, so what reaches the wire is one a peer's own [`IdentityVerifier`] can
/// actually admit.
struct SigningPeerTransport {
    inner: Arc<dyn PeerTransport>,
    local_peer: PeerId,
    signing_key_id: String,
    sign: SignFn,
}

impl SigningPeerTransport {
    fn new(
        inner: Arc<dyn PeerTransport>,
        local_peer: PeerId,
        signing_key_id: String,
        sign: SignFn,
    ) -> Self {
        Self {
            inner,
            local_peer,
            signing_key_id,
            sign,
        }
    }
}

impl PeerTransport for SigningPeerTransport {
    fn advertise(&self, ad: SignedAdvertisement) -> Result<(), TransportError> {
        let advertisement = PeerAdvertisement {
            peer: self.local_peer.clone(),
            signing_key_id: self.signing_key_id.clone(),
            ..ad.advertisement
        };
        let message =
            serde_json::to_vec(&advertisement).expect("a PeerAdvertisement always serialises");
        let signature = (self.sign)(&message);
        self.inner.advertise(SignedAdvertisement {
            advertisement,
            signature,
        })
    }

    fn peers(&self) -> Vec<SignedAdvertisement> {
        self.inner.peers()
    }

    fn send(&self, to: &PeerId, payload: WrappedRecords) -> Result<Ack, TransportError> {
        self.inner.send(to, payload)
    }
}

/// Reads `#keyring` 6/9's per-account sync standing off a live [`SyncEngine`]'s journal, for
/// [`tddy_accounts::AccountsServiceImpl::with_sync_status`].
struct EngineSyncStatusSource {
    engine: Arc<Mutex<SyncEngine>>,
}

impl EngineSyncStatusSource {
    fn new(engine: Arc<Mutex<SyncEngine>>) -> Self {
        Self { engine }
    }
}

impl tddy_accounts::SyncStatusSource for EngineSyncStatusSource {
    fn status_for(
        &self,
        provider: &tddy_credentials::ProviderId,
        account: &tddy_credentials::AccountId,
    ) -> Option<tddy_credential_sync::AccountSyncSummary> {
        let engine = self.engine.lock().unwrap_or_else(PoisonError::into_inner);
        engine
            .journal()
            .account_summary(&RecordKey::new(provider.clone(), account.clone()))
    }
}

/// Every enrolled subject with an open vault on this daemon right now.
///
/// `SessionVaults` has no enumeration of its own open handles to ask instead — by design, see
/// `docs/dev/todo/2026-10-05-keyring-sync-single-subject-only.md` — so this checks
/// [`SessionVaults::state`] (already public) against every subject `config.users` names, which is
/// the whole population a vault could belong to.
fn open_vault_subjects(config: &DaemonConfig, vaults: &SessionVaults) -> Vec<String> {
    config
        .users
        .snapshot()
        .into_iter()
        .map(|mapping| mapping.github_user)
        .filter(|subject| matches!(vaults.state(subject), tddy_credentials::VaultState::Open))
        .collect()
}

/// Publish this daemon's vault to its admitted peers, when exactly one subject's vault is open.
///
/// Zero open vaults: nobody is signed in, so there is nothing to sync. More than one: this daemon
/// cannot tell whose record is whose on the wire (see the module doc), so it sits this attempt out
/// rather than guess — logged at `info`, because a server with several signed-in operators is a
/// normal deployment shape this feature does not yet cover, not a fault.
fn publish_if_exactly_one_vault_is_open(
    config: &DaemonConfig,
    vaults: &SessionVaults,
    engine: &Mutex<SyncEngine>,
) {
    let open = open_vault_subjects(config, vaults);
    let [subject] = open.as_slice() else {
        log::info!(
            target: "tddy_daemon::credential_sync",
            "not syncing credentials: {} subject(s) have an open vault on this daemon, and \
             #keyring 6/9's wire format can only address exactly one",
            open.len()
        );
        return;
    };
    let Some(vault) = vaults.use_open(subject) else {
        return;
    };
    let entries = match vault.entries() {
        Ok(entries) => entries,
        Err(e) => {
            log::warn!(
                target: "tddy_daemon::credential_sync",
                "could not read {subject}'s vault to sync it: {e}"
            );
            return;
        }
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if let Err(e) = engine
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .publish(&entries, now)
    {
        log::warn!(target: "tddy_daemon::credential_sync", "credential sync publish failed: {e}");
    }
}

/// Watch the common-room roster for an arrival, and publish to it when one shows up.
///
/// Runs until its `registry` is dropped — the host stops it the same way it stops every other
/// `RuntimeTasks::spawn` loop, by aborting the handle. See [`PEER_JOIN_POLL_INTERVAL`] for why this
/// is a poll and not `RoomEvent::ParticipantConnected` itself.
pub async fn run_publish_watch(
    registry: Arc<CommonRoomPeerRegistry>,
    vaults: Arc<SessionVaults>,
    config: DaemonConfig,
    engine: Arc<Mutex<SyncEngine>>,
) {
    let mut known: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut ticker = tokio::time::interval(PEER_JOIN_POLL_INTERVAL);
    loop {
        ticker.tick().await;
        let current: std::collections::HashSet<String> = registry
            .snapshot_remotes()
            .into_iter()
            .map(|peer| peer.instance_id.0)
            .collect();
        let arrived = current.iter().any(|id| !known.contains(id));
        known = current;
        if arrived {
            publish_if_exactly_one_vault_is_open(&config, &vaults, &engine);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;

    use tddy_accounts::SyncStatusSource;
    use tddy_credentials::{AccountId, CredentialRecord, ProviderId, SecretString, FIRST_VERSION};

    fn a_signing_key() -> (DaemonSigningKey, tempfile::TempDir) {
        let home = tempfile::tempdir().expect("a temporary directory");
        let key = DaemonSigningKey::load_or_generate(
            &home.path().join(tddy_daemon_auth::SIGNING_KEY_FILE),
        )
        .expect("a daemon generates a keypair");
        (key, home)
    }

    /// A [`KeyDirectory`] over a fixed, in-memory set of published keys.
    struct FakeDirectory(HashMap<KeyId, VerifyingKey>);

    #[async_trait]
    impl KeyDirectory for FakeDirectory {
        async fn public_key_for(&self, key_id: &KeyId) -> anyhow::Result<Option<VerifyingKey>> {
            Ok(self.0.get(key_id).copied())
        }
    }

    // `KeyDirectoryIdentityVerifier::verify` bridges to an `async` directory with
    // `tokio::task::block_in_place`, which panics outside a multi-thread runtime — hence
    // `flavor = "multi_thread"` on every test here, exactly as `session_room_livekit_acceptance.rs`
    // already uses for the same reason.

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn refuses_a_signing_key_id_the_directory_has_not_published() {
        // Given a verifier whose directory has published nobody's key
        let (daemon, _home) = a_signing_key();
        let verifier = KeyDirectoryIdentityVerifier::new(Arc::new(FakeDirectory(HashMap::new())));
        let message = b"a peer advertisement this daemon signed";
        let signature = daemon.sign(message);

        // When a signature under that daemon's (unpublished) key id is checked
        let checked = verifier.verify(&daemon.key_id().to_string(), message, &signature);

        // Then it is refused as an unknown identity, not as a bad signature — the two have
        // different remedies and must not be conflated
        assert_eq!(checked, Err(RefusalReason::UnknownIdentity));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn rejects_a_signature_that_does_not_verify_under_the_published_key() {
        // Given a verifier whose directory has published the real key
        let (daemon, _home) = a_signing_key();
        let mut published = HashMap::new();
        published.insert(daemon.key_id(), daemon.verifying_key());
        let verifier = KeyDirectoryIdentityVerifier::new(Arc::new(FakeDirectory(published)));
        let message = b"a peer advertisement this daemon signed";

        // When a signature that does not match the message is checked
        let bad_signature = vec![0u8; 64];
        let checked = verifier.verify(&daemon.key_id().to_string(), message, &bad_signature);

        // Then the signature is rejected — a known identity, just not one that sent this
        assert_eq!(checked, Ok(false));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn accepts_a_genuine_signature_under_the_published_key() {
        // Given a verifier whose directory has published the real key
        let (daemon, _home) = a_signing_key();
        let mut published = HashMap::new();
        published.insert(daemon.key_id(), daemon.verifying_key());
        let verifier = KeyDirectoryIdentityVerifier::new(Arc::new(FakeDirectory(published)));
        let message = b"a peer advertisement this daemon signed";
        let signature = daemon.sign(message);

        // When that daemon's own signature over the message is checked
        let checked = verifier.verify(&daemon.key_id().to_string(), message, &signature);

        // Then it verifies
        assert_eq!(checked, Ok(true));
    }

    /// A transport that answers with one peer's advertisement, and is never asked to send — every
    /// test here fails admission before a send would happen.
    struct OnePeerTransport;

    impl PeerTransport for OnePeerTransport {
        fn advertise(&self, _ad: SignedAdvertisement) -> Result<(), TransportError> {
            Ok(())
        }

        fn peers(&self) -> Vec<SignedAdvertisement> {
            vec![SignedAdvertisement {
                advertisement: PeerAdvertisement {
                    peer: PeerId::new("peer-a"),
                    signing_key_id: "unpublished-key".to_string(),
                    transport_public_key: vec![0u8; 32],
                    challenge: vec![1, 2, 3],
                    group_proof: vec![4, 5, 6],
                },
                signature: Vec::new(),
            }]
        }

        fn send(&self, _to: &PeerId, _payload: WrappedRecords) -> Result<Ack, TransportError> {
            unreachable!("admission fails before this test's engine ever sends anything")
        }
    }

    /// A verifier that admits nobody — enough to drive the engine's journal without a real peer.
    struct AlwaysUnknown;

    impl IdentityVerifier for AlwaysUnknown {
        fn verify(&self, _: &str, _: &[u8], _: &[u8]) -> Result<bool, RefusalReason> {
            Err(RefusalReason::UnknownIdentity)
        }
    }

    fn a_credential_record(
        provider: &ProviderId,
        account: &AccountId,
    ) -> tddy_credentials::VaultEntry {
        tddy_credentials::VaultEntry::Record(CredentialRecord {
            provider: provider.clone(),
            account: account.clone(),
            label: "Work".to_string(),
            secret: SecretString::new("a-secret-nobody-should-log"),
            metadata: Default::default(),
            updated_at: 1,
            version: FIRST_VERSION,
        })
    }

    #[test]
    fn reads_the_engines_journal_for_an_account() {
        // Given an engine that has just tried to publish one account to a peer it cannot admit
        let provider = ProviderId::new("github");
        let account = AccountId::new("jane-doe");
        let mut engine = SyncEngine::new(
            Some(GroupSecret::new("the-deployment-secret")),
            Arc::new(OnePeerTransport),
            Arc::new(AlwaysUnknown),
            tddy_credential_sync::VaultTransportKey::generate(),
        );
        engine
            .publish(&[a_credential_record(&provider, &account)], 1)
            .expect("publish does not error just because a peer is refused");
        let engine = Arc::new(Mutex::new(engine));
        let source = EngineSyncStatusSource::new(Arc::clone(&engine));

        // When the Accounts screen's adapter asks how that account stands
        let status = source.status_for(&provider, &account);

        // Then it reads exactly what the engine's journal decided — refused, since the one peer
        // offered it could not be admitted
        assert_eq!(
            status,
            Some(tddy_credential_sync::AccountSyncSummary::Refused)
        );
    }

    #[test]
    fn has_no_opinion_about_an_account_nothing_was_ever_offered_for() {
        // Given a freshly built engine that has published nothing
        let engine = Arc::new(Mutex::new(SyncEngine::new(
            Some(GroupSecret::new("the-deployment-secret")),
            Arc::new(OnePeerTransport),
            Arc::new(AlwaysUnknown),
            tddy_credential_sync::VaultTransportKey::generate(),
        )));
        let source = EngineSyncStatusSource::new(Arc::clone(&engine));

        // When it is asked about an account it has never been told to sync
        let status = source.status_for(&ProviderId::new("github"), &AccountId::new("nobody"));

        // Then it says nothing happened, not that it succeeded
        assert_eq!(status, None);
    }
}
