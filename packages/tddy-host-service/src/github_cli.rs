//! The host's own GitHub CLI, asked as an OS user: who it is logged in as, and its token.
//!
//! A project set to **Local gh** takes its GitHub identity from this, instead of from a vault
//! account (docs/ft/daemon/github-identity.md). `gh` keeps its login per OS user
//! (`$HOME/.config/gh/hosts.yml`, or the keyring), so every question goes through
//! [`tddy_daemon_kernel::spawn_as_user`], exactly as the tooling probe does
//! ([`crate::host_tooling`]).
//!
//! Every question names the GitHub **host** (`--hostname`): a person logged in to `github.com` and
//! to an Enterprise host is two logins, and the project's git URL says which one it means.
//!
//! The environment is never a credential: `GH_TOKEN`, `GITHUB_TOKEN`, `GH_ENTERPRISE_TOKEN` and
//! `GITHUB_ENTERPRISE_TOKEN` are not passed to `gh`, which would otherwise prefer them over the login
//! this exists to read.

use std::path::PathBuf;

/// Which `gh` to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubCli {
    program: GhProgram,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum GhProgram {
    /// Looked up on the spawned child's `PATH` at each call, so a `gh` installed after the daemon
    /// started is found, and one removed is reported as not installed.
    OnSpawnChildPath,
    /// An explicit executable.
    At(PathBuf),
}

/// The GitHub user `gh` is logged in as, for one host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhUser {
    pub login: String,
    /// GitHub's immutable user id — the half of the noreply address a renamed login cannot change.
    pub id: u64,
}

/// Why `gh` gave no answer. Each names what the person would do about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GithubCliRefusal {
    /// No `gh` executable on this host.
    NotInstalled,
    /// `gh` is installed and is not logged in to `hostname` as `os_user`.
    NotLoggedIn { hostname: String, os_user: String },
    /// `gh` could not be run, timed out, or answered something this code does not recognise.
    Failed(String),
}

impl std::fmt::Display for GithubCliRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!("GithubCliRefusal: Display")
    }
}

impl GithubCli {
    /// The `gh` on the `PATH` a spawned child runs with, looked up at each call.
    pub fn on_spawn_child_path() -> Self {
        Self {
            program: GhProgram::OnSpawnChildPath,
        }
    }

    /// The `gh` at `program`.
    pub fn at(program: PathBuf) -> Self {
        Self {
            program: GhProgram::At(program),
        }
    }

    /// Who `gh api user --hostname <hostname>` says it is logged in as, asked as `os_user`.
    pub fn logged_in_user(&self, os_user: &str, hostname: &str) -> Result<GhUser, GithubCliRefusal> {
        todo!("GithubCli::logged_in_user")
    }

    /// What `gh auth token --hostname <hostname>` answers, asked as `os_user`.
    pub fn auth_token(&self, os_user: &str, hostname: &str) -> Result<String, GithubCliRefusal> {
        todo!("GithubCli::auth_token")
    }
}
