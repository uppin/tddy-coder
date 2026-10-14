//! What a **Local gh** project acts as, from facts the host's GitHub CLI reports.
//!
//! A Local gh project has no vault account: each host's own `gh` login, for the GitHub host the
//! project's git URL names, is the identity. This module holds the two pure halves — which GitHub
//! host to ask `gh` about, and the commit identity of the login it answers with. Asking `gh` is
//! `tddy_host_service::GithubCli`'s: this crate does not run processes.
//!
//! The identity rule is the vault's ([`crate::acting_identity`]): the name is the login, the email
//! is the login's noreply address built from GitHub's immutable user id.

use crate::GitIdentity;

/// Why a Local gh project has no GitHub host to ask about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalGhError {
    /// The project's git URL names no host. It is never assumed to be `github.com`.
    UnreadableGitUrl { git_url: String },
}

impl std::fmt::Display for LocalGhError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!("LocalGhError: Display")
    }
}

impl std::error::Error for LocalGhError {}

/// The GitHub host a project's git URL names: `https://H/…`, `ssh://git@H[:port]/…` or the
/// scp-style `git@H:…`.
pub fn github_hostname_of(git_url: &str) -> Result<String, LocalGhError> {
    todo!("github_hostname_of")
}

/// The commit identity of the GitHub login `login`, whose immutable user id is `id`.
pub fn local_gh_git_identity(login: &str, id: u64) -> GitIdentity {
    todo!("local_gh_git_identity")
}
