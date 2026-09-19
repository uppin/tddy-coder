//! (Shared by every test binary in this crate, so each one uses only part of it.)
#![allow(dead_code)]

//! The fake peer group every test in this crate runs against.
//!
//! **A fake and not a mock.** The engine's interesting properties are all about what a peer failing
//! exactly one check does *not* receive, so a test needs a peer group it can put into a precise
//! state and then read back — not a recording of which methods were called. This one holds its
//! advertisements and its deliveries in memory and answers questions about them.
//!
//! It is also why [`PeerTransport`] is a port at all: a real LiveKit room cannot be made to fail one
//! check at a time.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use tddy_credential_sync::{
    Ack, GroupSecret, IdentityVerifier, PeerAdvertisement, PeerId, PeerTransport, RecordKey,
    RefusalReason, SignedAdvertisement, TransportError, WrappedRecords,
};
use tddy_credentials::{
    AccountId, CredentialRecord, ProviderId, Tombstone, VaultEntry, FIRST_VERSION,
};

/// The deployment's configured secret, held by every daemon that is meant to sync.
pub fn the_group_secret() -> GroupSecret {
    GroupSecret::new("the-deployment-group-secret")
}

/// A second deployment's secret. A daemon holding this one is authorised by nobody here.
pub fn another_group_secret() -> GroupSecret {
    GroupSecret::new("some-other-deployments-secret")
}

/// The challenge a peer's advertisement answers. Fixed here so a test states one thing at a time.
pub fn a_challenge() -> Vec<u8> {
    b"the-challenge-this-advertisement-answers".to_vec()
}

/// An advertisement from `peer`, proving `secret` and naming `key_id` as its identity.
pub fn an_advertisement_from(peer: &str, key_id: &str, secret: &GroupSecret) -> PeerAdvertisement {
    let challenge = a_challenge();
    let group_proof = secret.proof_for(&challenge);

    PeerAdvertisement {
        peer: PeerId::new(peer),
        signing_key_id: key_id.to_string(),
        transport_public_key: vec![7u8; 32],
        challenge,
        group_proof,
    }
}

/// That advertisement, signed. The signature's bytes are what [`AKeyDirectory`] is told to accept.
pub fn signed_by(advertisement: PeerAdvertisement, signature: &str) -> SignedAdvertisement {
    SignedAdvertisement {
        advertisement,
        signature: signature.as_bytes().to_vec(),
    }
}

/// One credential, as a daemon's vault holds it. The secret is recognisable so a test that leaked
/// one would say so out loud.
pub fn a_credential(provider: &str, account: &str, secret: &str) -> CredentialRecord {
    CredentialRecord {
        provider: ProviderId::new(provider),
        account: AccountId::new(account),
        label: format!("{account} at {provider}"),
        secret: secret.to_string(),
        metadata: BTreeMap::new(),
        updated_at: 1_758_240_000,
        version: FIRST_VERSION,
    }
}

/// The same credential, edited at `updated_at` and carrying `version`.
pub fn an_edit_of(
    record: &CredentialRecord,
    secret: &str,
    updated_at: u64,
    version: u64,
) -> CredentialRecord {
    CredentialRecord {
        secret: secret.to_string(),
        updated_at,
        version,
        ..record.clone()
    }
}

/// The mark a removal of `record` leaves behind.
pub fn a_tombstone_for(record: &CredentialRecord, deleted_at: u64) -> Tombstone {
    Tombstone {
        provider: record.provider.clone(),
        account: record.account.clone(),
        version: record.version,
        deleted_at,
    }
}

/// The slot a record occupies.
pub fn the_slot_of(record: &CredentialRecord) -> RecordKey {
    RecordKey::new(record.provider.clone(), record.account.clone())
}

/// Resolves exactly the key ids it was given, and accepts exactly the signatures it was given.
///
/// Two sets rather than one, because "this signature does not verify" and "I have never heard of
/// this key" are different refusals with different remedies, and a directory that collapsed them
/// could not be used to test that the engine keeps them apart.
pub struct AKeyDirectory {
    known_key_ids: BTreeSet<String>,
    accepted_signatures: BTreeSet<Vec<u8>>,
}

impl AKeyDirectory {
    pub fn knowing(key_ids: &[&str]) -> Self {
        Self {
            known_key_ids: key_ids.iter().map(|id| (*id).to_string()).collect(),
            accepted_signatures: BTreeSet::new(),
        }
    }

    pub fn accepting(mut self, signatures: &[&str]) -> Self {
        self.accepted_signatures = signatures.iter().map(|s| s.as_bytes().to_vec()).collect();
        self
    }
}

impl IdentityVerifier for AKeyDirectory {
    fn verify(
        &self,
        signing_key_id: &str,
        _message: &[u8],
        signature: &[u8],
    ) -> Result<bool, RefusalReason> {
        self.known_key_ids
            .contains(signing_key_id)
            .then(|| self.accepted_signatures.contains(signature))
            .ok_or(RefusalReason::UnknownIdentity)
    }
}

/// A peer group held in memory: who is present, and what was delivered to whom.
pub struct AnInMemoryPeerGroup {
    present: Vec<SignedAdvertisement>,
    acks: BTreeMap<PeerId, Ack>,
    advertised: Mutex<Vec<SignedAdvertisement>>,
    delivered: Mutex<Vec<(PeerId, WrappedRecords)>>,
}

impl AnInMemoryPeerGroup {
    pub fn of(present: Vec<SignedAdvertisement>) -> Self {
        Self {
            present,
            acks: BTreeMap::new(),
            advertised: Mutex::new(Vec::new()),
            delivered: Mutex::new(Vec::new()),
        }
    }

    /// What `peer` answers when something is delivered to it.
    pub fn acknowledging(mut self, peer: &str, ack: Ack) -> Self {
        self.acks.insert(PeerId::new(peer), ack);
        self
    }

    /// Everything this daemon delivered, in order.
    pub fn deliveries(&self) -> Vec<(PeerId, WrappedRecords)> {
        self.delivered.lock().unwrap().clone()
    }

    /// The peers this daemon delivered anything to, deduplicated and ordered.
    pub fn recipients(&self) -> Vec<PeerId> {
        let mut peers: Vec<PeerId> = self
            .deliveries()
            .into_iter()
            .map(|(peer, _)| peer)
            .collect();
        peers.sort();
        peers.dedup();
        peers
    }

    /// What this daemon said about itself.
    pub fn advertisements(&self) -> Vec<SignedAdvertisement> {
        self.advertised.lock().unwrap().clone()
    }
}

impl PeerTransport for AnInMemoryPeerGroup {
    fn advertise(&self, ad: SignedAdvertisement) -> Result<(), TransportError> {
        self.advertised.lock().unwrap().push(ad);
        Ok(())
    }

    fn peers(&self) -> Vec<SignedAdvertisement> {
        self.present.clone()
    }

    fn send(&self, to: &PeerId, payload: WrappedRecords) -> Result<Ack, TransportError> {
        self.delivered.lock().unwrap().push((to.clone(), payload));
        self.acks
            .get(to)
            .cloned()
            .ok_or_else(|| TransportError::PeerUnreachable(to.clone()))
    }
}

/// An ack that accepted everything it was given.
pub fn an_ack_from(peer: &str, accepted: Vec<RecordKey>) -> Ack {
    Ack {
        from: PeerId::new(peer),
        accepted,
        superseded: Vec::new(),
    }
}

/// The vault contents a daemon publishes: its records and its tombstones.
pub fn a_vault_holding(entries: Vec<VaultEntry>) -> Vec<VaultEntry> {
    entries
}

/// Shorthand for the `Arc<dyn …>` the engine takes.
pub fn as_transport(group: Arc<AnInMemoryPeerGroup>) -> Arc<dyn PeerTransport> {
    group
}

/// Shorthand for the `Arc<dyn …>` the engine takes.
pub fn as_verifier(directory: AKeyDirectory) -> Arc<dyn IdentityVerifier> {
    Arc::new(directory)
}
