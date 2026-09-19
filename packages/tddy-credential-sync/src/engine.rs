//! Both checks, the wrapping, and last-writer-wins.
//!
//! Everything that decides *whether* a peer receives a credential lives here, in one place, behind
//! a port on either side — a [`PeerTransport`] outward and an [`IdentityVerifier`] for 1/9's keys.
//! One place, because an authorisation decision made in two is an authorisation decision that will
//! eventually disagree with itself.

use std::sync::Arc;

use tddy_credentials::VaultEntry;

use crate::journal::{RefusalReason, SyncJournal};
use crate::transport::{
    Ack, GroupSecret, PeerId, PeerTransport, SignedAdvertisement, WrappedRecords,
};
use crate::transport_key::VaultTransportKey;

/// Resolves an advertised key id to the public key that must have signed the advertisement.
///
/// A **port**, and synchronous, where `#keyring` 1/9's `KeyDirectory` is a trait in
/// `tddy-daemon-auth` and `async`. Both properties are deliberate:
///
/// - **A port**, so this crate does not depend on `tddy-daemon-auth` — which would pull LiveKit,
///   tokio and `tddy-service` onto the dependency path of an engine that needs none of them, and
///   would repeat exactly what `heavy-dependency-livekit-peer-forwarding` measures.
/// - **Synchronous**, because admitting a peer is a decision, not I/O. The daemon's adapter resolves
///   the key through `KeyDirectory` and hands the answer in; an `async` check here would make every
///   reconciliation path `async` to accommodate a lookup that has already happened.
pub trait IdentityVerifier: Send + Sync {
    /// Whether `signature` is a valid Ed25519 signature over `message` by the daemon that owns
    /// `signing_key_id`.
    ///
    /// Returns `Ok(false)` for a bad signature and [`RefusalReason::UnknownIdentity`] for a key id
    /// the directory has not published. **Not the same answer**: the first is a peer that is not who
    /// it claims, the second is usually a peer that has not advertised yet.
    fn verify(
        &self,
        signing_key_id: &str,
        message: &[u8],
        signature: &[u8],
    ) -> Result<bool, RefusalReason>;
}

/// What to do with one slot when a peer's answer for it arrives.
///
/// `conflicted` is carried alongside the decision rather than replacing it: a conflict still has a
/// winner, and a person is owed both facts — which edit survived, and that there was another one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reconciliation {
    /// The incoming entry is the later answer; write it.
    TakeIncoming { conflicted: bool },
    /// This daemon already holds the later answer; the peer is behind.
    KeepLocal { conflicted: bool },
    /// Both sides hold the same answer already.
    AlreadyConverged,
}

/// Why a sync did not happen.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SyncError {
    /// No `keyring.group_secret` is configured, so this daemon syncs with nobody.
    ///
    /// An error and not a silent no-op: an operator who expected propagation is owed the reason,
    /// and the alternative reading of an absent secret — sync with everyone — is the property this
    /// stack exists to remove.
    #[error("no keyring.group_secret is configured; this daemon syncs with no peer")]
    NotConfigured,

    /// The transport could not carry it. Authorisation is unaffected.
    #[error(transparent)]
    Transport(#[from] crate::transport::TransportError),

    /// A payload addressed to this daemon did not open under the agreed secret.
    #[error("a payload addressed to this daemon did not authenticate")]
    Undecryptable,

    /// A payload opened and its contents were not records.
    #[error("a payload opened but did not contain vault entries: {0}")]
    Malformed(String),
}

/// Decides who receives a credential, seals it for them, and reconciles what comes back.
///
/// Holds the journal, because every one of those decisions produces a line in it and a journal
/// written from outside would record what a caller *thinks* happened.
pub struct SyncEngine {
    // TODO(keyring 6/9): implement
    _group_secret: Option<GroupSecret>,
    _transport: Arc<dyn PeerTransport>,
    _verifier: Arc<dyn IdentityVerifier>,
    _transport_key: VaultTransportKey,
    _journal: SyncJournal,
}

impl SyncEngine {
    /// Build an engine for this daemon.
    ///
    /// `group_secret` is `Option` because a daemon may legitimately be configured without one — a
    /// desktop install with no fleet — and that daemon must sync with nobody rather than be unable
    /// to start.
    #[must_use]
    pub fn new(
        group_secret: Option<GroupSecret>,
        transport: Arc<dyn PeerTransport>,
        verifier: Arc<dyn IdentityVerifier>,
        transport_key: VaultTransportKey,
    ) -> Self {
        Self {
            _group_secret: group_secret,
            _transport: transport,
            _verifier: verifier,
            _transport_key: transport_key,
            _journal: SyncJournal::new(),
        }
    }

    /// Whether this peer may hold this daemon's credentials.
    ///
    /// **Both checks, and both are required.** The group proof answers *may that daemon hold my
    /// credentials* and the signature answers *did that daemon send this*; either alone admits
    /// somebody it should not. There is no third path in and no "already in our room" shortcut.
    pub fn admit(&self, _ad: &SignedAdvertisement) -> Result<PeerId, RefusalReason> {
        todo!("TODO(keyring 6/9): implement — verify the signature, then the group proof")
    }

    /// Advertise this daemon, then send every admitted peer what it does not already have.
    ///
    /// `entries` is the whole vault including tombstones ([`SessionVault::entries`]), not the
    /// records: a deletion that is not sent is a deletion a peer undoes.
    ///
    /// Called **on join and on change**, never on a timer. A periodic sweep would re-send a record
    /// a peer has refused, over and over, and turn a configuration mistake into traffic.
    ///
    /// [`SessionVault::entries`]: tddy_credentials::SessionVault::entries
    pub fn publish(&mut self, _entries: &[VaultEntry], _now: u64) -> Result<(), SyncError> {
        todo!("TODO(keyring 6/9): implement — advertise, admit, wrap per recipient, send, journal")
    }

    /// Take delivery of a peer's payload.
    ///
    /// Returns what the caller must retain — **already reconciled against `local`**, so a caller
    /// cannot accidentally write a record that loses to one it already holds, and never the
    /// contents of the payload verbatim.
    pub fn receive(
        &mut self,
        _payload: WrappedRecords,
        _local: &[VaultEntry],
        _now: u64,
    ) -> Result<ReceiveOutcome, SyncError> {
        todo!(
            "TODO(keyring 6/9): implement — open, reconcile against local, journal, build the ack"
        )
    }

    /// What this daemon knows about where every record stands with every peer.
    #[must_use]
    pub fn journal(&self) -> &SyncJournal {
        &self._journal
    }

    /// Which of two answers for one slot survives.
    ///
    /// Pure and associated rather than a method, because it is the property both sides must compute
    /// identically — a reconciliation that consulted engine state would converge differently on the
    /// two daemons, which is the one thing last-writer-wins must not do.
    ///
    /// **A tombstone is an answer, not an absence.** It competes on `deleted_at` exactly as a record
    /// competes on `updated_at`, which is what stops a deleted account resurrecting from a peer that
    /// still holds it — and equally what lets a genuinely later re-link win.
    #[must_use]
    pub fn reconcile(_local: Option<&VaultEntry>, _incoming: &VaultEntry) -> Reconciliation {
        todo!(
            "TODO(keyring 6/9): implement — later written_at wins; equal versions mean a conflict"
        )
    }
}

/// What a received payload leaves behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiveOutcome {
    /// The entries the caller must seal under **its own** vault key and retain. The sender's key
    /// does not open them once this is done, which is the point.
    pub retain: Vec<VaultEntry>,
    /// What to hand back to the sender so its journal can say what happened.
    pub ack: Ack,
}
