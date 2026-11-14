use crate::cli_session_manager::CliSessionManager;
use crate::connection_service::DaemonSessionHost;
use crate::peer_routing::PeerRouting;
use crate::test_util::builtin_coding_agents;
use crate::test_util::config_from_mapping;
use crate::test_util::TEST_USER;
use std::path::PathBuf;
use std::sync::Arc;
use tddy_daemon_kernel::SessionsBaseResolver;

/// The credentials a [`signing_test_host`] accepts for [`TEST_USER`], each minted with that host's
/// own signing key — real `v2` tokens, unlike [`TEST_TOKEN`].
pub struct SignedTestCredentials {
    /// The operator's own access token, as a signed-in browser holds it.
    pub user_token: String,
    /// The token a session's main agent is given: an access token under the operator's identity
    /// that carries the agent principal, as `mint_agent_session_token` mints it, naming
    /// [`TEST_AGENT_SESSION`] — a session no suite creates. For a token naming a real session, see
    /// [`SignedTestCredentials::agent_token_for`].
    pub agent_token: String,
    tokens: tddy_daemon_auth::SessionTokens,
    operator: tddy_github::GitHubUser,
    /// Owns the host's signing key on disk.
    _key_home: tempfile::TempDir,
}

/// The session [`SignedTestCredentials::agent_token`] names.
pub const TEST_AGENT_SESSION: &str = "test-agent-session";

impl SignedTestCredentials {
    /// The token the agent of `session_id` is given, as `mint_agent_session_token` mints it.
    #[must_use]
    pub fn agent_token_for(&self, session_id: &str) -> String {
        self.tokens.signer().mint_agent(
            &self.operator,
            std::time::Duration::from_secs(600),
            session_id,
        )
    }
}

/// A host wired to `sessions_base` that authenticates **real** session tokens: its user resolver is
/// the one the daemon's auth wiring builds, over a signing key of its own, and it holds those
/// tokens (`with_session_tokens`) as the daemon does — so it can tell the main agent's token from
/// the operator's. [`TEST_TOKEN`] is not accepted; [`SignedTestCredentials`] are.
#[must_use]
pub fn signing_test_host(sessions_base: PathBuf) -> (DaemonSessionHost, SignedTestCredentials) {
    signing_test_host_as(sessions_base, "testdev")
}

/// [`signing_test_host`], whose daemon maps [`TEST_USER`] to `os_user` — an OS user that resolves
/// in the passwd database, for a suite whose host spawns an agent process as that user.
pub fn signing_test_host_as(
    sessions_base: PathBuf,
    os_user: &str,
) -> (DaemonSessionHost, SignedTestCredentials) {
    let key_home = tempfile::tempdir().expect("create temp dir for the host's signing key");
    let key = tddy_daemon_auth::DaemonSigningKey::load_or_generate(
        &key_home.path().join(tddy_daemon_auth::SIGNING_KEY_FILE),
    )
    .expect("the host generates its signing key");
    let tokens = tddy_daemon_auth::SessionTokens::new(
        &key,
        Arc::new(tddy_daemon_auth::StandaloneKeyDirectory),
    );
    let config = config_from_mapping(
        SIGNING_CONFIG_YAML,
        &format!("- github_user: \"{TEST_USER}\"\n  os_user: \"{os_user}\"\n"),
    );
    let user_resolver = crate::auth::build_auth_entries_with(
        &config,
        "127.0.0.1",
        0,
        &tokens,
        Arc::new(tddy_daemon_extensions::room_tokens::NoRoomTokens),
    )
    .expect("the auth wiring builds")
    .user_resolver
    .expect("the auth wiring resolves session tokens");
    let operator = tddy_github::GitHubUser {
        id: 7,
        login: TEST_USER.to_string(),
        avatar_url: String::new(),
        name: TEST_USER.to_string(),
    };
    let credentials = SignedTestCredentials {
        user_token: tokens.signer().mint_access(&operator),
        agent_token: tokens.signer().mint_agent(
            &operator,
            std::time::Duration::from_secs(600),
            TEST_AGENT_SESSION,
        ),
        tokens: tokens.clone(),
        operator,
        _key_home: key_home,
    };
    let tddy_data_dir = sessions_base.clone();
    let sessions_base_resolver: SessionsBaseResolver =
        Arc::new(move |_| Some(sessions_base.clone()));
    let host = DaemonSessionHost::new(
        config.clone(),
        sessions_base_resolver,
        tddy_data_dir,
        user_resolver,
        None,
        tddy_spawn::supervisor_client::SpawnBackendChoice::ForkedWorker,
        PeerRouting::local_only(&config),
        Arc::new(tddy_daemon_kernel::operator_channel::NoOperatorChannel),
        Arc::new(CliSessionManager::new()),
        builtin_coding_agents(),
    )
    .with_session_tokens(tokens);
    (host, credentials)
}

/// [`CONFIG_YAML`] with a (stub) GitHub provider, which is what gives a daemon its token wiring.
const SIGNING_CONFIG_YAML: &str = r#"
github:
  stub: true
"#;
