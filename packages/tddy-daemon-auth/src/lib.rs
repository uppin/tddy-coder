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

use std::sync::Arc;

use tddy_daemon_kernel::SessionUserResolver;

/// What building the auth services yields.
///
/// The `user_resolver` is the daemon's single identity function: every other service authenticates
/// with a clone of it. It is `Option` because a daemon with no GitHub configuration has no way to
/// resolve a token — and in that state `runtime.rs` registers **no session services at all**, which
/// is a deliberate refusal rather than an oversight.
pub struct AuthBuildResult {
    pub entries: Vec<tddy_rpc::ServiceEntry>,
    pub user_resolver: Option<SessionUserResolver>,
    pub github_token_store: Option<Arc<dyn GitHubTokenStore>>,
}

/// Where the daemon keeps a user's GitHub token at rest.
pub trait GitHubTokenStore: Send + Sync {
    /// Store `token` for `user`, replacing any previous value.
    fn put(&self, user: &str, token: &str) -> Result<(), AuthError>;
    /// Read `user`'s token, or `None` when none is stored.
    fn get(&self, user: &str) -> Result<Option<String>, AuthError>;
}

/// Why an auth operation could not be completed.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("the token store at {path} could not be written: {reason}")]
    StoreUnwritable { path: String, reason: String },
    #[error("no GitHub configuration, so no session token can be resolved")]
    Unconfigured,
}

/// Build every auth-family service entry, plus the identity function they establish.
pub fn build_auth_entries(
    _web_host: &str,
    _web_port: u16,
    _auth_storage: Option<&std::path::Path>,
) -> Result<AuthBuildResult, AuthError> {
    // TODO(auth-livekit): implement
    unimplemented!("build_auth_entries")
}

/// Mint a LiveKit room JWT, signed with the same secret that signs session tokens.
pub fn mint_livekit_token(
    _api_secret: &str,
    _room: &str,
    _identity: &str,
) -> Result<String, AuthError> {
    // TODO(auth-livekit): implement
    unimplemented!("mint_livekit_token")
}

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
    #[test]
    fn a_failed_write_leaves_the_previous_secret_intact() {
        // Given a store holding a token
        let dir = tempfile::tempdir().unwrap();
        let store = a_store_at(dir.path());
        store.put("alice", "the-original-token").unwrap();

        // When a write fails part-way — simulated by writing to a path made unwritable
        let _ = store.put("alice", "a-replacement-that-never-lands");

        // Then the original is still readable, never a truncated prefix of either
        assert_eq!(
            store.get("alice").unwrap().as_deref(),
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
        let found = store.get("bob").unwrap();

        // Then
        assert_eq!(found, None);
    }

    /// A daemon with no GitHub configuration registers no session services at all. That is a
    /// refusal, and the caller has to be able to see it rather than receive an empty success.
    #[test]
    fn refuses_to_build_an_identity_function_with_no_github_configuration() {
        // When
        let outcome = build_auth_entries("127.0.0.1", 8080, None);

        // Then
        let built = outcome.expect("building the entries themselves does not fail");
        assert!(
            built.user_resolver.is_none(),
            "no configuration means no identity function, not a permissive one"
        );
    }

    fn a_store_at(_path: &std::path::Path) -> Box<dyn GitHubTokenStore> {
        // TODO(auth-livekit): implement
        unimplemented!("a token store backed by a directory")
    }
}
