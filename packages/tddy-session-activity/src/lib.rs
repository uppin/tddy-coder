//! What an agent is doing, what a session's status is, and the transcript of both.
//!
//! Extracted from `tddy-daemon` by `#unbundle` node 7, serving `activity.ActivityService` —
//! families M and N, 8 methods. It publishes into and reads from
//! [`tddy_daemon_kernel::AgentActivityHub`], which is node 1's.
//!
//! # The delta tick numbering changes here, deliberately
//!
//! `docs/dev/todo/2026-08-29-a-session-s-first-delta-is-numbered-0-which-is-also-the-wire-s-no-tick.md`
//! records that a session's first activity delta was numbered 0 — which is also the wire's "no tick"
//! value, so a consumer could not tell *the first delta* from *no delta yet*.
//!
//! `StreamAgentActivityDelta` moves to a **new proto** in this node, and carrying that ambiguity
//! into a fresh schema would make it permanent. So [`FIRST_TICK`] is 1 and [`NO_TICK`] is 0, and
//! `tddy-session-sync` — the only consumer — migrates in the same PR. This is the last cheap moment
//! to make that change.

use std::sync::Arc;

use tddy_daemon_kernel::AgentActivityHub;

/// The wire's "no tick yet" value.
pub const NO_TICK: u64 = 0;

/// The first tick a session's deltas are numbered with.
///
/// 1, not 0, so that [`NO_TICK`] means only what it says. See the module docs.
pub const FIRST_TICK: u64 = 1;

/// Why an activity or replay operation could not be completed.
#[derive(Debug, thiserror::Error)]
pub enum ActivityError {
    #[error("no session {session_id} on this host")]
    NoSuchSession { session_id: String },
    #[error("the replay for {session_id} ended without a final frame, so it is incomplete")]
    TruncatedReplay { session_id: String },
}

/// The `activity.ActivityService` entry the daemon's wiring layer registers.
pub fn build_activity_entry(_hub: Arc<AgentActivityHub>) -> tddy_rpc::ServiceEntry {
    // TODO(session-agent-services): implement
    unimplemented!("build_activity_entry")
}

/// The tick to stamp a session's next delta with, given the last one stamped.
pub fn next_tick(_last: Option<u64>) -> u64 {
    // TODO(session-agent-services): implement
    unimplemented!("next_tick")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_service_families_m_and_n_move_to() {
        // Given
        let hub = Arc::new(AgentActivityHub::default());

        // When
        let entry = build_activity_entry(hub);

        // Then
        assert_eq!(entry.name, "activity.ActivityService");
    }

    /// ⛔ The prerequisite this node carries. A session's first delta must be distinguishable from
    /// "no delta yet", and on the old coordinate both were 0.
    #[test]
    fn stamps_a_sessions_first_delta_distinguishably_from_no_delta_yet() {
        // Given a session that has never had a delta
        // When
        let first = next_tick(None);

        // Then
        assert_eq!(first, FIRST_TICK);
        assert_ne!(
            first, NO_TICK,
            "the first delta must not share the wire's absent value"
        );
    }

    #[test]
    fn stamps_each_later_delta_after_the_one_before() {
        // Given
        let first = next_tick(None);

        // When
        let second = next_tick(Some(first));

        // Then
        assert_eq!(second, first + 1);
    }

    /// A replay that ended without its final frame is incomplete, and answering with what arrived
    /// would make a partial transcript look like the whole conversation.
    #[test]
    fn refuses_a_replay_that_ended_without_a_final_frame() {
        let truncated = ActivityError::TruncatedReplay {
            session_id: "session-a".to_string(),
        };
        assert!(truncated.to_string().contains("incomplete"));
    }
}
