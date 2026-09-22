//! Pure `git` plumbing: worktrees, branches, refs and remotes.
//!
//! Every function here is a wrapper over the `git` command line and knows nothing about sessions,
//! changesets or workflows. Worktrees are stored in `.worktrees/` relative to the repo root.
//!
//! The crate is split by concern — `refs` (branch and ref names), `rev` (`rev-parse` wrappers),
//! `remote` (fetch, push and remote resolution), `worktree` (linked worktrees) and `ssh_worktree`
//! (worktrees on a remote host) — and every item is re-exported at the crate root.

pub mod ssh_exec;

mod refs;
pub use refs::*;
mod rev;
pub use rev::*;
mod remote;
pub use remote::*;
mod worktree;
pub use worktree::*;
mod ssh_worktree;
pub use ssh_worktree::*;

#[cfg(test)]
use std::path::Path;

/// Remote-agnostic validation, detection, and resolution: the `origin` assumption is gone.
#[cfg(test)]
mod remote_agnostic_tests {
    use super::*;
    use std::fs;
    use std::process::Command;

    fn git(cwd: &Path, args: &[&str]) {
        let out = Command::new("git")
            .current_dir(cwd)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn repo_with_remote_master(repo: &Path, remote: &str) {
        fs::create_dir_all(repo).unwrap();
        git(repo, &["init", "--initial-branch=master"]);
        git(repo, &["config", "user.email", "t@t.com"]);
        git(repo, &["config", "user.name", "T"]);
        fs::write(repo.join("f"), "x").unwrap();
        git(repo, &["add", "f"]);
        git(repo, &["commit", "-m", "initial"]);
        git(repo, &["remote", "add", remote, repo.to_str().unwrap()]);
        git(repo, &["push", "-u", remote, "master"]);
    }

    /// The single-segment validator accepts a non-`origin` remote.
    #[test]
    fn validate_integration_base_ref_accepts_a_non_origin_remote() {
        // Given / When
        let r = validate_integration_base_ref("upstream/main");

        // Then
        assert!(
            r.is_ok(),
            "validate_integration_base_ref must accept any safe <remote>/<branch>; got {:?}",
            r
        );
    }

    /// The chain-PR validator accepts a multi-segment ref under a non-`origin` remote.
    #[test]
    fn validate_chain_pr_integration_base_ref_accepts_a_non_origin_multi_segment_ref() {
        // Given / When
        let r = validate_chain_pr_integration_base_ref("upstream/feature/foo");

        // Then
        assert!(
            r.is_ok(),
            "validate_chain_pr_integration_base_ref must accept any safe <remote>/<path>; got {:?}",
            r
        );
    }

    /// A ref with no `/` (no remote segment) is rejected — the remote is mandatory.
    #[test]
    fn validate_integration_base_ref_rejects_a_ref_with_no_remote_segment() {
        // Given / When
        let r = validate_integration_base_ref("refs/heads/main");

        // Then
        assert!(
            r.is_err(),
            "a ref with no <remote>/<branch> form must be rejected; got {:?}",
            r
        );
    }

    /// `local_branch_name_for_remote` strips one leading `<remote>/` and leaves other names alone.
    #[test]
    fn local_branch_name_for_remote_strips_the_given_remote_once() {
        // Given / When
        let branch = local_branch_name_for_remote("upstream/feature/attach-docs", "upstream");

        // Then
        assert_eq!(branch, "feature/attach-docs");
    }

    /// A different remote prefix is left intact so the caller can detect the mismatch.
    #[test]
    fn local_branch_name_for_remote_leaves_a_foreign_remote_prefix_unchanged() {
        // Given / When
        let branch = local_branch_name_for_remote("origin/feature/x", "upstream");

        // Then
        assert_eq!(branch, "origin/feature/x");
    }

    /// `detect_default_remote_name` returns the remote the main worktree's branch tracks.
    #[test]
    fn detect_default_remote_name_returns_the_tracked_remote() {
        // Given — a repo whose `master` tracks `upstream/master`
        let base = std::env::temp_dir().join(format!(
            "tddy-core-detect-remote-tracked-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        let repo = base.join("repo");
        repo_with_remote_master(&repo, "upstream");

        // When
        let remote = detect_default_remote_name(&repo);

        // Then
        assert_eq!(
            remote.as_deref(),
            Some("upstream"),
            "the main worktree's upstream remote must be detected"
        );

        let _ = fs::remove_dir_all(&base);
    }

    /// `detect_default_remote_name` returns `None` on a detached HEAD (no upstream to read).
    #[test]
    fn detect_default_remote_name_returns_none_on_detached_head() {
        // Given
        let base = std::env::temp_dir().join(format!(
            "tddy-core-detect-remote-detached-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        let repo = base.join("repo");
        repo_with_remote_master(&repo, "origin");
        git(&repo, &["checkout", "--detach", "master"]);

        // When
        let remote = detect_default_remote_name(&repo);

        // Then
        assert!(
            remote.is_none(),
            "a detached HEAD has no upstream; got {:?}",
            remote
        );

        let _ = fs::remove_dir_all(&base);
    }

    /// `resolve_default_integration_base_ref_with_remote` probes `<remote>/master` first.
    #[test]
    fn resolve_with_remote_chooses_remote_master_when_present() {
        // Given
        let base = std::env::temp_dir().join(format!(
            "tddy-core-resolve-with-remote-master-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        let repo = base.join("repo");
        repo_with_remote_master(&repo, "upstream");

        // When
        let resolved = resolve_default_integration_base_ref_with_remote(&repo, Some("upstream"));

        // Then
        assert_eq!(
            resolved.as_deref(),
            Ok("upstream/master"),
            "must probe <remote>/master for a non-origin remote"
        );

        let _ = fs::remove_dir_all(&base);
    }

    /// `resolve_default_integration_base_ref_with_remote` falls through to `<remote>/main` when
    /// `master` is absent.
    #[test]
    fn resolve_with_remote_chooses_remote_main_when_master_absent() {
        // Given — a repo whose only mainline branch is `main`, pushed under `upstream`
        let base = std::env::temp_dir().join(format!(
            "tddy-core-resolve-with-remote-main-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        let repo = base.join("repo");
        fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "--initial-branch=main"]);
        git(&repo, &["config", "user.email", "t@t.com"]);
        git(&repo, &["config", "user.name", "T"]);
        fs::write(repo.join("f"), "x").unwrap();
        git(&repo, &["add", "f"]);
        git(&repo, &["commit", "-m", "initial"]);
        git(
            &repo,
            &["remote", "add", "upstream", repo.to_str().unwrap()],
        );
        git(&repo, &["push", "-u", "upstream", "main"]);

        // When
        let resolved = resolve_default_integration_base_ref_with_remote(&repo, Some("upstream"));

        // Then
        assert_eq!(
            resolved.as_deref(),
            Ok("upstream/main"),
            "must fall through to <remote>/main when <remote>/master is absent"
        );

        let _ = fs::remove_dir_all(&base);
    }

    /// `list_recent_remote_branches` filters under the requested remote, not hardcoded `origin`.
    #[test]
    fn list_recent_remote_branches_filters_under_the_requested_remote() {
        // Given — a repo with two remotes: `origin` and `upstream`, each carrying a distinct branch
        let base = std::env::temp_dir().join(format!(
            "tddy-core-list-recent-remote-multi-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        let repo = base.join("repo");
        fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "--initial-branch=master"]);
        git(&repo, &["config", "user.email", "t@t.com"]);
        git(&repo, &["config", "user.name", "T"]);
        fs::write(repo.join("f"), "x").unwrap();
        git(&repo, &["add", "f"]);
        git(&repo, &["commit", "-m", "initial"]);
        git(&repo, &["remote", "add", "origin", repo.to_str().unwrap()]);
        git(&repo, &["push", "-u", "origin", "master"]);
        git(
            &repo,
            &["remote", "add", "upstream", repo.to_str().unwrap()],
        );
        git(&repo, &["checkout", "-b", "feature/up-only"]);
        fs::write(repo.join("g"), "y").unwrap();
        git(&repo, &["add", "g"]);
        git(&repo, &["commit", "-m", "up-only"]);
        git(&repo, &["push", "-u", "upstream", "feature/up-only"]);
        git(&repo, &["checkout", "master"]);

        // When
        let list = list_recent_remote_branches(&repo, "upstream", 10).unwrap();

        // Then
        assert!(
            list.iter().any(|r| r == "upstream/feature/up-only"),
            "expected upstream/feature/up-only in {:?}",
            list
        );
        assert!(
            !list.iter().any(|r| r.starts_with("origin/")),
            "origin/* must not appear when filtering for upstream: {:?}",
            list
        );

        let _ = fs::remove_dir_all(&base);
    }
}
