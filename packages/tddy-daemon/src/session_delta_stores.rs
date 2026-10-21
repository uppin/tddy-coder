//! The delta rings of the session rooms this daemon hosts, as the activity service reads them.
//!
//! The ring is filled by the session room's poll loop in `tddy-daemon-livekit`; what the activity
//! service wants is its own port, [`SessionDeltaStores`]. This daemon is where the two are brought
//! together, so neither crate names the other.
//!
//! TODO(#satellite follow-up): the adapter belongs in the plugin, beside the ring it reads, which
//! needs a `tddy-daemon-livekit` → `tddy-session-activity` dependency edge nobody has agreed to
//! yet. `docs/dev/todo/2026-10-21-session-delta-stores-adapter-lives-in-the-daemon.md`.

use std::sync::Arc;

use tddy_daemon_livekit::session_room::{
    ActivityDelta, DeltaLookupError, DeltaScope as RoomDeltaScope, SessionDeltaStore,
    SessionRoomRegistry,
};
use tddy_rpc::Status;
use tddy_session_lifecycle::activity_service::{
    DeltaLookup, DeltaScope, MeasuredDelta, SessionDeltaStores,
};

/// The delta rings of the session rooms this daemon hosts.
///
/// A patch is a measurement of a live checkout, and only the daemon hosting that room ever took
/// one — so a session with no room here has no delta and never will, which is the
/// [`DeltaLookup::NoRoomHere`] the crate reports as an absence rather than a failure.
pub struct RoomsHostedByThisDaemon {
    rooms: Arc<SessionRoomRegistry>,
}

impl RoomsHostedByThisDaemon {
    #[must_use]
    pub fn new(rooms: Arc<SessionRoomRegistry>) -> Self {
        Self { rooms }
    }
}

impl SessionDeltaStores for RoomsHostedByThisDaemon {
    fn delta_for_call(
        &self,
        session_id: &str,
        call_id: &str,
        scope: DeltaScope,
    ) -> Result<DeltaLookup, Status> {
        let Some(store) = self.rooms.delta_store(session_id) else {
            return Ok(DeltaLookup::NoRoomHere);
        };
        let store = store
            .lock()
            .map_err(|_| Status::internal("session delta store is poisoned"))?;
        Ok(lookup_in(&store, call_id, scope))
    }
}

/// `call_id`'s delta in one room's ring, as the activity service describes the answer.
fn lookup_in(store: &SessionDeltaStore, call_id: &str, scope: DeltaScope) -> DeltaLookup {
    let room_scope = match scope {
        DeltaScope::Call => RoomDeltaScope::Call,
        DeltaScope::Residual => RoomDeltaScope::Residual,
        DeltaScope::Tick => RoomDeltaScope::Tick,
    };
    match store.delta_for_call(call_id, room_scope) {
        Ok(delta) => DeltaLookup::Found(measured_delta(delta)),
        Err(DeltaLookupError::UnknownCall { .. }) => DeltaLookup::UnknownCall,
        Err(DeltaLookupError::AgedOut { seq, .. }) => DeltaLookup::AgedOut { seq },
    }
}

/// A session room's delta, as the activity service describes one: the same five fields.
#[must_use]
pub fn measured_delta(delta: ActivityDelta) -> MeasuredDelta {
    MeasuredDelta {
        seq: delta.seq,
        prev_seq: delta.prev_seq,
        base_commit: delta.base_commit,
        patch: delta.patch,
        scoped_paths: delta.scoped_paths,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EDITED_BY_THE_CALL: &str = "diff --git a/src/main.rs b/src/main.rs\n\
        --- a/src/main.rs\n\
        +++ b/src/main.rs\n\
        @@ -1 +1 @@\n\
        -old\n\
        +new\n";
    const EDITED_BY_NOBODY: &str = "diff --git a/README.md b/README.md\n\
        --- a/README.md\n\
        +++ b/README.md\n\
        @@ -1 +1 @@\n\
        -before\n\
        +after\n";

    /// A ring holding one tick that edited two files, one of them declared by `call-1`.
    fn a_ring_with_one_attributed_call() -> SessionDeltaStore {
        let mut store = SessionDeltaStore::new(4, 1024 * 1024);
        store.record(ActivityDelta {
            seq: 7,
            prev_seq: 6,
            base_commit: "9f1c0b6".to_string(),
            patch: format!("{EDITED_BY_THE_CALL}{EDITED_BY_NOBODY}").into_bytes(),
            scoped_paths: Vec::new(),
        });
        store.attribute("call-1", 7, &["src/main.rs".to_string()]);
        store
    }

    /// The patch a found lookup answered, as text.
    fn found_patch(lookup: DeltaLookup) -> (String, Vec<String>) {
        match lookup {
            DeltaLookup::Found(delta) => {
                assert_eq!((delta.seq, delta.prev_seq), (7, 6));
                assert_eq!(delta.base_commit, "9f1c0b6");
                (String::from_utf8(delta.patch).unwrap(), delta.scoped_paths)
            }
            other => panic!("expected the call's delta, got {other:?}"),
        }
    }

    #[test]
    fn a_session_with_no_room_hosted_here_has_no_delta_and_never_will() {
        let stores = RoomsHostedByThisDaemon::new(Arc::new(SessionRoomRegistry::new()));

        let looked_up = stores
            .delta_for_call("1780828020298-abc", "call-1", DeltaScope::Call)
            .expect("an absent room is an answer, not a failure");

        assert_eq!(looked_up, DeltaLookup::NoRoomHere);
    }

    #[test]
    fn each_scope_is_read_from_the_ring_as_the_same_scope() {
        let store = a_ring_with_one_attributed_call();

        let call = found_patch(lookup_in(&store, "call-1", DeltaScope::Call));
        let residual = found_patch(lookup_in(&store, "call-1", DeltaScope::Residual));
        let tick = found_patch(lookup_in(&store, "call-1", DeltaScope::Tick));

        assert_eq!(
            [call, residual, tick],
            [
                (
                    EDITED_BY_THE_CALL.to_string(),
                    vec!["src/main.rs".to_string()]
                ),
                (EDITED_BY_NOBODY.to_string(), vec!["README.md".to_string()]),
                (
                    format!("{EDITED_BY_THE_CALL}{EDITED_BY_NOBODY}"),
                    Vec::new()
                ),
            ]
        );
    }

    #[test]
    fn a_call_the_ring_never_heard_of_is_unknown() {
        let store = a_ring_with_one_attributed_call();

        assert_eq!(
            lookup_in(&store, "call-never-made", DeltaScope::Call),
            DeltaLookup::UnknownCall
        );
    }

    #[test]
    fn a_call_whose_tick_left_the_ring_aged_out_at_that_tick() {
        let mut store = SessionDeltaStore::new(1, 1024 * 1024);
        store.attribute("call-1", 7, &[]);
        for seq in [7, 8] {
            store.record(ActivityDelta {
                seq,
                prev_seq: seq - 1,
                base_commit: "9f1c0b6".to_string(),
                patch: Vec::new(),
                scoped_paths: Vec::new(),
            });
        }

        assert_eq!(
            lookup_in(&store, "call-1", DeltaScope::Call),
            DeltaLookup::AgedOut { seq: 7 }
        );
    }
}
