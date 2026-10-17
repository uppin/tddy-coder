//! Which owning daemons are admitted to the session rooms this daemon facilitates, as a port.

/// Admissions to this daemon's session rooms, by session and owning daemon.
pub trait RoomAdmission: Send + Sync {
    /// Record `owning_instance_id` as admitted to `session_id`'s room; `true` when it was not yet.
    fn admit(&self, session_id: &str, owning_instance_id: &str) -> bool;
    /// Withdraw that admission; `true` when there was one.
    fn revoke(&self, session_id: &str, owning_instance_id: &str) -> bool;
    fn is_admitted(&self, session_id: &str, owning_instance_id: &str) -> bool;
    /// Withdraw every admission to `session_id`'s room; how many there were.
    fn revoke_all_for_session(&self, session_id: &str) -> usize;
}

/// No rooms to admit anyone to: nobody is ever admitted.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoRoomAdmission;

impl RoomAdmission for NoRoomAdmission {
    fn admit(&self, _session_id: &str, _owning_instance_id: &str) -> bool {
        todo!("NoRoomAdmission::admit")
    }
    fn revoke(&self, _session_id: &str, _owning_instance_id: &str) -> bool {
        todo!("NoRoomAdmission::revoke")
    }
    fn is_admitted(&self, _session_id: &str, _owning_instance_id: &str) -> bool {
        todo!("NoRoomAdmission::is_admitted")
    }
    fn revoke_all_for_session(&self, _session_id: &str) -> usize {
        todo!("NoRoomAdmission::revoke_all_for_session")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_nothing_wired_an_admission_is_never_recorded() {
        let admission = NoRoomAdmission;

        let recorded = admission.admit("1780828020298-abc", "peer-daemon-1");

        assert_eq!(
            (recorded, admission.is_admitted("1780828020298-abc", "peer-daemon-1")),
            (false, false)
        );
    }
}
