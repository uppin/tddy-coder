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

use std::sync::Arc;

use tddy_credential_sync::{
    Ack, PeerId, PeerTransport, SignedAdvertisement, TransportError, WrappedRecords,
};

use crate::livekit_peer_discovery::CommonRoomPeerRegistry;

/// Carries credential-sync traffic over the daemon's common LiveKit room.
///
/// Holds the registry rather than a `Room`: the registry is already the crate's answer to "who is
/// in the room right now", and a second membership view would drift from the one
/// `ListEligibleDaemons` reports.
pub struct LiveKitPeerTransport {
    // TODO(keyring 6/9): implement
    _registry: Arc<CommonRoomPeerRegistry>,
    /// This daemon's own instance id, so its own advertisement is excluded from [`peers`].
    ///
    /// [`peers`]: PeerTransport::peers
    _local_instance_id: String,
}

impl LiveKitPeerTransport {
    #[must_use]
    pub fn new(
        registry: Arc<CommonRoomPeerRegistry>,
        local_instance_id: impl Into<String>,
    ) -> Self {
        Self {
            _registry: registry,
            _local_instance_id: local_instance_id.into(),
        }
    }

    /// Sync this daemon's vault to its admitted peers, because the room changed.
    ///
    /// **On join and on change, never on a timer.** A periodic sweep would re-send a record to a
    /// peer that has refused it, over and over, and turn one configuration mistake into steady
    /// traffic — and it would make "nothing has propagated" and "the last sweep has not run yet"
    /// indistinguishable in the journal.
    pub fn on_peer_joined(&self, _peer: &PeerId) -> Result<(), TransportError> {
        todo!("TODO(keyring 6/9): implement — hand the join to the engine's publish")
    }
}

impl PeerTransport for LiveKitPeerTransport {
    fn advertise(&self, _ad: SignedAdvertisement) -> Result<(), TransportError> {
        todo!("TODO(keyring 6/9): implement — publish into this participant's metadata")
    }

    fn peers(&self) -> Vec<SignedAdvertisement> {
        todo!("TODO(keyring 6/9): implement — read every remote participant's advertisement")
    }

    fn send(&self, _to: &PeerId, _payload: WrappedRecords) -> Result<Ack, TransportError> {
        todo!("TODO(keyring 6/9): implement — deliver over the room's data channel, await the ack")
    }
}
