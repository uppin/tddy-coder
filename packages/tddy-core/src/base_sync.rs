//! How a branch stands against its base: how far behind, how far ahead, and whether taking the
//! base's commits would conflict — computed **without touching the repository's state**.
//!
//! This runs on a status poll, against worktrees that may have a child session's agent working in
//! them mid-turn. That is the whole constraint the implementation is shaped by:
//!
//! - `git rev-list --left-right --count` reads history and writes nothing.
//! - `git merge-tree --write-tree` performs the merge entirely in memory and writes only the
//!   resulting tree (and the blobs it needed) into `.git/objects`. It touches **no index, no
//!   working tree, no `HEAD` and no ref** — nothing a concurrent agent can observe. The loose
//!   objects it leaves are unreferenced and collected by the next `git gc` like any other.
//! - Nothing here fetches. [`crate::worktree::resolve_default_integration_base_ref`] runs
//!   `git fetch origin`, which cannot be on a five-second path; the base is therefore read as of
//!   the last fetch, which makes the probe conservative in the same direction the existing
//!   remote-branch probe already is — it can report a branch behind that has just caught up, never
//!   the reverse.
//!
//! `orchestrate_pr_stack::pr_actions::pr_resolve_conflicts_action` is the workspace's other
//! conflict detector and is **explicitly not reusable here**: it runs `git merge --no-commit
//! --no-ff` followed by `git merge --abort`, which mutates the index and the working tree. Running
//! it on a poll would corrupt the turn of any agent working in that worktree, and the abort would
//! discard whatever that agent had staged.
//!
//! Every failure is an [`Err`], never a zeroed success. A comparison that could not be made arrives
//! byte-identical to a healthy one — nothing behind, no conflicts — so collapsing it to a default
//! would render "could not tell" as "clean" (PRD D27).

use std::path::Path;
use std::process::Command;

use crate::worktree::{detect_default_remote_name, local_branch_name_for_remote};

/// The two refs a comparison is made between, resolved to names and to commits.
///
/// Separated from [`compare_base_sync_refs`] so a caller polling every few seconds can key a cache
/// on the whole of it: a comparison between the same two commits can never produce a different
/// answer, so such an entry can never go stale — it only becomes unreachable when a ref moves. The
/// ref *names* belong in that key too, because the answer carries them: two branches sitting at the
/// same commit are two rows with the same counts and different identities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseSyncRefs {
    /// The ref the base resolved to, as a name — e.g. `origin/master` or `master`.
    pub base_ref: String,
    pub base_sha: String,
    /// The ref the head resolved to, as a name — e.g. `feature/x` or `origin/feature/x`.
    pub head_ref: String,
    pub head_sha: String,
}

/// How a branch stands against its base.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchBaseSync {
    /// The base branch name that was probed, with any `<remote>/` prefix normalised off.
    pub base_branch: String,
    /// The ref the base actually resolved to. This — not what the caller asked for — is what a row
    /// naming the comparison must state: the counts are meaningless without the ref they came from.
    pub base_ref: String,
    /// The ref the head actually resolved to.
    pub head_ref: String,
    /// Commits on the base that the branch lacks.
    pub behind_count: u32,
    /// Commits on the branch that the base lacks.
    pub ahead_count: u32,
    pub has_conflicts: bool,
    /// The files that cannot be merged, sorted and deduplicated. Empty unless `has_conflicts`.
    pub conflicted_paths: Vec<String>,
}

/// Compare `branch` against `base_branch` in `repo_root` — [`resolve_base_sync_refs`] followed by
/// [`compare_base_sync_refs`].
pub fn branch_base_sync(
    repo_root: &Path,
    branch: &str,
    base_branch: &str,
) -> Result<BranchBaseSync, String> {
    let refs = resolve_base_sync_refs(repo_root, branch, base_branch)?;
    compare_base_sync_refs(repo_root, &refs)
}

/// Resolve both sides of the comparison to a ref name and a commit.
///
/// The base resolves **remote-first** (`refs/remotes/<remote>/<b>`, then `refs/heads/<b>`): the
/// remote-tracking ref is what a descendant is actually built from, and a stale local branch of the
/// same name would understate how far behind the branch is. The head resolves **local-first**: it
/// is the branch being worked on, and its local tip is the truth about it — the remote ref is the
/// fallback for a branch whose local copy has been deleted.
///
/// A `<remote>/` prefix is stripped off `base_branch` before probing. Callers pass
/// `ProjectEntry.main_branch_ref`, which is usually already `origin/master`, and
/// `refs/remotes/origin/origin/master` resolves to nothing at all.
pub fn resolve_base_sync_refs(
    repo_root: &Path,
    branch: &str,
    base_branch: &str,
) -> Result<BaseSyncRefs, String> {
    let branch = branch.trim();
    if branch.is_empty() {
        return Err("no branch was named to compare".to_string());
    }
    let base_branch = base_branch.trim();
    if base_branch.is_empty() {
        return Err(
            "no base branch was named, so there is nothing to compare this branch against"
                .to_string(),
        );
    }

    let remote = detect_default_remote_name(repo_root).unwrap_or_else(|| "origin".to_string());
    let base_branch = local_branch_name_for_remote(base_branch, &remote);

    let (base_ref, base_sha) = resolve_first(
        repo_root,
        &[
            (
                format!("{remote}/{base_branch}"),
                format!("refs/remotes/{remote}/{base_branch}"),
            ),
            (base_branch.to_string(), format!("refs/heads/{base_branch}")),
        ],
    )
    .ok_or_else(|| {
        format!(
            "base branch '{base_branch}' resolves to no ref in this repository (looked for \
             {remote}/{base_branch} and {base_branch})"
        )
    })?;

    let (head_ref, head_sha) = resolve_first(
        repo_root,
        &[
            (branch.to_string(), format!("refs/heads/{branch}")),
            (
                format!("{remote}/{branch}"),
                format!("refs/remotes/{remote}/{branch}"),
            ),
        ],
    )
    .ok_or_else(|| {
        format!(
            "branch '{branch}' resolves to no ref in this repository (looked for {branch} and \
             {remote}/{branch})"
        )
    })?;

    Ok(BaseSyncRefs {
        base_ref,
        base_sha,
        head_ref,
        head_sha,
    })
}

/// Count the divergence between two resolved commits and, when the branch is behind, whether taking
/// the base would conflict.
///
/// The conflict probe is **skipped entirely when the branch is behind by nothing**: there is no
/// commit to take, so there is nothing that could conflict — and the merge is the expensive half.
pub fn compare_base_sync_refs(
    repo_root: &Path,
    refs: &BaseSyncRefs,
) -> Result<BranchBaseSync, String> {
    let (behind_count, ahead_count) = count_divergence(repo_root, &refs.base_sha, &refs.head_sha)?;

    let conflicted_paths = if behind_count == 0 {
        Vec::new()
    } else {
        conflicting_paths(repo_root, &refs.base_sha, &refs.head_sha)?
    };

    Ok(BranchBaseSync {
        base_branch: local_branch_name_for_remote(
            &refs.base_ref,
            &detect_default_remote_name(repo_root).unwrap_or_else(|| "origin".to_string()),
        )
        .to_string(),
        base_ref: refs.base_ref.clone(),
        head_ref: refs.head_ref.clone(),
        behind_count,
        ahead_count,
        has_conflicts: !conflicted_paths.is_empty(),
        conflicted_paths,
    })
}

/// `(behind, ahead)` — `git rev-list --left-right --count <base>...<head>` reports the base-only
/// count on the left and the head-only count on the right.
///
/// A reply that cannot be parsed is an error rather than a zero: `(0, 0)` is what a branch that is
/// perfectly in sync looks like, and reporting a failed count that way is the exact conflation this
/// module exists to prevent.
fn count_divergence(
    repo_root: &Path,
    base_sha: &str,
    head_sha: &str,
) -> Result<(u32, u32), String> {
    let range = format!("{base_sha}...{head_sha}");
    let out = git(repo_root, &["rev-list", "--left-right", "--count", &range])?;
    if !out.status.success() {
        return Err(format!(
            "git rev-list --left-right --count {range}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut counts = stdout.split_whitespace();
    let parsed = (|| {
        let behind = counts.next()?.parse().ok()?;
        let ahead = counts.next()?.parse().ok()?;
        Some((behind, ahead))
    })();
    parsed.ok_or_else(|| {
        format!(
            "git rev-list --left-right --count {range} reported '{}', which is not two counts",
            stdout.trim()
        )
    })
}

/// The paths a merge of `base_sha` into `head_sha` could not resolve, sorted and deduplicated.
///
/// `git merge-tree --write-tree` exits 0 for a clean merge and 1 for a conflicted one; any other
/// status is a merge that could not be attempted at all (unrelated histories, a corrupt object)
/// and is reported as an error rather than as a clean result.
///
/// With `--name-only -z` the stdout is NUL-separated: field 0 is the OID of the merged tree, the
/// fields after it are the conflicting paths, and an **empty field terminates that list** — what
/// follows is git's informational message section, which is not a path.
fn conflicting_paths(
    repo_root: &Path,
    base_sha: &str,
    head_sha: &str,
) -> Result<Vec<String>, String> {
    let out = git(
        repo_root,
        &[
            "merge-tree",
            "--write-tree",
            "--name-only",
            "-z",
            base_sha,
            head_sha,
        ],
    )?;
    match out.status.code() {
        Some(0) => return Ok(Vec::new()),
        Some(1) => {}
        _ => {
            return Err(format!(
                "git merge-tree --write-tree {base_sha} {head_sha}: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ))
        }
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut paths: Vec<String> = stdout
        .split('\0')
        .skip(1)
        .take_while(|field| !field.is_empty())
        .map(str::to_string)
        .collect();
    paths.sort();
    paths.dedup();
    Ok(paths)
}

/// The first `(display name, full ref)` candidate that resolves to a commit, as
/// `(display name, sha)`.
fn resolve_first(repo_root: &Path, candidates: &[(String, String)]) -> Option<(String, String)> {
    candidates.iter().find_map(|(name, full_ref)| {
        let sha = rev_parse_commit(repo_root, full_ref)?;
        Some((name.clone(), sha))
    })
}

/// The commit a ref points at, or `None` when the ref does not resolve — including when `repo_root`
/// is not a repository at all.
fn rev_parse_commit(repo_root: &Path, full_ref: &str) -> Option<String> {
    let out = git(
        repo_root,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{full_ref}^{{commit}}"),
        ],
    )
    .ok()?;
    if !out.status.success() {
        return None;
    }
    let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!sha.is_empty()).then_some(sha)
}

fn git(repo_root: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    Command::new("git")
        .current_dir(repo_root)
        .args(args)
        .output()
        .map_err(|e| format!("git {} in {}: {e}", args.join(" "), repo_root.display()))
}
