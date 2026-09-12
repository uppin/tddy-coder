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
mod local_token;
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
pub use local_token::{build_local_token_entry, mint_local_token, LocalTokenError};

/// Where the daemon keeps a user's GitHub token at rest.
///
/// The trait is `tddy-github`'s, not this crate's: `AuthServiceImpl` writes through it at the end
/// of an OAuth exchange and `DaemonSessionHost` reads through it when it looks up an
/// operator's PRs, so a second definition here would be a second trait two crates could not pass
/// to one another. [`github_token_store::FileGitHubTokenStore`] is this crate's implementation of
/// it.
pub use tddy_github::token_store::GitHubTokenStore;

#[cfg(test)]
mod tests {
    use super::*;

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

#[cfg(test)]
mod unbundle_local_token_tests {
    use std::sync::Arc;

    use tddy_github::SessionTokenSigner;

    use super::{build_local_token_entry, mint_local_token};

    #[test]
    fn names_the_service_family_q_moves_to() {
        let signer = Arc::new(SessionTokenSigner::new(b"family-q-name-test"));
        assert_eq!(
            build_local_token_entry(signer).name,
            "local_token.LocalTokenService"
        );
    }

    /// The mint takes an identity the transport resolved. It never reads a socket, which is the whole
    /// reason the credential read stays with the transport and only the signing moves here.
    #[test]
    fn mints_for_an_identity_the_transport_already_resolved() {
        // Given a signer the wiring layer would have installed
        let signer = Arc::new(SessionTokenSigner::new(b"family-q-mint-test"));
        build_local_token_entry(signer);

        // When
        let token = mint_local_token("alice").expect("a resolved identity mints");

        // Then
        assert!(!token.is_empty());
    }
}
