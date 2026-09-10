//! Screen sharing: the `screen_sharing.ScreenSharingService` surface, and the encrypted credential
//! vault each session keeps its desktops' passwords in.
//!
//! Extracted from `tddy-daemon` by `#unbundle` node 2. What made it an early node is that it has
//! **zero** code references to `connection_service` — every apparent one is a doc comment. Its
//! proto, `screen_sharing.proto` plus `screen_sharing_input.proto`, was already its own, so
//! `screen_sharing.ScreenSharingService` keeps its wire coordinate and no client migrates.
//!
//! The plan also credited it with zero inline `#[cfg(test)]` lines; that was read off file sizes
//! rather than the files. `screen_sharing_service.rs` carries a 1,209-line
//! `#[cfg(all(test, unix))]` module — `#hosts-screen`'s host-scope suite — which moved with it.
//!
//! `argon2`, `chacha20poly1305` and `rand` are the vault's primitives, attributable to this
//! subsystem alone, and left `tddy-daemon` with it. `tddy-screenshare` did not: it is the bridge
//! library `packages/tddy-vnc` and `packages/tddy-rdp` are built from — the processes this service
//! *spawns* — and was never a `tddy-daemon` dependency to move.
//!
//! # The dead service next door
//!
//! Node 2 also **deletes** `vnc_service.rs` and `vnc_vault.rs` with their two acceptance suites —
//! 1,008 lines that were never reachable. `runtime.rs` registers
//! `screen_sharing.ScreenSharingService` but never `vnc.VncService`, and the only references
//! anywhere under `packages/` were the daemon's own `lib.rs` declaration, the two source files and
//! their tests. The deletion rides with this node rather than getting its own because this is the
//! only node whose reviewer is already reading the live screen-sharing service beside it.

pub mod screen_sharing_service;
pub mod screen_sharing_vault;

use std::sync::Arc;

pub use screen_sharing_service::{ScreenSharingKeyCache, ScreenSharingServiceImpl, SessionsBase};
pub use screen_sharing_vault::{
    vault_path, DerivedKey, ScreenSharingTarget, ScreenSharingVault, VAULT_FILENAME,
};

/// The `screen_sharing.ScreenSharingService` entry the daemon's wiring layer registers.
///
/// A subsystem crate's whole contract with the wiring layer is to produce a
/// [`tddy_rpc::ServiceEntry`]; nothing else about the daemon's assembly needs to know this crate
/// exists.
///
/// The service is taken already assembled rather than built from its parts here: config and host
/// scope are genuinely optional on [`ScreenSharingServiceImpl`] — a daemon with neither still
/// serves the session-scoped vault calls — and flattening the builder into this signature would
/// turn two optional collaborators into required arguments.
pub fn build_screen_sharing_entry(service: ScreenSharingServiceImpl) -> tddy_rpc::ServiceEntry {
    let server = tddy_service::ScreenSharingServiceServer::new(service);
    tddy_rpc::ServiceEntry {
        name: "screen_sharing.ScreenSharingService",
        service: Arc::new(server) as Arc<dyn tddy_rpc::RpcService>,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashMap;

    use tddy_service::proto::screen_sharing::Protocol;

    const A_PASSPHRASE: &str = "correct-horse-battery-staple";
    const A_DESKTOP_PASSWORD: &str = "the-desktop-password";

    /// Whatever the wiring layer would resolve a session token with — the entry constructor asks
    /// nothing of it, so a resolver that knows nobody is enough.
    type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

    fn a_service() -> ScreenSharingServiceImpl {
        let user_resolver: UserResolver = Arc::new(|_| None);
        let sessions_base: SessionsBase = Arc::new(|_| None);
        let key_cache: ScreenSharingKeyCache = Arc::new(tokio::sync::Mutex::new(HashMap::new()));
        ScreenSharingServiceImpl::new(user_resolver, sessions_base, key_cache)
    }

    #[test]
    fn names_the_service_the_wiring_layer_registers() {
        // Given
        let service = a_service();

        // When
        let entry = build_screen_sharing_entry(service);

        // Then — the coordinate `screen_sharing.proto` declares: `package screen_sharing;` over
        // `service ScreenSharingService`. Registering it as anything else moves the service off
        // the address every browser dials.
        assert_eq!(entry.name, "screen_sharing.ScreenSharingService");
    }

    #[test]
    fn returns_a_sealed_key_to_the_session_that_sealed_it() {
        // Given a session whose vault holds a desktop password
        let session = tempfile::tempdir().unwrap();
        let path = vault_path(session.path());
        let (mut vault, key) = ScreenSharingVault::create(&path, A_PASSPHRASE).unwrap();
        let target = vault
            .add_target(
                "the desk",
                "10.0.0.4",
                5900,
                "ada",
                A_DESKTOP_PASSWORD,
                Protocol::Vnc,
                &key,
            )
            .unwrap();

        // When that same session unlocks its vault again
        let (reopened, key) = ScreenSharingVault::unlock(&path, A_PASSPHRASE).unwrap();

        // Then
        assert_eq!(
            reopened.decrypt_password(&target.id, &key).unwrap(),
            A_DESKTOP_PASSWORD
        );
    }

    /// A session that never negotiated a key has none — distinct from a key that could not be read,
    /// which is an error. Collapsing the two would make a broken vault look like a fresh session.
    #[test]
    fn has_no_key_for_a_session_that_never_sealed_one() {
        // Given a session directory with no vault in it
        let session = tempfile::tempdir().unwrap();
        let path = vault_path(session.path());

        // When
        let valid = ScreenSharingVault::is_passphrase_valid(&path, A_PASSPHRASE);

        // Then
        assert!(
            !valid.expect("a session with no vault is an answer, not a failure"),
            "no vault means no key, which is not the same as a vault that will not open"
        );
    }

    /// Both sessions use the *same* passphrase, deliberately: the scope of a sealed credential is
    /// the session directory it was sealed in, not the secret it was sealed under.
    #[test]
    fn does_not_return_one_sessions_key_to_another() {
        // Given two sessions, one of which has a desktop sealed in its own vault
        let session_a = tempfile::tempdir().unwrap();
        let session_b = tempfile::tempdir().unwrap();
        let (mut vault_a, key_a) =
            ScreenSharingVault::create(&vault_path(session_a.path()), A_PASSPHRASE).unwrap();
        vault_a
            .add_target(
                "the desk",
                "10.0.0.4",
                5900,
                "ada",
                A_DESKTOP_PASSWORD,
                Protocol::Vnc,
                &key_a,
            )
            .unwrap();
        let (vault_b, _) =
            ScreenSharingVault::create(&vault_path(session_b.path()), A_PASSPHRASE).unwrap();

        // When
        let bs_targets = vault_b.list_targets();

        // Then
        assert!(
            bs_targets.is_empty(),
            "a sealed key is scoped to its own session"
        );
    }
}
