//! Git merge-base and diff context for the **inspect** step (deterministic, same in all environments).

use std::path::{Path, PathBuf};
use std::process::Command;

/// Max bytes of `git diff --stat` output embedded in agent prompts (memory / model limits).
const MAX_DIFF_STAT_BYTES: usize = 48_000;
/// Max bytes of `git diff` body embedded in agent prompts.
const MAX_DIFF_BODY_BYTES: usize = 48_000;

/// Operator-facing documentation for merge-base behavior (single source of truth with
/// [`merge_base_commit_for_review`]).
#[must_use]
pub fn merge_base_strategy_documentation() -> &'static str {
    "Merge-base for branch review is computed deterministically: try `git merge-base HEAD` against \
     `origin/HEAD`, then `origin/main`, `origin/master`, `main`, `master`; if no ref resolves, use \
     `git rev-parse HEAD` (or the literal ref `HEAD` if needed) so the diff may be empty rather than \
     failing the workflow. Review scope is `git diff <merge_base>..HEAD` in the agent repository root."
}

/// Truncate `s` to at most `max_bytes` UTF-8 bytes without splitting a codepoint.
#[must_use]
fn truncate_utf8_prefix(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Walk upward from `start` until a `.git` directory is found.
#[must_use]
pub fn resolve_git_repo_root(start: &Path) -> Option<PathBuf> {
    let mut dir = start;
    loop {
        if dir.join(".git").exists() {
            return Some(dir.to_path_buf());
        }
        dir = dir.parent()?;
    }
}

fn git_output(repo: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .current_dir(repo)
        .args(args)
        .output()
        .map_err(|e| format!("git {}: {e}", args.join(" ")))?;
    if !out.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn git_merge_base(repo: &Path, other: &str) -> Option<String> {
    let out = Command::new("git")
        .current_dir(repo)
        .args(["merge-base", "HEAD", other])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Merge base for `HEAD` vs a tracked integration branch, deterministic order:
/// `origin/HEAD`, `origin/main`, `origin/master`, `main`, `master`, then `HEAD` (empty diff).
///
/// This matches common CI/mainline workflows; operators may override focus via elicitation.
#[must_use]
pub fn merge_base_commit_for_review(repo: &Path) -> String {
    log::debug!("merge_base_commit_for_review: repo={}", repo.display());
    for candidate in [
        "origin/HEAD",
        "origin/main",
        "origin/master",
        "main",
        "master",
    ] {
        if let Some(base) = git_merge_base(repo, candidate) {
            log::info!(
                "merge_base_commit_for_review: using merge-base with {} -> {}",
                candidate,
                base
            );
            return base;
        }
    }
    if let Ok(h) = git_output(repo, &["rev-parse", "HEAD"]) {
        log::info!(
            "merge_base_commit_for_review: fallback to HEAD (no shared base with mainline refs) -> {}",
            h
        );
        return h;
    }
    log::warn!("merge_base_commit_for_review: could not resolve HEAD; using literal HEAD");
    "HEAD".to_string()
}

/// Diff stat and truncated diff for agent prompt context (read-only review scope).
#[must_use]
pub fn format_diff_context_for_prompt(repo: &Path, merge_base: &str) -> String {
    let stat = Command::new("git")
        .current_dir(repo)
        .args(["diff", "--stat", &format!("{merge_base}..HEAD")])
        .output();
    let stat_block = match stat {
        Ok(o) if o.status.success() => {
            let raw = String::from_utf8_lossy(&o.stdout);
            let t = truncate_utf8_prefix(&raw, MAX_DIFF_STAT_BYTES);
            if raw.len() > MAX_DIFF_STAT_BYTES {
                format!(
                    "{}\n\n… truncated {} bytes from git diff --stat …",
                    t,
                    raw.len() - t.len()
                )
            } else {
                raw.to_string()
            }
        }
        Ok(o) => format!(
            "(git diff --stat unavailable: {})",
            String::from_utf8_lossy(&o.stderr)
        ),
        Err(e) => format!("(git diff --stat failed: {e})"),
    };

    let diff = Command::new("git")
        .current_dir(repo)
        .args(["diff", &format!("{merge_base}..HEAD"), "--", "."])
        .output();
    let diff_block = match diff {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout);
            let prefix = truncate_utf8_prefix(&s, MAX_DIFF_BODY_BYTES);
            if s.len() > MAX_DIFF_BODY_BYTES {
                format!(
                    "{}\n\n… truncated {} bytes …",
                    prefix,
                    s.len() - prefix.len()
                )
            } else {
                s.to_string()
            }
        }
        Ok(o) => format!(
            "(git diff unavailable: {})",
            String::from_utf8_lossy(&o.stderr)
        ),
        Err(e) => format!("(git diff failed: {e})"),
    };

    format!("### git diff --stat {merge_base}..HEAD\n```\n{stat_block}\n```\n\n### git diff {merge_base}..HEAD (truncated)\n```diff\n{diff_block}\n```")
}

#[cfg(test)]
mod tests {
    use super::truncate_utf8_prefix;

    #[test]
    fn truncate_utf8_prefix_respects_char_boundary() {
        // "a" (1) + "é" U+00E9 (2 UTF-8 bytes) + "b" (1) = 4 bytes total
        let s = "aéb";
        assert_eq!(s.len(), 4);
        assert_eq!(truncate_utf8_prefix(s, 1), "a");
        assert_eq!(truncate_utf8_prefix(s, 2), "a");
        assert_eq!(truncate_utf8_prefix(s, 3), "aé");
        assert_eq!(truncate_utf8_prefix(s, 4), s);
        assert_eq!(truncate_utf8_prefix(s, 100), s);
    }
}
