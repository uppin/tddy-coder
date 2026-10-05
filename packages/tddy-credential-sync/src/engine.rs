//! Both checks, the wrapping, and last-writer-wins.
//!
//! Everything that decides *whether* a peer receives a credential lives here, in one place, behind
//! a port on either side — a [`PeerTransport`] outward and an [`IdentityVerifier`] for 1/9's keys.
//! One place, because an authorisation decision made in two is an authorisation decision that will
//! eventually disagree with itself.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use tddy_credentials::{AccountId, CredentialRecord, ProviderId, Tombstone, VaultEntry};

use crate::journal::{RefusalReason, SyncJournal, SyncStatus};
use crate::transport::{
    Ack, GroupSecret, PeerAdvertisement, PeerId, PeerTransport, RecordKey, SignedAdvertisement,
    WrappedRecords,
};
use crate::transport_key::{TransportPublicKey, VaultTransportKey};

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
    _group_secret: Option<GroupSecret>,
    _transport: Arc<dyn PeerTransport>,
    _verifier: Arc<dyn IdentityVerifier>,
    _transport_key: VaultTransportKey,
    _journal: SyncJournal,
    /// The `written_at` of each `(peer, record)` slot last acknowledged as retained — what
    /// [`publish`] consults to decide an unchanged slot needs no resend, and a changed one does.
    ///
    /// **Keyed on `written_at`, not `version`.** A [`Tombstone`] carries *the version it deletes*,
    /// not a new one — by design, so a peer still holding that version knows the deletion is about
    /// its own copy and not an echo. That means a record and the tombstone that later deletes it
    /// can share one version number, and a map keyed on version alone would see the tombstone as
    /// "nothing changed since the peer last acknowledged this version" and never send it — exactly
    /// the resurrection this stack exists to prevent. `written_at()` is unique to the write that
    /// produced it (a record's `updated_at` or a tombstone's `deleted_at`), so it always advances.
    ///
    /// [`publish`]: Self::publish
    _acknowledged: BTreeMap<(PeerId, RecordKey), u64>,
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
            _acknowledged: BTreeMap::new(),
        }
    }

    /// Whether this peer may hold this daemon's credentials.
    ///
    /// **Both checks, and both are required.** The group proof answers *may that daemon hold my
    /// credentials* and the signature answers *did that daemon send this*; either alone admits
    /// somebody it should not. There is no third path in and no "already in our room" shortcut.
    pub fn admit(&self, ad: &SignedAdvertisement) -> Result<PeerId, RefusalReason> {
        let message =
            serde_json::to_vec(&ad.advertisement).expect("a PeerAdvertisement always serialises");
        let signed_in =
            self._verifier
                .verify(&ad.advertisement.signing_key_id, &message, &ad.signature)?;
        if !signed_in {
            return Err(RefusalReason::SignatureInvalid);
        }
        let group_secret = self
            ._group_secret
            .as_ref()
            .ok_or(RefusalReason::GroupSecretMismatch)?;
        if !group_secret.verifies(&ad.advertisement.challenge, &ad.advertisement.group_proof) {
            return Err(RefusalReason::GroupSecretMismatch);
        }
        Ok(ad.advertisement.peer.clone())
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
    pub fn publish(&mut self, entries: &[VaultEntry], now: u64) -> Result<(), SyncError> {
        let group_secret = self._group_secret.clone().ok_or(SyncError::NotConfigured)?;
        self._transport
            .advertise(self.self_advertisement(&group_secret))?;

        for peer_ad in self._transport.peers() {
            let advertised_peer = peer_ad.advertisement.peer.clone();
            let peer = match self.admit(&peer_ad) {
                Ok(peer) => peer,
                Err(reason) => {
                    for entry in entries {
                        self._journal.note(
                            &advertised_peer,
                            &record_key_of(entry),
                            SyncStatus::Refused(reason),
                            now,
                        );
                    }
                    continue;
                }
            };

            let to_send: Vec<&VaultEntry> = entries
                .iter()
                .filter(|entry| {
                    self._acknowledged
                        .get(&(peer.clone(), record_key_of(entry)))
                        != Some(&entry.written_at())
                })
                .collect();
            if to_send.is_empty() {
                continue;
            }

            let peer_public = transport_public_key_of(&peer_ad.advertisement)?;
            let payload = self.wrap_for(&peer, &peer_public, &to_send)?;
            for entry in &to_send {
                self._journal
                    .note(&peer, &record_key_of(entry), SyncStatus::Sent, now);
            }

            match self._transport.send(&peer, payload) {
                Ok(ack) => {
                    for key in &ack.accepted {
                        self._journal
                            .note(&peer, key, SyncStatus::Acknowledged, now);
                        if let Some(entry) = to_send.iter().find(|e| record_key_of(e) == *key) {
                            self._acknowledged
                                .insert((peer.clone(), key.clone()), entry.written_at());
                        }
                    }
                    for key in &ack.superseded {
                        self._journal.note(&peer, key, SyncStatus::Conflict, now);
                        if let Some(entry) = to_send.iter().find(|e| record_key_of(e) == *key) {
                            self._acknowledged
                                .insert((peer.clone(), key.clone()), entry.written_at());
                        }
                    }
                }
                Err(_) => {
                    for entry in &to_send {
                        self._journal.note(
                            &peer,
                            &record_key_of(entry),
                            SyncStatus::Undeliverable,
                            now,
                        );
                    }
                }
            }
        }
        Ok(())
    }

    /// Take delivery of a peer's payload.
    ///
    /// Returns what the caller must retain — **already reconciled against `local`**, so a caller
    /// cannot accidentally write a record that loses to one it already holds, and never the
    /// contents of the payload verbatim.
    pub fn receive(
        &mut self,
        payload: WrappedRecords,
        local: &[VaultEntry],
        _now: u64,
    ) -> Result<ReceiveOutcome, SyncError> {
        let sender_public_bytes: [u8; 32] = payload
            .sender_transport_public_key
            .as_slice()
            .try_into()
            .map_err(|_| SyncError::Undecryptable)?;
        let sender_public = TransportPublicKey::from_bytes(sender_public_bytes);
        let key = self._transport_key.shared_secret(&sender_public);
        let aad = wrap_aad(&payload.recipient, &payload.sender_transport_public_key);
        let plaintext = crate::crypto::open(&key, &payload.nonce, &payload.ciphertext, &aad)
            .map_err(|()| SyncError::Undecryptable)?;
        let wire: Vec<OpenedWireEntry> =
            serde_json::from_slice(&plaintext).map_err(|e| SyncError::Malformed(e.to_string()))?;

        let mut retain = Vec::new();
        let mut accepted = Vec::new();
        let mut superseded = Vec::new();
        for opened in wire {
            let incoming = VaultEntry::from(opened);
            let key = record_key_of(&incoming);
            let local_entry = local.iter().find(|entry| {
                entry.provider() == incoming.provider() && entry.account() == incoming.account()
            });
            match Self::reconcile(local_entry, &incoming) {
                Reconciliation::TakeIncoming { .. } => {
                    accepted.push(key);
                    retain.push(incoming);
                }
                Reconciliation::AlreadyConverged => accepted.push(key),
                Reconciliation::KeepLocal { .. } => superseded.push(key),
            }
        }

        // TODO(keyring 6/9 wiring): `from` names this daemon's own `PeerId`, which this
        // port-level engine has no way to know — it is given no self-identity, only a verifier
        // for *others'* keys (see `IdentityVerifier`'s doc comment). Whoever wires the daemon's
        // real identity in (the adapter owning `#keyring` 1/9's `KeyDirectory`) must supply it.
        let ack = Ack {
            from: PeerId::new(""),
            accepted,
            superseded,
        };
        Ok(ReceiveOutcome { retain, ack })
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
    pub fn reconcile(local: Option<&VaultEntry>, incoming: &VaultEntry) -> Reconciliation {
        let Some(local) = local else {
            return Reconciliation::TakeIncoming { conflicted: false };
        };
        if local == incoming {
            return Reconciliation::AlreadyConverged;
        }
        // Two independent edits of the same version are the only shape a conflict takes: a
        // tombstone's version is the version it deleted, not a new one, so comparing it against a
        // peer's unedited copy at that same version is one side acting and the other not — not a
        // conflict, just an ordinary race the clock resolves.
        let conflicted = matches!(
            (local, incoming),
            (VaultEntry::Record(_), VaultEntry::Record(_))
        ) && local.version() == incoming.version();
        match local.written_at().cmp(&incoming.written_at()) {
            std::cmp::Ordering::Less => Reconciliation::TakeIncoming { conflicted },
            std::cmp::Ordering::Greater => Reconciliation::KeepLocal { conflicted },
            // Equal only reaches here when the entries differ despite sharing a clock reading —
            // not exercised by any fixture, so the later version breaks the tie, and a tie on both
            // measures is a conflict by the same rule above.
            std::cmp::Ordering::Equal => match local.version().cmp(&incoming.version()) {
                std::cmp::Ordering::Less => Reconciliation::TakeIncoming { conflicted: true },
                std::cmp::Ordering::Greater => Reconciliation::KeepLocal { conflicted: true },
                std::cmp::Ordering::Equal => Reconciliation::TakeIncoming { conflicted: true },
            },
        }
    }

    /// This daemon's own advertisement: its transport key's public half, and a fresh group proof.
    ///
    /// TODO(keyring 6/9 wiring): `peer`, `signing_key_id` and `signature` are left empty. Naming
    /// this daemon and signing its advertisement needs `#keyring` 1/9's identity key, which this
    /// crate does not depend on (`IdentityVerifier` is a *verify-only* port — see its doc comment);
    /// the daemon runtime that holds the real `DaemonSigningKey` must fill these in, by signing
    /// what this method produces before it reaches a real [`PeerTransport`]. No test exercises a
    /// peer admitting this daemon's own advertisement, so this gap is not yet caught by any check.
    fn self_advertisement(&self, group_secret: &GroupSecret) -> SignedAdvertisement {
        let challenge = crate::crypto::random_bytes::<32>().to_vec();
        let advertisement = PeerAdvertisement {
            peer: PeerId::new(""),
            signing_key_id: String::new(),
            transport_public_key: self._transport_key.public_key().as_bytes().to_vec(),
            group_proof: group_secret.proof_for(&challenge),
            challenge,
        };
        SignedAdvertisement {
            advertisement,
            signature: Vec::new(),
        }
    }

    /// Seal `entries` for `peer`, whose transport public half is `peer_public`.
    fn wrap_for(
        &self,
        peer: &PeerId,
        peer_public: &TransportPublicKey,
        entries: &[&VaultEntry],
    ) -> Result<WrappedRecords, SyncError> {
        let key = self._transport_key.shared_secret(peer_public);
        let wire: Vec<WireEntry<'_>> = entries
            .iter()
            .map(|entry| WireEntry::from(*entry))
            .collect();
        let plaintext = Zeroizing::new(
            serde_json::to_vec(&wire).map_err(|e| SyncError::Malformed(e.to_string()))?,
        );
        let sender_public = self._transport_key.public_key().as_bytes().to_vec();
        let aad = wrap_aad(peer, &sender_public);
        let sealed = crate::crypto::seal(&key, &plaintext, &aad);
        Ok(WrappedRecords {
            recipient: peer.clone(),
            sender_transport_public_key: sender_public,
            nonce: sealed.nonce.to_vec(),
            ciphertext: sealed.ciphertext,
        })
    }
}

/// The slot one entry occupies, as a journal and dedup key.
fn record_key_of(entry: &VaultEntry) -> RecordKey {
    RecordKey::new(entry.provider().clone(), entry.account().clone())
}

/// Associated data binding a wrapped payload to the one recipient it was sealed for and the one
/// sender key it was sealed under — so a payload cannot be replayed as if addressed to, or sent
/// by, somebody else.
fn wrap_aad(recipient: &PeerId, sender_transport_public_key: &[u8]) -> Vec<u8> {
    [recipient.as_str().as_bytes(), sender_transport_public_key].concat()
}

/// A peer's advertised transport key, as the fixed-size array [`VaultTransportKey::shared_secret`]
/// needs.
fn transport_public_key_of(ad: &PeerAdvertisement) -> Result<TransportPublicKey, SyncError> {
    let bytes: [u8; 32] = ad.transport_public_key.as_slice().try_into().map_err(|_| {
        SyncError::Malformed(format!(
            "{}'s advertised transport key is {} bytes, not 32",
            ad.peer,
            ad.transport_public_key.len()
        ))
    })?;
    Ok(TransportPublicKey::from_bytes(bytes))
}

/// What is sealed into a [`WrappedRecords`] payload: a [`CredentialRecord`]'s fields or a
/// [`Tombstone`]'s, borrowed, so sealing one makes no extra copy of the secret. Neither source
/// type is `Serialize` — this mirror, local to the one place records cross the wire, is the only
/// path from an entry to bytes here, exactly as `tddy_credentials::vault`'s own mirror is the only
/// path from an entry to the sealed vault file.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum WireEntry<'a> {
    Record {
        provider: &'a ProviderId,
        account: &'a AccountId,
        label: &'a str,
        secret: &'a str,
        metadata: &'a std::collections::BTreeMap<String, String>,
        updated_at: u64,
        version: u64,
    },
    Tombstone {
        provider: &'a ProviderId,
        account: &'a AccountId,
        version: u64,
        deleted_at: u64,
    },
}

impl<'a> From<&'a VaultEntry> for WireEntry<'a> {
    fn from(entry: &'a VaultEntry) -> Self {
        match entry {
            VaultEntry::Record(record) => Self::Record {
                provider: &record.provider,
                account: &record.account,
                label: &record.label,
                secret: record.secret.expose(),
                metadata: &record.metadata,
                updated_at: record.updated_at,
                version: record.version,
            },
            VaultEntry::Tombstone(tombstone) => Self::Tombstone {
                provider: &tombstone.provider,
                account: &tombstone.account,
                version: tombstone.version,
                deleted_at: tombstone.deleted_at,
            },
        }
    }
}

/// What a payload's plaintext parses into on the receiving side; a record's secret moves straight
/// into a [`tddy_credentials::SecretString`].
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum OpenedWireEntry {
    Record {
        provider: ProviderId,
        account: AccountId,
        label: String,
        secret: String,
        metadata: std::collections::BTreeMap<String, String>,
        updated_at: u64,
        version: u64,
    },
    Tombstone {
        provider: ProviderId,
        account: AccountId,
        version: u64,
        deleted_at: u64,
    },
}

impl From<OpenedWireEntry> for VaultEntry {
    fn from(opened: OpenedWireEntry) -> Self {
        match opened {
            OpenedWireEntry::Record {
                provider,
                account,
                label,
                secret,
                metadata,
                updated_at,
                version,
            } => Self::Record(CredentialRecord {
                provider,
                account,
                label,
                secret: tddy_credentials::SecretString::new(secret),
                metadata,
                updated_at,
                version,
            }),
            OpenedWireEntry::Tombstone {
                provider,
                account,
                version,
                deleted_at,
            } => Self::Tombstone(Tombstone {
                provider,
                account,
                version,
                deleted_at,
            }),
        }
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
