pub mod auth_service;
pub mod provider;
pub mod real;
// TODO(signing-key): the `v1` HMAC format, replaced by `session_token_v2`. Its callers move over
// and this module is deleted in this PR — `v1` and `v2` never ship together.
pub mod session_token;
pub mod session_token_v2;
pub mod stub;
pub mod token_store;

pub use auth_service::AuthServiceImpl;
pub use provider::{GitHubOAuthProvider, GitHubUser};
pub use real::RealGitHubProvider;
pub use session_token::{
    SessionClaims, SessionTokenError, SessionTokenSigner, TokenKind, REFRESH_TOKEN_TTL,
    SESSION_TOKEN_TTL,
};
pub use stub::StubGitHubProvider;
pub use token_store::GitHubTokenStore;
