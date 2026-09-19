//! Which peers may hold a credential, what crosses the wire, and how both sides know where they
//! stand.
//!
//! # Room membership is not the gate
//!
//! `docs/ft/daemon/livekit-peer-discovery.md` § *Trust model* states the position this crate
//! changes: room membership defines the peer group, and *"there is no separate cryptographic
//! attestation that a participant runs `tddy-daemon`"*. That is defensible for forwarding a session
//! start to a host an operator picked from a list. It is not defensible for handing out a person's
//! GitHub credential — anything holding the room's LiveKit credentials would receive it.
//!
//! So a peer is admitted by **two independent checks, both required**:
//!
//! 1. a **configured group secret** (`keyring.group_secret`) — the authorisation question, whose
//!    answer is per deployment: *may that daemon hold my credentials?*
//! 2. `#keyring` 1/9's **Ed25519 signature** — the authentication question: *did that daemon send
//!    this?*
//!
//! Neither alone admits anybody, and [`SyncEngine`] has two tests that say so. The group secret
//! without the signature lets anyone replay an advertisement; the signature without the group
//! secret authenticates a daemon nobody authorised.
//!
//! # Three lines this crate does not cross
//!
//! 1. **The vault's data key, its KEK and the login credential never leave the daemon.** A
//!    recipient re-seals what it receives under its own vault key. Two daemons end up sharing a
//!    *record*, never a *key*, so compromising one does not unwrap the other's disk.
//! 2. **Neither check admits a peer alone.**
//! 3. **No path here re-derives authorisation from room membership.** That is the property being
//!    removed, and a "well, it's already in our room" shortcut would restore it.
//!
//! # Why the transport is a port
//!
//! [`PeerTransport`] is a trait this crate defines and LiveKit implements
//! (`tddy_daemon_livekit::LiveKitPeerTransport`), rather than a LiveKit client this crate calls.
//! Two reasons, and the second is the one that matters here:
//!
//! - `heavy-dependency-livekit-peer-forwarding` measures what the alternative costs — one
//!   misplaced dependency, 14 dependents, 6 of them paying for something they never use. 1/9's
//!   `KeyDirectory` is the precedent this follows.
//! - **Authorisation, wrapping and reconciliation are only testable through a fake.** Every
//!   interesting property here is about what a peer failing exactly one check does *not* receive,
//!   and a real LiveKit room cannot be made to fail one check at a time.

pub mod engine;
pub mod journal;
pub mod transport;
pub mod transport_key;

pub use engine::{IdentityVerifier, ReceiveOutcome, Reconciliation, SyncEngine, SyncError};
pub use journal::{JournalEntry, RefusalReason, SyncJournal, SyncStatus};
pub use transport::{
    Ack, GroupSecret, PeerAdvertisement, PeerId, PeerTransport, RecordKey, SignedAdvertisement,
    TransportError, WrappedRecords,
};
pub use transport_key::{TransportPublicKey, VaultTransportKey};
