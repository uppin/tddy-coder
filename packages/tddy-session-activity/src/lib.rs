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

pub mod service;
pub mod session_notification_subscribers;
pub mod session_notifications;
pub mod streams;

pub use service::{
    build_activity_entry, ActivityPorts, ActivityServiceImpl, DeltaLookup, DeltaScope,
    MeasuredDelta, OsUserResolver, SessionDeltaStores, SessionLabels,
};

/// The wire's "no tick yet" value.
pub const NO_TICK: u64 = 0;

/// The first tick a session's deltas are numbered with.
///
/// 1, not 0, so that [`NO_TICK`] means only what it says. See the module docs.
pub const FIRST_TICK: u64 = 1;

/// The tick to stamp a session's next delta with, given the last one stamped.
///
/// `None` — no delta has been stamped for this session yet — is [`FIRST_TICK`], not [`NO_TICK`].
/// That is the whole point of the change: on the old coordinate the first delta and "no delta yet"
/// were both 0, and a consumer could not tell them apart.
///
/// Saturating rather than wrapping at the top of the range, for the same reason: a wrap would land
/// the next tick on [`NO_TICK`] and reintroduce the ambiguity this function exists to remove. A
/// session would have to produce 2^64 deltas to reach it, and two deltas sharing the last tick is
/// a smaller lie than one claiming it does not exist.
#[must_use]
pub fn next_tick(last: Option<u64>) -> u64 {
    match last {
        None => FIRST_TICK,
        Some(last) => last.saturating_add(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use tddy_daemon_kernel::AgentActivityHub;
    use tddy_rpc::Status;

    use crate::session_notifications::SessionNotificationBus;

    /// The one operator this host is configured for. A token it minted resolves to their OS user;
    /// anything else is the `UNAUTHENTICATED` a caller re-authenticates after.
    fn tokens_minted_for(os_user: &'static str) -> OsUserResolver {
        Arc::new(move |session_token: &str| match session_token {
            "web-token-for-ada" => Ok(os_user.to_string()),
            _ => Err(Status::unauthenticated("invalid or expired session")),
        })
    }

    /// A host that names every session by its short id — what the daemon's own resolver falls back
    /// to for a session whose directory records no repository and no workflow goal.
    struct SessionsNamedByTheirId;

    impl SessionLabels for SessionsNamedByTheirId {
        fn label_for(&self, _sessions_base: &Path, session_id: &str) -> String {
            session_id.to_string()
        }
    }

    /// A host hosting no session rooms: it measured no checkout, so it holds no delta for any call.
    struct NoSessionRoomsHere;

    impl SessionDeltaStores for NoSessionRoomsHere {
        fn delta_for_call(
            &self,
            _session_id: &str,
            _call_id: &str,
            _scope: DeltaScope,
        ) -> Result<DeltaLookup, Status> {
            Ok(DeltaLookup::NoRoomHere)
        }
    }

    /// What a daemon serving one operator's sessions out of `/var/lib/tddy` hands this crate: its
    /// token mapping, its data dir, the hub its sandboxes publish into, the bus it raises
    /// notifications on, and the two answers only it has.
    fn ports_of_a_host_serving_one_operator() -> ActivityPorts {
        ActivityPorts {
            os_users: tokens_minted_for("ada"),
            tddy_data_dir: PathBuf::from("/var/lib/tddy"),
            activity: Arc::new(AgentActivityHub::default()),
            notifications: Some(Arc::new(SessionNotificationBus::new())),
            session_labels: Arc::new(SessionsNamedByTheirId),
            deltas: Arc::new(NoSessionRoomsHere),
        }
    }

    #[test]
    fn names_the_service_families_m_and_n_move_to() {
        // Given
        let ports = ports_of_a_host_serving_one_operator();

        // When
        let entry = build_activity_entry(ports);

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
}
