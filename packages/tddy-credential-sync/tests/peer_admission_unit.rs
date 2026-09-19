//! Who may hold this daemon's credentials — and, mostly, who may not.
//!
//! Room membership is not the gate. Two independent checks are, and the two tests that matter most
//! here are the ones asserting that **each check alone admits nobody**: the group secret without
//! the signature lets anyone replay an advertisement, and the signature without the group secret
//! authenticates a daemon nobody authorised.

mod support;

use std::sync::Arc;

use pretty_assertions::assert_eq;
use support::*;
use tddy_credential_sync::{
    PeerId, RefusalReason, SyncEngine, SyncError, SyncStatus, VaultTransportKey,
};
use tddy_credentials::{SecretBytes, VaultEntry};

const THE_PEERS_KEY: &str = "peer-signing-key";
const THE_PEERS_SIGNATURE: &str = "a-signature-the-directory-accepts";

fn an_engine_over(
    group: Arc<AnInMemoryPeerGroup>,
    directory: AKeyDirectory,
    secret: Option<tddy_credential_sync::GroupSecret>,
) -> SyncEngine {
    SyncEngine::new(
        secret,
        as_transport(group),
        as_verifier(directory),
        VaultTransportKey::from_secret(SecretBytes::new([3u8; 32])),
    )
}

#[test]
fn a_peer_proving_another_deployments_group_secret_receives_nothing() {
    // Given a peer whose signature verifies but whose group proof is another deployment's
    let peer = signed_by(
        an_advertisement_from("peer-a", THE_PEERS_KEY, &another_group_secret()),
        THE_PEERS_SIGNATURE,
    );
    let group = Arc::new(AnInMemoryPeerGroup::of(vec![peer]));
    let directory = AKeyDirectory::knowing(&[THE_PEERS_KEY]).accepting(&[THE_PEERS_SIGNATURE]);
    let mut engine = an_engine_over(group.clone(), directory, Some(the_group_secret()));

    // When this daemon publishes its vault
    let vault = a_vault_holding(vec![VaultEntry::Record(a_credential(
        "github",
        "operator",
        "gho_the_secret",
    ))]);
    engine.publish(&vault, 1_758_240_100).unwrap();

    // Then nothing was delivered to it
    assert_eq!(group.recipients(), Vec::<PeerId>::new());
}

#[test]
fn a_peer_proving_another_deployments_group_secret_is_journaled_as_a_group_secret_mismatch() {
    // Given a peer whose signature verifies but whose group proof is another deployment's
    let peer = signed_by(
        an_advertisement_from("peer-a", THE_PEERS_KEY, &another_group_secret()),
        THE_PEERS_SIGNATURE,
    );
    let group = Arc::new(AnInMemoryPeerGroup::of(vec![peer]));
    let directory = AKeyDirectory::knowing(&[THE_PEERS_KEY]).accepting(&[THE_PEERS_SIGNATURE]);
    let mut engine = an_engine_over(group, directory, Some(the_group_secret()));

    // When this daemon publishes one account
    let record = a_credential("github", "operator", "gho_the_secret");
    engine
        .publish(&[VaultEntry::Record(record.clone())], 1_758_240_100)
        .unwrap();

    // Then the journal says why that peer got nothing
    assert_eq!(
        engine
            .journal()
            .status(&PeerId::new("peer-a"), &the_slot_of(&record)),
        Some(&SyncStatus::Refused(RefusalReason::GroupSecretMismatch))
    );
}

#[test]
fn a_peer_whose_signature_does_not_verify_receives_nothing() {
    // Given a peer proving the right group secret under a signature the directory rejects
    let peer = signed_by(
        an_advertisement_from("peer-a", THE_PEERS_KEY, &the_group_secret()),
        "a-signature-nobody-produced",
    );
    let group = Arc::new(AnInMemoryPeerGroup::of(vec![peer]));
    let directory = AKeyDirectory::knowing(&[THE_PEERS_KEY]).accepting(&[THE_PEERS_SIGNATURE]);
    let mut engine = an_engine_over(group.clone(), directory, Some(the_group_secret()));

    // When this daemon publishes its vault
    let vault = a_vault_holding(vec![VaultEntry::Record(a_credential(
        "github",
        "operator",
        "gho_the_secret",
    ))]);
    engine.publish(&vault, 1_758_240_100).unwrap();

    // Then nothing was delivered to it
    assert_eq!(group.recipients(), Vec::<PeerId>::new());
}

#[test]
fn a_peer_whose_signature_does_not_verify_is_journaled_as_an_invalid_signature() {
    // Given a peer proving the right group secret under a signature the directory rejects
    let peer = signed_by(
        an_advertisement_from("peer-a", THE_PEERS_KEY, &the_group_secret()),
        "a-signature-nobody-produced",
    );
    let group = Arc::new(AnInMemoryPeerGroup::of(vec![peer]));
    let directory = AKeyDirectory::knowing(&[THE_PEERS_KEY]).accepting(&[THE_PEERS_SIGNATURE]);
    let mut engine = an_engine_over(group, directory, Some(the_group_secret()));

    // When this daemon publishes one account
    let record = a_credential("github", "operator", "gho_the_secret");
    engine
        .publish(&[VaultEntry::Record(record.clone())], 1_758_240_100)
        .unwrap();

    // Then the journal says the sender was not who it claimed
    assert_eq!(
        engine
            .journal()
            .status(&PeerId::new("peer-a"), &the_slot_of(&record)),
        Some(&SyncStatus::Refused(RefusalReason::SignatureInvalid))
    );
}

#[test]
fn holding_the_group_secret_alone_admits_nobody() {
    // Given a peer proving the deployment's group secret and nothing else
    let peer = signed_by(
        an_advertisement_from("peer-a", THE_PEERS_KEY, &the_group_secret()),
        "a-signature-nobody-produced",
    );
    let group = Arc::new(AnInMemoryPeerGroup::of(vec![peer.clone()]));
    let directory = AKeyDirectory::knowing(&[THE_PEERS_KEY]).accepting(&[THE_PEERS_SIGNATURE]);
    let engine = an_engine_over(group, directory, Some(the_group_secret()));

    // When that peer asks to be admitted
    let admission = engine.admit(&peer);

    // Then it is refused
    assert_eq!(admission, Err(RefusalReason::SignatureInvalid));
}

#[test]
fn a_verified_signature_alone_admits_nobody() {
    // Given a peer the directory vouches for, proving another deployment's group secret
    let peer = signed_by(
        an_advertisement_from("peer-a", THE_PEERS_KEY, &another_group_secret()),
        THE_PEERS_SIGNATURE,
    );
    let group = Arc::new(AnInMemoryPeerGroup::of(vec![peer.clone()]));
    let directory = AKeyDirectory::knowing(&[THE_PEERS_KEY]).accepting(&[THE_PEERS_SIGNATURE]);
    let engine = an_engine_over(group, directory, Some(the_group_secret()));

    // When that peer asks to be admitted
    let admission = engine.admit(&peer);

    // Then it is refused
    assert_eq!(admission, Err(RefusalReason::GroupSecretMismatch));
}

#[test]
fn a_peer_whose_signing_key_the_directory_has_never_published_is_refused_as_an_unknown_identity() {
    // Given a peer proving the group secret under a key nobody has published
    let peer = signed_by(
        an_advertisement_from("peer-a", "a-key-id-nobody-published", &the_group_secret()),
        THE_PEERS_SIGNATURE,
    );
    let group = Arc::new(AnInMemoryPeerGroup::of(vec![peer.clone()]));
    let directory = AKeyDirectory::knowing(&[THE_PEERS_KEY]).accepting(&[THE_PEERS_SIGNATURE]);
    let engine = an_engine_over(group, directory, Some(the_group_secret()));

    // When that peer asks to be admitted
    let admission = engine.admit(&peer);

    // Then it is refused for the reason that tells an operator to wait rather than to reconfigure
    assert_eq!(admission, Err(RefusalReason::UnknownIdentity));
}

#[test]
fn a_peer_passing_both_checks_is_admitted() {
    // Given a peer the directory vouches for, proving this deployment's group secret
    let peer = signed_by(
        an_advertisement_from("peer-a", THE_PEERS_KEY, &the_group_secret()),
        THE_PEERS_SIGNATURE,
    );
    let group = Arc::new(AnInMemoryPeerGroup::of(vec![peer.clone()]));
    let directory = AKeyDirectory::knowing(&[THE_PEERS_KEY]).accepting(&[THE_PEERS_SIGNATURE]);
    let engine = an_engine_over(group, directory, Some(the_group_secret()));

    // When that peer asks to be admitted
    let admission = engine.admit(&peer);

    // Then it is
    assert_eq!(admission, Ok(PeerId::new("peer-a")));
}

#[test]
fn a_daemon_with_no_group_secret_configured_syncs_with_nobody() {
    // Given a daemon with no keyring.group_secret, and a peer that would otherwise be admitted
    let peer = signed_by(
        an_advertisement_from("peer-a", THE_PEERS_KEY, &the_group_secret()),
        THE_PEERS_SIGNATURE,
    );
    let group = Arc::new(AnInMemoryPeerGroup::of(vec![peer]));
    let directory = AKeyDirectory::knowing(&[THE_PEERS_KEY]).accepting(&[THE_PEERS_SIGNATURE]);
    let mut engine = an_engine_over(group.clone(), directory, None);

    // When it tries to publish its vault
    let vault = a_vault_holding(vec![VaultEntry::Record(a_credential(
        "github",
        "operator",
        "gho_the_secret",
    ))]);
    let outcome = engine.publish(&vault, 1_758_240_100);

    // Then it is told why nothing happened, rather than quietly syncing with everyone present
    assert_eq!(outcome, Err(SyncError::NotConfigured));
}

#[test]
fn a_daemon_with_no_group_secret_configured_delivers_nothing() {
    // Given a daemon with no keyring.group_secret, and a peer that would otherwise be admitted
    let peer = signed_by(
        an_advertisement_from("peer-a", THE_PEERS_KEY, &the_group_secret()),
        THE_PEERS_SIGNATURE,
    );
    let group = Arc::new(AnInMemoryPeerGroup::of(vec![peer]));
    let directory = AKeyDirectory::knowing(&[THE_PEERS_KEY]).accepting(&[THE_PEERS_SIGNATURE]);
    let mut engine = an_engine_over(group.clone(), directory, None);

    // When it tries to publish its vault
    let vault = a_vault_holding(vec![VaultEntry::Record(a_credential(
        "github",
        "operator",
        "gho_the_secret",
    ))]);
    let _ = engine.publish(&vault, 1_758_240_100);

    // Then the peer group saw no delivery at all
    assert_eq!(group.recipients(), Vec::<PeerId>::new());
}
