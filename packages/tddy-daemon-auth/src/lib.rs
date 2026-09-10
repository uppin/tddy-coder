//! The daemon's identity boundary: who a session token belongs to, and every credential the daemon
//! holds on their behalf.
//!
//! Extracted from `tddy-daemon` by `#unbundle` node 4. Its **entire** dependency on the
//! 23,099-line god module was one type alias — `auth.rs:25: use
//! crate::connection_service::SessionUserResolver` — which node 1 moved into
//! `tddy-daemon-kernel`. That is a striking amount of weak coupling behind 2,145 production lines,
//! and it is why this subsystem could leave in an early node.
//!
//! Its four proto services — `auth.AuthService`, `auth.LiveKitTokenService`, `token.TokenService`
//! and `loopback_tunnel.LoopbackTunnelService` — were **already their own protos**, so no wire
//! coordinate changes here and no client migrates.
//!
//! # One secret signs two things
//!
//! `config.livekit.api_secret` signs **both** LiveKit room JWTs and session tokens, through
//! `tddy_github::SessionTokenSigner`. Splitting auth from LiveKit into two crates does not split
//! that secret, and **neither crate may start deriving its own** — a second signer would silently
//! partition which tokens each half accepts.
//!
//! # This crate is smaller than "auth" suggests
//!
//! The host-key path — `host_keypair`, `host_private_key`, `ssh_agent`, `ssh_agent_add`, 1,994
//! production lines — went to `tddy-host-service` in node 1, because `AddHostKey` and
//! `ListHostKeyCandidates` are host-service methods and because that move is what cut the
//! `host_tooling ⇄ ssh_agent` cycle. The boundary is real rather than convenient: **what is left
//! here signs and verifies; what left with node 1 unlocks and loads.**

pub mod auth;
mod codex_oauth_participant_metadata;
pub mod codex_oauth_relay;
pub mod github_pr_credentials;
pub mod github_token_store;
pub mod oauth_loopback_tunnel;
pub mod token_provider;

/// The crate's own surface, at the crate root, so a caller writes `tddy_daemon_auth::…` for the
/// four things the daemon's wiring layer needs and reaches into a module for nothing else.
///
/// `AuthBuildResult::user_resolver` is the daemon's single identity function: every other service,
/// in every other crate, authenticates with a clone of it. It is `Option` because a daemon with no
/// GitHub configuration has no way to resolve a token — and in that state `runtime.rs` registers
/// **no session services at all**, which is a deliberate refusal rather than an oversight.
pub use auth::{
    build_auth_entries, build_token_service_entry, session_token_authenticator, AuthBuildResult,
    LiveKitTokenServiceImpl,
};

/// Where the daemon keeps a user's GitHub token at rest.
///
/// The trait is `tddy-github`'s, not this crate's: `AuthServiceImpl` writes through it at the end
/// of an OAuth exchange and `ConnectionServiceImpl` reads through it when it looks up an
/// operator's PRs, so a second definition here would be a second trait two crates could not pass
/// to one another. [`github_token_store::FileGitHubTokenStore`] is this crate's implementation of
/// it.
pub use tddy_github::token_store::GitHubTokenStore;

#[cfg(test)]
mod tests {
    use super::*;

    /// ⛔ `docs/dev/todo/2026-08-16-the-daemon-s-secret-stores-still-truncate-in-place.md`.
    ///
    /// The store truncates in place, so a crash between truncate and write leaves a **truncated
    /// secret at rest**. This crate exists to be the identity boundary, so shipping it with that
    /// path at its centre would be worse than a slightly larger diff — and hand-rolling an atomic
    /// write beside `tddy_core::atomic_file::write_atomic` would deepen the duplication the entry is
    /// about. It is routed through the existing helper as part of the move.
    ///
    /// A read-only storage directory stands in for the failure: it is what a full filesystem does
    /// to the swap file the replacement is staged in, and it is the same stand-in
    /// `tddy_core::atomic_file`'s own disk-full test uses.
    #[cfg(unix)]
    #[test]
    fn a_failed_write_leaves_the_previous_secret_intact() {
        use std::os::unix::fs::PermissionsExt;

        // Given a store holding a token
        let dir = tempfile::tempdir().unwrap();
        let store = a_store_at(dir.path());
        store.put("alice", "the-original-token").unwrap();

        // When a write fails part-way — simulated by writing to a path made unwritable
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
        // root ignores the permission bits, so there is nothing for this case to observe there.
        let unwritable = std::fs::File::create(dir.path().join(".probe")).is_err();
        let refused = store.put("alice", "a-replacement-that-never-lands");
        let survivor = store.get("alice");
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        if !unwritable {
            return;
        }

        // Then the original is still readable, never a truncated prefix of either
        assert!(
            refused.is_err(),
            "a write into a read-only directory must be reported, not swallowed"
        );
        assert_eq!(
            survivor.as_deref(),
            Some("the-original-token"),
            "a partial write must not destroy the value it was replacing"
        );
    }

    #[test]
    fn has_no_token_for_a_user_none_was_stored_for() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let store = a_store_at(dir.path());

        // When
        let found = store.get("bob");

        // Then
        assert_eq!(found, None);
    }

    /// A daemon with no GitHub configuration registers no session services at all. That is a
    /// refusal, and the caller has to be able to see it rather than receive an empty success.
    #[test]
    fn refuses_to_build_an_identity_function_with_no_github_configuration() {
        // Given
        let (config, _dir) = a_daemon_with_no_github_block();

        // When
        let outcome = build_auth_entries(&config, "127.0.0.1", 8080);

        // Then
        let built = outcome.expect("building the entries themselves does not fail");
        assert!(
            built.user_resolver.is_none(),
            "no configuration means no identity function, not a permissive one"
        );
    }

    fn a_store_at(path: &std::path::Path) -> Box<dyn GitHubTokenStore> {
        Box::new(github_token_store::FileGitHubTokenStore::new(path))
    }

    /// A daemon whose `daemon.yaml` carries no `github:` block at all — the state a fresh install
    /// starts in, and the one this crate has to refuse rather than paper over.
    fn a_daemon_with_no_github_block(
    ) -> (tddy_daemon_kernel::config::DaemonConfig, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, "users: []\n").unwrap();
        (
            tddy_daemon_kernel::config::DaemonConfig::load(&path).unwrap(),
            dir,
        )
    }
}
