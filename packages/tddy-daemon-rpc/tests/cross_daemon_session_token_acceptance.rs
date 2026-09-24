//! A peer daemon verifies the session tokens other daemons mint, asked through `ListProjects`.
//!
//! Moved from `tddy-session-lifecycle`'s in-crate tests with the project handlers it calls.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tddy_daemon_auth::{DaemonSigningKey, KeyDirectory, SessionTokens, SIGNING_KEY_FILE};
use tddy_daemon_kernel::SessionsBaseResolver;
use tddy_daemon_rpc::test_util::TestDaemon;
use tddy_github::session_token_v2::{Ed25519VerifyingKey, KeyId};
use tddy_rpc::Request;
use tddy_service::proto::project::{ListProjectsRequest, ProjectService};
use tddy_session_lifecycle::cli_session_manager::CliSessionManager;
use tddy_session_lifecycle::connection_service::DaemonSessionHost;

/// A daemon config with GitHub auth enabled, mapping GitHub login "u" to OS user "u", and not one
/// line of LiveKit — no secret any two daemons could share.
fn a_daemon_config() -> (
    tddy_session_lifecycle::config::DaemonConfig,
    tempfile::TempDir,
) {
    let dir = tempfile::tempdir().unwrap();
    let yaml = "users:\n  - github_user: \"u\"\n    os_user: \"u\"\ngithub:\n  stub: true\n";
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).unwrap();
    let config = tddy_session_lifecycle::config::DaemonConfig::load(&path).unwrap();
    (config, dir)
}

fn a_github_user(login: &str) -> tddy_github::GitHubUser {
    tddy_github::GitHubUser {
        id: 7,
        login: login.to_string(),
        avatar_url: format!("https://github.com/{login}.png"),
        name: login.to_string(),
    }
}

/// A daemon's signing identity, in a data directory of its own.
fn a_daemon_key() -> (DaemonSigningKey, tempfile::TempDir) {
    let home = tempfile::tempdir().unwrap();
    let key = DaemonSigningKey::load_or_generate(&home.path().join(SIGNING_KEY_FILE))
        .expect("a daemon generates a keypair");
    (key, home)
}

/// The fleet's key distribution as a fake: every daemon announces onto it and reads back from it.
#[derive(Default)]
struct AFleet {
    announced: Mutex<HashMap<KeyId, Ed25519VerifyingKey>>,
}

impl AFleet {
    /// A daemon joining the fleet with `key` — what its common-room advertisement does for real.
    fn announce(&self, key: &DaemonSigningKey) {
        self.announced
            .lock()
            .unwrap()
            .insert(key.key_id(), key.verifying_key());
    }
}

#[async_trait]
impl KeyDirectory for AFleet {
    async fn public_key_for(&self, key_id: &KeyId) -> anyhow::Result<Option<Ed25519VerifyingKey>> {
        Ok(self.announced.lock().unwrap().get(key_id).copied())
    }
}

/// A ConnectionService whose `user_resolver` is exactly the one the daemon's auth wiring
/// produces for `config` and `tokens` — i.e. what a *peer* daemon verifies incoming tokens with.
fn a_peer_daemon(
    config: tddy_session_lifecycle::config::DaemonConfig,
    tokens: &SessionTokens,
    data_dir: std::path::PathBuf,
) -> TestDaemon {
    let resolver =
        tddy_session_lifecycle::auth::build_auth_entries_with(&config, "127.0.0.1", 0, tokens)
            .expect("auth wiring should build")
            .user_resolver
            .expect("auth wiring should produce a session resolver");
    let base = data_dir.clone();
    let sessions_base_resolver: SessionsBaseResolver = Arc::new(move |_| Some(base.clone()));
    TestDaemon::from_host(DaemonSessionHost::new(
        config,
        sessions_base_resolver,
        data_dir,
        resolver,
        None,
        None,
        None,
        Arc::new(CliSessionManager::new()),
    ))
}

#[tokio::test]
async fn a_peer_daemon_accepts_a_token_minted_by_a_daemon_whose_key_it_has_learned() {
    // Given two daemons in one fleet, each with a key of its own, both having announced it
    let fleet = Arc::new(AFleet::default());
    let (minting_daemon, _minting_home) = a_daemon_key();
    let (peer_key, _peer_home) = a_daemon_key();
    for daemon in [&minting_daemon, &peer_key] {
        fleet.announce(daemon);
    }
    let (config, dir) = a_daemon_config();
    let service = a_peer_daemon(
        config,
        &SessionTokens::new(&peer_key, fleet.clone()),
        dir.path().to_path_buf(),
    );
    // and a token the other daemon minted
    let token = minting_daemon
        .signer()
        .mint(&a_github_user("u"), tddy_github::SESSION_TOKEN_TTL);

    // When the peer lists projects with that token
    let request = Request::direct(ListProjectsRequest {
        session_token: token,
        local_only: true,
    });
    let result = service.list_projects(request).await;

    // Then the peer accepts it — no "invalid or expired session", and no shared secret
    assert!(
        result.is_ok(),
        "peer daemon should accept a token signed by a daemon whose key it has learned"
    );
}

#[tokio::test]
async fn a_peer_daemon_rejects_a_token_signed_by_a_daemon_it_has_never_heard_of() {
    // Given a peer daemon whose fleet has never heard of the minting daemon
    let (peer_key, _peer_home) = a_daemon_key();
    let (config, dir) = a_daemon_config();
    let service = a_peer_daemon(
        config,
        &SessionTokens::new(&peer_key, Arc::new(AFleet::default())),
        dir.path().to_path_buf(),
    );
    // and a token minted by that stranger
    let (stranger, _stranger_home) = a_daemon_key();
    let token = stranger
        .signer()
        .mint(&a_github_user("u"), tddy_github::SESSION_TOKEN_TTL);

    // When the peer lists projects with that token
    let request = Request::direct(ListProjectsRequest {
        session_token: token,
        local_only: true,
    });
    let result = service.list_projects(request).await;

    // Then the peer rejects it as an invalid session — an unknown signer is not a signer to trust
    let err = result.expect_err("a token from an unannounced daemon must be rejected");
    assert_eq!(err.code, tddy_rpc::Code::Unauthenticated);
}
