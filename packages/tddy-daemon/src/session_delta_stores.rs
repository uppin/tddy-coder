//! The delta rings of the session rooms this daemon hosts, as the activity service reads them.
//!
//! The ring is filled by the session room's poll loop in `tddy-daemon-livekit`; what the activity
//! service wants is its own port, [`SessionDeltaStores`]. This daemon is where the two are brought
//! together, so neither crate names the other.

use std::sync::Arc;

use tddy_daemon_livekit::session_room::{
    ActivityDelta, DeltaLookupError, DeltaScope as RoomDeltaScope, SessionRoomRegistry,
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
        let room_scope = match scope {
            DeltaScope::Call => RoomDeltaScope::Call,
            DeltaScope::Residual => RoomDeltaScope::Residual,
            DeltaScope::Tick => RoomDeltaScope::Tick,
        };
        let looked_up = {
            let store = store
                .lock()
                .map_err(|_| Status::internal("session delta store is poisoned"))?;
            store.delta_for_call(call_id, room_scope)
        };
        Ok(match looked_up {
            Ok(delta) => DeltaLookup::Found(measured_delta(delta)),
            Err(DeltaLookupError::UnknownCall { .. }) => DeltaLookup::UnknownCall,
            Err(DeltaLookupError::AgedOut { seq, .. }) => DeltaLookup::AgedOut { seq },
        })
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

    #[test]
    fn a_session_with_no_room_hosted_here_has_no_delta_and_never_will() {
        let stores = RoomsHostedByThisDaemon::new(Arc::new(SessionRoomRegistry::new()));

        let looked_up = stores
            .delta_for_call("1780828020298-abc", "call-1", DeltaScope::Call)
            .expect("an absent room is an answer, not a failure");

        assert!(matches!(looked_up, DeltaLookup::NoRoomHere));
    }

    #[test]
    fn a_rooms_delta_is_described_by_the_same_five_fields() {
        let measured = measured_delta(ActivityDelta {
            seq: 7,
            prev_seq: 6,
            base_commit: "9f1c0b6".to_string(),
            patch: b"diff".to_vec(),
            scoped_paths: vec!["src/main.rs".to_string()],
        });

        assert_eq!(
            (
                measured.seq,
                measured.prev_seq,
                measured.base_commit.as_str(),
                measured.patch.as_slice(),
                measured.scoped_paths.as_slice()
            ),
            (
                7,
                6,
                "9f1c0b6",
                b"diff".as_slice(),
                ["src/main.rs".to_string()].as_slice()
            )
        );
    }
}
