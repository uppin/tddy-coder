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
        match self {
            Self::UnreadableGitUrl { git_url } => write!(
                f,
                "the project's git URL {git_url:?} names no GitHub host; set a git URL such as \
                 https://github.com/<owner>/<repo>.git to use local gh"
            ),
        }
    }
}

impl std::error::Error for LocalGhError {}

/// The GitHub host a project's git URL names: `https://H/…`, `ssh://git@H[:port]/…` or the
/// scp-style `git@H:…`.
pub fn github_hostname_of(git_url: &str) -> Result<String, LocalGhError> {
    let unreadable = || LocalGhError::UnreadableGitUrl {
        git_url: git_url.to_string(),
    };
    let url = git_url.trim();
    let host = if let Some((scheme, rest)) = url.split_once("://") {
        if scheme.is_empty() {
            return Err(unreadable());
        }
        let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
        let host_and_port = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
        host_and_port
            .split_once(':')
            .map_or(host_and_port, |(h, _)| h)
    } else {
        // scp-style `[user@]host:path`; a path with no colon before its first slash names no host.
        let (before_colon, _path) = url.split_once(':').ok_or_else(unreadable)?;
        if before_colon.contains('/') {
            return Err(unreadable());
        }
        before_colon
            .rsplit_once('@')
            .map_or(before_colon, |(_, h)| h)
    };
    if host.is_empty() {
        return Err(unreadable());
    }
    Ok(host.to_string())
}

/// The commit identity of the GitHub login `login`, whose immutable user id is `id`.
pub fn local_gh_git_identity(login: &str, id: u64) -> GitIdentity {
    GitIdentity {
        name: login.to_string(),
        email: format!("{id}+{login}@users.noreply.github.com"),
    }
}
