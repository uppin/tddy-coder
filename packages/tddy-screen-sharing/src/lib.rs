//! Screen sharing: the `screen_sharing.ScreenSharingService` surface and the vault that seals the
//! per-session key it hands out.
//!
//! Extracted from `tddy-daemon` by `#unbundle` node 2. Two properties made it an early node: it has
//! **zero** code references to `connection_service` (every apparent one is a doc comment) and
//! **zero** inline `#[cfg(test)]` lines. Its proto — `screen_sharing.proto` plus
//! `screen_sharing_input.proto` — was already its own, so no wire coordinate changes and no client
//! migrates.
//!
//! `tddy-screenshare` is attributable to this subsystem alone and leaves `tddy-daemon` with it.
//!
//! # The dead service next door
//!
//! Node 2 also **deletes** `vnc_service.rs` and `vnc_vault.rs` with their two acceptance suites —
//! 1,008 lines that were never reachable. `runtime.rs` registers
//! `screen_sharing.ScreenSharingService` but never `vnc.VncService`, and the only references
//! anywhere under `packages/` were the daemon's own `lib.rs` declaration, the two source files and
//! their tests. The deletion rides with this node rather than getting its own because this is the
//! only node whose reviewer is already reading the live screen-sharing service beside it.

use std::path::PathBuf;
use std::sync::Arc;

/// Resolve a session token to the base directory that user's sessions live under.
///
/// Mirrors the resolver the rest of the daemon uses; the wiring layer supplies one closure and
/// every service clones it.
pub type SessionsBase = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;

/// Per-session sealed screen-sharing keys, so a reconnect does not re-negotiate one.
#[derive(Debug, Default)]
pub struct ScreenSharingVault {
    // TODO(model-telegram-screen): implement
}

impl ScreenSharingVault {
    /// Seal a key for a session, replacing any previous one.
    pub fn seal(&self, _session_id: &str, _key: &[u8]) -> Result<(), ScreenSharingError> {
        // TODO(model-telegram-screen): implement
        unimplemented!("ScreenSharingVault::seal")
    }

    /// Unseal a session's key, or `None` when none was ever sealed.
    pub fn unseal(&self, _session_id: &str) -> Result<Option<Vec<u8>>, ScreenSharingError> {
        // TODO(model-telegram-screen): implement
        unimplemented!("ScreenSharingVault::unseal")
    }
}

/// Why a screen-sharing operation could not be completed.
#[derive(Debug, thiserror::Error)]
pub enum ScreenSharingError {
    #[error("the sealed key for session {session_id} could not be read: {reason}")]
    Unsealable { session_id: String, reason: String },
}

/// The `screen_sharing.ScreenSharingService` entry the daemon's wiring layer registers.
pub fn build_screen_sharing_entry(
    _sessions_base: SessionsBase,
    _vault: Arc<ScreenSharingVault>,
) -> tddy_rpc::ServiceEntry {
    // TODO(model-telegram-screen): implement
    unimplemented!("build_screen_sharing_entry")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_service_the_wiring_layer_registers() {
        // Given
        let sessions_base: SessionsBase = Arc::new(|_| None);
        let vault = Arc::new(ScreenSharingVault::default());

        // When
        let entry = build_screen_sharing_entry(sessions_base, vault);

        // Then
        assert_eq!(entry.name, "screen_sharing.ScreenSharingService");
    }

    #[test]
    fn returns_a_sealed_key_to_the_session_that_sealed_it() {
        // Given
        let vault = ScreenSharingVault::default();
        vault.seal("session-a", b"a-negotiated-key").unwrap();

        // When
        let unsealed = vault.unseal("session-a").unwrap();

        // Then
        assert_eq!(unsealed.as_deref(), Some(b"a-negotiated-key".as_slice()));
    }

    /// A session that never negotiated a key has none — distinct from a key that could not be read,
    /// which is an error. Collapsing the two would make a broken vault look like a fresh session.
    #[test]
    fn has_no_key_for_a_session_that_never_sealed_one() {
        // Given
        let vault = ScreenSharingVault::default();

        // When
        let unsealed = vault.unseal("session-b").unwrap();

        // Then
        assert_eq!(unsealed, None);
    }

    #[test]
    fn does_not_return_one_sessions_key_to_another() {
        // Given
        let vault = ScreenSharingVault::default();
        vault.seal("session-a", b"a-negotiated-key").unwrap();

        // When
        let other = vault.unseal("session-b").unwrap();

        // Then
        assert_eq!(other, None, "a sealed key is scoped to its own session");
    }
}
