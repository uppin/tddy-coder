use std::sync::Arc;

use async_trait::async_trait;

use tddy_rpc::{Request, Response, Status};

use crate::provider::{GitHubOAuthProvider, GitHubUser};

use tddy_service::proto::auth::{
    AuthService as AuthServiceTrait, ExchangeCodeRequest, ExchangeCodeResponse,
    GetAuthStatusRequest, GetAuthStatusResponse, GetAuthUrlRequest, GetAuthUrlResponse,
    GitHubUser as ProtoGitHubUser, LogoutRequest, LogoutResponse, RefreshSessionRequest,
    RefreshSessionResponse,
};

fn to_proto_user(user: &GitHubUser) -> ProtoGitHubUser {
    ProtoGitHubUser {
        id: user.id,
        login: user.login.clone(),
        avatar_url: user.avatar_url.clone(),
        name: user.name.clone(),
    }
}

fn proto_user_from_claims(claims: &crate::session_token::SessionClaims) -> ProtoGitHubUser {
    ProtoGitHubUser {
        id: claims.id,
        login: claims.login.clone(),
        avatar_url: claims.avatar_url.clone(),
        name: claims.name.clone(),
    }
}

/// Auth service implementation. Delegates OAuth to a GitHubOAuthProvider and issues stateless,
/// HMAC-signed session tokens (see [`crate::session_token`]). No session state is kept
/// server-side, so a token is verifiable by any daemon holding the same signing secret.
pub struct AuthServiceImpl<P: GitHubOAuthProvider> {
    provider: Arc<P>,
    /// When set, session tokens are stateless HMAC-signed tokens (mint/verify). When `None`,
    /// authentication is non-functional: minting fails and every token is rejected.
    signer: Option<crate::session_token::SessionTokenSigner>,
}

impl<P: GitHubOAuthProvider> AuthServiceImpl<P> {
    /// Create without a signer. Authentication is non-functional — minting fails and every token
    /// is rejected — used when no shared signing secret is configured.
    pub fn new(provider: P) -> Self {
        Self {
            provider: Arc::new(provider),
            signer: None,
        }
    }

    /// Create with a stateless HMAC session-token signer. Tokens are self-describing and
    /// verifiable by any daemon holding the same secret — no shared/persisted session store.
    pub fn new_signed(provider: P, signer: crate::session_token::SessionTokenSigner) -> Self {
        Self {
            provider: Arc::new(provider),
            signer: Some(signer),
        }
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
        let (_access_token, user) = self
            .provider
            .exchange_code(&req.code, &req.state)
            .await
            .map_err(Status::internal)?;

        let proto_user = to_proto_user(&user);

        // Signed mode: return a stateless token verifiable by any daemon holding the same secret.
        // No server-side session state is kept.
        let Some(ref signer) = self.signer else {
            return Err(Status::failed_precondition(
                "session token signing is not configured",
            ));
        };
        // A short-lived access token for RPCs plus a long-lived refresh token to mint further
        // access tokens without re-login.
        let session_token = signer.mint_access(&user);
        let refresh_token = signer.mint_refresh(&user);

        Ok(Response::new(ExchangeCodeResponse {
            session_token,
            user: Some(proto_user),
            refresh_token,
        }))
    }

    async fn get_auth_status(
        &self,
        request: Request<GetAuthStatusRequest>,
    ) -> Result<Response<GetAuthStatusResponse>, Status> {
        let req = request.into_inner();
        // Only an access-kind token authenticates a session — a refresh token is a minting
        // credential, never proof of an authenticated session (matches the daemon RPC resolver).
        let claims = self
            .signer
            .as_ref()
            .and_then(|signer| signer.verify(&req.session_token).ok())
            .filter(|claims| claims.kind == crate::session_token::TokenKind::Access);
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
        let Some(ref signer) = self.signer else {
            return Err(Status::failed_precondition(
                "session token signing is not configured",
            ));
        };
        // Only a currently-valid refresh token can extend a session; an expired/forged one forces
        // re-login.
        let claims = signer
            .verify(&req.refresh_token)
            .map_err(|e| Status::unauthenticated(e.to_string()))?;
        // A short-lived access token must not be usable to mint — only a refresh token can.
        if claims.kind != crate::session_token::TokenKind::Refresh {
            return Err(Status::unauthenticated(
                "session token: not a refresh token",
            ));
        }
        let user = GitHubUser {
            id: claims.id,
            login: claims.login,
            avatar_url: claims.avatar_url,
            name: claims.name,
        };
        // Mint a fresh access token plus a slid refresh token (fresh 7-day window).
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
        // Given an auth service with no signing secret configured
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
    // Cross-daemon session tokens — a token minted by one daemon is verifiable by another that
    // shares the signing secret, with no shared or persisted session store.
    // -------------------------------------------------------------------------

    fn signed_bridge(
        code: &str,
        secret: &[u8],
    ) -> RpcBridge<AuthServiceServer<AuthServiceImpl<StubGitHubProvider>>> {
        let (stub, user) = setup();
        stub.register_code(code, user);
        let signer = crate::session_token::SessionTokenSigner::new(secret);
        let service = AuthServiceImpl::new_signed(stub, signer);
        RpcBridge::new(AuthServiceServer::new(service))
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
    async fn a_token_minted_by_one_daemon_is_authenticated_by_another_sharing_the_secret() {
        // Given one daemon that mints a session token through the GitHub login flow
        let secret = b"shared-livekit-api-secret";
        let serving = signed_bridge("login-code", secret);
        let state = do_get_auth_url_state(&serving).await;
        let token = do_exchange(&serving, "login-code", &state).await;

        // When a *different* daemon — its own service, no shared session store, same secret —
        // checks that token
        let (peer_stub, _) = setup();
        let peer = RpcBridge::new(AuthServiceServer::new(AuthServiceImpl::new_signed(
            peer_stub,
            crate::session_token::SessionTokenSigner::new(secret),
        )));
        let (authenticated, login) = do_get_status(&peer, &token).await;

        // Then the peer authenticates it from the signature alone — no lookup, no shared state
        assert!(
            authenticated,
            "peer daemon should accept a token minted by another daemon with the same secret"
        );
        assert_eq!(login.as_deref(), Some("testuser"));
    }

    // -------------------------------------------------------------------------
    // Signed-token minting, refresh, and the "no signer configured" guard.
    // -------------------------------------------------------------------------

    use crate::session_token::{SessionTokenSigner, TokenKind, REFRESH_TOKEN_TTL};
    use std::time::{Duration, SystemTime};

    #[tokio::test]
    async fn exchange_code_returns_a_signed_token_rather_than_an_opaque_uuid() {
        // Given a signed auth service
        let bridge = signed_bridge("login-code", b"shared-secret");
        let state = do_get_auth_url_state(&bridge).await;

        // When a code is exchanged
        let token = do_exchange(&bridge, "login-code", &state).await;

        // Then the returned token is a signed, self-describing token, not a bare UUID
        assert!(
            token.starts_with("v1."),
            "expected a signed token, got '{token}'"
        );
    }

    #[tokio::test]
    async fn exchange_code_fails_when_no_signer_is_configured() {
        // Given an auth service with no signing secret
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

        // Then it is rejected — there is no secret to mint a verifiable token with
        assert!(
            result.is_err(),
            "exchange must fail without a configured signer"
        );
    }

    #[tokio::test]
    async fn exchange_code_returns_both_an_access_token_and_a_refresh_token() {
        // Given a signed auth service that knows a login code
        let secret = b"shared-secret";
        let (stub, user) = setup();
        stub.register_code("login-code", user);
        let service = AuthServiceImpl::new_signed(stub, SessionTokenSigner::new(secret));
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
        let verifier = SessionTokenSigner::new(secret);
        assert_eq!(
            verifier
                .verify(&resp.session_token)
                .expect("access verifies")
                .kind,
            TokenKind::Access
        );
        assert_eq!(
            verifier
                .verify(&resp.refresh_token)
                .expect("refresh verifies")
                .kind,
            TokenKind::Refresh
        );
    }

    #[tokio::test]
    async fn refresh_session_mints_a_new_access_token_and_a_sliding_refresh_token() {
        // Given a signed service and a valid refresh token
        let secret = b"shared-secret";
        let (stub, user) = setup();
        let service = AuthServiceImpl::new_signed(stub, SessionTokenSigner::new(secret));
        let refresh_token = SessionTokenSigner::new(secret).mint_refresh(&user);

        // When the session is refreshed
        let resp = service
            .refresh_session(Request::new(RefreshSessionRequest { refresh_token }))
            .await
            .expect("refresh of a valid refresh token should succeed")
            .into_inner();

        // Then it returns a new access token plus a refresh token slid to a fresh 7-day window
        let verifier = SessionTokenSigner::new(secret);
        let access = verifier
            .verify(&resp.session_token)
            .expect("new access valid");
        let refresh = verifier
            .verify(&resp.refresh_token)
            .expect("new refresh valid");
        assert_eq!(access.kind, TokenKind::Access);
        assert_eq!(refresh.kind, TokenKind::Refresh);
        assert_eq!(refresh.exp - refresh.iat, REFRESH_TOKEN_TTL.as_secs());
        assert_eq!(resp.user.expect("user").login, "testuser");
    }

    #[tokio::test]
    async fn refresh_session_rejects_an_access_kind_token() {
        // Given a signed service and an *access*-kind token
        let secret = b"shared-secret";
        let (stub, user) = setup();
        let service = AuthServiceImpl::new_signed(stub, SessionTokenSigner::new(secret));
        let access_token = SessionTokenSigner::new(secret).mint_access(&user);

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
        let secret = b"shared-secret";
        let (stub, user) = setup();
        let service = AuthServiceImpl::new_signed(stub, SessionTokenSigner::new(secret));
        let expired = SessionTokenSigner::new(secret).mint_kind_with_issued_at(
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
