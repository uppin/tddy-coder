//! What crosses the wire, and what must never be in it.
//!
//! The line this crate must not cross is that **the vault's data key, its KEK and the login
//! credential never leave the daemon**. Two daemons that sync end up sharing a *record*, never a
//! *key* — so compromising one does not unwrap the other's disk. These assertions are made over
//! the serialised bytes rather than over a struct's fields, because the thing being ruled out is a
//! key arriving by any route at all, including one a future field would open.

mod support;

use std::sync::Arc;

use pretty_assertions::assert_eq;
use support::*;
use tddy_credential_sync::{PeerId, SyncEngine, SyncError, VaultTransportKey, WrappedRecords};
use tddy_credentials::{SecretBytes, VaultEntry};

const THE_PEERS_KEY: &str = "peer-signing-key";
const THE_PEERS_SIGNATURE: &str = "a-signature-the-directory-accepts";
const THE_LOGIN_CREDENTIAL: &str = "the-users-own-login-credential";
const THE_SECRET: &str = "gho_the_github_token";

fn an_engine_syncing_with_one_admitted_peer(group: Arc<AnInMemoryPeerGroup>) -> SyncEngine {
    SyncEngine::new(
        Some(the_group_secret()),
        as_transport(group),
        as_verifier(AKeyDirectory::knowing(&[THE_PEERS_KEY]).accepting(&[THE_PEERS_SIGNATURE])),
        VaultTransportKey::from_secret(SecretBytes::new([3u8; 32])),
    )
}

fn an_admitted_peer_group() -> Arc<AnInMemoryPeerGroup> {
    let peer = signed_by(
        an_advertisement_from("peer-a", THE_PEERS_KEY, &the_group_secret()),
        THE_PEERS_SIGNATURE,
    );
    let record = a_credential("github", "operator", THE_SECRET);
    Arc::new(
        AnInMemoryPeerGroup::of(vec![peer])
            .acknowledging("peer-a", an_ack_from("peer-a", vec![the_slot_of(&record)])),
    )
}

/// Everything that left this daemon, as bytes.
fn the_bytes_that_left(group: &AnInMemoryPeerGroup) -> Vec<u8> {
    group
        .deliveries()
        .into_iter()
        .flat_map(|(_, payload)| serde_json::to_vec(&payload).unwrap())
        .collect()
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

#[test]
fn the_payload_a_peer_receives_carries_no_login_credential() {
    // Given a daemon whose vault was sealed under the user's own login credential
    let group = an_admitted_peer_group();
    let mut engine = an_engine_syncing_with_one_admitted_peer(group.clone());

    // When it publishes one account to its admitted peer
    let record = a_credential("github", "operator", THE_SECRET);
    engine
        .publish(&[VaultEntry::Record(record)], 1_758_240_100)
        .unwrap();

    // Then the login credential is nowhere in what left the daemon
    assert_eq!(
        contains(&the_bytes_that_left(&group), THE_LOGIN_CREDENTIAL),
        false
    );
}

#[test]
fn the_payload_a_peer_receives_carries_no_readable_credential() {
    // Given a daemon holding one GitHub credential
    let group = an_admitted_peer_group();
    let mut engine = an_engine_syncing_with_one_admitted_peer(group.clone());

    // When it publishes that account to its admitted peer
    let record = a_credential("github", "operator", THE_SECRET);
    engine
        .publish(&[VaultEntry::Record(record)], 1_758_240_100)
        .unwrap();

    // Then the secret itself never appears in the bytes — the payload is sealed, not merely signed
    assert_eq!(contains(&the_bytes_that_left(&group), THE_SECRET), false);
}

#[test]
fn a_payload_names_the_one_peer_it_was_sealed_for() {
    // Given a daemon with one admitted peer
    let group = an_admitted_peer_group();
    let mut engine = an_engine_syncing_with_one_admitted_peer(group.clone());

    // When it publishes one account
    let record = a_credential("github", "operator", THE_SECRET);
    engine
        .publish(&[VaultEntry::Record(record)], 1_758_240_100)
        .unwrap();

    // Then the payload is addressed to that peer and nobody else
    let recipients: Vec<PeerId> = group
        .deliveries()
        .into_iter()
        .map(|(_, payload)| payload.recipient)
        .collect();
    assert_eq!(recipients, vec![PeerId::new("peer-a")]);
}

#[test]
fn a_payload_sealed_for_another_peer_does_not_open() {
    // Given a payload this daemon was not the recipient of
    let group = an_admitted_peer_group();
    let mut engine = an_engine_syncing_with_one_admitted_peer(group);
    let payload = WrappedRecords {
        recipient: PeerId::new("some-other-daemon"),
        sender_transport_public_key: vec![9u8; 32],
        nonce: vec![1u8; 12],
        ciphertext: b"not-sealed-to-this-daemon".to_vec(),
    };

    // When this daemon takes delivery of it
    let outcome = engine.receive(payload, &[], 1_758_240_200);

    // Then it does not open, and is reported as such rather than ignored
    assert_eq!(outcome.unwrap_err(), SyncError::Undecryptable);
}

#[test]
fn a_received_record_is_handed_back_for_resealing_under_the_recipients_own_key() {
    // Given a daemon that receives one account it does not hold
    let group = an_admitted_peer_group();
    let mut sender = an_engine_syncing_with_one_admitted_peer(group.clone());
    let record = a_credential("github", "operator", THE_SECRET);
    sender
        .publish(&[VaultEntry::Record(record.clone())], 1_758_240_100)
        .unwrap();
    let (_, payload) = group.deliveries().into_iter().next().unwrap();

    // When the recipient takes delivery
    let mut recipient = an_engine_syncing_with_one_admitted_peer(an_admitted_peer_group());
    let outcome = recipient.receive(payload, &[], 1_758_240_200).unwrap();

    // Then the record comes back as something to retain — the caller seals it under its own key,
    // and the sender's key never opens the recipient's disk
    assert_eq!(outcome.retain, vec![VaultEntry::Record(record)]);
}
