//! Branch and ref names: local-name derivation, ref validation, slugs and free branch names.

use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

/// The local branch name behind a branch reference offered by a remote-branch picker.
///
/// Branch *pickers* deal in remote-tracking names — [`list_recent_remote_branches`](crate::list_recent_remote_branches) reads
/// `refs/remotes/<remote>`, so `ListProjectBranches` (and the Telegram branch picker) offer
/// `<remote>/<branch>`. Everything that operates on a branch needs the local name instead:
/// `git worktree add <path> <remote>/feature/x` succeeds with a **detached HEAD**, so a session
/// started that way looks healthy while every commit it makes is unreachable, whereas the
/// unprefixed name makes git create or reuse the local branch that tracks `<remote>/feature/x`.
///
/// This legacy helper strips one leading `origin/` and is retained for the `origin` last-resort
/// fallback path. Code that has resolved the actual default remote should call
/// [`local_branch_name_for_remote`] instead so a non-`origin` remote is normalized correctly.
///
/// Exactly one leading prefix is stripped: a repository may legitimately hold
/// `refs/heads/origin/foo`, and stripping repeatedly would rename it.
#[must_use]
pub fn local_branch_name(reference: &str) -> &str {
    local_branch_name_for_remote(reference, "origin")
}

/// The local branch name behind a `<remote>/<branch>` reference: strips one leading
/// `<remote>/` when present, and leaves any other name (already local, or carrying a different
/// remote prefix) unchanged.
///
/// Exactly one leading `<remote>/` is stripped: a repository may legitimately hold a local branch
/// whose name starts with `<remote>/`, and stripping repeatedly would rename it.
#[must_use]
pub fn local_branch_name_for_remote<'a>(reference: &'a str, remote: &str) -> &'a str {
    let reference = reference.trim();
    let prefix = format!("{remote}/");
    reference.strip_prefix(&prefix).unwrap_or(reference)
}

/// Characters that must never appear in a remote name or branch path segment passed to a `git`
/// invocation — they could widen a single `git fetch <remote> <path>` into something else.
const FORBIDDEN_REF_CHARS: [char; 7] = [';', '|', '&', '$', '`', '\n', '\r'];

/// Validates a remote name segment (the part before the first `/` in a remote-tracking ref):
/// non-empty, no whitespace, no `..`, no `--`, and none of [`FORBIDDEN_REF_CHARS`]. Pure string
/// rules — no git probe, so the remote is not required to exist in any particular repository.
fn validate_remote_segment(remote: &str) -> Result<(), String> {
    if remote.is_empty() {
        return Err("integration base ref must be <remote>/<branch>: remote is empty".to_string());
    }
    if remote.chars().any(|c| c.is_whitespace()) {
        return Err("integration base ref remote must not contain whitespace".to_string());
    }
    if remote.contains("..") {
        return Err("integration base ref remote must not contain `..`".to_string());
    }
    if remote.contains("--") {
        return Err("integration base ref remote must not contain `--`".to_string());
    }
    for forbidden in FORBIDDEN_REF_CHARS {
        if remote.contains(forbidden) {
            return Err(format!(
                "integration base ref remote contains forbidden character: {:?}",
                forbidden
            ));
        }
    }
    Ok(())
}

/// Validates a single branch path segment: non-empty, no whitespace, no `..`, no `--`, no
/// [`FORBIDDEN_REF_CHARS`].
fn validate_branch_segment(segment: &str) -> Result<(), String> {
    if segment.is_empty() {
        return Err("integration base ref must not contain empty path segments".to_string());
    }
    if segment.chars().any(|c| c.is_whitespace()) {
        return Err("integration base ref must not contain whitespace".to_string());
    }
    if segment.contains("..") {
        return Err("integration base ref must not contain `..`".to_string());
    }
    if segment.contains("--") {
        return Err("integration base ref must not contain `--`".to_string());
    }
    for forbidden in FORBIDDEN_REF_CHARS {
        if segment.contains(forbidden) {
            return Err(format!(
                "integration base ref contains forbidden character: {:?}",
                forbidden
            ));
        }
    }
    Ok(())
}

/// Splits a remote-tracking ref into `(remote, branch_path)` after validating the remote segment.
/// Returns `Err` when there is no `/` (no remote segment) or the remote segment is unsafe.
fn split_remote_ref(s: &str) -> Result<(&str, &str), String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("integration base ref must not be empty".to_string());
    }
    let Some((remote, rest)) = s.split_once('/') else {
        return Err("integration base ref must be <remote>/<branch-name>".to_string());
    };
    validate_remote_segment(remote)?;
    Ok((remote, rest))
}

/// Validates a per-project integration base ref: a single remote-tracking ref
/// `<remote>/<branch>` with no shell metacharacters or extra git arguments. The remote is not
/// required to be `origin` — any safe remote name is accepted (pure string rules, no git probe).
pub fn validate_integration_base_ref(s: &str) -> Result<(), String> {
    let (remote, rest) = split_remote_ref(s)?;
    if rest.is_empty() {
        return Err("integration base ref must be <remote>/<branch-name>".to_string());
    }
    if rest.contains('/') {
        return Err(
            "integration base ref must be a single remote branch segment: <remote>/<branch-name>"
                .to_string(),
        );
    }
    validate_branch_segment(rest)?;
    let _ = remote;
    Ok(())
}

/// Validates a chain-PR integration base ref: `<remote>/<branch-path>` where `<branch-path>` may
/// contain `/` (e.g. `upstream/feature/foo`). Rejects empty strings, shell metacharacters, and
/// `..`. The remote is not required to be `origin` — any safe remote name is accepted (pure string
/// rules, no git probe).
pub fn validate_chain_pr_integration_base_ref(s: &str) -> Result<(), String> {
    let (remote, rest) = split_remote_ref(s)?;
    if rest.is_empty() {
        return Err("chain PR integration base ref must be <remote>/<branch-path>".to_string());
    }
    for segment in rest.split('/') {
        validate_branch_segment(segment)?;
    }
    let _ = remote;
    Ok(())
}

/// The first `<branch>-<n>` (`n` from 1) that no local branch in `repo_root` holds — the name
/// [`create_worktree_with_retry`](crate::create_worktree_with_retry) would land on, computed without creating a branch or a worktree.
///
/// Used for the `suggested_branch_name` a refused session creation reports, which pre-fills the
/// operator's rename field, so the answer has to be usable as-is. A repo whose branches cannot be
/// listed (not a git repository, no `git`) reports no branch as taken and so suggests `<branch>-1`,
/// the same name the retry loop would try first.
///
/// See docs/ft/daemon/session-branch-conflict.md.
#[must_use]
pub fn first_free_suffixed_branch_name(repo_root: &Path, branch: &str) -> String {
    let taken = local_branch_names(repo_root);
    let mut suffix = 1u32;
    loop {
        let candidate = format!("{branch}-{suffix}");
        if !taken.contains(&candidate) {
            return candidate;
        }
        suffix += 1;
    }
}

/// Every local branch name in `repo_root`, or an empty set when its refs cannot be listed.
fn local_branch_names(repo_root: &Path) -> HashSet<String> {
    let Ok(out) = Command::new("git")
        .args(["for-each-ref", "--format=%(refname)", "refs/heads"])
        .current_dir(repo_root)
        .output()
    else {
        return HashSet::new();
    };
    if !out.status.success() {
        return HashSet::new();
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| line.trim().strip_prefix("refs/heads/"))
        .map(str::to_string)
        .collect()
}

pub fn slugify_for_branch(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// The local branch name behind a reference offered by a remote-branch picker.
#[cfg(test)]
mod local_branch_name_tests {
    use super::*;

    #[test]
    fn strips_the_remote_prefix_from_a_remote_tracking_name() {
        // Given / When — the form `ListProjectBranches` offers
        let branch = local_branch_name("origin/feature/attach-docs/attach-store");

        // Then
        assert_eq!(branch, "feature/attach-docs/attach-store");
    }

    #[test]
    fn leaves_a_name_that_is_already_local_unchanged() {
        // Given / When
        let branch = local_branch_name("feature/attach-docs/attach-store");

        // Then
        assert_eq!(branch, "feature/attach-docs/attach-store");
    }

    #[test]
    fn strips_only_one_remote_prefix_so_a_local_origin_branch_keeps_its_name() {
        // Given / When — `refs/heads/origin/legacy` is a legal local branch; stripping twice renames it
        let branch = local_branch_name("origin/origin/legacy");

        // Then
        assert_eq!(branch, "origin/legacy");
    }

    #[test]
    fn trims_surrounding_whitespace() {
        // Given / When
        let branch = local_branch_name("  origin/master\n");

        // Then
        assert_eq!(branch, "master");
    }
}
