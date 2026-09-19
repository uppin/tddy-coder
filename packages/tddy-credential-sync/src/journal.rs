//! What each side knows about where it stands.
//!
//! The developer's requirement is explicit: *"This propagation should be journaled so that each
//! node knows the sync status."* So every decision this crate makes about a `(peer, record)` pair
//! leaves a line here — including the decisions that are refusals, which are the ones a person
//! actually needs to see. A sync that quietly does nothing for a misconfigured peer is
//! indistinguishable from a sync that has not happened yet.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::transport::{PeerId, RecordKey};

/// Why a peer was not admitted.
///
/// Three reasons and not one boolean, because the remedies differ and only the person holding the
/// configuration can apply them: a group-secret mismatch is a deployment that disagrees with itself,
/// an invalid signature is a peer that is not who it claims, and an unknown identity is a key the
/// directory has not published yet — which is often a matter of waiting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefusalReason {
    /// The peer's proof was not the one this daemon's group secret would have produced.
    GroupSecretMismatch,
    /// The advertisement's Ed25519 signature did not verify under the key it names.
    SignatureInvalid,
    /// `#keyring` 1/9's `KeyDirectory` has no public key for the advertised key id, so there is
    /// nothing to verify against. **Not** a licence to admit the peer on the group secret alone.
    UnknownIdentity,
}

/// Where one record stands with one peer.
///
/// `Refused` and `Undeliverable` are deliberately distinct. A refusal is the peer group working as
/// configured and the remedy is configuration; `Undeliverable` is the network and the remedy is to
/// wait. A single "failed" would tell a person to do the wrong thing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SyncStatus {
    /// This daemon intends to send the record and has not yet.
    Pending,
    /// The payload left this daemon and has not been acknowledged.
    Sent,
    /// The peer says it retained the record.
    Acknowledged,
    /// The peer was not admitted, and received nothing.
    Refused(RefusalReason),
    /// Both sides had written the slot independently; last-writer-wins picked one. Recorded rather
    /// than hidden, because a person who edits an account on two machines is owed an explanation of
    /// which edit survived.
    Conflict,
    /// The transport could not reach the peer. Unchanged authorisation — this peer is admitted, it
    /// is simply not there.
    Undeliverable,
}

/// One line: what happened, to which record, with which peer, and when.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalEntry {
    pub peer: PeerId,
    pub record: RecordKey,
    pub status: SyncStatus,
    /// Seconds since the Unix epoch — the same clock the records are reconciled on.
    pub at: u64,
}

/// The per-`(peer, record)` sync state, on both sides of a sync.
///
/// Holds **one line per pair**, the latest: a status that only ever appended would grow without
/// bound in a daemon that syncs on every change, and a screen showing "where does this account
/// stand with that peer" wants the answer, not the history. What is worth keeping of the history is
/// the refusal, and a refusal is sticky until the peer is admitted.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncJournal {
    entries: BTreeMap<(PeerId, RecordKey), JournalEntry>,
}

impl SyncJournal {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Note where `record` now stands with `peer`, replacing the previous line for that pair.
    pub fn note(&mut self, _peer: &PeerId, _record: &RecordKey, _status: SyncStatus, _at: u64) {
        todo!("TODO(keyring 6/9): implement — replace this pair's line")
    }

    /// Where `record` stands with `peer`, or `None` when this journal has never decided.
    ///
    /// `None` is "nothing has been attempted", which a screen renders differently from
    /// [`SyncStatus::Pending`] — one means the engine has not looked, the other means it has and is
    /// waiting.
    #[must_use]
    pub fn status(&self, _peer: &PeerId, _record: &RecordKey) -> Option<&SyncStatus> {
        todo!("TODO(keyring 6/9): implement — look the pair up")
    }

    /// Every line, in `(peer, record)` order.
    #[must_use]
    pub fn entries(&self) -> Vec<&JournalEntry> {
        todo!("TODO(keyring 6/9): implement — the stored order is already the answer")
    }

    /// Every line about one record, across peers — what the Accounts screen renders per account.
    #[must_use]
    pub fn for_record(&self, _record: &RecordKey) -> Vec<&JournalEntry> {
        todo!("TODO(keyring 6/9): implement — filter by record, keep peer order")
    }
}
