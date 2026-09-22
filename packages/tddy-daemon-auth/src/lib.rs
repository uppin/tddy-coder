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
//! # One key signs one thing
//!
//! Each daemon signs session tokens with an Ed25519 key of its own ([`DaemonSigningKey`]), and
//! `config.livekit.api_secret` signs LiveKit room JWTs and nothing else. A token names the key that
//! signed it, so a daemon verifying a peer's token resolves that key through a [`KeyDirectory`]
//! rather than sharing a secret with it. The daemon holds **one** [`SessionTokens`] and every
//! signer in it — the login flow, the local-socket mint, a split session's agent credential —
//! comes from that one value: a second key would be a second identity no peer was told about.
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
mod local_token;
pub mod oauth_loopback_tunnel;
pub mod signing_key;
pub mod token_provider;

/// The crate's own surface, at the crate root, so a caller writes `tddy_daemon_auth::…` for the
/// four things the daemon's wiring layer needs and reaches into a module for nothing else.
///
/// `AuthBuildResult::user_resolver` is the daemon's single identity function: every other service,
/// in every other crate, authenticates with a clone of it. It is `Option` because a daemon with no
/// GitHub configuration has no way to resolve a token — and in that state `runtime.rs` registers
/// **no session services at all**, which is a deliberate refusal rather than an oversight.
pub use auth::{
    build_auth_entries, build_auth_entries_with, build_token_service_entry,
    session_token_authenticator, AuthBuildResult, LiveKitTokenServiceImpl,
};
pub use local_token::{build_local_token_entry, mint_local_token, LocalTokenError};

/// The daemon's own signing identity and the port that resolves peers' keys.
///
/// At the root for the same reason the four above are: `runtime.rs` wires the keypair, the
/// directory implementation and the verifier together, and reaches into no module to do it.
pub use signing_key::{
    load_signing_key, signing_key_path, DaemonSigningKey, DirectorySessionTokenVerifier,
    KeyDirectory, SessionTokens, StandaloneKeyDirectory, SIGNING_KEY_FILE,
};

/// Where the daemon keeps each user's credentials at rest: their own vault, opened by a login.
///
/// The type is `tddy-credentials`', not this crate's — `AuthServiceImpl` writes through it at the
/// end of an OAuth exchange and `DaemonSessionHost` reads through it when it looks up an
/// operator's PRs, and neither needs to reach through this crate to do so.
pub use tddy_credentials::SessionVaults;

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

    use super::{build_local_token_entry, mint_local_token, DaemonSigningKey, SIGNING_KEY_FILE};

    /// A signer over a key of its own, as the daemon's wiring layer would hold.
    fn a_daemons_signer() -> (Arc<SessionTokenSigner>, tempfile::TempDir) {
        let home = tempfile::tempdir().unwrap();
        let key = DaemonSigningKey::load_or_generate(&home.path().join(SIGNING_KEY_FILE))
            .expect("a daemon generates a keypair");
        (Arc::new(key.signer()), home)
    }

    #[test]
    fn names_the_service_family_q_moves_to() {
        let (signer, _home) = a_daemons_signer();
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
        let (signer, _home) = a_daemons_signer();
        build_local_token_entry(signer);

        // When
        let token = mint_local_token("alice").expect("a resolved identity mints");

        // Then
        assert!(!token.is_empty());
    }
}
