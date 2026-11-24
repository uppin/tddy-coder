//! What removing a worktree would lose — the dirty-state reader behind `PreviewCloseSession`
//! (docs/ft/daemon/session-liveness.md § Closing a session).
//!
//! Two kinds of loss, per worktree:
//! - every `git status --porcelain --untracked-files=all` entry: staged, unstaged, deleted, renamed,
//!   and untracked files that are not ignored;
//! - every commit on `HEAD` that no remote-tracking ref and not the session's recorded base hold
//!   (`git log HEAD --not --remotes [<base>]`), listed up to [`LISTED_UNPUSHED_COMMITS`], counted in
//!   full.
//!
//! Read here for a worktree on this host, and through a [`RemoteShell`] for one on a session's
//! target — the same commands, the same parser.

use std::path::Path;

use tddy_remote_shell::{ExecOutput, ExecRequest, RemoteShell};
use tddy_rpc::Status;

use crate::worktree_sync::{
    git_stdout, run_git, CommitLine, DirtyChange, DirtyFile, WorktreeSyncError,
};

/// How many unpushed commits a loss lists; [`WorktreeLoss::unpushed_count`] counts them all.
pub const LISTED_UNPUSHED_COMMITS: usize = 20;

/// What removing one worktree would lose.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorktreeLoss {
    /// Uncommitted files, in `git status` order.
    pub dirty_files: Vec<DirtyFile>,
    /// Commits no remote and not the base hold, newest first, at most [`LISTED_UNPUSHED_COMMITS`].
    pub unpushed: Vec<CommitLine>,
    /// How many such commits there are in all.
    pub unpushed_count: u32,
}

impl WorktreeLoss {
    /// Whether removing the worktree would lose nothing.
    pub fn is_empty(&self) -> bool {
        self.dirty_files.is_empty() && self.unpushed_count == 0
    }
}

/// The `git status` whose output [`parse_porcelain_status`] reads: every staged, unstaged, deleted,
/// renamed and untracked-not-ignored path, NUL-separated so a path may hold any byte but NUL.
pub const PORCELAIN_STATUS_ARGS: [&str; 4] =
    ["status", "--porcelain", "-z", "--untracked-files=all"];

/// Parse `git status --porcelain -z --untracked-files=all` output into the files it names.
///
/// A rename or copy is one entry, named by its new path: the original path that follows it is
/// consumed. An entry too short to carry its two status letters and a path is an error rather than
/// skipped, so a format git changed is never read as "nothing would be lost".
pub fn parse_porcelain_status(status: &str) -> Result<Vec<DirtyFile>, WorktreeSyncError> {
    let mut entries = status.split('\0').filter(|entry| !entry.is_empty());
    let mut files = Vec::new();
    while let Some(entry) = entries.next() {
        let (Some(codes), Some(path)) = (entry.get(..2), entry.get(3..)) else {
            return Err(WorktreeSyncError(format!(
                "unrecognised git status entry {entry:?}"
            )));
        };
        let mut codes = codes.chars();
        let (index, tree) = (codes.next().unwrap_or(' '), codes.next().unwrap_or(' '));
        let renamed = matches!(index, 'R' | 'C') || matches!(tree, 'R' | 'C');
        let change = match (index, tree) {
            ('?', '?') => DirtyChange::Untracked,
            _ if renamed => DirtyChange::Renamed,
            ('D', _) | (_, 'D') => DirtyChange::Deleted,
            (_, ' ') => DirtyChange::Staged,
            _ => DirtyChange::Modified,
        };
        if renamed {
            entries.next();
        }
        files.push(DirtyFile {
            path: path.to_string(),
            change,
        });
    }
    Ok(files)
}

/// `git rev-parse` of the commit `base` names: exit 0 with it, exit 1 when nothing resolves.
fn base_commit_args(base: &str) -> Vec<String> {
    ["rev-parse", "--verify", "--quiet"]
        .iter()
        .map(|arg| arg.to_string())
        .chain([format!("{base}^{{commit}}")])
        .collect()
}

/// `HEAD --not --remotes [<base>] --`: the commits on `HEAD` no remote-tracking ref and not `base`
/// hold. `--not` negates every revision after it.
fn unpushed_range(base: Option<&str>) -> Vec<String> {
    ["HEAD", "--not", "--remotes"]
        .iter()
        .map(|arg| arg.to_string())
        .chain(base.map(str::to_string))
        .chain(["--".to_string()])
        .collect()
}

fn unpushed_count_args(base: Option<&str>) -> Vec<String> {
    ["rev-list", "--count"]
        .iter()
        .map(|arg| arg.to_string())
        .chain(unpushed_range(base))
        .collect()
}

fn unpushed_listing_args(base: Option<&str>) -> Vec<String> {
    ["log", "--format=%H%x1f%s", "-n"]
        .iter()
        .map(|arg| arg.to_string())
        .chain([LISTED_UNPUSHED_COMMITS.to_string()])
        .chain(unpushed_range(base))
        .collect()
}

/// The commits `git log --format=%H%x1f%s` listed, in its order (newest first).
fn parse_commit_lines(listing: &str) -> Vec<CommitLine> {
    listing
        .lines()
        .filter_map(|line| line.split_once('\u{1f}'))
        .map(|(sha, subject)| CommitLine {
            sha: sha.to_string(),
            subject: subject.to_string(),
        })
        .collect()
}

fn parse_count(count: &str) -> Result<u32, WorktreeSyncError> {
    count.trim().parse::<u32>().map_err(|_| {
        WorktreeSyncError(format!(
            "git rev-list --count answered {:?}, not a count",
            count.trim()
        ))
    })
}

/// What removing `worktree`, on this host, would lose. `base` is the commit the session was cut
/// from, when it recorded one: commits it holds are not losses. A recorded base that no longer
/// resolves holds nothing, so the commits it held are listed as losses.
pub fn read_local_worktree_loss(
    worktree: &Path,
    base: Option<&str>,
) -> Result<WorktreeLoss, WorktreeSyncError> {
    let status = git_stdout(worktree, &PORCELAIN_STATUS_ARGS)?;
    let dirty_files = parse_porcelain_status(&status)?;
    let base = match base {
        Some(base) => local_base_commit(worktree, base)?,
        None => None,
    };
    let count = git_stdout(worktree, &as_strs(&unpushed_count_args(base.as_deref())))?;
    let listing = git_stdout(worktree, &as_strs(&unpushed_listing_args(base.as_deref())))?;
    Ok(WorktreeLoss {
        dirty_files,
        unpushed: parse_commit_lines(&listing),
        unpushed_count: parse_count(&count)?,
    })
}

fn as_strs(args: &[String]) -> Vec<&str> {
    args.iter().map(String::as_str).collect()
}

/// The commit `base` names in `worktree`'s repository; `None` when it names none.
fn local_base_commit(worktree: &Path, base: &str) -> Result<Option<String>, WorktreeSyncError> {
    let args = base_commit_args(base);
    let out = run_git(worktree, &as_strs(&args))?;
    match out.status.code() {
        Some(0) => Ok(Some(
            String::from_utf8_lossy(&out.stdout).trim().to_string(),
        )),
        Some(1) => Ok(None),
        _ => Err(WorktreeSyncError(format!(
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))),
    }
}

/// What removing `worktree` on a session's target would lose, read through `shell` with the
/// commands [`read_local_worktree_loss`] runs and the same parser. A target that cannot be reached
/// is `UNAVAILABLE`, a path git cannot read there `FAILED_PRECONDITION`; either says why.
pub async fn read_target_worktree_loss(
    shell: &dyn RemoteShell,
    worktree: &str,
    base: Option<&str>,
) -> Result<WorktreeLoss, Status> {
    let status = git_on_target(shell, worktree, &PORCELAIN_STATUS_ARGS.map(str::to_string))
        .await?
        .succeeded()?;
    let dirty_files =
        parse_porcelain_status(&status).map_err(|e| Status::internal(e.to_string()))?;
    let base = match base {
        Some(base) => {
            let resolved = git_on_target(shell, worktree, &base_commit_args(base)).await?;
            match resolved.output.status {
                0 => Some(resolved.stdout_text().trim().to_string()),
                1 => None,
                _ => return Err(resolved.refusal()),
            }
        }
        None => None,
    };
    let count = git_on_target(shell, worktree, &unpushed_count_args(base.as_deref()))
        .await?
        .succeeded()?;
    let listing = git_on_target(shell, worktree, &unpushed_listing_args(base.as_deref()))
        .await?
        .succeeded()?;
    Ok(WorktreeLoss {
        dirty_files,
        unpushed: parse_commit_lines(&listing),
        unpushed_count: parse_count(&count).map_err(|e| Status::internal(e.to_string()))?,
    })
}

/// One `git -C <worktree> <args>` that ran on a target, whatever its exit status.
struct TargetGit {
    args: Vec<String>,
    output: ExecOutput,
}

impl TargetGit {
    fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.output.stdout).into_owned()
    }

    /// The refusal of a git that exited non-zero: what it said on stderr.
    fn refusal(&self) -> Status {
        Status::failed_precondition(format!(
            "git {:?} failed on the target: {}",
            self.args,
            String::from_utf8_lossy(&self.output.stderr).trim()
        ))
    }

    /// Its stdout, when it exited zero.
    fn succeeded(self) -> Result<String, Status> {
        if self.output.status == 0 {
            Ok(self.stdout_text())
        } else {
            Err(self.refusal())
        }
    }
}

/// Run `git -C <worktree> <args>` through `shell`. Only a target that could not run it at all is an
/// error here (`UNAVAILABLE`, saying why); a git that ran and failed is the caller's to read.
async fn git_on_target(
    shell: &dyn RemoteShell,
    worktree: &str,
    args: &[String],
) -> Result<TargetGit, Status> {
    let command = std::iter::once("git".to_string())
        .chain(["-C".to_string(), quoted(worktree)])
        .chain(args.iter().map(|arg| quoted(arg)))
        .collect::<Vec<_>>()
        .join(" ");
    let output = shell
        .exec(ExecRequest::new(command))
        .await
        .map_err(|e| Status::unavailable(e.to_string()))?;
    Ok(TargetGit {
        args: args.to_vec(),
        output,
    })
}

fn quoted(text: &str) -> String {
    tddy_core::ssh_exec::shell_single_quote(text)
}
