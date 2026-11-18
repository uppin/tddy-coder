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
//! Every command runs as its own shell call, so the target's answer to each — a home it cannot
//! print, a clone it refuses, a fetch or a cut that fails — is reported with its own words.
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
/// (`qape-hq` for `git@github.com:wix-private/qape-hq.git` and for `https://…/qape-hq.git`). A URL
/// naming no segment — the empty URL of a project with no remote yet — names `repo`, as the
/// checkout it is cloned into still needs a directory.
#[must_use]
pub fn repository_name_of(git_url: &str) -> String {
    let trimmed = git_url.trim().trim_end_matches('/');
    let segment = trimmed
        .rsplit(['/', ':'])
        .next()
        .unwrap_or_default()
        .trim_end_matches(".git");
    if segment.is_empty() {
        "repo".to_string()
    } else {
        segment.to_string()
    }
}

/// What a failed command on the target printed, as one trimmed line of text.
fn target_words(output: &tddy_remote_shell::ExecOutput) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let words = if stderr.trim().is_empty() {
        String::from_utf8_lossy(&output.stdout)
    } else {
        stderr
    };
    words.trim().to_string()
}

/// The sentinels the home probe prints around the target's home. A transport may merge
/// the command's streams or add words of its own around the answer — a pty does both — so
/// the value is parsed out from between them rather than read as the whole output.
const HOME_BEGIN: &str = "TDDY_REMOTE_HOME_BEGIN:";
const HOME_END: &str = ":TDDY_REMOTE_HOME_END";

/// The home framed by the probe's sentinels in `stdout`, or `None` when the answer does
/// not carry them. Nothing outside the sentinels is taken for part of the path.
fn home_between_sentinels(stdout: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(stdout);
    let value = text
        .split_once(HOME_BEGIN)?
        .1
        .split_once(HOME_END)?
        .0
        .trim()
        .trim_end_matches('/');
    (!value.is_empty()).then(|| value.to_string())
}

/// Where the project `git_url` names is cloned on the target `shell` reaches: the target user's
/// home, read through the shell, then [`REMOTE_REPOS_DIR`] and the repository name.
pub async fn remote_checkout_root(
    shell: &dyn RemoteShell,
    git_url: &str,
) -> Result<String, RemoteCheckoutError> {
    let probe = format!("printf '%s%s%s\\n' '{HOME_BEGIN}' \"$HOME\" '{HOME_END}'");
    let output = shell
        .exec(tddy_remote_shell::ExecRequest::new(probe))
        .await?;
    if output.status != 0 {
        return Err(RemoteCheckoutError::HomeUnresolved(target_words(&output)));
    }
    let Some(home) = home_between_sentinels(&output.stdout) else {
        return Err(RemoteCheckoutError::HomeUnresolved(target_words(&output)));
    };
    Ok(format!(
        "{home}/{REMOTE_REPOS_DIR}/{}",
        repository_name_of(git_url)
    ))
}

/// Cut `request`'s worktree on the target `shell` reaches: clone the project under
/// [`remote_checkout_root`] when it is not there yet, fetch, and add `.worktrees/<session>` on the
/// requested branch. Idempotent for a worktree that already exists at that path.
pub async fn setup_remote_worktree(
    shell: &dyn RemoteShell,
    request: &RemoteWorktreeRequest,
) -> Result<RemoteCheckout, RemoteCheckoutError> {
    use crate::ssh_exec::shell_single_quote;

    let root = remote_checkout_root(shell, &request.git_url).await?;
    let worktree = format!("{root}/.worktrees/{}", request.session_id);
    ensure_the_clone(shell, &root, &request.git_url).await?;

    let branch = match &request.branch {
        RemoteBranch::NewFrom { name, .. } | RemoteBranch::Existing { name } => name.clone(),
    };
    let checkout = RemoteCheckout {
        root: root.clone(),
        worktree: worktree.clone(),
        branch,
    };

    // A worktree already cut at this path is the cut, asked for again.
    let already_cut = shell
        .exec(tddy_remote_shell::ExecRequest::new(format!(
            "test -d {}",
            shell_single_quote(&worktree)
        )))
        .await?;
    if already_cut.status == 0 {
        return Ok(checkout);
    }

    let fetched = shell
        .exec(tddy_remote_shell::ExecRequest::new(format!(
            "git -C {} fetch --all",
            shell_single_quote(&root)
        )))
        .await?;
    if fetched.status != 0 {
        return Err(RemoteCheckoutError::Fetch(target_words(&fetched)));
    }

    let add = match &request.branch {
        RemoteBranch::NewFrom { name, base_ref } => format!(
            "git -C {} worktree add -b {} {} {}",
            shell_single_quote(&root),
            shell_single_quote(name),
            shell_single_quote(&worktree),
            shell_single_quote(base_ref),
        ),
        RemoteBranch::Existing { name } => format!(
            "git -C {} worktree add {} {}",
            shell_single_quote(&root),
            shell_single_quote(&worktree),
            shell_single_quote(name),
        ),
    };
    let added = shell.exec(tddy_remote_shell::ExecRequest::new(add)).await?;
    if added.status != 0 {
        return Err(RemoteCheckoutError::Worktree(target_words(&added)));
    }
    Ok(checkout)
}

/// The clone of `git_url` at `root` on the target, made when it is not there yet: cloned from the
/// URL, or — for a project with no URL yet — initialised in place with one empty commit, as the
/// local cut's repository begins.
async fn ensure_the_clone(
    shell: &dyn RemoteShell,
    root: &str,
    git_url: &str,
) -> Result<(), RemoteCheckoutError> {
    use crate::ssh_exec::shell_single_quote;

    let there = shell
        .exec(tddy_remote_shell::ExecRequest::new(format!(
            "test -d {}",
            shell_single_quote(&format!("{root}/.git"))
        )))
        .await?;
    if there.status == 0 {
        return Ok(());
    }

    let url = git_url.trim();
    if url.is_empty() {
        for command in [
            format!("git init {}", shell_single_quote(root)),
            format!(
                "git -C {} config user.email t@t.com",
                shell_single_quote(root)
            ),
            format!("git -C {} config user.name T", shell_single_quote(root)),
            format!(
                "git -C {} commit --allow-empty -m init",
                shell_single_quote(root)
            ),
        ] {
            let ran = shell
                .exec(tddy_remote_shell::ExecRequest::new(command))
                .await?;
            if ran.status != 0 {
                return Err(RemoteCheckoutError::Clone {
                    url: git_url.to_string(),
                    stderr: target_words(&ran),
                });
            }
        }
        return Ok(());
    }

    let cloned = shell
        .exec(tddy_remote_shell::ExecRequest::new(format!(
            "git clone {} {}",
            shell_single_quote(url),
            shell_single_quote(root)
        )))
        .await?;
    if cloned.status != 0 {
        return Err(RemoteCheckoutError::Clone {
            url: git_url.to_string(),
            stderr: target_words(&cloned),
        });
    }
    Ok(())
}
