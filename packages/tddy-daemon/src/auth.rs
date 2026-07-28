//! Build AuthService from daemon config.

use std::sync::Arc;

use tddy_github::token_store::GitHubTokenStore;
use tddy_github::{
    AuthServiceImpl, GitHubOAuthProvider, RealGitHubProvider, SessionTokenSigner,
    StubGitHubProvider, TokenKind,
};
use tddy_rpc::ServiceEntry;
use tddy_service::AuthServiceServer;

use crate::config::DaemonConfig;
use crate::connection_service::SessionUserResolver;

/// Result of building auth: RPC entries, a resolver for session token -> GitHub login, and the
/// GitHub access tokens logins granted (the credential `ConnectionService` reads PRs with).
pub struct AuthBuildResult {
    pub entries: Vec<ServiceEntry>,
    pub user_resolver: Option<SessionUserResolver>,
    /// `Some` when `auth_storage` is configured. Shared with `ConnectionServiceImpl`, which reads
    /// the caller's token from it; `None` leaves PR status *unavailable* for a real login.
    pub github_token_store: Option<Arc<dyn GitHubTokenStore>>,
}

/// Build RPC entries for AuthService when GitHub is configured.
/// Returns entries and a user resolver for ConnectionService.
///
/// Session tokens are stateless, HMAC-signed tokens (see `tddy_github::session_token`) keyed on
/// the shared `livekit.api_secret`, so a token minted by one daemon is verifiable by every daemon
/// that holds the same secret. When no secret is configured the daemon still starts, but auth is
/// non-functional: minting fails and the resolver rejects every token.
///
/// Signed tokens are stateless, so no session state is persisted — hence no data-dir argument.
///
/// Fails when a configured `auth_storage` cannot hold a token file. Retention is a hard login
/// dependency now (a failed `put` fails the exchange, PRD D13), so an unwritable path breaks *every*
/// login rather than merely degrading PR status — and `install` only creates and chowns the parent
/// `/var/lib/tddy` on the root/systemd path, so it is a reachable misconfiguration.
pub fn build_auth_entries(
    config: &DaemonConfig,
    web_host: &str,
    web_port: u16,
) -> anyhow::Result<AuthBuildResult> {
    let github = match &config.github {
        Some(g) => g,
        None => {
            return Ok(AuthBuildResult {
                entries: vec![],
                user_resolver: None,
                github_token_store: None,
            });
        }
    };

    // The one secret every daemon in a deployment shares (it also signs LiveKit room JWTs).
    let signing_secret = config.livekit.as_ref().and_then(|lk| lk.api_secret.clone());
    let signer = signing_secret
        .as_deref()
        .map(|s| SessionTokenSigner::new(s.as_bytes()));

    // Where a real login's GitHub access token is retained, so the daemon can later read that
    // operator's PRs. No `auth_storage` means no retention — PR status then reads as *unavailable*
    // rather than as "no PR" (PR-stack UX recovery, D7/D8).
    //
    // A configured path is probed before it is trusted: created, written to, and the probe removed.
    // Starting anyway and letting the store fail later is not an option — every login would then
    // fail with an internal error, one operator at a time, for a fault that is entirely visible at
    // boot. The probe runs for a stub provider too: a stub retains nothing, but the path the
    // operator configured is unusable either way, and skipping the check for one provider kind is
    // exactly the sort of quiet degradation this replaces.
    let github_token_store: Option<Arc<dyn GitHubTokenStore>> = match config.auth_storage.as_ref() {
        Some(dir) => {
            let store = crate::github_token_store::FileGitHubTokenStore::new(dir);
            store.probe_writable().map_err(|e| {
                anyhow::anyhow!(
                    "config.auth_storage ({}) cannot hold GitHub access tokens: {e}. \
                     Every GitHub login fails until it is writable by the daemon user.",
                    dir.display()
                )
            })?;
            Some(Arc::new(store) as Arc<dyn GitHubTokenStore>)
        }
        None => None,
    };

    let auth_entry = if github.stub.unwrap_or(false) {
        let client_id = github.client_id.as_deref().unwrap_or("stub-client-id");
        let callback_url = github
            .redirect_uri
            .clone()
            .unwrap_or_else(|| format!("http://{}:{}/auth/callback", web_host, web_port));
        let stub = StubGitHubProvider::new_with_callback(&callback_url, client_id);
        if let Some(ref codes) = github.stub_codes {
            register_stub_codes(&stub, codes);
        }
        auth_service_entry(stub, signer.clone(), github_token_store.clone())
    } else if let (Some(id), Some(secret)) = (&github.client_id, &github.client_secret) {
        let redirect_uri = github
            .redirect_uri
            .clone()
            .unwrap_or_else(|| format!("http://{}:{}/auth/callback", web_host, web_port));
        let real = RealGitHubProvider::new(id, secret, &redirect_uri);
        auth_service_entry(real, signer.clone(), github_token_store.clone())
    } else {
        return Ok(AuthBuildResult {
            entries: vec![],
            user_resolver: None,
            github_token_store: None,
        });
    };

    // Verify the token's signature/expiry and extract the login. Only access-kind tokens
    // authenticate an RPC — a long-lived refresh token is rejected here so it cannot be used as
    // an RPC credential. With no signer, every token is rejected (returns `None`), so all
    // token-gated RPCs are unauthenticated.
    let user_resolver: SessionUserResolver = match signer {
        Some(signer) => Arc::new(move |token: &str| {
            signer
                .verify(token)
                .ok()
                .filter(|c| c.kind == TokenKind::Access)
                .map(|c| c.login)
        }),
        None => Arc::new(|_: &str| None),
    };

    Ok(AuthBuildResult {
        entries: vec![auth_entry],
        user_resolver: Some(user_resolver),
        github_token_store,
    })
}

/// Register `code:login` mappings (from `github.stub_codes`, comma-separated) on the stub provider
/// so tests/dev can complete the OAuth exchange without a real GitHub app. Malformed entries are
/// skipped.
fn register_stub_codes(stub: &StubGitHubProvider, codes: &str) {
    for mapping in codes.split(',') {
        let parts: Vec<&str> = mapping.splitn(2, ':').collect();
        if parts.len() == 2 {
            stub.register_code(
                parts[0],
                tddy_github::GitHubUser {
                    id: 1,
                    login: parts[1].to_string(),
                    avatar_url: format!("https://github.com/{}.png", parts[1]),
                    name: parts[1].to_string(),
                },
            );
        }
    }
}

/// Wrap an OAuth provider in an `auth.AuthService` RPC entry. When a signer is present, tokens are
/// stateless HMAC-signed tokens; otherwise the service cannot mint and every token is rejected.
///
/// `token_store`, when present, retains each real login's GitHub access token. A stub provider
/// stores nothing regardless — its token is synthetic (PRD D12).
fn auth_service_entry<P: GitHubOAuthProvider>(
    provider: P,
    signer: Option<SessionTokenSigner>,
    token_store: Option<Arc<dyn GitHubTokenStore>>,
) -> ServiceEntry {
    let with_store = |service: AuthServiceImpl<P>| match token_store {
        Some(store) => service.with_token_store(store),
        None => service,
    };
    let server = match signer {
        Some(signer) => {
            AuthServiceServer::new(with_store(AuthServiceImpl::new_signed(provider, signer)))
        }
        None => AuthServiceServer::new(with_store(AuthServiceImpl::new(provider))),
    };
    ServiceEntry {
        name: "auth.AuthService",
        service: Arc::new(server) as Arc<dyn tddy_rpc::RpcService>,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A daemon config with GitHub auth enabled and, when `api_secret` is `Some`, a LiveKit
    /// secret used to sign/verify session tokens.
    fn a_config(api_secret: Option<&str>) -> (DaemonConfig, tempfile::TempDir) {
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
        (DaemonConfig::load(&path).unwrap(), dir)
    }

    /// A daemon config whose `auth_storage` — where a login's GitHub access token is retained —
    /// points at `auth_storage`.
    fn a_config_storing_tokens_in(
        auth_storage: &std::path::Path,
    ) -> (DaemonConfig, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let yaml = format!(
            "users:\n  - github_user: \"u\"\n    os_user: \"u\"\ngithub:\n  stub: true\nlivekit:\n  api_secret: \"shared-secret\"\nauth_storage: \"{}\"\n",
            auth_storage.display()
        );
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, yaml).unwrap();
        (DaemonConfig::load(&path).unwrap(), dir)
    }

    fn a_github_user(login: &str) -> tddy_github::GitHubUser {
        tddy_github::GitHubUser {
            id: 1,
            login: login.to_string(),
            avatar_url: String::new(),
            name: login.to_string(),
        }
    }

    #[test]
    fn the_resolver_accepts_a_token_signed_with_the_configured_secret() {
        // Given auth wired with a shared signing secret
        let (config, _dir) = a_config(Some("shared-secret"));
        let resolver = build_auth_entries(&config, "127.0.0.1", 0)
            .expect("auth should build")
            .user_resolver
            .expect("auth should produce a resolver");
        // and a token minted with that same secret
        let token = tddy_github::SessionTokenSigner::new(b"shared-secret")
            .mint(&a_github_user("u"), tddy_github::SESSION_TOKEN_TTL);

        // When the resolver resolves it
        let login = (resolver)(&token);

        // Then it maps to the token's GitHub login
        assert_eq!(login.as_deref(), Some("u"));
    }

    #[test]
    fn the_resolver_rejects_a_token_signed_with_a_foreign_secret() {
        // Given auth wired with one signing secret
        let (config, _dir) = a_config(Some("this-daemons-secret"));
        let resolver = build_auth_entries(&config, "127.0.0.1", 0)
            .expect("auth should build")
            .user_resolver
            .expect("auth should produce a resolver");
        // and a token minted with a different secret
        let token = tddy_github::SessionTokenSigner::new(b"some-other-secret")
            .mint(&a_github_user("u"), tddy_github::SESSION_TOKEN_TTL);

        // When the resolver resolves it
        let login = (resolver)(&token);

        // Then it is rejected
        assert_eq!(login, None);
    }

    #[test]
    fn the_resolver_accepts_an_access_kind_token() {
        // Given auth wired with a shared signing secret
        let (config, _dir) = a_config(Some("shared-secret"));
        let resolver = build_auth_entries(&config, "127.0.0.1", 0)
            .expect("auth should build")
            .user_resolver
            .expect("auth should produce a resolver");
        // and an access-kind token minted with that secret
        let token =
            tddy_github::SessionTokenSigner::new(b"shared-secret").mint_access(&a_github_user("u"));

        // When the resolver resolves it
        let login = (resolver)(&token);

        // Then the RPC is authenticated
        assert_eq!(login.as_deref(), Some("u"));
    }

    #[test]
    fn the_resolver_rejects_a_refresh_kind_token() {
        // Given auth wired with a shared signing secret
        let (config, _dir) = a_config(Some("shared-secret"));
        let resolver = build_auth_entries(&config, "127.0.0.1", 0)
            .expect("auth should build")
            .user_resolver
            .expect("auth should produce a resolver");
        // and a *refresh*-kind token minted with that same secret
        let refresh = tddy_github::SessionTokenSigner::new(b"shared-secret")
            .mint_refresh(&a_github_user("u"));

        // When the resolver resolves it
        let login = (resolver)(&refresh);

        // Then it is rejected — the long-lived refresh token cannot authenticate an RPC
        assert_eq!(
            login, None,
            "a refresh-kind token must not authenticate an RPC"
        );
    }

    #[test]
    fn the_resolver_rejects_every_token_when_no_secret_is_configured() {
        // Given auth wired without a signing secret
        let (config, _dir) = a_config(None);
        let resolver = build_auth_entries(&config, "127.0.0.1", 0)
            .expect("auth should build")
            .user_resolver
            .expect("auth should produce a resolver");
        // and any signed token
        let token = tddy_github::SessionTokenSigner::new(b"some-secret")
            .mint(&a_github_user("u"), tddy_github::SESSION_TOKEN_TTL);

        // When the resolver resolves it
        let login = (resolver)(&token);

        // Then no token can be authenticated — there is no secret to verify against
        assert_eq!(login, None);
    }

    // -------------------------------------------------------------------------
    // `auth_storage` boot probe — retention is a hard login dependency, so an unusable path is a
    // startup failure rather than something the first operator to sign in discovers.
    // -------------------------------------------------------------------------

    #[test]
    fn retains_tokens_in_the_configured_auth_storage_once_it_probes_writable() {
        // Given `auth_storage` at a path the daemon may create
        let storage = tempfile::tempdir().unwrap();
        let (config, _dir) = a_config_storing_tokens_in(&storage.path().join("auth"));

        // When auth is wired
        let store = build_auth_entries(&config, "127.0.0.1", 0)
            .expect("a writable auth_storage should let the daemon start")
            .github_token_store
            .expect("a configured auth_storage should produce a token store");
        store.put("operator", "gho_granted").unwrap();

        // Then the credential a login grants is retained where the operator configured it
        assert_eq!(store.get("operator").as_deref(), Some("gho_granted"));
    }

    #[test]
    fn refuses_to_start_when_the_configured_auth_storage_cannot_be_written() {
        // Given `auth_storage` under an existing *file*, so no directory can be created there —
        // the shape of the misconfiguration `install` leaves when the daemon user cannot write
        // /var/lib/tddy
        let dir = tempfile::tempdir().unwrap();
        let occupied = dir.path().join("not-a-directory");
        std::fs::write(&occupied, "").unwrap();
        let (config, _config_dir) = a_config_storing_tokens_in(&occupied.join("auth"));

        // When auth is wired
        let failure = build_auth_entries(&config, "127.0.0.1", 0)
            .err()
            .map(|e| e.to_string());

        // Then startup fails naming the setting at fault, rather than the daemon serving an auth
        // surface on which every login would fail one operator at a time
        assert!(
            failure
                .as_deref()
                .is_some_and(|m| m.contains("config.auth_storage")),
            "expected a startup failure naming config.auth_storage, got: {failure:?}"
        );
    }
}
