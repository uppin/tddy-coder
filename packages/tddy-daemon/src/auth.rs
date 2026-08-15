//! Build AuthService from daemon config.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tddy_github::token_store::GitHubTokenStore;
use tddy_github::{
    AuthServiceImpl, GitHubOAuthProvider, RealGitHubProvider, SessionTokenSigner,
    StubGitHubProvider, TokenKind,
};
use tddy_livekit::TokenGenerator;
use tddy_rpc::{Request, Response, ServiceEntry, Status};
use tddy_service::proto::auth::{
    LiveKitTokenService as LiveKitTokenServiceTrait, MintLiveKitTokenRequest,
    MintLiveKitTokenResponse,
};
use tddy_service::{AuthServiceServer, LiveKitTokenServiceServer};

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

    // The room-JWT mint. It is an entry of its own rather than a method on `auth.AuthService`
    // because it needs the daemon's own config (LiveKit endpoint, common room, `users:` map),
    // which `AuthServiceImpl` — a tddy-github type that knows only about OAuth — does not have.
    let livekit_token_entry = ServiceEntry {
        name: "auth.LiveKitTokenService",
        service: Arc::new(LiveKitTokenServiceServer::new(
            LiveKitTokenServiceImpl::new(user_resolver.clone(), Arc::new(config.clone())),
        )) as Arc<dyn tddy_rpc::RpcService>,
    };

    Ok(AuthBuildResult {
        entries: vec![auth_entry, livekit_token_entry],
        user_resolver: Some(user_resolver),
        github_token_store,
    })
}

/// Build the `token.TokenService` entry the daemon serves, or `None` when it holds no LiveKit API
/// credentials to mint with.
///
/// Unlike [`LiveKitTokenServiceImpl`], this mint takes the room *and* the identity from the
/// request — the web UI joins presenter, session and lobby rooms through it, and there is no
/// room-ownership model on which to authorize one room over another (see `docs/dev/TODO.md`).
/// What it does enforce is that the caller is somebody: the same resolver that gates every other
/// daemon RPC, so a caller that could not call `ListSessions` cannot mint a room JWT either. The
/// service refuses `daemon-*` identities on its own, on every registration.
///
/// `user_resolver` is `None` when the daemon has no GitHub auth configured, and then *nothing* on
/// this daemon can authenticate a caller. The mint is still registered — so a client gets a
/// refusal it can read rather than an unimplemented method — but it admits nobody, and says so at
/// startup. Serving it open in that case would be the exact hole this closes.
pub fn build_token_service_entry(
    config: &DaemonConfig,
    user_resolver: Option<&SessionUserResolver>,
) -> Option<ServiceEntry> {
    let livekit = config.livekit.as_ref()?;
    let (api_key, api_secret) = (livekit.api_key.as_ref()?, livekit.api_secret.as_ref()?);

    let authenticate: tddy_service::SessionTokenAuthenticator = match user_resolver {
        Some(resolver) => {
            let resolver = resolver.clone();
            Arc::new(move |token: &str| resolver(token).is_some())
        }
        None => {
            log::warn!(
                target: "tddy_daemon::auth",
                "serving token.TokenService with no way to verify a session token — every mint \
                 will be refused. Configure `github:` to make it usable."
            );
            Arc::new(|_: &str| false)
        }
    };

    // Room and identity are per-request, so the generator is constructed with placeholders it
    // never mints under; `TokenProvider::generate_token` overrides both.
    let token_generator = Arc::new(TokenGenerator::new(
        api_key.clone(),
        api_secret.clone(),
        "daemon".to_string(),
        "token-provider".to_string(),
        Duration::from_secs(tddy_livekit::DEFAULT_LIVEKIT_JWT_TTL_SECS),
    ));
    let service = tddy_service::TokenServiceImpl::authenticated(
        crate::token_provider::LiveKitTokenProvider(token_generator),
        authenticate,
    );
    Some(ServiceEntry {
        name: "token.TokenService",
        service: Arc::new(tddy_service::TokenServiceServer::new(service))
            as Arc<dyn tddy_rpc::RpcService>,
    })
}

/// TTL of a minted LiveKit room JWT. One git operation, however large, is long done inside an
/// hour; a client that needs another asks for another. Short on purpose: this token is handed to
/// whoever presented a valid access token, and it is the only thing standing between them and the
/// room after their access token has expired.
pub const MINTED_ROOM_TOKEN_TTL: Duration = Duration::from_secs(3600);

/// Prefix of every server-generated participant identity. Deliberately not `daemon-`: that prefix
/// addresses a daemon's RPC-serving participant, and a client able to choose it could join the
/// common room *as* a daemon and be sent other participants' calls.
pub const MINTED_IDENTITY_PREFIX: &str = "remote-git-";

/// `auth.LiveKitTokenService`: mints a LiveKit room JWT for a caller that already holds a valid
/// daemon access token.
///
/// The LiveKit API secret never leaves the daemon. It is the same HMAC key
/// [`SessionTokenSigner`] uses, so a client holding it could sign an access token for any GitHub
/// user on the fleet — which would make every `session_token` check in the daemon decorative.
pub struct LiveKitTokenServiceImpl {
    user_resolver: SessionUserResolver,
    config: Arc<DaemonConfig>,
}

impl LiveKitTokenServiceImpl {
    pub fn new(user_resolver: SessionUserResolver, config: Arc<DaemonConfig>) -> Self {
        Self {
            user_resolver,
            config,
        }
    }
}

#[async_trait]
impl LiveKitTokenServiceTrait for LiveKitTokenServiceImpl {
    async fn mint_live_kit_token(
        &self,
        request: Request<MintLiveKitTokenRequest>,
    ) -> Result<Response<MintLiveKitTokenResponse>, Status> {
        let session_token = request.into_inner().session_token;

        // Same resolver as every other token-gated RPC: signature, expiry, and access-kind.
        let login = (self.user_resolver)(&session_token).ok_or_else(|| {
            Status::unauthenticated(
                "session token is missing, expired, or not an access token; no room token minted",
            )
        })?;
        let os_user = self.config.os_user_for_github(&login).ok_or_else(|| {
            Status::permission_denied(format!(
                "GitHub user \"{login}\" is not mapped to an OS user on this daemon"
            ))
        })?;

        let livekit = self.config.livekit.as_ref();
        let configured = |value: Option<&String>| {
            value
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        let (url, api_key, api_secret, room) = match (
            configured(livekit.and_then(|lk| lk.url.as_ref())),
            configured(livekit.and_then(|lk| lk.api_key.as_ref())),
            configured(livekit.and_then(|lk| lk.api_secret.as_ref())),
            configured(livekit.and_then(|lk| lk.common_room.as_ref())),
        ) {
            (Some(url), Some(key), Some(secret), Some(room)) => (url, key, secret, room),
            _ => {
                return Err(Status::failed_precondition(
                    "this daemon has no LiveKit url, api_key, api_secret and common_room \
                     configured, so it can mint no room token",
                ))
            }
        };

        // The identity is generated here, never taken from the request. It also carries no login:
        // a room roster is visible to every participant, and the identity's only job is to be
        // unique so two concurrent git commands do not evict one another.
        let identity = format!("{MINTED_IDENTITY_PREFIX}{}", uuid::Uuid::new_v4());
        let token = TokenGenerator::new(
            api_key,
            api_secret,
            room.clone(),
            identity,
            MINTED_ROOM_TOKEN_TTL,
        )
        .generate()
        .map_err(|e| Status::internal(format!("could not mint a livekit token: {e}")))?;

        log::info!(
            target: "tddy_daemon::auth",
            "minted a {}s livekit token for {login} (os user {os_user}) in room {room}",
            MINTED_ROOM_TOKEN_TTL.as_secs()
        );

        // `public_url` is the endpoint reachable from outside this host; the daemon's own `url` may
        // be a loopback or in-cluster address that no client could dial.
        let client_url = configured(livekit.and_then(|lk| lk.public_url.as_ref())).unwrap_or(url);

        Ok(Response::new(MintLiveKitTokenResponse {
            token,
            url: client_url,
            room,
            ttl_seconds: MINTED_ROOM_TOKEN_TTL.as_secs(),
        }))
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
    use tddy_service::proto::token::{GenerateTokenRequest, GenerateTokenResponse};

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

    // -------------------------------------------------------------------------
    // `auth.LiveKitTokenService` — the daemon mints the room JWT so no client ever holds
    // `livekit.api_secret`, which is also the key that signs every session token on the fleet.
    // -------------------------------------------------------------------------

    /// The one secret a deployment shares. Signs session tokens *and* LiveKit JWTs, which is
    /// exactly why a client must never be given it.
    const FLEET_SECRET: &str = "shared-secret";
    const COMMON_ROOM: &str = "tddy-lobby";

    /// A daemon with GitHub auth, one mapped operator, and `livekit_yaml` appended verbatim.
    fn a_daemon(livekit_yaml: &str) -> (DaemonConfig, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let yaml = format!(
            "users:\n  - github_user: \"operator\"\n    os_user: \"operator-os\"\n\
             github:\n  stub: true\n{livekit_yaml}"
        );
        let path = dir.path().join("config.yaml");
        std::fs::write(&path, yaml).unwrap();
        (DaemonConfig::load(&path).unwrap(), dir)
    }

    /// A daemon whose LiveKit is fully configured and whose common room is `tddy-lobby`.
    fn a_daemon_serving_a_common_room() -> (DaemonConfig, tempfile::TempDir) {
        a_daemon(&format!(
            "livekit:\n  url: \"ws://livekit.internal:7880\"\n  api_key: \"devkey\"\n  \
             api_secret: \"{FLEET_SECRET}\"\n  common_room: \"{COMMON_ROOM}\"\n"
        ))
    }

    /// The mint, wired to the same resolver every other token-gated RPC uses.
    fn a_mint(config: &DaemonConfig) -> LiveKitTokenServiceImpl {
        let user_resolver = build_auth_entries(config, "127.0.0.1", 0)
            .expect("auth should build")
            .user_resolver
            .expect("auth should produce a resolver");
        LiveKitTokenServiceImpl::new(user_resolver, Arc::new(config.clone()))
    }

    fn an_access_token_for(login: &str) -> String {
        SessionTokenSigner::new(FLEET_SECRET.as_bytes()).mint_access(&a_github_user(login))
    }

    async fn mint_with(
        service: &LiveKitTokenServiceImpl,
        session_token: &str,
    ) -> Result<MintLiveKitTokenResponse, Status> {
        service
            .mint_live_kit_token(Request::new(MintLiveKitTokenRequest {
                session_token: session_token.to_string(),
            }))
            .await
            .map(|response| response.into_inner())
    }

    /// The claims LiveKit itself will read out of a minted JWT. Asserting on the response fields
    /// alone would not prove what the *token* grants, which is the whole security property.
    fn jwt_claims(token: &str) -> serde_json::Value {
        use base64::Engine as _;
        let payload = token.split('.').nth(1).expect("a JWT has three parts");
        let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(payload)
            .expect("the JWT payload must be base64url");
        serde_json::from_slice(&decoded).expect("the JWT payload must be JSON")
    }

    #[tokio::test]
    async fn mints_a_room_token_for_a_caller_holding_a_valid_access_token() {
        // Given a daemon serving a common room, and an operator's access token
        let (config, _dir) = a_daemon_serving_a_common_room();

        // When
        let minted = mint_with(&a_mint(&config), &an_access_token_for("operator"))
            .await
            .expect("a mapped operator must be able to mint");

        // Then the client is told everything it needs to join, and holds no API secret
        assert_eq!(minted.room, COMMON_ROOM);
        assert_eq!(minted.url, "ws://livekit.internal:7880");
        assert_eq!(minted.ttl_seconds, 3600);
    }

    #[tokio::test]
    async fn grants_only_the_daemons_own_common_room_because_the_caller_names_no_room() {
        // Given a daemon whose common room is `tddy-lobby`
        let (config, _dir) = a_daemon_serving_a_common_room();

        // When
        let minted = mint_with(&a_mint(&config), &an_access_token_for("operator"))
            .await
            .expect("must mint");

        // Then the grant inside the JWT names that room — there is no request field a caller
        // could use to be admitted anywhere else
        assert_eq!(jwt_claims(&minted.token)["video"]["room"], COMMON_ROOM);
    }

    #[tokio::test]
    async fn mints_an_identity_a_caller_cannot_choose_so_no_client_can_join_as_a_daemon() {
        // Given a daemon serving a common room
        let (config, _dir) = a_daemon_serving_a_common_room();

        // When
        let minted = mint_with(&a_mint(&config), &an_access_token_for("operator"))
            .await
            .expect("must mint");

        // Then the identity the token carries is server-generated and prefixed. `daemon-<id>`
        // addresses a daemon's RPC-serving participant; a client admitted under that identity
        // would be sent other participants' calls.
        let identity = jwt_claims(&minted.token)["sub"]
            .as_str()
            .expect("a livekit JWT carries its identity in `sub`")
            .to_string();
        assert!(
            identity.starts_with(MINTED_IDENTITY_PREFIX),
            "expected a {MINTED_IDENTITY_PREFIX}* identity, got: {identity}"
        );
    }

    #[tokio::test]
    async fn mints_a_distinct_identity_per_call_so_concurrent_git_commands_do_not_evict_each_other()
    {
        // Given two callers presenting the same operator's token
        let (config, _dir) = a_daemon_serving_a_common_room();
        let mint = a_mint(&config);
        let token = an_access_token_for("operator");

        // When
        let first = mint_with(&mint, &token).await.expect("must mint");
        let second = mint_with(&mint, &token).await.expect("must mint");

        // Then
        assert_ne!(
            jwt_claims(&first.token)["sub"],
            jwt_claims(&second.token)["sub"]
        );
    }

    #[tokio::test]
    async fn hands_back_the_public_livekit_url_when_one_is_configured() {
        // Given a daemon whose own LiveKit address is in-cluster and unreachable from a client
        let (config, _dir) = a_daemon(&format!(
            "livekit:\n  url: \"ws://127.0.0.1:7880\"\n  public_url: \"wss://livekit.example:443\"\n  \
             api_key: \"devkey\"\n  api_secret: \"{FLEET_SECRET}\"\n  common_room: \"{COMMON_ROOM}\"\n"
        ));

        // When
        let minted = mint_with(&a_mint(&config), &an_access_token_for("operator"))
            .await
            .expect("must mint");

        // Then
        assert_eq!(minted.url, "wss://livekit.example:443");
    }

    #[tokio::test]
    async fn refuses_a_session_token_signed_with_a_foreign_secret() {
        // Given a token minted by something that does not hold this fleet's secret
        let (config, _dir) = a_daemon_serving_a_common_room();
        let forged =
            SessionTokenSigner::new(b"some-other-secret").mint_access(&a_github_user("operator"));

        // When
        let refusal = mint_with(&a_mint(&config), &forged)
            .await
            .expect_err("a forged token must mint nothing");

        // Then
        assert_eq!(refusal.code, tddy_rpc::Code::Unauthenticated);
    }

    #[tokio::test]
    async fn refuses_a_refresh_kind_token_because_it_is_not_an_rpc_credential() {
        // Given the 7-day refresh token, presented where an access token belongs
        let (config, _dir) = a_daemon_serving_a_common_room();
        let refresh = SessionTokenSigner::new(FLEET_SECRET.as_bytes())
            .mint_refresh(&a_github_user("operator"));

        // When
        let refusal = mint_with(&a_mint(&config), &refresh)
            .await
            .expect_err("a refresh token must mint nothing");

        // Then
        assert_eq!(refusal.code, tddy_rpc::Code::Unauthenticated);
    }

    #[tokio::test]
    async fn refuses_a_login_this_daemon_maps_to_no_os_user() {
        // Given a validly signed token for a GitHub login absent from `users:`
        let (config, _dir) = a_daemon_serving_a_common_room();

        // When
        let refusal = mint_with(&a_mint(&config), &an_access_token_for("a-stranger"))
            .await
            .expect_err("an unmapped login must mint nothing");

        // Then it is refused by name — the login authenticated, it is just not served here
        assert_eq!(refusal.code, tddy_rpc::Code::PermissionDenied);
        assert!(
            refusal.message.contains("a-stranger"),
            "the refusal must name the login, got: {}",
            refusal.message
        );
    }

    #[tokio::test]
    async fn refuses_to_mint_when_the_daemon_has_no_common_room_configured() {
        // Given a daemon with a signing secret but no LiveKit endpoint or room
        let (config, _dir) = a_daemon(&format!("livekit:\n  api_secret: \"{FLEET_SECRET}\"\n"));

        // When
        let refusal = mint_with(&a_mint(&config), &an_access_token_for("operator"))
            .await
            .expect_err("an unconfigured daemon must mint nothing");

        // Then the misconfiguration is reported rather than a token issued for some default room
        assert_eq!(refusal.code, tddy_rpc::Code::FailedPrecondition);
    }

    // -------------------------------------------------------------------------
    // `token.TokenService` — the room/identity-choosing mint the web UI uses. On the daemon it is
    // reachable by anything that can reach `/rpc`, so it carries the same session-token gate as
    // every other daemon RPC.
    // -------------------------------------------------------------------------

    /// The daemon's own registration of the mint, wired to the same resolver every other
    /// token-gated RPC uses.
    fn a_registered_mint(config: &DaemonConfig) -> ServiceEntry {
        let user_resolver = build_auth_entries(config, "127.0.0.1", 0)
            .expect("auth should build")
            .user_resolver;
        build_token_service_entry(config, user_resolver.as_ref())
            .expect("a daemon with livekit credentials should register the mint")
    }

    /// A request the web UI would send: a room, a `web-*` identity, and a credential.
    fn a_generate_request(session_token: &str) -> GenerateTokenRequest {
        GenerateTokenRequest {
            room: COMMON_ROOM.to_string(),
            identity: "web-operator".to_string(),
            session_token: session_token.to_string(),
        }
    }

    /// Drive the registered entry the way Connect-HTTP and the common room both do — through the
    /// bridge, so the test proves what is actually *served*, not what a hand-built impl would do.
    async fn generate_through(
        entry: ServiceEntry,
        request: GenerateTokenRequest,
    ) -> Result<GenerateTokenResponse, Status> {
        use prost::Message as _;
        let bridge = tddy_rpc::RpcBridge::new(tddy_rpc::MultiRpcService::new(vec![entry]));
        let message = tddy_rpc::RpcMessage {
            payload: request.encode_to_vec(),
            metadata: tddy_rpc::RequestMetadata::default(),
        };
        let body = bridge
            .handle_messages("token.TokenService", "GenerateToken", &[message])
            .await?;
        match body {
            tddy_rpc::ResponseBody::Complete(chunks) => Ok(GenerateTokenResponse::decode(
                &chunks[0][..],
            )
            .expect("a unary response decodes")),
            _ => panic!("GenerateToken is unary"),
        }
    }

    #[tokio::test]
    async fn mints_a_room_token_for_a_caller_holding_a_valid_access_token_on_the_daemons_mint() {
        // Given a daemon serving a common room, and an operator's access token
        let (config, _dir) = a_daemon_serving_a_common_room();

        // When
        let minted = generate_through(
            a_registered_mint(&config),
            a_generate_request(&an_access_token_for("operator")),
        )
        .await
        .expect("an authenticated caller must be able to mint");

        // Then the JWT admits it to the room and identity it asked for
        assert_eq!(jwt_claims(&minted.token)["video"]["room"], COMMON_ROOM);
        assert_eq!(jwt_claims(&minted.token)["sub"], "web-operator");
    }

    #[tokio::test]
    async fn refuses_an_anonymous_caller_of_the_daemons_mint() {
        // Given a request carrying no credential — what every caller sent before this gate existed
        let (config, _dir) = a_daemon_serving_a_common_room();

        // When
        let refusal = generate_through(a_registered_mint(&config), a_generate_request(""))
            .await
            .expect_err("reaching /rpc must not be enough to mint a livekit token");

        // Then
        assert_eq!(refusal.code, tddy_rpc::Code::Unauthenticated);
    }

    #[tokio::test]
    async fn refuses_a_daemons_mint_caller_whose_token_was_signed_with_a_foreign_secret() {
        // Given a token minted by something that does not hold this fleet's secret
        let (config, _dir) = a_daemon_serving_a_common_room();
        let forged =
            SessionTokenSigner::new(b"some-other-secret").mint_access(&a_github_user("operator"));

        // When
        let refusal = generate_through(a_registered_mint(&config), a_generate_request(&forged))
            .await
            .expect_err("a forged token must mint nothing");

        // Then
        assert_eq!(refusal.code, tddy_rpc::Code::Unauthenticated);
    }

    #[tokio::test]
    async fn refuses_a_refresh_kind_token_on_the_daemons_mint() {
        // Given the 7-day refresh token, presented where an access token belongs
        let (config, _dir) = a_daemon_serving_a_common_room();
        let refresh = SessionTokenSigner::new(FLEET_SECRET.as_bytes())
            .mint_refresh(&a_github_user("operator"));

        // When
        let refusal = generate_through(a_registered_mint(&config), a_generate_request(&refresh))
            .await
            .expect_err("a refresh token must mint nothing");

        // Then the mint uses the very resolver that gates every other daemon RPC, so the two
        // cannot drift apart
        assert_eq!(refusal.code, tddy_rpc::Code::Unauthenticated);
    }

    #[tokio::test]
    async fn refuses_to_mint_a_daemon_rpc_identity_for_an_authenticated_operator() {
        // Given an operator who authenticates, asking for the identity this very daemon serves
        // its RPC on
        let (config, _dir) = a_daemon_serving_a_common_room();
        let request = GenerateTokenRequest {
            identity: crate::livekit_peer_discovery::daemon_rpc_identity("udoo"),
            ..a_generate_request(&an_access_token_for("operator"))
        };

        // When
        let refusal = generate_through(a_registered_mint(&config), request)
            .await
            .expect_err("no caller may be admitted under a daemon's RPC identity");

        // Then — a participant under that identity is handed other participants' calls
        assert_eq!(refusal.code, tddy_rpc::Code::PermissionDenied);
    }

    #[tokio::test]
    async fn refuses_every_caller_of_the_mint_when_the_daemon_can_verify_no_session_token() {
        // Given a daemon with LiveKit credentials but no GitHub auth, so nothing on it can
        // authenticate a caller
        let (config, _dir) = a_daemon(&format!(
            "livekit:\n  url: \"ws://livekit.internal:7880\"\n  api_key: \"devkey\"\n  \
             api_secret: \"{FLEET_SECRET}\"\n  common_room: \"{COMMON_ROOM}\"\n"
        ));
        let entry = build_token_service_entry(&config, None)
            .expect("livekit credentials alone should still register the mint");

        // When a caller presents a token that would verify on an authenticated daemon
        let refusal = generate_through(entry, a_generate_request(&an_access_token_for("operator")))
            .await
            .expect_err("a daemon that can verify nothing must admit nobody");

        // Then it is closed rather than open — an unverifiable deployment mints nothing
        assert_eq!(refusal.code, tddy_rpc::Code::Unauthenticated);
    }

    #[test]
    fn registers_no_mint_when_the_daemon_holds_no_livekit_api_credentials() {
        // Given a daemon with a signing secret but no LiveKit api_key
        let (config, _dir) = a_daemon(&format!("livekit:\n  api_secret: \"{FLEET_SECRET}\"\n"));

        // When
        let entry = build_token_service_entry(&config, None);

        // Then there is nothing to mint with, so nothing is served
        assert!(
            entry.is_none(),
            "a daemon with no api_key must serve no mint"
        );
    }

    #[test]
    fn addresses_a_daemons_rpc_participant_with_the_prefix_the_mint_refuses() {
        // Given the identity a daemon actually serves RPC on
        let identity = crate::livekit_peer_discovery::daemon_rpc_identity("udoo");

        // When compared against the prefix `token.TokenService` refuses to mint

        // Then they are the same convention — if this drifts, the mint starts refusing the wrong
        // names and admitting the right ones
        assert!(
            identity.starts_with(tddy_service::RESERVED_DAEMON_IDENTITY_PREFIX),
            "expected {identity} to carry {}",
            tddy_service::RESERVED_DAEMON_IDENTITY_PREFIX
        );
    }

    #[test]
    fn serves_the_mint_alongside_the_oauth_surface_so_both_transports_carry_it() {
        // Given auth wired for a daemon serving a common room
        let (config, _dir) = a_daemon_serving_a_common_room();

        // When
        let entries = build_auth_entries(&config, "127.0.0.1", 0)
            .expect("auth should build")
            .entries;

        // Then both services are registered — `main.rs` seeds `rpc_entries` from these, and that
        // one list feeds Connect-HTTP, the local socket and the common room alike
        let names: Vec<&str> = entries.iter().map(|e| e.name).collect();
        assert_eq!(names, vec!["auth.AuthService", "auth.LiveKitTokenService"]);
    }
}
