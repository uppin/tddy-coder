//! What a conversation has already handed to its caller, so no commit is applied twice.
//!
//! Held here, with the conversation, rather than on the daemon: the conversation lives in this
//! process and the branch on the daemon, so each pull carries the ledger and the daemon keeps no
//! per-conversation state. It dies with the conversation, as the conversation's own history does.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

/// The commits one conversation has pulled, by short hash.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PullLedger {
    pulled: BTreeSet<String>,
}

impl PullLedger {
    /// Remember `commits` as pulled.
    pub(crate) fn record(&mut self, commits: &[String]) {
        self.pulled.extend(commits.iter().cloned());
    }

    /// Every commit pulled so far — what a pull request tells the daemon to skip.
    pub(crate) fn pulled(&self) -> Vec<String> {
        self.pulled.iter().cloned().collect()
    }

    /// Of `dropped` (a rewind's `droppedCommits`), the ones already pulled — which are then
    /// forgotten, since the branch no longer holds them and a later pull cannot meet them again.
    pub(crate) fn dropped(&mut self, dropped: &[String]) -> Vec<String> {
        dropped
            .iter()
            .filter(|commit| self.pulled.remove(*commit))
            .cloned()
            .collect()
    }

    /// Add `droppedPulledCommits` to a turn outcome's `worktreeReset`, when the rewind dropped any
    /// commit this ledger holds; leaves the outcome as it was otherwise.
    pub(crate) fn annotate_reset(&mut self, outcome: &mut serde_json::Value) {
        let Some(reset) = outcome.get_mut("worktreeReset") else {
            return;
        };
        let dropped: Vec<String> = reset
            .get("droppedCommits")
            .and_then(|commits| commits.as_array())
            .into_iter()
            .flatten()
            .filter_map(|commit| commit.as_str().map(str::to_string))
            .collect();
        let dropped_pulled = self.dropped(&dropped);
        if let (false, Some(reset)) = (dropped_pulled.is_empty(), reset.as_object_mut()) {
            reset.insert(
                "droppedPulledCommits".to_string(),
                serde_json::json!(dropped_pulled),
            );
        }
    }
}

/// What this process knows of one conversation's pulls.
#[derive(Default)]
struct ConversationPulls {
    ledger: PullLedger,
    /// Outcomes already annotated, by response id, so collecting the same turn again reads the
    /// same answer even though the ledger has forgotten what the first read named.
    annotated: HashMap<String, String>,
}

/// Every open conversation's pulls, by conversation id.
fn conversations() -> MutexGuard<'static, HashMap<String, ConversationPulls>> {
    static PULLS: OnceLock<Mutex<HashMap<String, ConversationPulls>>> = OnceLock::new();
    PULLS
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// What `conversation_id` has pulled so far.
pub(crate) fn pulled_by(conversation_id: &str) -> Vec<String> {
    conversations()
        .get(conversation_id)
        .map(|pulls| pulls.ledger.pulled())
        .unwrap_or_default()
}

/// Remember that `conversation_id` pulled `commits`.
pub(crate) fn record_pulled(conversation_id: &str, commits: &[String]) {
    conversations()
        .entry(conversation_id.to_string())
        .or_default()
        .ledger
        .record(commits);
}

/// Forget everything about `conversation_id`'s pulls — the conversation is closed.
pub(crate) fn forget_conversation(conversation_id: &str) {
    conversations().remove(conversation_id);
}

/// The turn result `result` (collected under `response_id`) with `droppedPulledCommits` added to its
/// `worktreeReset` when the rewind dropped commits `conversation_id` pulled. The same turn collected
/// again reads the same answer.
pub(crate) fn annotated_turn_result(
    conversation_id: &str,
    response_id: &str,
    result: String,
) -> String {
    let mut conversations = conversations();
    if let Some(known) = conversations
        .get(conversation_id)
        .and_then(|pulls| pulls.annotated.get(response_id))
    {
        return known.clone();
    }
    let Some(pulls) = conversations.get_mut(conversation_id) else {
        return result;
    };
    let Ok(mut outcome) = serde_json::from_str::<serde_json::Value>(&result) else {
        return result;
    };
    let before = outcome.clone();
    pulls.ledger.annotate_reset(&mut outcome);
    if outcome == before {
        return result;
    }
    let annotated = outcome.to_string();
    pulls
        .annotated
        .insert(response_id.to_string(), annotated.clone());
    annotated
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
