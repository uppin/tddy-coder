use super::*;
use tddy_daemon_kernel::SessionsBaseResolver;
use tddy_service::proto::project::{ListProjectsRequest, ProjectService};

/// A daemon config with GitHub auth enabled and, when `api_secret` is `Some`, a LiveKit
/// secret that signs/verifies session tokens. Maps GitHub login "u" to OS user "u".
fn a_daemon_config(api_secret: Option<&str>) -> (crate::config::DaemonConfig, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let livekit = match api_secret {
        Some(s) => format!("livekit:\n  api_secret: \"{s}\"\n"),
        None => String::new(),
    };
    let yaml = format!(
        "users:\n  - github_user: \"u\"\n    os_user: \"u\"\ngithub:\n  stub: true\n{livekit}"
    );
    let path = dir.path().join("config.yaml");
    std::fs::write(&path, yaml).unwrap();
    let config = crate::config::DaemonConfig::load(&path).unwrap();
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

/// A ConnectionService whose `user_resolver` is exactly the one the daemon's auth wiring
/// produces for `config` — i.e. what a *peer* daemon verifies incoming tokens with.
fn a_peer_daemon(
    config: crate::config::DaemonConfig,
    data_dir: std::path::PathBuf,
) -> DaemonSessionHost {
    let resolver = crate::auth::build_auth_entries(&config, "127.0.0.1", 0)
        .expect("auth wiring should build")
        .user_resolver
        .expect("auth wiring should produce a session resolver");
    let base = data_dir.clone();
    let sessions_base_resolver: SessionsBaseResolver = Arc::new(move |_| Some(base.clone()));
    DaemonSessionHost::new(
        config,
        sessions_base_resolver,
        data_dir,
        resolver,
        None,
        None,
        None,
        Arc::new(CliSessionManager::new()),
    )
}

#[tokio::test]
async fn a_peer_daemon_accepts_a_token_minted_with_the_same_shared_secret() {
    // Given a peer daemon whose auth is wired with a shared signing secret
    let (config, dir) = a_daemon_config(Some("shared-secret"));
    let service = a_peer_daemon(config, dir.path().to_path_buf());
    // and a token minted by another daemon holding that same secret
    let signer = tddy_github::SessionTokenSigner::new(b"shared-secret");
    let token = signer.mint(&a_github_user("u"), tddy_github::SESSION_TOKEN_TTL);

    // When the peer lists projects with that token
    let request = Request::new(ListProjectsRequest {
        session_token: token,
        local_only: true,
    });
    let result = service.list_projects(request).await;

    // Then the peer accepts it — no "invalid or expired session"
    assert!(
        result.is_ok(),
        "peer daemon should accept a token signed with the shared secret"
    );
}

#[tokio::test]
async fn a_peer_daemon_rejects_a_token_signed_with_a_different_secret() {
    // Given a peer daemon wired with one signing secret
    let (config, dir) = a_daemon_config(Some("this-daemons-secret"));
    let service = a_peer_daemon(config, dir.path().to_path_buf());
    // and a token minted with a different secret
    let foreign = tddy_github::SessionTokenSigner::new(b"some-other-secret");
    let token = foreign.mint(&a_github_user("u"), tddy_github::SESSION_TOKEN_TTL);

    // When the peer lists projects with that token
    let request = Request::new(ListProjectsRequest {
        session_token: token,
        local_only: true,
    });
    let result = service.list_projects(request).await;

    // Then the peer rejects it as an invalid session
    let err = result.expect_err("a token signed with a foreign secret must be rejected");
    assert_eq!(err.code, tddy_rpc::Code::Unauthenticated);
}
