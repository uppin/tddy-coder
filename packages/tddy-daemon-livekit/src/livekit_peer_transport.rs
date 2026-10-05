//! `tddy-credential-sync`'s [`PeerTransport`], over the common room.
//!
//! The adapter, and **only** the adapter: it publishes an advertisement into participant metadata,
//! reports the advertisements it sees, and delivers a sealed payload to one participant. It decides
//! nothing about who may receive a credential — [`SyncEngine`] does, on the other side of the port,
//! where it can be tested against a fake peer that fails exactly one check.
//!
//! [`SyncEngine`]: tddy_credential_sync::SyncEngine
//!
//! # What this deliberately does not change
//!
//! `ListEligibleDaemons` and `StartSession` forwarding keep the trust model
//! `docs/ft/daemon/livekit-peer-discovery.md` § *Trust model* describes. `#keyring` 6/9 adds a gate
//! **for credentials** and does not retrofit one onto forwarding — that is a separate change with
//! its own blast radius. So this module rides the existing participant-metadata path rather than
//! widening it.

use std::collections::HashMap;
use std::sync::Arc;

use livekit::prelude::Room;

use tddy_credential_sync::{
    Ack, PeerId, PeerTransport, SignedAdvertisement, TransportError, WrappedRecords,
};
use tddy_daemon_kernel::peer_forwarding::CommonRoom;

use crate::livekit_peer_discovery::CommonRoomPeerRegistry;

/// The participant **attribute** (`LocalParticipant::set_attributes` /
/// `RemoteParticipant::attributes`) this daemon's advertisement rides under.
///
/// Attributes, not `metadata` — LiveKit gives a participant both, and `metadata` is already
/// `livekit_peer_discovery`'s single-writer channel for [`crate::livekit_peer_discovery::DaemonAdvertisement`],
/// republished on its own eventual-consistency loop. Riding that same string would make this
/// adapter a second writer racing the first; attributes are untouched by anything else in this
/// crate, so this is additive in the way the module's own docs ask for, not a second place the one
/// published string can disagree with itself.
const CREDENTIAL_SYNC_ADVERTISEMENT_ATTRIBUTE: &str = "tddy.credential_sync.advertisement";

/// The RPC name a [`WrappedRecords`] delivery is forwarded under, over the same data-channel RPC
/// mechanism `ListProjects`/`StartSession` forwarding already uses ([`CommonRoom::forward_to_peer`]).
///
/// TODO(keyring 6/9 wiring): nothing yet **registers** a handler for this name on the receiving
/// side — that registration needs a live `SyncEngine` to call `receive` on, which is constructed
/// where the engine and the room are wired together (`tddy-daemon`'s `runtime::build`), not here.
/// Until it exists, [`LiveKitPeerTransport::send`] reaches a peer that has not registered this
/// method and gets back the RPC layer's "unknown method" refusal.
const CREDENTIAL_SYNC_SERVICE: &str = "tddy.credential_sync.CredentialSync";
const CREDENTIAL_SYNC_SEND_METHOD: &str = "Send";

/// Carries credential-sync traffic over the daemon's common LiveKit room.
///
/// Holds the registry rather than only a `Room`: the registry is already the crate's answer to
/// "who is in the room right now", kept here so a future caller that wants the richer `PeerDaemon`
/// view (its durable host id, its label) alongside a `SignedAdvertisement` has one `Arc` to reach
/// both through, rather than two independently-synced views of the same room.
pub struct LiveKitPeerTransport {
    _registry: Arc<CommonRoomPeerRegistry>,
    /// This daemon's own instance id, so its own advertisement is excluded from [`peers`].
    ///
    /// [`peers`]: PeerTransport::peers
    _local_instance_id: String,
    /// The shared room handle `livekit_peer_discovery`'s own connect loop populates — `None`
    /// whenever this daemon is between connections, exactly as that loop sees it.
    room_slot: Arc<tokio::sync::RwLock<Option<Arc<Room>>>>,
    /// The already-connected peer-forwarding mechanism `ListProjects`/`StartSession` forwarding
    /// uses, reused here rather than duplicated — see [`CREDENTIAL_SYNC_SERVICE`].
    common_room: CommonRoom,
}

impl LiveKitPeerTransport {
    #[must_use]
    pub fn new(
        registry: Arc<CommonRoomPeerRegistry>,
        local_instance_id: impl Into<String>,
        room_slot: Arc<tokio::sync::RwLock<Option<Arc<Room>>>>,
        common_room: CommonRoom,
    ) -> Self {
        Self {
            _registry: registry,
            _local_instance_id: local_instance_id.into(),
            room_slot,
            common_room,
        }
    }

    /// Sync this daemon's vault to its admitted peers, because the room changed.
    ///
    /// **On join and on change, never on a timer.** A periodic sweep would re-send a record to a
    /// peer that has refused it, over and over, and turn one configuration mistake into steady
    /// traffic — and it would make "nothing has propagated" and "the last sweep has not run yet"
    /// indistinguishable in the journal.
    ///
    /// TODO(keyring 6/9 wiring): a genuine implementation needs the daemon's live `SyncEngine` and
    /// its open vault's current entries — neither of which this adapter holds, by design: holding
    /// the engine here (which itself holds this transport, as its [`PeerTransport`]) would be a
    /// reference cycle, and this crate has no business reading vault entries at all. The real
    /// trigger belongs to whoever constructs both together (`tddy-daemon`'s `runtime::build`),
    /// calling `engine.publish(vault.entries()?, now)` when its own join-event watcher fires. This
    /// method is therefore a deliberate no-op rather than a fabricated call to state it has none
    /// of, and no test depends on it doing more.
    pub fn on_peer_joined(&self, _peer: &PeerId) -> Result<(), TransportError> {
        Ok(())
    }

    /// Block the calling thread on `fut`.
    ///
    /// [`PeerTransport::advertise`] and [`PeerTransport::send`] are synchronous — deliberately, the
    /// same way [`tddy_credential_sync::engine::IdentityVerifier::verify`] is, so admitting and
    /// wrapping stay plain decisions an engine can run without an executor. LiveKit's SDK is not:
    /// `set_attributes` and an RPC round trip are both futures. Bridging needs a multi-threaded
    /// Tokio runtime so blocking this call's thread does not block the one driving the reactor the
    /// future needs to complete — true of `tddy-daemon`'s runtime in every deployment this adapter
    /// ships in, and the reason for [`tokio::task::block_in_place`] rather than a bare `block_on`.
    fn block_on<F: std::future::Future>(&self, fut: F) -> F::Output {
        tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(fut))
    }

    fn connected_room(&self) -> Result<Arc<Room>, TransportError> {
        self.block_on(async { self.room_slot.read().await.clone() })
            .ok_or(TransportError::NotConnected)
    }
}

impl PeerTransport for LiveKitPeerTransport {
    fn advertise(&self, ad: SignedAdvertisement) -> Result<(), TransportError> {
        let room = self.connected_room()?;
        let json = serde_json::to_string(&ad).map_err(|e| {
            TransportError::Io(format!("encoding this daemon's own advertisement: {e}"))
        })?;
        let mut attributes = HashMap::with_capacity(1);
        attributes.insert(CREDENTIAL_SYNC_ADVERTISEMENT_ATTRIBUTE.to_string(), json);
        self.block_on(room.local_participant().set_attributes(attributes))
            .map_err(|e| TransportError::Io(format!("publishing this daemon's advertisement: {e}")))
    }

    fn peers(&self) -> Vec<SignedAdvertisement> {
        let Ok(room) = self.connected_room() else {
            return Vec::new();
        };
        room.remote_participants()
            .into_iter()
            .filter_map(|(_, participant)| {
                let json = participant
                    .attributes()
                    .get(CREDENTIAL_SYNC_ADVERTISEMENT_ATTRIBUTE)?
                    .clone();
                match serde_json::from_str(&json) {
                    Ok(ad) => Some(ad),
                    Err(e) => {
                        log::warn!(
                            "LiveKitPeerTransport::peers: {} published an unparseable credential-sync advertisement: {e}",
                            participant.identity()
                        );
                        None
                    }
                }
            })
            .collect()
    }

    fn send(&self, to: &PeerId, payload: WrappedRecords) -> Result<Ack, TransportError> {
        let body = serde_json::to_vec(&payload)
            .map_err(|e| TransportError::Io(format!("encoding a payload for {to}: {e}")))?;
        let response = self.block_on(self.common_room.forward_to_peer(
            to.as_str(),
            CREDENTIAL_SYNC_SERVICE,
            CREDENTIAL_SYNC_SEND_METHOD,
            body,
        ));
        let bytes = response.map_err(|status| {
            if status.code == tddy_rpc::Code::Unavailable {
                TransportError::PeerUnreachable(to.clone())
            } else {
                TransportError::Io(status.message)
            }
        })?;
        serde_json::from_slice(&bytes)
            .map_err(|e| TransportError::Io(format!("decoding {to}'s acknowledgement: {e}")))
    }
}
