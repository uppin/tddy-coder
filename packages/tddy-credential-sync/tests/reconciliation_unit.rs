//! Which of two answers for one account survives.
//!
//! Last-writer-wins on the one clock both variants share, computed identically on both daemons —
//! a reconciliation that consulted engine state would converge differently on the two sides, which
//! is the one thing this must not do.
//!
//! The test that matters most here is the tombstone one. A deletion that simply removed the record
//! would not survive a single sync: the peer that still holds it would send it straight back, and
//! the person's removal would silently undo itself.

mod support;

use pretty_assertions::assert_eq;
use support::*;
use tddy_credential_sync::{Reconciliation, SyncEngine};
use tddy_credentials::VaultEntry;

#[test]
fn the_later_of_two_edits_to_one_account_wins() {
    // Given this daemon's copy, and a peer's later edit of the same account
    let local = a_credential("github", "operator", "gho_the_older_token");
    let incoming = an_edit_of(&local, "gho_the_newer_token", 1_758_240_500, 2);

    // When the two are reconciled
    let outcome = SyncEngine::reconcile(
        Some(&VaultEntry::Record(local)),
        &VaultEntry::Record(incoming),
    );

    // Then the peer's edit is taken
    assert_eq!(outcome, Reconciliation::TakeIncoming { conflicted: false });
}

#[test]
fn the_earlier_of_two_edits_to_one_account_loses() {
    // Given this daemon's copy, and a peer's older edit of the same account
    let local = an_edit_of(
        &a_credential("github", "operator", "gho_the_older_token"),
        "gho_the_newer_token",
        1_758_240_500,
        2,
    );
    let incoming = a_credential("github", "operator", "gho_the_older_token");

    // When the two are reconciled
    let outcome = SyncEngine::reconcile(
        Some(&VaultEntry::Record(local)),
        &VaultEntry::Record(incoming),
    );

    // Then this daemon keeps what it has
    assert_eq!(outcome, Reconciliation::KeepLocal { conflicted: false });
}

#[test]
fn an_account_this_daemon_has_never_held_is_taken_from_the_peer() {
    // Given this daemon holding nothing for the slot
    let incoming = a_credential("github", "operator", "gho_the_token");

    // When a peer's record for it arrives
    let outcome = SyncEngine::reconcile(None, &VaultEntry::Record(incoming));

    // Then it is taken, and nothing is in conflict
    assert_eq!(outcome, Reconciliation::TakeIncoming { conflicted: false });
}

#[test]
fn an_account_both_daemons_already_agree_on_is_reconciled_as_converged() {
    // Given both daemons holding the same record for one slot
    let record = a_credential("github", "operator", "gho_the_token");

    // When the two are reconciled
    let outcome = SyncEngine::reconcile(
        Some(&VaultEntry::Record(record.clone())),
        &VaultEntry::Record(record),
    );

    // Then there is nothing to do
    assert_eq!(outcome, Reconciliation::AlreadyConverged);
}

#[test]
fn two_edits_made_from_the_same_version_are_reconciled_as_a_conflict() {
    // Given one account edited independently on two daemons, both from version 1
    let original = a_credential("github", "operator", "gho_the_original");
    let local = an_edit_of(&original, "gho_edited_here", 1_758_240_400, 2);
    let incoming = an_edit_of(&original, "gho_edited_there", 1_758_240_500, 2);

    // When the two are reconciled
    let outcome = SyncEngine::reconcile(
        Some(&VaultEntry::Record(local)),
        &VaultEntry::Record(incoming),
    );

    // Then the later edit still wins, and the person is owed the fact that another one existed
    assert_eq!(outcome, Reconciliation::TakeIncoming { conflicted: true });
}

#[test]
fn an_account_a_peer_still_holds_does_not_come_back_after_it_was_deleted() {
    // Given this daemon's deletion of an account, and a peer that still holds the record
    let record = a_credential("github", "operator", "gho_the_token");
    let local = VaultEntry::Tombstone(a_tombstone_for(&record, 1_758_240_600));

    // When the peer sends the record back
    let outcome = SyncEngine::reconcile(Some(&local), &VaultEntry::Record(record));

    // Then the deletion stands
    assert_eq!(outcome, Reconciliation::KeepLocal { conflicted: false });
}

#[test]
fn an_account_relinked_after_a_deletion_comes_back() {
    // Given this daemon's deletion, and a peer that re-linked the account afterwards
    let record = a_credential("github", "operator", "gho_the_token");
    let local = VaultEntry::Tombstone(a_tombstone_for(&record, 1_758_240_600));
    let relinked = an_edit_of(&record, "gho_the_new_token", 1_758_240_700, 2);

    // When the two are reconciled
    let outcome = SyncEngine::reconcile(Some(&local), &VaultEntry::Record(relinked));

    // Then the re-link wins — a tombstone is an answer on the same clock, not a permanent veto
    assert_eq!(outcome, Reconciliation::TakeIncoming { conflicted: false });
}

#[test]
fn a_peers_deletion_of_an_account_this_daemon_still_holds_is_taken() {
    // Given this daemon holding a record a peer deleted afterwards
    let record = a_credential("github", "operator", "gho_the_token");
    let incoming = VaultEntry::Tombstone(a_tombstone_for(&record, 1_758_240_600));

    // When the two are reconciled
    let outcome = SyncEngine::reconcile(Some(&VaultEntry::Record(record)), &incoming);

    // Then the deletion propagates rather than being dropped as "nothing to write"
    assert_eq!(outcome, Reconciliation::TakeIncoming { conflicted: false });
}
