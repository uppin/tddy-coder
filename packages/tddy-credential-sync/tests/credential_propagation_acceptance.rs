//! Two daemons, one room, and the third one that is not authorised.
//!
//! The acceptance shape the `#keyring` 6/9 changeset names: daemons sharing a group secret
//! converge; a daemon present in the same room without one receives nothing and appears in the
//! journal as refused; propagation happens **on join and on change**, and no sweep exists.
//!
//! The last test here is the boundary this node does *not* cross. `ListEligibleDaemons` and
//! `StartSession` forwarding keep the trust model they have — credential authorisation is an extra
//! gate, not a retrofit onto forwarding — so a peer refused for credentials is still a peer the
//! transport reports as present.

mod support;

use std::sync::Arc;

use pretty_assertions::assert_eq;
use support::*;
use tddy_credential_sync::{PeerId, RefusalReason, SyncEngine, SyncStatus, VaultTransportKey};
use tddy_credentials::{SecretBytes, VaultEntry};

const AUTHORISED_KEY: &str = "authorised-peer-key";
const AUTHORISED_SIGNATURE: &str = "the-authorised-peers-signature";
const OUTSIDER_KEY: &str = "outsider-peer-key";
const OUTSIDER_SIGNATURE: &str = "the-outsiders-signature";

fn the_directory() -> AKeyDirectory {
    AKeyDirectory::knowing(&[AUTHORISED_KEY, OUTSIDER_KEY])
        .accepting(&[AUTHORISED_SIGNATURE, OUTSIDER_SIGNATURE])
}

fn an_engine_over(group: Arc<AnInMemoryPeerGroup>) -> SyncEngine {
    SyncEngine::new(
        Some(the_group_secret()),
        as_transport(group),
        as_verifier(the_directory()),
        VaultTransportKey::from_secret(SecretBytes::new([3u8; 32])),
    )
}

/// The room: one daemon holding this deployment's secret, one holding another's.
fn a_room_with_an_authorised_peer_and_an_outsider(
    record: &tddy_credentials::CredentialRecord,
) -> Arc<AnInMemoryPeerGroup> {
    let authorised = signed_by(
        an_advertisement_from("authorised-daemon", AUTHORISED_KEY, &the_group_secret()),
        AUTHORISED_SIGNATURE,
    );
    let outsider = signed_by(
        an_advertisement_from("outsider-daemon", OUTSIDER_KEY, &another_group_secret()),
        OUTSIDER_SIGNATURE,
    );

    Arc::new(
        AnInMemoryPeerGroup::of(vec![authorised, outsider]).acknowledging(
            "authorised-daemon",
            an_ack_from("authorised-daemon", vec![the_slot_of(record)]),
        ),
    )
}

#[test]
fn a_daemon_sharing_the_group_secret_receives_the_account() {
    // Given a room holding one authorised daemon and one outsider
    let record = a_credential("github", "operator", "gho_the_token");
    let group = a_room_with_an_authorised_peer_and_an_outsider(&record);
    let mut engine = an_engine_over(group.clone());

    // When this daemon publishes its vault
    engine
        .publish(&[VaultEntry::Record(record)], 1_758_240_100)
        .unwrap();

    // Then only the authorised daemon was delivered to
    assert_eq!(group.recipients(), vec![PeerId::new("authorised-daemon")]);
}

#[test]
fn a_daemon_in_the_room_without_the_group_secret_is_journaled_as_refused() {
    // Given a room holding one authorised daemon and one outsider
    let record = a_credential("github", "operator", "gho_the_token");
    let group = a_room_with_an_authorised_peer_and_an_outsider(&record);
    let mut engine = an_engine_over(group);

    // When this daemon publishes its vault
    engine
        .publish(&[VaultEntry::Record(record.clone())], 1_758_240_100)
        .unwrap();

    // Then the outsider's refusal is recorded rather than passed over in silence
    assert_eq!(
        engine
            .journal()
            .status(&PeerId::new("outsider-daemon"), &the_slot_of(&record)),
        Some(&SyncStatus::Refused(RefusalReason::GroupSecretMismatch))
    );
}

#[test]
fn an_acknowledged_account_is_journaled_as_acknowledged() {
    // Given a room whose authorised daemon acknowledges what it receives
    let record = a_credential("github", "operator", "gho_the_token");
    let group = a_room_with_an_authorised_peer_and_an_outsider(&record);
    let mut engine = an_engine_over(group);

    // When this daemon publishes its vault
    engine
        .publish(&[VaultEntry::Record(record.clone())], 1_758_240_100)
        .unwrap();

    // Then the journal says the account landed, on that peer, by name
    assert_eq!(
        engine
            .journal()
            .status(&PeerId::new("authorised-daemon"), &the_slot_of(&record)),
        Some(&SyncStatus::Acknowledged)
    );
}

#[test]
fn a_deletion_propagates_to_the_peers_that_hold_the_account() {
    // Given an account this daemon has deleted
    let record = a_credential("github", "operator", "gho_the_token");
    let group = a_room_with_an_authorised_peer_and_an_outsider(&record);
    let mut engine = an_engine_over(group.clone());

    // When the vault — which now holds the tombstone, not the record — is published
    let tombstone = VaultEntry::Tombstone(a_tombstone_for(&record, 1_758_240_600));
    engine.publish(&[tombstone], 1_758_240_600).unwrap();

    // Then the authorised daemon was told, so it cannot send the record back
    assert_eq!(group.recipients(), vec![PeerId::new("authorised-daemon")]);
}

#[test]
fn publishing_an_unchanged_vault_a_second_time_delivers_nothing_further() {
    // Given a daemon that has already published its vault to an acknowledging peer
    let record = a_credential("github", "operator", "gho_the_token");
    let group = a_room_with_an_authorised_peer_and_an_outsider(&record);
    let mut engine = an_engine_over(group.clone());
    let vault = a_vault_holding(vec![VaultEntry::Record(record)]);
    engine.publish(&vault, 1_758_240_100).unwrap();

    // When nothing has changed and it publishes again
    engine.publish(&vault, 1_758_240_200).unwrap();

    // Then there is still exactly one delivery — a sweep would make this two, and would re-send
    // to a peer that refused, over and over
    assert_eq!(group.deliveries().len(), 1);
}

#[test]
fn a_changed_account_is_delivered_again() {
    // Given a daemon that has already published one account
    let record = a_credential("github", "operator", "gho_the_token");
    let group = a_room_with_an_authorised_peer_and_an_outsider(&record);
    let mut engine = an_engine_over(group.clone());
    engine
        .publish(&[VaultEntry::Record(record.clone())], 1_758_240_100)
        .unwrap();

    // When the account changes and the vault is published again
    let edited = an_edit_of(&record, "gho_the_rotated_token", 1_758_240_300, 2);
    engine
        .publish(&[VaultEntry::Record(edited)], 1_758_240_300)
        .unwrap();

    // Then the change went out
    assert_eq!(group.deliveries().len(), 2);
}

#[test]
fn this_daemon_advertises_itself_before_it_looks_for_peers() {
    // Given a room this daemon has not advertised itself into
    let record = a_credential("github", "operator", "gho_the_token");
    let group = a_room_with_an_authorised_peer_and_an_outsider(&record);
    let mut engine = an_engine_over(group.clone());

    // When it publishes its vault
    engine
        .publish(&[VaultEntry::Record(record)], 1_758_240_100)
        .unwrap();

    // Then it said who it is, so a peer can apply the same two checks to it
    assert_eq!(group.advertisements().len(), 1);
}

#[test]
fn a_peer_refused_for_credentials_is_still_present_in_the_room() {
    // Given a room holding one authorised daemon and one outsider
    let record = a_credential("github", "operator", "gho_the_token");
    let group = a_room_with_an_authorised_peer_and_an_outsider(&record);
    let mut engine = an_engine_over(group.clone());

    // When this daemon publishes its vault, refusing the outsider
    engine
        .publish(&[VaultEntry::Record(record)], 1_758_240_100)
        .unwrap();

    // Then the transport still reports both peers — credential authorisation is an extra gate,
    // and `ListEligibleDaemons` and `StartSession` forwarding are untouched by it
    let present: Vec<PeerId> = tddy_credential_sync::PeerTransport::peers(group.as_ref())
        .into_iter()
        .map(|ad| ad.advertisement.peer)
        .collect();
    assert_eq!(
        present,
        vec![
            PeerId::new("authorised-daemon"),
            PeerId::new("outsider-daemon")
        ]
    );
}
