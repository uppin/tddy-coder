//! What a conversation has already handed to its caller, so no commit is applied twice.
//!
//! Held here, with the conversation, rather than on the daemon: the conversation lives in this
//! process and the branch on the daemon, so each pull carries the ledger and the daemon keeps no
//! per-conversation state. It dies with the conversation, as the conversation's own history does.

use std::collections::BTreeSet;

/// The commits one conversation has pulled, by short hash.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PullLedger {
    pulled: BTreeSet<String>,
}

#[allow(dead_code)] // TODO(range-pull): used by `subagent_pull`, `subagent_end` and the reset annotation
impl PullLedger {
    /// Remember `commits` as pulled.
    pub(crate) fn record(&mut self, commits: &[String]) {
        // TODO(range-pull): implement
        let _ = commits;
        todo!("PullLedger::record")
    }

    /// Every commit pulled so far — what a pull request tells the daemon to skip.
    pub(crate) fn pulled(&self) -> Vec<String> {
        // TODO(range-pull): implement
        todo!("PullLedger::pulled")
    }

    /// Of `dropped` (a rewind's `droppedCommits`), the ones already pulled — which are then
    /// forgotten, since the branch no longer holds them and a later pull cannot meet them again.
    pub(crate) fn dropped(&mut self, dropped: &[String]) -> Vec<String> {
        // TODO(range-pull): implement
        let _ = dropped;
        todo!("PullLedger::dropped")
    }

    /// Add `droppedPulledCommits` to a turn outcome's `worktreeReset`, when the rewind dropped any
    /// commit this ledger holds; leaves the outcome as it was otherwise.
    pub(crate) fn annotate_reset(&mut self, outcome: &mut serde_json::Value) {
        // TODO(range-pull): implement
        let _ = outcome;
        todo!("PullLedger::annotate_reset")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn hashes(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn a_recorded_commit_is_reported_as_pulled() {
        // Given
        let mut ledger = PullLedger::default();

        // When
        ledger.record(&hashes(&["c2", "c1"]));

        // Then
        assert_eq!(ledger.pulled(), hashes(&["c1", "c2"]));
    }

    #[test]
    fn an_empty_ledger_has_pulled_nothing() {
        assert_eq!(PullLedger::default().pulled(), Vec::<String>::new());
    }

    #[test]
    fn dropped_names_only_the_pulled_ones_and_forgets_them() {
        // Given
        let mut ledger = PullLedger::default();
        ledger.record(&hashes(&["c1", "c2"]));

        // When
        let dropped = ledger.dropped(&hashes(&["c2", "c3"]));

        // Then
        assert_eq!(
            (dropped, ledger.pulled()),
            (hashes(&["c2"]), hashes(&["c1"]))
        );
    }

    #[test]
    fn a_reset_that_dropped_a_pulled_commit_is_annotated() {
        // Given
        let mut ledger = PullLedger::default();
        ledger.record(&hashes(&["c2"]));
        let mut outcome = json!({
            "stopReason": "end_turn",
            "worktreeReset": { "to": "c1", "droppedCommits": ["c2", "c3"] }
        });

        // When
        ledger.annotate_reset(&mut outcome);

        // Then
        assert_eq!(
            outcome["worktreeReset"],
            json!({ "to": "c1", "droppedCommits": ["c2", "c3"], "droppedPulledCommits": ["c2"] })
        );
    }

    #[test]
    fn a_reset_that_dropped_nothing_pulled_is_left_as_it_was() {
        // Given
        let mut ledger = PullLedger::default();
        ledger.record(&hashes(&["c1"]));
        let before = json!({ "worktreeReset": { "to": "c1", "droppedCommits": ["c2"] } });
        let mut outcome = before.clone();

        // When
        ledger.annotate_reset(&mut outcome);

        // Then
        assert_eq!(outcome, before);
    }

    #[test]
    fn an_outcome_without_a_reset_is_left_as_it_was() {
        // Given
        let mut ledger = PullLedger::default();
        ledger.record(&hashes(&["c1"]));
        let before = json!({ "stopReason": "end_turn" });
        let mut outcome = before.clone();

        // When
        ledger.annotate_reset(&mut outcome);

        // Then
        assert_eq!(outcome, before);
    }
}
