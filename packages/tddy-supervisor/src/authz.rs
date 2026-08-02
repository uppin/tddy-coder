//! Peer-credential authorization — the outer gate of the privileged surface.
//!
//! This answers only "may this process talk to me at all?". What it may then *ask for* is
//! [`crate::policy`]'s job. The two are separate so a regression in one cannot hide behind the
//! other.

use std::collections::BTreeSet;

use crate::error::SupervisorError;

/// Credentials of the process on the other end of the socket, from `SO_PEERCRED`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerIdentity {
    pub uid: u32,
    pub gid: u32,
    pub pid: u32,
}

/// Decides which peers may issue requests.
///
/// The allowed set is exactly the uids of the services the supervisor itself started. Nothing
/// else is trusted — not root, not the supervisor's own uid, not a member of the socket group who
/// happens to be able to `connect()`.
#[derive(Debug, Clone)]
pub struct Authorizer {
    authorized_uids: BTreeSet<u32>,
}

impl Authorizer {
    /// Build from the uids of the declared services.
    pub fn from_service_uids(uids: impl IntoIterator<Item = u32>) -> Authorizer {
        Authorizer {
            authorized_uids: uids.into_iter().collect(),
        }
    }

    /// Allow or deny a peer. Denial is opaque by construction — see [`SupervisorError::Denied`].
    pub fn authorize(&self, peer: &PeerIdentity) -> Result<(), SupervisorError> {
        if self.authorized_uids.contains(&peer.uid) {
            Ok(())
        } else {
            Err(SupervisorError::Denied)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_peer_with_uid(uid: u32) -> PeerIdentity {
        PeerIdentity {
            uid,
            gid: uid,
            pid: 4242,
        }
    }

    const TDDY_UID: u32 = 998;
    const RELAY_UID: u32 = 997;
    const ROOT_UID: u32 = 0;

    #[test]
    fn authorizes_a_peer_that_owns_a_declared_service() {
        // Given
        let authorizer = Authorizer::from_service_uids([TDDY_UID]);

        // When
        let result = authorizer.authorize(&a_peer_with_uid(TDDY_UID));

        // Then
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn authorizes_every_uid_that_owns_one_of_several_declared_services() {
        // Given
        let authorizer = Authorizer::from_service_uids([TDDY_UID, RELAY_UID]);

        // When
        let result = authorizer.authorize(&a_peer_with_uid(RELAY_UID));

        // Then
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn denies_a_peer_that_owns_no_declared_service() {
        // Given
        let authorizer = Authorizer::from_service_uids([TDDY_UID]);

        // When
        let result = authorizer.authorize(&a_peer_with_uid(1000));

        // Then
        assert_eq!(result, Err(SupervisorError::Denied));
    }

    #[test]
    fn denies_every_peer_when_no_service_is_declared() {
        // Given a supervisor managing nothing — its privileged surface belongs to nobody.
        let authorizer = Authorizer::from_service_uids([]);

        // When
        let result = authorizer.authorize(&a_peer_with_uid(TDDY_UID));

        // Then
        assert_eq!(result, Err(SupervisorError::Denied));
    }

    #[test]
    fn denies_root_when_root_owns_no_declared_service() {
        // Given
        let authorizer = Authorizer::from_service_uids([TDDY_UID]);

        // When
        let result = authorizer.authorize(&a_peer_with_uid(ROOT_UID));

        // Then root is not special-cased in. A root process can already do everything directly;
        // letting it through here would only add an unaudited path into the broker.
        assert_eq!(result, Err(SupervisorError::Denied));
    }

    #[test]
    fn denies_without_revealing_anything_about_the_peer_or_the_allowed_set() {
        // Given
        let authorizer = Authorizer::from_service_uids([TDDY_UID]);

        // When
        let error = authorizer
            .authorize(&a_peer_with_uid(1000))
            .expect_err("an unauthorized peer must be denied");

        // Then
        assert_eq!(error.to_string(), "request denied");
    }
}
