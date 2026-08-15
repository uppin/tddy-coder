//! Token service implementation for LiveKit token generation via RPC.

use async_trait::async_trait;
use std::sync::Arc;

use tddy_rpc::{Request, Response, Status};

use crate::proto::token::{
    GenerateTokenRequest, GenerateTokenResponse, RefreshTokenRequest, RefreshTokenResponse,
    TokenService as TokenServiceTrait,
};

/// Trait for providing LiveKit tokens. Implementations delegate to credential holders
/// (e.g. TokenGenerator) without exposing credentials to the service layer.
pub trait TokenProvider: Send + Sync + 'static {
    /// Generate a JWT token for the given room and identity.
    fn generate_token(&self, room: &str, identity: &str) -> Result<String, String>;
    /// Token TTL in seconds.
    fn ttl_seconds(&self) -> u64;
}

/// Decides whether the caller presenting `session_token` may mint a room JWT. `true` admits the
/// call; `false` refuses it with `UNAUTHENTICATED`.
///
/// Injected as a closure so this crate never learns what a session token *is*: the daemon passes
/// the very resolver (signature, expiry, access-kind) that gates its every other RPC, and its
/// config stays on the daemon side.
pub type SessionTokenAuthenticator = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// Prefix of the LiveKit identity a daemon **serves** its RPC on —
/// `tddy_daemon::livekit_peer_discovery::daemon_rpc_identity` composes its identities from this
/// constant so the two cannot drift.
///
/// This service mints no identity carrying it, on any registration. A participant admitted to a
/// room under a `daemon-*` identity is handed the RPC calls other participants address to that
/// daemon, so a caller free to choose it would be reading everyone else's traffic.
pub const RESERVED_DAEMON_IDENTITY_PREFIX: &str = "daemon-";

/// Token service implementation. Delegates to a TokenProvider.
pub struct TokenServiceImpl<P: TokenProvider> {
    provider: Arc<P>,
    authenticate: Option<SessionTokenAuthenticator>,
}

impl<P: TokenProvider> TokenServiceImpl<P> {
    /// Serve the mint to anything that can reach the transport. Correct only where reachability is
    /// itself the authorization — a session coder's own web port, or its own LiveKit room, where a
    /// caller already holds the room credential this would hand back.
    ///
    /// Named for the property so that no registration acquires it merely by writing `new`.
    pub fn unauthenticated(provider: P) -> Self {
        Self {
            provider: Arc::new(provider),
            authenticate: None,
        }
    }

    /// Serve the mint only to callers whose `session_token` `authenticate` accepts.
    pub fn authenticated(provider: P, authenticate: SessionTokenAuthenticator) -> Self {
        Self {
            provider: Arc::new(provider),
            authenticate: Some(authenticate),
        }
    }

    /// Mint one JWT, applying both gates in turn: the caller must authenticate (where an
    /// authenticator is installed), and the identity it asks for must not be a daemon's.
    fn mint(
        &self,
        session_token: &str,
        room: &str,
        identity: &str,
    ) -> Result<(String, u64), Status> {
        if let Some(authenticate) = &self.authenticate {
            if !authenticate(session_token) {
                return Err(Status::unauthenticated(
                    "session token is missing, expired, or not an access token; \
                     no livekit token minted",
                ));
            }
        }
        refuse_reserved_identity(identity)?;
        let token = self
            .provider
            .generate_token(room, identity)
            .map_err(Status::internal)?;
        Ok((token, self.provider.ttl_seconds()))
    }
}

/// Refuse an identity a daemon's RPC participant answers to.
///
/// Compared case-insensitively and after trimming, so the check cannot be stepped around with
/// `Daemon-x` or ` daemon-x`. Both refuse more than the exact string LiveKit routes on, which is
/// the safe direction: no legitimate caller names itself this way.
fn refuse_reserved_identity(identity: &str) -> Result<(), Status> {
    if identity
        .trim()
        .to_ascii_lowercase()
        .starts_with(RESERVED_DAEMON_IDENTITY_PREFIX)
    {
        return Err(Status::permission_denied(format!(
            "identity \"{identity}\" is reserved: \"{RESERVED_DAEMON_IDENTITY_PREFIX}\" addresses \
             a daemon's RPC-serving participant, which is handed other participants' calls"
        )));
    }
    Ok(())
}

#[async_trait]
impl<P: TokenProvider> TokenServiceTrait for TokenServiceImpl<P> {
    async fn generate_token(
        &self,
        request: Request<GenerateTokenRequest>,
    ) -> Result<Response<GenerateTokenResponse>, Status> {
        let req = request.into_inner();
        let (token, ttl_seconds) = self.mint(&req.session_token, &req.room, &req.identity)?;
        Ok(Response::new(GenerateTokenResponse { token, ttl_seconds }))
    }

    async fn refresh_token(
        &self,
        request: Request<RefreshTokenRequest>,
    ) -> Result<Response<RefreshTokenResponse>, Status> {
        let req = request.into_inner();
        // A refresh mints a brand-new JWT, so it runs the same gates as the first one rather than
        // trusting the (unseen) token being replaced.
        let (token, ttl_seconds) = self.mint(&req.session_token, &req.room, &req.identity)?;
        Ok(Response::new(RefreshTokenResponse { token, ttl_seconds }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tddy_rpc::Code;

    /// The one session token `a_gated_mint()` accepts. Stands in for a signed, unexpired,
    /// access-kind daemon token — what the token *is* lives in the daemon, not in this crate.
    const AN_ACCEPTED_SESSION_TOKEN: &str = "an-access-token";

    /// Mints a JWT that spells out the grant it carries, so a test can assert on *what* was minted
    /// rather than merely that something was.
    struct EchoingTokenProvider;

    impl TokenProvider for EchoingTokenProvider {
        fn generate_token(&self, room: &str, identity: &str) -> Result<String, String> {
            Ok(format!("jwt:{room}:{identity}"))
        }
        fn ttl_seconds(&self) -> u64 {
            120
        }
    }

    /// The mint as a session coder registers it — no authenticator, reachability is the
    /// authorization.
    fn an_open_mint() -> TokenServiceImpl<EchoingTokenProvider> {
        TokenServiceImpl::unauthenticated(EchoingTokenProvider)
    }

    /// The mint as the daemon registers it — only `AN_ACCEPTED_SESSION_TOKEN` authenticates.
    fn a_gated_mint() -> TokenServiceImpl<EchoingTokenProvider> {
        TokenServiceImpl::authenticated(
            EchoingTokenProvider,
            Arc::new(|token: &str| token == AN_ACCEPTED_SESSION_TOKEN),
        )
    }

    /// A well-formed request from a browser: a room it may join, a `web-*` identity, no credential.
    fn a_generate_request() -> GenerateTokenRequest {
        GenerateTokenRequest {
            room: "tddy-lobby".to_string(),
            identity: "web-alice".to_string(),
            session_token: String::new(),
        }
    }

    fn a_refresh_request() -> RefreshTokenRequest {
        RefreshTokenRequest {
            room: "tddy-lobby".to_string(),
            identity: "web-alice".to_string(),
            session_token: String::new(),
        }
    }

    async fn generate(
        service: &TokenServiceImpl<EchoingTokenProvider>,
        request: GenerateTokenRequest,
    ) -> Result<GenerateTokenResponse, Status> {
        service
            .generate_token(Request::new(request))
            .await
            .map(Response::into_inner)
    }

    async fn refresh(
        service: &TokenServiceImpl<EchoingTokenProvider>,
        request: RefreshTokenRequest,
    ) -> Result<RefreshTokenResponse, Status> {
        service
            .refresh_token(Request::new(request))
            .await
            .map(Response::into_inner)
    }

    // -------------------------------------------------------------------------
    // Reserved identities — enforced by the service itself, so *every* registration is covered,
    // not only the ones that also authenticate.
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn refuses_to_mint_a_daemon_rpc_identity_for_an_authenticated_caller() {
        // Given a caller that authenticates, asking for the identity a daemon serves RPC on
        let request = GenerateTokenRequest {
            identity: "daemon-udoo".to_string(),
            session_token: AN_ACCEPTED_SESSION_TOKEN.to_string(),
            ..a_generate_request()
        };

        // When
        let refusal = generate(&a_gated_mint(), request)
            .await
            .expect_err("a daemon identity must never be minted");

        // Then authenticating buys the caller no right to impersonate a daemon
        assert_eq!(refusal.code, Code::PermissionDenied);
        assert!(
            refusal.message.contains("daemon-udoo"),
            "the refusal must name the identity it refused, got: {}",
            refusal.message
        );
    }

    #[tokio::test]
    async fn refuses_to_mint_a_daemon_rpc_identity_on_an_unauthenticated_registration() {
        // Given the open registration a session coder serves
        let request = GenerateTokenRequest {
            identity: "daemon-udoo".to_string(),
            ..a_generate_request()
        };

        // When
        let refusal = generate(&an_open_mint(), request)
            .await
            .expect_err("a daemon identity must never be minted");

        // Then the block is the service's own, not the daemon registration's
        assert_eq!(refusal.code, Code::PermissionDenied);
    }

    #[tokio::test]
    async fn refuses_to_refresh_into_a_daemon_rpc_identity() {
        // Given a refresh that swaps in a daemon identity — the second mint is gated like the first
        let request = RefreshTokenRequest {
            identity: "daemon-udoo".to_string(),
            session_token: AN_ACCEPTED_SESSION_TOKEN.to_string(),
            ..a_refresh_request()
        };

        // When
        let refusal = refresh(&a_gated_mint(), request)
            .await
            .expect_err("a daemon identity must never be minted");

        // Then
        assert_eq!(refusal.code, Code::PermissionDenied);
    }

    #[tokio::test]
    async fn refuses_a_daemon_identity_spelled_in_a_different_case() {
        // Given the same prefix in mixed case
        let request = GenerateTokenRequest {
            identity: "DaEmOn-udoo".to_string(),
            ..a_generate_request()
        };

        // When
        let refusal = generate(&an_open_mint(), request)
            .await
            .expect_err("a daemon identity must never be minted");

        // Then
        assert_eq!(refusal.code, Code::PermissionDenied);
    }

    #[tokio::test]
    async fn refuses_a_daemon_identity_padded_with_whitespace() {
        // Given the prefix behind padding
        let request = GenerateTokenRequest {
            identity: "  daemon-udoo".to_string(),
            ..a_generate_request()
        };

        // When
        let refusal = generate(&an_open_mint(), request)
            .await
            .expect_err("a daemon identity must never be minted");

        // Then
        assert_eq!(refusal.code, Code::PermissionDenied);
    }

    #[tokio::test]
    async fn mints_an_identity_that_merely_mentions_a_daemon_elsewhere_in_its_name() {
        // Given an identity that is not a daemon's, but contains the word
        let request = GenerateTokenRequest {
            identity: "web-daemon-watcher".to_string(),
            ..a_generate_request()
        };

        // When
        let minted = generate(&an_open_mint(), request)
            .await
            .expect("only the reserved *prefix* is refused");

        // Then
        assert_eq!(minted.token, "jwt:tddy-lobby:web-daemon-watcher");
    }

    // -------------------------------------------------------------------------
    // Authentication — installed by the daemon registration, absent from the coder's.
    // -------------------------------------------------------------------------

    #[tokio::test]
    async fn mints_for_a_caller_whose_session_token_authenticates() {
        // Given
        let request = GenerateTokenRequest {
            session_token: AN_ACCEPTED_SESSION_TOKEN.to_string(),
            ..a_generate_request()
        };

        // When
        let minted = generate(&a_gated_mint(), request)
            .await
            .expect("an authenticated caller must be able to mint");

        // Then
        assert_eq!(minted.token, "jwt:tddy-lobby:web-alice");
        assert_eq!(minted.ttl_seconds, 120);
    }

    #[tokio::test]
    async fn refuses_a_caller_presenting_no_session_token() {
        // Given a request carrying no credential — every caller of the pre-existing contract
        // looked like this
        // When
        let refusal = generate(&a_gated_mint(), a_generate_request())
            .await
            .expect_err("an anonymous caller must mint nothing");

        // Then
        assert_eq!(refusal.code, Code::Unauthenticated);
    }

    #[tokio::test]
    async fn refuses_a_caller_whose_session_token_does_not_verify() {
        // Given
        let request = GenerateTokenRequest {
            session_token: "a-forged-token".to_string(),
            ..a_generate_request()
        };

        // When
        let refusal = generate(&a_gated_mint(), request)
            .await
            .expect_err("an unverifiable token must mint nothing");

        // Then
        assert_eq!(refusal.code, Code::Unauthenticated);
    }

    #[tokio::test]
    async fn refuses_a_refresh_whose_session_token_does_not_verify() {
        // Given a refresh presenting a credential the daemon does not accept
        let request = RefreshTokenRequest {
            session_token: "a-forged-token".to_string(),
            ..a_refresh_request()
        };

        // When
        let refusal = refresh(&a_gated_mint(), request)
            .await
            .expect_err("an unverifiable token must refresh nothing");

        // Then a lapsed caller cannot keep renewing its room admission
        assert_eq!(refusal.code, Code::Unauthenticated);
    }

    #[tokio::test]
    async fn authenticates_against_the_session_token_the_request_carried() {
        // Given an authenticator that records what it was handed
        let presented: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let seen = presented.clone();
        let service = TokenServiceImpl::authenticated(
            EchoingTokenProvider,
            Arc::new(move |token: &str| {
                seen.lock().unwrap().push(token.to_string());
                true
            }),
        );

        // When
        generate(
            &service,
            GenerateTokenRequest {
                session_token: "the-callers-own-token".to_string(),
                ..a_generate_request()
            },
        )
        .await
        .expect("must mint");

        // Then
        assert_eq!(
            presented.lock().unwrap().as_slice(),
            ["the-callers-own-token"]
        );
    }

    #[tokio::test]
    async fn mints_without_a_session_token_where_no_authenticator_is_installed() {
        // Given the open registration a session coder serves on its own port
        // When
        let minted = generate(&an_open_mint(), a_generate_request())
            .await
            .expect("an unauthenticated registration must still mint");

        // Then
        assert_eq!(minted.token, "jwt:tddy-lobby:web-alice");
    }
}
