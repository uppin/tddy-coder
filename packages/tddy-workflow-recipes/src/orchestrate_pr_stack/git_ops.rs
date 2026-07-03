//! Git operations for orchestrate-pr-stack: rebase, force-push, merge-base, integration refs.

fn run_git(
    repo: &std::path::Path,
    args: &[&str],
) -> Result<std::process::Output, tddy_core::WorkflowError> {
    std::process::Command::new("git")
        .current_dir(repo)
        .args(args)
        .output()
        .map_err(|e| {
            tddy_core::WorkflowError::WriteFailed(format!(
                "git {} in {}: {e}",
                args.join(" "),
                repo.display()
            ))
        })
}

/// Rebase `branch` onto `new_base`, replacing `old_base` as the fork point.
/// Aborts the rebase on conflict and returns `Err`.
#[allow(dead_code)]
pub fn rebase_onto(
    repo_root: &std::path::Path,
    new_base: &str,
    old_base: &str,
    branch: &str,
) -> Result<(), tddy_core::WorkflowError> {
    // Checkout the branch first
    let checkout = run_git(repo_root, &["checkout", branch])?;
    if !checkout.status.success() {
        return Err(tddy_core::WorkflowError::WriteFailed(format!(
            "git checkout {branch}: {}",
            String::from_utf8_lossy(&checkout.stderr)
        )));
    }

    let rebase = run_git(repo_root, &["rebase", "--onto", new_base, old_base, branch])?;
    if !rebase.status.success() {
        // Abort so the repo is left clean
        let _ = run_git(repo_root, &["rebase", "--abort"]);
        return Err(tddy_core::WorkflowError::WriteFailed(format!(
            "git rebase --onto {new_base} {old_base} {branch} failed: {}",
            String::from_utf8_lossy(&rebase.stderr)
        )));
    }
    Ok(())
}

/// Force-push `branch` to origin, aborting if origin no longer matches `expected_sha`.
#[allow(dead_code)]
pub fn force_push_with_lease(
    repo_root: &std::path::Path,
    branch: &str,
    expected_sha: &str,
) -> Result<(), tddy_core::WorkflowError> {
    let lease_spec = format!("{branch}:{expected_sha}");
    let out = run_git(
        repo_root,
        &[
            "push",
            &format!("--force-with-lease={lease_spec}"),
            "origin",
            branch,
        ],
    )?;
    if !out.status.success() {
        return Err(tddy_core::WorkflowError::WriteFailed(format!(
            "git push --force-with-lease={lease_spec} origin {branch}: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(())
}

/// Compute `git merge-base a b`, returning the common ancestor SHA.
#[allow(dead_code)]
pub fn merge_base(
    repo_root: &std::path::Path,
    a: &str,
    b: &str,
) -> Result<String, tddy_core::WorkflowError> {
    let out = run_git(repo_root, &["merge-base", a, b])?;
    if !out.status.success() {
        return Err(tddy_core::WorkflowError::WriteFailed(format!(
            "git merge-base {a} {b}: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

// TODO: implement — octopus merge of parent_branches into stack-int/<node_id>; used for multi-parent DAG nodes
/// Build or refresh a local integration ref (`stack-int/<node_id>`) from multiple parent tips.
/// Returns the SHA of the resulting ref.
#[allow(dead_code)]
pub fn build_integration_ref(
    _repo_root: &std::path::Path,
    _node_id: &str,
    _parent_branches: &[String],
) -> Result<String, tddy_core::WorkflowError> {
    unimplemented!("build_integration_ref: not yet implemented")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_repo_with_commit(dir: &std::path::Path) -> String {
        std::process::Command::new("git")
            .args(["init", "--quiet", "-b", "master"])
            .current_dir(dir)
            .status()
            .expect("git init");
        std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(dir)
            .status()
            .expect("git config email");
        std::process::Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(dir)
            .status()
            .expect("git config name");
        let f = dir.join("file.txt");
        std::fs::write(&f, "initial").expect("write file");
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(dir)
            .status()
            .expect("git add");
        std::process::Command::new("git")
            .args(["commit", "--quiet", "-m", "initial"])
            .current_dir(dir)
            .status()
            .expect("git commit");
        // Return HEAD sha
        let out = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(dir)
            .output()
            .expect("git rev-parse");
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    /// `merge_base_returns_common_ancestor_in_real_repo` — given a repo where `branch` and
    /// `master` share an initial commit, `merge_base` must return that commit's SHA.
    #[test]
    fn merge_base_returns_common_ancestor_in_real_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let base_sha = init_repo_with_commit(root);

        // Create a feature branch with one more commit
        std::process::Command::new("git")
            .args(["checkout", "-b", "feature"])
            .current_dir(root)
            .status()
            .expect("git checkout feature");
        std::fs::write(root.join("feature.txt"), "feature").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(root)
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "--quiet", "-m", "feature commit"])
            .current_dir(root)
            .status()
            .unwrap();

        // When — merge_base(feature, master) should return the initial commit SHA
        let got = merge_base(root, "feature", "master")
            .expect("merge_base must succeed for branches sharing a common ancestor");

        assert_eq!(
            got.trim(),
            base_sha.trim(),
            "merge_base(feature, master) must return the shared initial commit SHA"
        );
    }

    /// `rebase_onto_succeeds_for_clean_rebase` — a branch with no conflicting changes can be
    /// cleanly rebased onto a new base.
    #[test]
    fn rebase_onto_succeeds_for_clean_rebase() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        init_repo_with_commit(root);
        // Record the initial commit as old_base
        let old_base_sha = {
            let out = std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(root)
                .output()
                .unwrap();
            String::from_utf8(out.stdout).unwrap().trim().to_string()
        };

        // Create new_base branch (adds base.txt)
        std::process::Command::new("git")
            .args(["checkout", "-b", "new-base"])
            .current_dir(root)
            .status()
            .unwrap();
        std::fs::write(root.join("base.txt"), "base change").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(root)
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "--quiet", "-m", "new base change"])
            .current_dir(root)
            .status()
            .unwrap();

        // Create feature branch off old_base (adds feature.txt — no conflict with base.txt)
        std::process::Command::new("git")
            .args(["checkout", "-b", "feature", &old_base_sha])
            .current_dir(root)
            .status()
            .unwrap();
        std::fs::write(root.join("feature.txt"), "feature change").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(root)
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "--quiet", "-m", "feature change"])
            .current_dir(root)
            .status()
            .unwrap();

        // When — rebase feature onto new-base, replacing old_base as fork point
        let result = rebase_onto(root, "new-base", &old_base_sha, "feature");

        assert!(
            result.is_ok(),
            "rebase_onto must succeed when there are no conflicts; got: {result:?}"
        );
    }

    /// `rebase_onto_returns_err_and_aborts_on_conflict` — when a rebase produces a conflict,
    /// `rebase_onto` must return `Err` and leave the repo in a clean state (no in-progress rebase).
    #[test]
    fn rebase_onto_returns_err_and_aborts_on_conflict() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        init_repo_with_commit(root);

        let old_base_sha = {
            let out = std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(root)
                .output()
                .unwrap();
            String::from_utf8(out.stdout).unwrap().trim().to_string()
        };

        // new-base changes file.txt (same file as feature will change → conflict)
        std::process::Command::new("git")
            .args(["checkout", "-b", "new-base"])
            .current_dir(root)
            .status()
            .unwrap();
        std::fs::write(root.join("file.txt"), "new base version").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(root)
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "--quiet", "-m", "new base version"])
            .current_dir(root)
            .status()
            .unwrap();

        // feature branch also changes file.txt differently → guaranteed conflict
        std::process::Command::new("git")
            .args(["checkout", "-b", "feature", &old_base_sha])
            .current_dir(root)
            .status()
            .unwrap();
        std::fs::write(root.join("file.txt"), "feature version").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(root)
            .status()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "--quiet", "-m", "feature version"])
            .current_dir(root)
            .status()
            .unwrap();

        // When — rebase with conflict
        let result = rebase_onto(root, "new-base", &old_base_sha, "feature");

        // Then — must return Err
        assert!(
            result.is_err(),
            "rebase_onto must return Err on conflict; got Ok"
        );

        // …and no in-progress rebase must remain (git status clean enough to run git commands)
        let _status_out = std::process::Command::new("git")
            .args(["rebase", "--show-current-patch"])
            .current_dir(root)
            .output();
        // If rebase was properly aborted, `git rebase --show-current-patch` returns non-zero or
        // exits immediately. The real check: .git/rebase-merge must not exist.
        let rebase_merge_dir = root.join(".git").join("rebase-merge");
        let rebase_apply_dir = root.join(".git").join("rebase-apply");
        assert!(
            !rebase_merge_dir.exists() && !rebase_apply_dir.exists(),
            "git rebase state directories must be absent after rebase_onto returns Err; \
             rebase-merge: {}, rebase-apply: {}",
            rebase_merge_dir.exists(),
            rebase_apply_dir.exists()
        );
    }
}
