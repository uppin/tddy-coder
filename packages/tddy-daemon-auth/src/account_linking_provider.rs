//! The GitHub provider an account link runs on, kept apart from the login wiring in [`crate::auth`].

use std::sync::Arc;

use tddy_daemon_kernel::config::DaemonConfig;
use tddy_github::{GitHubOAuthProvider, RealGitHubProvider};

use crate::auth::{github_provider_kind, GitHubProviderKind};

/// The GitHub provider an account link drives the device flow through, or `None` when this daemon's
/// `github:` block cannot serve one.
///
/// Built from the same `client_id` as the login provider, and so asking for the same scopes (the
/// scope string belongs to [`RealGitHubProvider`], not to either flow). A second instance rather
/// than a handle on the login one: the device flow keeps its attempts per provider, and a link must
/// share nothing with a login but the OAuth App. It is the *token* half only — a
/// [`GitHubOAuthProvider`] has no notion of a session, so nothing built on this can mint one.
///
/// `None` for a stub (`stub: true`): its tokens are synthetic and GitHub would reject them, so
/// storing one as a linked account would put a credential in the vault that can never work. A
/// confidential client gets a public-client provider because the device flow authenticates with the
/// client id alone — the secret is never sent on it.
pub fn github_account_linking_provider(
    config: &DaemonConfig,
) -> Option<Arc<dyn GitHubOAuthProvider>> {
    match config.github.as_ref().and_then(github_provider_kind)? {
        GitHubProviderKind::Stub => None,
        GitHubProviderKind::Confidential { client_id, .. }
        | GitHubProviderKind::Public { client_id } => {
            Some(Arc::new(RealGitHubProvider::new_public(client_id)))
        }
    }
}
