//! Build AuthService from daemon config.

use std::sync::Arc;

use tddy_github::{
    AuthServiceImpl, GitHubOAuthProvider, RealGitHubProvider, SessionTokenSigner,
    StubGitHubProvider, TokenKind,
};
use tddy_rpc::ServiceEntry;
use tddy_service::AuthServiceServer;

use crate::config::DaemonConfig;
use crate::connection_service::SessionUserResolver;

/// Result of building auth: RPC entries and a resolver for session token -> GitHub login.
pub struct AuthBuildResult {
    pub entries: Vec<ServiceEntry>,
    pub user_resolver: Option<SessionUserResolver>,
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
pub fn build_auth_entries(config: &DaemonConfig, web_host: &str, web_port: u16) -> AuthBuildResult {
    let github = match &config.github {
        Some(g) => g,
        None => {
            return AuthBuildResult {
                entries: vec![],
                user_resolver: None,
            };
        }
    };

    // The one secret every daemon in a deployment shares (it also signs LiveKit room JWTs).
    let signing_secret = config.livekit.as_ref().and_then(|lk| lk.api_secret.clone());
    let signer = signing_secret
        .as_deref()
        .map(|s| SessionTokenSigner::new(s.as_bytes()));

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
        auth_service_entry(stub, signer.clone())
    } else if let (Some(id), Some(secret)) = (&github.client_id, &github.client_secret) {
        let redirect_uri = github
            .redirect_uri
            .clone()
            .unwrap_or_else(|| format!("http://{}:{}/auth/callback", web_host, web_port));
        let real = RealGitHubProvider::new(id, secret, &redirect_uri);
        auth_service_entry(real, signer.clone())
    } else {
        return AuthBuildResult {
            entries: vec![],
            user_resolver: None,
        };
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

    AuthBuildResult {
        entries: vec![auth_entry],
        user_resolver: Some(user_resolver),
    }
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
fn auth_service_entry<P: GitHubOAuthProvider>(
    provider: P,
    signer: Option<SessionTokenSigner>,
) -> ServiceEntry {
    let server = match signer {
        Some(signer) => AuthServiceServer::new(AuthServiceImpl::new_signed(provider, signer)),
        None => AuthServiceServer::new(AuthServiceImpl::new(provider)),
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
}
