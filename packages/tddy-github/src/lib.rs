pub mod auth_service;
pub mod provider;
pub mod real;
pub mod session_token;
pub mod stub;

pub use auth_service::AuthServiceImpl;
pub use provider::{GitHubOAuthProvider, GitHubUser};
pub use real::RealGitHubProvider;
pub use session_token::{SessionClaims, SessionTokenError, SessionTokenSigner, SESSION_TOKEN_TTL};
pub use stub::StubGitHubProvider;
