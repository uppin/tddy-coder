//! A screen-sharing credential propagates like any other, with no code that knows what it is.
//!
//! `#keyring` 6/9 built the propagation engine against `tddy-credentials`' own types. If a second
//! provider needed the engine to learn anything about it — a branch, a type parameter, a special
//! case in reconciliation — then 3/9 and 6/9 built a GitHub store with a longer name, and the claim
//! this stack rests on is false. So the test is deliberately blunt: a desktop's record goes through
//! the same `reconcile` a GitHub token goes through, and is addressed by the same `RecordKey`.

use tddy_credential_sync::{Reconciliation, RecordKey, SyncEngine};
use tddy_credentials::VaultEntry;
use tddy_screen_sharing::screen_sharing_records::{
    account_for, record_for, screen_sharing_provider,
};
use tddy_service::proto::screen_sharing::{Protocol, ScreenSharingTarget};

const A_DESKTOP_PASSWORD: &str = "the-desktop-password";
const AN_EARLIER_WRITE: u64 = 1_758_240_000;
const A_LATER_WRITE: u64 = 1_758_243_600;

fn a_desktop() -> ScreenSharingTarget {
    ScreenSharingTarget {
        id: "target-0a1b".to_string(),
        label: "dev box".to_string(),
        host: "10.0.0.5".to_string(),
        port: 5900,
        protocol: Protocol::Vnc as i32,
        username: "ada".to_string(),
    }
}

fn the_desktop_as_written_at(written_at: u64) -> VaultEntry {
    VaultEntry::Record(record_for(&a_desktop(), A_DESKTOP_PASSWORD, written_at))
}

#[test]
fn a_peers_later_edit_of_a_desktop_wins_the_same_way_any_credential_does() {
    // Given this daemon's copy of a desktop, and a peer's later one
    let local = the_desktop_as_written_at(AN_EARLIER_WRITE);
    let incoming = the_desktop_as_written_at(A_LATER_WRITE);

    // When the engine reconciles them — the same call a GitHub token takes
    let decision = SyncEngine::reconcile(Some(&local), &incoming);

    // Then
    assert_eq!(decision, Reconciliation::TakeIncoming { conflicted: false });
}

#[test]
fn a_desktop_a_peer_has_not_seen_is_taken_whole() {
    // Given a daemon holding nothing for this slot
    let incoming = the_desktop_as_written_at(A_LATER_WRITE);

    // When the engine reconciles
    let decision = SyncEngine::reconcile(None, &incoming);

    // Then — a first sight of a desktop is a write, not a conflict
    assert_eq!(decision, Reconciliation::TakeIncoming { conflicted: false });
}

#[test]
fn a_desktop_is_addressed_in_the_journal_by_provider_and_account_like_any_other_credential() {
    // Given a desktop stored as a record
    let record = record_for(&a_desktop(), A_DESKTOP_PASSWORD, AN_EARLIER_WRITE);

    // When the journal names the slot it occupies
    let key = RecordKey::new(record.provider.clone(), record.account.clone());

    // Then it is the same two-part key every provider uses — nothing screen-sharing-shaped
    assert_eq!(
        key,
        RecordKey::new(screen_sharing_provider(), account_for(&a_desktop().id))
    );
}
