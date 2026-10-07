//! Screen sharing: the `screen_sharing.ScreenSharingService` surface. A desktop's password is a
//! `screen-sharing` record in the credential store (`tddy-credentials`); this crate holds no
//! cryptography of its own.
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
//! # The dead service next door
//!
//! Node 2 also **deletes** `vnc_service.rs` and `vnc_vault.rs` with their two acceptance suites —
//! 1,008 lines that were never reachable. `runtime.rs` registers
//! `screen_sharing.ScreenSharingService` but never `vnc.VncService`, and the only references
//! anywhere under `packages/` were the daemon's own `lib.rs` declaration, the two source files and
//! their tests. The deletion rides with this node rather than getting its own because this is the
//! only node whose reviewer is already reading the live screen-sharing service beside it.

pub mod screen_sharing_records;
pub mod screen_sharing_service;
pub mod session_vault_target_store;

use std::sync::Arc;

pub use screen_sharing_records::{
    account_for, record_for, screen_sharing_provider, target_from, ScreenSharingTargetStore,
    TargetError, META_HOST, META_PORT, META_PROTOCOL, META_USERNAME, SCREEN_SHARING_PROVIDER,
};
pub use screen_sharing_service::{ScreenSharingServiceImpl, SessionsBase};
pub use session_vault_target_store::{SessionSubjectResolver, SessionVaultTargetStore};

/// The `screen_sharing.ScreenSharingService` entry the daemon's wiring layer registers.
///
/// A subsystem crate's whole contract with the wiring layer is to produce a
/// [`tddy_rpc::ServiceEntry`]; nothing else about the daemon's assembly needs to know this crate
/// exists.
///
/// The service is taken already assembled rather than built from its parts here: config and host
/// scope are genuinely optional on [`ScreenSharingServiceImpl`] — a daemon with neither still
/// serves the session-scoped target calls — and flattening the builder into this signature would
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

    /// Whatever the wiring layer would resolve a session token with — the entry constructor asks
    /// nothing of it, so a resolver that knows nobody is enough.
    type UserResolver = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

    fn a_service() -> ScreenSharingServiceImpl {
        let user_resolver: UserResolver = Arc::new(|_| None);
        let sessions_base: SessionsBase = Arc::new(|_| None);
        ScreenSharingServiceImpl::new(user_resolver, sessions_base)
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
}
