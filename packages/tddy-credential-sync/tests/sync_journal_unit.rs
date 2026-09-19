//! What each node knows about where it stands.
//!
//! The journal is keyed per `(peer, record)` because *"this peer is behind"* and *"this account
//! will not propagate to anybody"* are different problems with different remedies, and a status
//! collapsed to the peer cannot tell them apart.

mod support;

use pretty_assertions::assert_eq;
use support::*;
use tddy_credential_sync::{JournalEntry, PeerId, RefusalReason, SyncJournal, SyncStatus};

fn the_github_slot() -> tddy_credential_sync::RecordKey {
    the_slot_of(&a_credential("github", "operator", "gho_the_token"))
}

fn the_cloudflare_slot() -> tddy_credential_sync::RecordKey {
    the_slot_of(&a_credential("cloudflare", "the-zone", "cf_the_token"))
}

#[test]
fn a_journal_that_has_decided_nothing_about_a_pair_reports_no_status() {
    // Given a journal that has recorded nothing
    let journal = SyncJournal::new();

    // When one pair is looked up
    let status = journal.status(&PeerId::new("peer-a"), &the_github_slot());

    // Then there is no answer — distinct from Pending, which means the engine has looked
    assert_eq!(status, None);
}

#[test]
fn the_journal_reports_what_was_sent_to_a_peer() {
    // Given a journal told that one account went to one peer
    let mut journal = SyncJournal::new();

    // When the send is noted
    journal.note(
        &PeerId::new("peer-a"),
        &the_github_slot(),
        SyncStatus::Sent,
        1_758_240_100,
    );

    // Then that is what it reports for the pair
    assert_eq!(
        journal.status(&PeerId::new("peer-a"), &the_github_slot()),
        Some(&SyncStatus::Sent)
    );
}

#[test]
fn a_pairs_later_status_replaces_its_earlier_one() {
    // Given a pair already noted as sent
    let mut journal = SyncJournal::new();
    journal.note(
        &PeerId::new("peer-a"),
        &the_github_slot(),
        SyncStatus::Sent,
        1_758_240_100,
    );

    // When the peer acknowledges it
    journal.note(
        &PeerId::new("peer-a"),
        &the_github_slot(),
        SyncStatus::Acknowledged,
        1_758_240_200,
    );

    // Then the journal holds one line for the pair, carrying the later answer
    assert_eq!(
        journal.entries(),
        vec![&JournalEntry {
            peer: PeerId::new("peer-a"),
            record: the_github_slot(),
            status: SyncStatus::Acknowledged,
            at: 1_758_240_200,
        }]
    );
}

#[test]
fn the_journal_reports_one_accounts_standing_with_every_peer() {
    // Given one account that reached one peer and was refused by another
    let mut journal = SyncJournal::new();
    journal.note(
        &PeerId::new("peer-a"),
        &the_github_slot(),
        SyncStatus::Acknowledged,
        1_758_240_100,
    );
    journal.note(
        &PeerId::new("peer-b"),
        &the_github_slot(),
        SyncStatus::Refused(RefusalReason::GroupSecretMismatch),
        1_758_240_100,
    );
    journal.note(
        &PeerId::new("peer-a"),
        &the_cloudflare_slot(),
        SyncStatus::Pending,
        1_758_240_100,
    );

    // When that account's standing is read
    let standing = journal.for_record(&the_github_slot());

    // Then both peers are reported, and the other account is not
    assert_eq!(
        standing,
        vec![
            &JournalEntry {
                peer: PeerId::new("peer-a"),
                record: the_github_slot(),
                status: SyncStatus::Acknowledged,
                at: 1_758_240_100,
            },
            &JournalEntry {
                peer: PeerId::new("peer-b"),
                record: the_github_slot(),
                status: SyncStatus::Refused(RefusalReason::GroupSecretMismatch),
                at: 1_758_240_100,
            },
        ]
    );
}

#[test]
fn a_peer_the_transport_could_not_reach_is_journaled_as_undeliverable_rather_than_refused() {
    // Given a journal recording a transport failure for an admitted peer
    let mut journal = SyncJournal::new();

    // When the failure is noted
    journal.note(
        &PeerId::new("peer-a"),
        &the_github_slot(),
        SyncStatus::Undeliverable,
        1_758_240_100,
    );

    // Then it reads as the network and not as the configuration — the remedies differ
    assert_eq!(
        journal.status(&PeerId::new("peer-a"), &the_github_slot()),
        Some(&SyncStatus::Undeliverable)
    );
}
