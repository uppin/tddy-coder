use std::sync::Arc;

use async_trait::async_trait;

use tddy_rpc::{Request, Response, Status};

use crate::provider::{DeviceLoginPoll, GitHubOAuthProvider, GitHubUser};
use crate::session_token_v2::{
    SessionClaims, SessionTokenAuthority, SessionTokenSigner, TokenKind,
};

use tddy_service::proto::auth::{
    AuthService as AuthServiceTrait, DeviceLoginState, ExchangeCodeRequest, ExchangeCodeResponse,
    GetAuthStatusRequest, GetAuthStatusResponse, GetAuthUrlRequest, GetAuthUrlResponse,
    GitHubUser as ProtoGitHubUser, LogoutRequest, LogoutResponse, PollDeviceLoginRequest,
    PollDeviceLoginResponse, RefreshSessionRequest, RefreshSessionResponse,
    StartDeviceLoginRequest, StartDeviceLoginResponse,
};

fn to_proto_user(user: &GitHubUser) -> ProtoGitHubUser {
    ProtoGitHubUser {
        id: user.id,
        login: user.login.clone(),
        avatar_url: user.avatar_url.clone(),
        name: user.name.clone(),
    }
}

fn proto_user_from_claims(claims: &SessionClaims) -> ProtoGitHubUser {
    ProtoGitHubUser {
        id: claims.id,
        login: claims.login.clone(),
        avatar_url: claims.avatar_url.clone(),
        name: claims.name.clone(),
    }
}

/// Auth service implementation. Delegates OAuth to a GitHubOAuthProvider and issues stateless
/// session tokens signed with this daemon's own key (see [`crate::session_token_v2`]). No session
/// state is kept server-side: a token is verifiable by any daemon that can resolve the key it names.
pub struct AuthServiceImpl<P: GitHubOAuthProvider> {
    provider: Arc<P>,
    /// When set, sign-in mints tokens and status/refresh verify them. When `None`, authentication
    /// is non-functional: minting fails and every token is rejected.
    signing: Option<Signing>,
    /// When set, a real provider's GitHub access token is retained here on login so the server can
    /// later act on that operator's behalf (e.g. read their PRs). Separate from `signing` on purpose:
    /// the GitHub token never enters the session token and is never returned to the client.
    token_store: Option<Arc<dyn crate::token_store::GitHubTokenStore>>,
}

/// What a signed service mints with and verifies through.
struct Signing {
    /// This daemon's own key: every token the service hands out is signed with it.
    signer: SessionTokenSigner,
    /// Decides whether a presented token is genuine, whichever daemon signed it — so a status
    /// check or a refresh accepts a peer's token exactly as far as the fleet's key directory does.
    authority: Arc<dyn SessionTokenAuthority>,
}

impl<P: GitHubOAuthProvider> AuthServiceImpl<P> {
    /// Create without a signer. Authentication is non-functional — minting fails and every token
    /// is rejected. A process that can hand out an authorize URL but holds no identity of its own
    /// (a session coder's web surface) serves this.
    pub fn new(provider: P) -> Self {
        Self {
            provider: Arc::new(provider),
            signing: None,
            token_store: None,
        }
    }

    /// Create a service that mints with `signer` and verifies through `authority`.
    ///
    /// Tokens are self-describing — no shared or persisted session store — and `authority` is
    /// what lets a token minted by one daemon be recognised by another.
    pub fn new_signed(
        provider: P,
        signer: SessionTokenSigner,
        authority: Arc<dyn SessionTokenAuthority>,
    ) -> Self {
        Self {
            provider: Arc::new(provider),
            signing: Some(Signing { signer, authority }),
            token_store: None,
        }
    }

    /// Retain each real login's GitHub access token in `store` (builder). Without a store the token
    /// is dropped at the end of the exchange, and GitHub-backed reads report themselves unavailable.
    pub fn with_token_store(
        mut self,
        store: Arc<dyn crate::token_store::GitHubTokenStore>,
    ) -> Self {
        self.token_store = Some(store);
        self
    }
}

/// What a completed sign-in hands the client, whichever flow completed it.
struct MintedSession {
    session_token: String,
    user: ProtoGitHubUser,
    refresh_token: String,
}

impl<P: GitHubOAuthProvider> AuthServiceImpl<P> {
    /// Finish a sign-in GitHub has vouched for: retain its access token, then mint the session.
    ///
    /// One implementation for both flows, so a device login and a redirect login cannot drift
    /// into producing different sessions or retaining the credential under different rules.
    fn complete_login(
        &self,
        access_token: &str,
        user: &GitHubUser,
    ) -> Result<MintedSession, Status> {
        // Retain the operator's own credential for later server-side GitHub reads. A stub provider's
        // token is synthetic and is never stored (D12), so a demo login holds none by construction.
        //
        // Retention failure fails the login. A session minted without its token is a half-login: the
        // operator appears signed in while every GitHub-backed read silently reports itself
        // unavailable, and re-authenticating is the one action that could fix it — which they have no
        // reason to attempt, because they are already signed in. Failing here surfaces the real fault
        // (an unwritable token store) at the moment it is caused and keeps retry the obvious remedy.
        if self.provider.issues_usable_access_token() {
            if let Some(ref store) = self.token_store {
                store.put(&user.login, access_token).map_err(|e| {
                    // The store's own error names server-side detail — the filesystem path it could
                    // not write, the OS error behind it. That belongs in the daemon log, not in a
                    // status handed to a browser, so the client is told only *what* failed and for
                    // which login.
                    log::error!(
                        target: "tddy_github::auth_service",
                        "could not retain the GitHub access token for login '{}': {e}",
                        user.login
                    );
                    Status::internal(format!(
                        "could not retain the GitHub access token for login '{}'",
                        user.login
                    ))
                })?;
            }
        }

        // Signed mode: return a stateless token any daemon that can resolve this daemon's key
        // verifies. No server-side session state is kept.
        let Some(Signing { ref signer, .. }) = self.signing else {
            return Err(Status::failed_precondition(
                "session token signing is not configured",
            ));
        };
        // A short-lived access token for RPCs plus a long-lived refresh token to mint further
        // access tokens without re-login.
        Ok(MintedSession {
            session_token: signer.mint_access(user),
            user: to_proto_user(user),
            refresh_token: signer.mint_refresh(user),
        })
    }
}

#[async_trait]
impl<P: GitHubOAuthProvider> AuthServiceTrait for AuthServiceImpl<P> {
    async fn get_auth_url(
        &self,
        _request: Request<GetAuthUrlRequest>,
    ) -> Result<Response<GetAuthUrlResponse>, Status> {
        let (authorize_url, state) = self.provider.authorize_url();
        Ok(Response::new(GetAuthUrlResponse {
            authorize_url,
            state,
        }))
    }

    async fn exchange_code(
        &self,
        request: Request<ExchangeCodeRequest>,
    ) -> Result<Response<ExchangeCodeResponse>, Status> {
        let req = request.into_inner();
        let (access_token, user) = self
            .provider
            .exchange_code(&req.code, &req.state)
            .await
            .map_err(Status::internal)?;

        let session = self.complete_login(&access_token, &user)?;
        Ok(Response::new(ExchangeCodeResponse {
            session_token: session.session_token,
            user: Some(session.user),
            refresh_token: session.refresh_token,
        }))
    }

    async fn get_auth_status(
        &self,
        request: Request<GetAuthStatusRequest>,
    ) -> Result<Response<GetAuthStatusResponse>, Status> {
        let req = request.into_inner();
        // Only an access-kind token authenticates a session — a refresh token is a minting
        // credential, never proof of an authenticated session (matches the daemon RPC resolver).
        let claims = match &self.signing {
            Some(signing) => signing.authority.verify(&req.session_token).await.ok(),
            None => None,
        }
        .filter(|claims| claims.kind == TokenKind::Access);
        Ok(Response::new(match claims {
            Some(claims) => GetAuthStatusResponse {
                authenticated: true,
                user: Some(proto_user_from_claims(&claims)),
            },
            None => GetAuthStatusResponse {
                authenticated: false,
                user: None,
            },
        }))
    }

    async fn refresh_session(
        &self,
        request: Request<RefreshSessionRequest>,
    ) -> Result<Response<RefreshSessionResponse>, Status> {
        let req = request.into_inner();
        let Some(Signing {
            ref signer,
            ref authority,
        }) = self.signing
        else {
            return Err(Status::failed_precondition(
                "session token signing is not configured",
            ));
        };
        // Only a currently-valid refresh token can extend a session; an expired/forged one forces
        // re-login.
        let claims = authority
            .verify(&req.refresh_token)
            .await
            .map_err(|e| Status::unauthenticated(e.to_string()))?;
        // A short-lived access token must not be usable to mint — only a refresh token can.
        if claims.kind != TokenKind::Refresh {
            return Err(Status::unauthenticated(
                "session token: not a refresh token",
            ));
        }
        let user = claims.user();
        // Mint a fresh access token plus a slid refresh token (fresh 7-day window), signed with
        // this daemon's own key whichever daemon signed the refresh token it was given.
        let session_token = signer.mint_access(&user);
        let refresh_token = signer.mint_refresh(&user);
        Ok(Response::new(RefreshSessionResponse {
            session_token,
            user: Some(to_proto_user(&user)),
            refresh_token,
        }))
    }

    async fn logout(
        &self,
        request: Request<LogoutRequest>,
    ) -> Result<Response<LogoutResponse>, Status> {
        // Signed session tokens are stateless — logout is client-side (the client discards its
        // token). There is nothing to invalidate server-side.
        let _ = request.into_inner();
        Ok(Response::new(LogoutResponse {}))
    }

    async fn start_device_login(
        &self,
        _request: Request<StartDeviceLoginRequest>,
    ) -> Result<Response<StartDeviceLoginResponse>, Status> {
        // Nothing is remembered here — the device code is the client's to hold and present again.
        let started = self
            .provider
            .start_device_login()
            .await
            .map_err(Status::internal)?;
        Ok(Response::new(StartDeviceLoginResponse {
            device_code: started.device_code,
            user_code: started.user_code,
            verification_uri: started.verification_uri,
            expires_in_seconds: started.expires_in_seconds,
            interval_seconds: started.interval_seconds,
        }))
    }

    async fn poll_device_login(
        &self,
        request: Request<PollDeviceLoginRequest>,
    ) -> Result<Response<PollDeviceLoginResponse>, Status> {
        let req = request.into_inner();
        let polled = self
            .provider
            .poll_device_login(&req.device_code)
            .await
            .map_err(Status::internal)?;
        let not_yet = |state: DeviceLoginState| PollDeviceLoginResponse {
            state: state as i32,
            ..Default::default()
        };
        Ok(Response::new(match polled {
            DeviceLoginPoll::Pending => not_yet(DeviceLoginState::Pending),
            DeviceLoginPoll::SlowDown { interval_seconds } => PollDeviceLoginResponse {
                interval_seconds,
                ..not_yet(DeviceLoginState::SlowDown)
            },
            DeviceLoginPoll::Denied => not_yet(DeviceLoginState::Denied),
            DeviceLoginPoll::Expired => not_yet(DeviceLoginState::Expired),
            DeviceLoginPoll::Complete { access_token, user } => {
                let session = self.complete_login(&access_token, &user)?;
                PollDeviceLoginResponse {
                    state: DeviceLoginState::Complete as i32,
                    interval_seconds: 0,
                    session_token: session.session_token,
                    user: Some(session.user),
                    refresh_token: session.refresh_token,
                }
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stub::StubGitHubProvider;
    use tddy_rpc::RpcBridge;
    use tddy_service::proto::auth::AuthServiceServer;

    fn setup() -> (StubGitHubProvider, GitHubUser) {
        let stub = StubGitHubProvider::new("https://github.com", "test-client-id");
        let user = GitHubUser {
            id: 123,
            login: "testuser".to_string(),
            avatar_url: "https://example.com/avatar.png".to_string(),
            name: "Test User".to_string(),
        };
        (stub, user)
    }

    #[tokio::test]
    async fn get_auth_status_with_invalid_session() {
        // Given an auth service with no signing key configured
        let (stub, _) = setup();
        let service = AuthServiceImpl::new(stub);
        let server = AuthServiceServer::new(service);
        let bridge = RpcBridge::new(server);

        // When checking status for an unverifiable session token
        let req = GetAuthStatusRequest {
            session_token: "nonexistent-token".to_string(),
        };
        let msg = tddy_rpc::RpcMessage {
            payload: prost::Message::encode_to_vec(&req),
            metadata: Default::default(),
        };
        let resp = bridge
            .handle_messages("auth.AuthService", "GetAuthStatus", &[msg])
            .await
            .expect("should succeed");
        let chunks = match resp {
            tddy_rpc::ResponseBody::Complete(c) => c,
            _ => panic!("expected Complete"),
        };

        // Then the response indicates not authenticated
        let status_resp =
            <GetAuthStatusResponse as prost::Message>::decode(&chunks[0][..]).unwrap();
        assert!(!status_resp.authenticated);
    }

    // -------------------------------------------------------------------------
    // Shared RPC step helpers
    // -------------------------------------------------------------------------

    /// Exchange a code and return the session token (shared test step).
    async fn do_exchange(
        bridge: &RpcBridge<AuthServiceServer<AuthServiceImpl<StubGitHubProvider>>>,
        code: &str,
        state: &str,
    ) -> String {
        let req = ExchangeCodeRequest {
            code: code.to_string(),
            state: state.to_string(),
        };
        let msg = tddy_rpc::RpcMessage {
            payload: prost::Message::encode_to_vec(&req),
            metadata: Default::default(),
        };
        let resp = bridge
            .handle_messages("auth.AuthService", "ExchangeCode", &[msg])
            .await
            .expect("ExchangeCode should succeed");
        let chunks = match resp {
            tddy_rpc::ResponseBody::Complete(c) => c,
            _ => panic!("expected Complete"),
        };
        <ExchangeCodeResponse as prost::Message>::decode(&chunks[0][..])
            .unwrap()
            .session_token
    }

    /// Check auth status and return (authenticated, login).
    async fn do_get_status(
        bridge: &RpcBridge<AuthServiceServer<AuthServiceImpl<StubGitHubProvider>>>,
        token: &str,
    ) -> (bool, Option<String>) {
        let req = GetAuthStatusRequest {
            session_token: token.to_string(),
        };
        let msg = tddy_rpc::RpcMessage {
            payload: prost::Message::encode_to_vec(&req),
            metadata: Default::default(),
        };
        let resp = bridge
            .handle_messages("auth.AuthService", "GetAuthStatus", &[msg])
            .await
            .expect("GetAuthStatus should succeed");
        let chunks = match resp {
            tddy_rpc::ResponseBody::Complete(c) => c,
            _ => panic!("expected Complete"),
        };
        let r = <GetAuthStatusResponse as prost::Message>::decode(&chunks[0][..]).unwrap();
        (r.authenticated, r.user.map(|u| u.login))
    }

    // -------------------------------------------------------------------------
    // Cross-daemon session tokens — a token minted by one daemon is verifiable by another that has
    // learned the first one's public key, with no shared secret and no shared session store.
    // -------------------------------------------------------------------------

    /// A daemon's signing key. Fixed bytes, so a failure names a behaviour rather than a seed.
    fn a_daemon_key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn a_signer_for(key: &SigningKey) -> SessionTokenSigner {
        SessionTokenSigner::new(key.clone(), KeyId::of(&key.verifying_key()))
    }

    /// A signed service minting with `key` and admitting the tokens of every key in `trusted`.
    fn a_signed_service(
        provider: StubGitHubProvider,
        key: &SigningKey,
        trusted: &[&SigningKey],
    ) -> AuthServiceImpl<StubGitHubProvider> {
        AuthServiceImpl::new_signed(
            provider,
            a_signer_for(key),
            Arc::new(TrustsKeys(
                trusted.iter().map(|key| key.verifying_key()).collect(),
            )),
        )
    }

    fn signed_bridge(
        code: &str,
        key: &SigningKey,
        trusted: &[&SigningKey],
    ) -> RpcBridge<AuthServiceServer<AuthServiceImpl<StubGitHubProvider>>> {
        let (stub, user) = setup();
        stub.register_code(code, user);
        RpcBridge::new(AuthServiceServer::new(a_signed_service(stub, key, trusted)))
    }

    /// A daemon's view of the fleet, as a fixed set of public keys it has learned.
    struct TrustsKeys(Vec<VerifyingKey>);

    #[async_trait]
    impl SessionTokenAuthority for TrustsKeys {
        async fn verify(&self, token: &str) -> Result<SessionClaims, SessionTokenError> {
            let key_id = SessionTokenVerifier::key_id_of(token)?;
            let key = self
                .0
                .iter()
                .find(|key| KeyId::of(key) == key_id)
                .ok_or(SessionTokenError::UnknownKeyId(key_id))?;
            SessionTokenVerifier::verify(token, key, SystemTime::now())
        }
    }

    /// The claims `token` carries, checked under `key`.
    fn claims_under(key: &SigningKey, token: &str) -> SessionClaims {
        SessionTokenVerifier::verify(token, &key.verifying_key(), SystemTime::now())
            .expect("a token this daemon minted verifies under its key")
    }

    async fn do_get_auth_url_state(
        bridge: &RpcBridge<AuthServiceServer<AuthServiceImpl<StubGitHubProvider>>>,
    ) -> String {
        let msg = tddy_rpc::RpcMessage {
            payload: prost::Message::encode_to_vec(&GetAuthUrlRequest {}),
            metadata: Default::default(),
        };
        let resp = bridge
            .handle_messages("auth.AuthService", "GetAuthUrl", &[msg])
            .await
            .expect("GetAuthUrl should succeed");
        let chunks = match resp {
            tddy_rpc::ResponseBody::Complete(c) => c,
            _ => panic!("expected Complete"),
        };
        <GetAuthUrlResponse as prost::Message>::decode(&chunks[0][..])
            .unwrap()
            .state
    }

    #[tokio::test]
    async fn a_token_minted_by_one_daemon_is_authenticated_by_another_that_has_learned_its_key() {
        // Given one daemon that mints a session token through the GitHub login flow
        let (daemon_a, daemon_b) = (a_daemon_key(7), a_daemon_key(9));
        let serving = signed_bridge("login-code", &daemon_a, &[&daemon_a]);
        let state = do_get_auth_url_state(&serving).await;
        let token = do_exchange(&serving, "login-code", &state).await;

        // When a *different* daemon — its own key, its own service, no shared session store and no
        // shared secret — checks that token, having learned the first daemon's public key
        let (peer_stub, _) = setup();
        let peer = RpcBridge::new(AuthServiceServer::new(a_signed_service(
            peer_stub,
            &daemon_b,
            &[&daemon_b, &daemon_a],
        )));
        let status = do_get_status(&peer, &token).await;

        // Then the peer authenticates it from the signature and the signer's public key alone
        assert_eq!(
            status,
            (true, Some("testuser".to_string())),
            "a peer daemon must accept a token minted by a daemon whose key it has learned"
        );
    }

    #[tokio::test]
    async fn a_token_minted_by_one_daemon_is_refused_by_another_that_has_not_learned_its_key() {
        // Given one daemon that mints a session token through the GitHub login flow
        let (daemon_a, daemon_b) = (a_daemon_key(7), a_daemon_key(9));
        let serving = signed_bridge("login-code", &daemon_a, &[&daemon_a]);
        let state = do_get_auth_url_state(&serving).await;
        let token = do_exchange(&serving, "login-code", &state).await;

        // When a daemon that knows only its own key checks it
        let (peer_stub, _) = setup();
        let peer = RpcBridge::new(AuthServiceServer::new(a_signed_service(
            peer_stub,
            &daemon_b,
            &[&daemon_b],
        )));
        let status = do_get_status(&peer, &token).await;

        // Then it is not authenticated — there is no shared secret left to fall back on
        assert_eq!(status, (false, None));
    }

    // -------------------------------------------------------------------------
    // Signed-token minting, refresh, and the "no signer configured" guard.
    // -------------------------------------------------------------------------

    use crate::session_token_v2::{
        KeyId, SessionTokenError, SessionTokenVerifier, REFRESH_TOKEN_TTL,
    };
    use ed25519_dalek::{SigningKey, VerifyingKey};
    use std::time::{Duration, SystemTime};

    #[tokio::test]
    async fn exchange_code_returns_a_signed_token_rather_than_an_opaque_uuid() {
        // Given a signed auth service
        let daemon = a_daemon_key(7);
        let bridge = signed_bridge("login-code", &daemon, &[&daemon]);
        let state = do_get_auth_url_state(&bridge).await;

        // When a code is exchanged
        let token = do_exchange(&bridge, "login-code", &state).await;

        // Then the returned token is a signed, self-describing token, not a bare UUID
        assert!(
            token.starts_with("v2."),
            "expected a signed token, got '{token}'"
        );
    }

    #[tokio::test]
    async fn exchange_code_fails_when_no_signer_is_configured() {
        // Given an auth service with no signing key
        let (stub, user) = setup();
        stub.register_code("login-code", user);
        let bridge = RpcBridge::new(AuthServiceServer::new(AuthServiceImpl::new(stub)));
        let state = do_get_auth_url_state(&bridge).await;

        // When a code is exchanged
        let exchange_req = ExchangeCodeRequest {
            code: "login-code".to_string(),
            state,
        };
        let msg = tddy_rpc::RpcMessage {
            payload: prost::Message::encode_to_vec(&exchange_req),
            metadata: Default::default(),
        };
        let result = bridge
            .handle_messages("auth.AuthService", "ExchangeCode", &[msg])
            .await;

        // Then it is rejected — there is no key to mint a verifiable token with
        assert!(
            result.is_err(),
            "exchange must fail without a configured signer"
        );
    }

    #[tokio::test]
    async fn exchange_code_returns_both_an_access_token_and_a_refresh_token() {
        // Given a signed auth service that knows a login code
        let daemon = a_daemon_key(7);
        let (stub, user) = setup();
        stub.register_code("login-code", user);
        let service = a_signed_service(stub, &daemon, &[&daemon]);
        let state = service
            .get_auth_url(Request::new(GetAuthUrlRequest {}))
            .await
            .expect("auth url")
            .into_inner()
            .state;

        // When a code is exchanged
        let resp = service
            .exchange_code(Request::new(ExchangeCodeRequest {
                code: "login-code".to_string(),
                state,
            }))
            .await
            .expect("exchange should succeed")
            .into_inner();

        // Then login returns an access token and a refresh token of the right kinds
        assert_eq!(
            (
                claims_under(&daemon, &resp.session_token).kind,
                claims_under(&daemon, &resp.refresh_token).kind
            ),
            (TokenKind::Access, TokenKind::Refresh)
        );
    }

    #[tokio::test]
    async fn refresh_session_mints_a_new_access_token_and_a_sliding_refresh_token() {
        // Given a signed service and a valid refresh token
        let daemon = a_daemon_key(7);
        let (stub, user) = setup();
        let service = a_signed_service(stub, &daemon, &[&daemon]);
        let refresh_token = a_signer_for(&daemon).mint_refresh(&user);

        // When the session is refreshed
        let resp = service
            .refresh_session(Request::new(RefreshSessionRequest { refresh_token }))
            .await
            .expect("refresh of a valid refresh token should succeed")
            .into_inner();

        // Then it returns a new access token plus a refresh token slid to a fresh 7-day window
        let access = claims_under(&daemon, &resp.session_token);
        let refresh = claims_under(&daemon, &resp.refresh_token);
        assert_eq!(access.kind, TokenKind::Access);
        assert_eq!(refresh.kind, TokenKind::Refresh);
        assert_eq!(refresh.exp - refresh.iat, REFRESH_TOKEN_TTL.as_secs());
        assert_eq!(resp.user.expect("user").login, "testuser");
    }

    #[tokio::test]
    async fn refresh_session_rejects_an_access_kind_token() {
        // Given a signed service and an *access*-kind token
        let daemon = a_daemon_key(7);
        let (stub, user) = setup();
        let service = a_signed_service(stub, &daemon, &[&daemon]);
        let access_token = a_signer_for(&daemon).mint_access(&user);

        // When it is presented to refresh
        let result = service
            .refresh_session(Request::new(RefreshSessionRequest {
                refresh_token: access_token,
            }))
            .await;

        // Then it is rejected — a short-lived access token cannot extend a session
        assert!(
            result.is_err(),
            "an access-kind token must not be refreshable"
        );
    }

    #[tokio::test]
    async fn refresh_session_rejects_an_expired_refresh_token() {
        // Given a signed service and a refresh token whose 7-day window lapsed a day ago
        let daemon = a_daemon_key(7);
        let (stub, user) = setup();
        let service = a_signed_service(stub, &daemon, &[&daemon]);
        let expired = a_signer_for(&daemon).mint_kind_with_issued_at(
            &user,
            TokenKind::Refresh,
            SystemTime::now() - (REFRESH_TOKEN_TTL + Duration::from_secs(86_400)),
            REFRESH_TOKEN_TTL,
        );

        // When a refresh is attempted
        let result = service
            .refresh_session(Request::new(RefreshSessionRequest {
                refresh_token: expired,
            }))
            .await;

        // Then it is rejected — the session has ended and re-login is required
        assert!(
            result.is_err(),
            "refresh must reject an expired refresh token"
        );
    }
}
