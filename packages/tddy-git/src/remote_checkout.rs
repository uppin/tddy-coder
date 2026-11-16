//! A session's checkout on a target — an SSH host or a Coder workspace — through the remote-shell
//! port (`#remote-shell` 10/11).
//!
//! Where it lives is decided **on the target**: the target user's own home, read through the shell,
//! then `.tddy/repos/<repository>` for the project's clone and `.worktrees/<session>` for the
//! session's worktree. Nothing here assumes a path a test host happened to have.
//!
//! The cut honours what the start asked for — a new branch from a base ref, or an existing branch —
//! as the local cut does. A failure carries the target's own words: a clone fails for reasons only
//! the target knows.
//!
//! The functions are declared here with their contract; the bodies land with the implementation in
//! this same PR.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-16-remote-shell-remote-checkout-root.md

use std::fmt;

use tddy_remote_shell::{RemoteShell, RemoteShellError};

/// The directory under the target user's home that holds tddy's clones: `~/.tddy/repos`.
pub const REMOTE_REPOS_DIR: &str = ".tddy/repos";

/// The branch a remote worktree is cut on, as the start asked for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteBranch {
    /// Create `name` from `base_ref` (a remote-tracking ref such as `origin/main`) and check it out.
    NewFrom { name: String, base_ref: String },
    /// Check out the branch `name` the clone already has, fetching it first.
    Existing { name: String },
}

/// What a remote cut needs to know.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteWorktreeRequest {
    /// The project's git URL: what is cloned when the target has no clone yet, and what names the
    /// clone's directory (`qape-hq` for `git@github.com:wix-private/qape-hq.git`).
    pub git_url: String,
    /// The session whose worktree is cut; names the worktree directory.
    pub session_id: String,
    pub branch: RemoteBranch,
}

/// A checkout on the target, as the cut left it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteCheckout {
    /// The project's clone on the target: `<home>/.tddy/repos/<repository>`.
    pub root: String,
    /// The session's worktree: `<root>/.worktrees/<session id>`.
    pub worktree: String,
    /// The branch checked out there.
    pub branch: String,
}

/// Why a checkout could not be made on the target. Every variant that comes from a command the
/// target ran carries what the target printed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteCheckoutError {
    /// The target user's home could not be read. Carries the target's words.
    HomeUnresolved(String),
    /// `git clone <url>` failed on the target. Carries the URL and the target's stderr.
    Clone { url: String, stderr: String },
    /// `git fetch` failed on the target. Carries the stderr.
    Fetch(String),
    /// `git worktree add` failed on the target. Carries the stderr.
    Worktree(String),
    /// The shell itself failed: unreachable, timed out.
    Shell(RemoteShellError),
}

impl fmt::Display for RemoteCheckoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HomeUnresolved(words) => {
                write!(f, "the target did not say where its home is: {words}")
            }
            Self::Clone { url, stderr } => {
                write!(f, "cloning {url} on the target failed: {stderr}")
            }
            Self::Fetch(stderr) => write!(f, "fetching on the target failed: {stderr}"),
            Self::Worktree(stderr) => {
                write!(f, "adding the worktree on the target failed: {stderr}")
            }
            Self::Shell(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for RemoteCheckoutError {}

impl From<RemoteShellError> for RemoteCheckoutError {
    fn from(error: RemoteShellError) -> Self {
        Self::Shell(error)
    }
}

/// The repository name a git URL names: the last path segment without `.git`
/// (`qape-hq` for `git@github.com:wix-private/qape-hq.git` and for `https://…/qape-hq.git`).
#[must_use]
pub fn repository_name_of(git_url: &str) -> String {
    let _ = git_url;
    todo!("#remote-shell 10/11: the repository name a git URL names")
}

/// Where the project `git_url` names is cloned on the target `shell` reaches: the target user's
/// home, read through the shell, then [`REMOTE_REPOS_DIR`] and the repository name.
pub async fn remote_checkout_root(
    shell: &dyn RemoteShell,
    git_url: &str,
) -> Result<String, RemoteCheckoutError> {
    let _ = (shell, git_url);
    todo!("#remote-shell 10/11: the checkout root is resolved on the target")
}

/// Cut `request`'s worktree on the target `shell` reaches: clone the project under
/// [`remote_checkout_root`] when it is not there yet, fetch, and add `.worktrees/<session>` on the
/// requested branch. Idempotent for a worktree that already exists at that path.
pub async fn setup_remote_worktree(
    shell: &dyn RemoteShell,
    request: &RemoteWorktreeRequest,
) -> Result<RemoteCheckout, RemoteCheckoutError> {
    let _ = (shell, request);
    todo!("#remote-shell 10/11: the worktree is cut on the target from the requested branch")
}
