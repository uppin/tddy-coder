pub mod auth_service;
pub mod provider;
pub mod real;
pub mod stub;

pub use auth_service::AuthServiceImpl;
pub use provider::{GitHubOAuthProvider, GitHubUser};
pub use real::RealGitHubProvider;
pub use stub::StubGitHubProvider;
