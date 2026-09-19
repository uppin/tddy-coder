//! The port a peer is reached through, and the shapes that cross it.
//!
//! Nothing here knows what LiveKit is. What a transport must do is advertise this daemon, name the
//! peers it can see, and deliver a sealed payload to one of them — three things a room, a mesh or
//! an in-memory fake can all do.

use serde::{Deserialize, Serialize};

use tddy_credentials::{AccountId, ProviderId};

/// Stable identity of a peer daemon, as its transport names it.
///
/// A transport's own addressing, not a cryptographic identity: the daemon behind this id is
/// established by the signature over its advertisement, never by the id itself. Keeping the two
/// separate is what stops an id somebody can choose from becoming an authorisation decision.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PeerId(String);

impl PeerId {
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PeerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The one `(provider, account)` slot a journal line is about.
///
/// The journal is keyed per peer *and* per record, because "this peer is behind" and "this account
/// will not propagate to anyone" are different problems with different remedies, and a status
/// collapsed to the peer cannot tell them apart.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RecordKey {
    pub provider: ProviderId,
    pub account: AccountId,
}

impl RecordKey {
    #[must_use]
    pub fn new(provider: ProviderId, account: AccountId) -> Self {
        Self { provider, account }
    }
}

/// The deployment's shared answer to *"may that daemon hold my credentials?"*.
///
/// Configured (`keyring.group_secret`), never derived and never negotiated. A daemon without one
/// syncs with **nobody** — the absent case is deliberately not "sync with everyone", which is the
/// shape this stack exists to remove.
///
/// Never `Debug`-printed: see the manual implementation below.
#[derive(Clone, PartialEq, Eq)]
pub struct GroupSecret(String);

impl GroupSecret {
    #[must_use]
    pub fn new(secret: impl Into<String>) -> Self {
        Self(secret.into())
    }

    /// The proof this secret produces for `challenge`, which is what actually travels.
    ///
    /// The secret itself is never advertised: a peer proves it holds the same one, so an observer
    /// who captures an advertisement learns nothing it can configure a daemon with.
    #[must_use]
    pub fn proof_for(&self, _challenge: &[u8]) -> Vec<u8> {
        todo!("TODO(keyring 6/9): implement — HMAC the challenge under the group secret")
    }

    /// Whether `proof` is the one this secret would have produced for `challenge`.
    ///
    /// Compared in constant time; a byte-by-byte `==` over a MAC is a timing oracle for the secret.
    #[must_use]
    pub fn verifies(&self, _challenge: &[u8], _proof: &[u8]) -> bool {
        todo!("TODO(keyring 6/9): implement — recompute and compare in constant time")
    }
}

impl std::fmt::Debug for GroupSecret {
    /// Prints the type and nothing else. A derived `Debug` would put the deployment's authorisation
    /// secret in every log line that formats a struct holding one.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GroupSecret(<redacted>)")
    }
}

/// What a daemon says about itself so peers can decide whether to sync with it.
///
/// Carries no secret: the group secret appears only as a proof over a challenge, and the transport
/// key is the *public* half. An observer who captures one of these learns which daemons are
/// present, which is what a room already tells them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerAdvertisement {
    pub peer: PeerId,
    /// `#keyring` 1/9's key id, resolved to a public key through its `KeyDirectory`.
    pub signing_key_id: String,
    /// The X25519 public half records are wrapped to. Signed by the identity key, so substituting
    /// it is a signature failure rather than a silent downgrade to an attacker's key.
    pub transport_public_key: Vec<u8>,
    /// The challenge this advertisement's group proof answers. Fresh per advertisement, so a
    /// captured proof cannot be replayed against a later one.
    pub challenge: Vec<u8>,
    /// [`GroupSecret::proof_for`] over `challenge`.
    pub group_proof: Vec<u8>,
}

/// An advertisement plus the identity signature over it.
///
/// Separate from [`PeerAdvertisement`] so that "the bytes that were signed" is a type rather than a
/// convention — a signature checked against a re-serialisation that differs by a field order is a
/// check that passes for the wrong reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedAdvertisement {
    pub advertisement: PeerAdvertisement,
    /// Ed25519 over the canonical encoding of `advertisement`.
    pub signature: Vec<u8>,
}

/// Records sealed for exactly one recipient.
///
/// **Nothing in here is readable by the sender's vault key, and nothing in here is a key.** The
/// payload is sealed under a secret derived from this daemon's [`VaultTransportKey`] and the
/// recipient's public half, and the recipient re-seals the contents under *its own* vault key
/// before they touch its disk.
///
/// [`VaultTransportKey`]: crate::transport_key::VaultTransportKey
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrappedRecords {
    /// Who this was sealed for. A payload delivered to anyone else does not open.
    pub recipient: PeerId,
    /// The sender's X25519 public half, so the recipient can derive the same shared secret.
    pub sender_transport_public_key: Vec<u8>,
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

/// What a recipient says it did with a payload.
///
/// An ack names the slots it took, which is what lets the sender's journal say
/// [`SyncStatus::Acknowledged`] about a record rather than about a message.
///
/// [`SyncStatus::Acknowledged`]: crate::journal::SyncStatus::Acknowledged
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ack {
    pub from: PeerId,
    /// The slots the recipient retained.
    pub accepted: Vec<RecordKey>,
    /// The slots the recipient already held a later answer for. Not a failure — this is
    /// last-writer-wins working — but the sender records it so a person can see why a record did
    /// not take.
    pub superseded: Vec<RecordKey>,
}

/// Why a transport could not carry something.
///
/// Distinct from a refusal: a refusal is the peer group working as configured, and this is the
/// network. Collapsing them would put `Undeliverable` and `Refused(GroupSecretMismatch)` in one
/// bucket, and the remedies are "wait" and "fix your configuration".
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransportError {
    /// This daemon has no live connection to the peer group at all.
    #[error("not connected to the peer group")]
    NotConnected,

    /// The peer is not currently reachable. It may be later, unchanged.
    #[error("peer {0} is not reachable")]
    PeerUnreachable(PeerId),

    /// Anything else the transport ran into, named verbatim.
    #[error("{0}")]
    Io(String),
}

/// How this daemon reaches its peers.
///
/// Implemented by `tddy_daemon_livekit::LiveKitPeerTransport` in production and by an in-memory
/// fake in this crate's tests — a fake peer that can be made to fail exactly one check, which is
/// the only way the "neither check alone" properties can be asserted at all.
pub trait PeerTransport: Send + Sync {
    /// Publish this daemon's advertisement to the peer group.
    fn advertise(&self, ad: SignedAdvertisement) -> Result<(), TransportError>;

    /// Every advertisement the transport currently sees, this daemon's excluded.
    ///
    /// **Unfiltered**: these are the peers that are *present*, not the peers that are *admitted*.
    /// The two checks are [`SyncEngine`]'s, and a transport that pre-filtered would be a second
    /// place authorisation is decided.
    ///
    /// [`SyncEngine`]: crate::engine::SyncEngine
    fn peers(&self) -> Vec<SignedAdvertisement>;

    /// Deliver a sealed payload to one peer and return what it says it did with it.
    fn send(&self, to: &PeerId, payload: WrappedRecords) -> Result<Ack, TransportError>;
}
