//! Resolve `origin/...` chain integration base from a parent workflow session (session chaining PRD).
//!
//! Callers integrate with worktree bootstrap via
//! [`integrate_chain_base_into_session_worktree_bootstrap`] and
//! [`crate::setup_worktree_for_session_with_optional_chain_base`].

use std::path::Path;

use crate::changeset::read_changeset;
use crate::session_lifecycle::unified_session_dir_path;
use crate::worktree::validate_chain_pr_integration_base_ref;
use crate::WorkflowError;

const NO_BRANCH_CHAIN_MSG: &str = "PRD acceptance copy: parent session must record a branch before chaining (operators: push or persist branch name)";

const NO_REPO_PATH_CHAIN_MSG: &str = "Parent session changeset must record repo_path for repository alignment when chaining (operators: persist the workspace repository path in changeset.yaml)";

/// Resolve `origin/<parent-branch>` from the parent session's persisted `changeset.yaml`.
///
/// Requires **`repo_path`** on the parent changeset whenever a branch is present so the child
/// repository can be verified. Validates the ref, canonicalizes **`repo_path`** against
/// **`child_project_repo`**, and returns the canonical remote-tracking ref string.
pub fn resolve_chain_integration_base_ref_from_parent_session(
    sessions_root: &Path,
    parent_session_id: &str,
    child_project_repo: &Path,
) -> Result<String, WorkflowError> {
    log::info!(
        "resolve_chain_integration_base_ref_from_parent_session: sessions_root={} parent_session_id={} child_repo={}",
        sessions_root.display(),
        parent_session_id,
        child_project_repo.display()
    );
    let parent_dir = unified_session_dir_path(sessions_root, parent_session_id);
    if !parent_dir.is_dir() {
        log::debug!(
            "resolve_chain_integration_base_ref_from_parent_session: parent_dir missing {}",
            parent_dir.display()
        );
        return Err(WorkflowError::SessionMissing(format!(
            "parent session not found under sessions tree: {}",
            parent_dir.display()
        )));
    }

    let cs = read_changeset(&parent_dir)?;
    log::debug!(
        "resolve_chain_integration_base_ref_from_parent_session: read parent changeset branch={:?} branch_suggestion={:?} repo_path={:?}",
        cs.branch,
        cs.branch_suggestion,
        cs.repo_path
    );
    let branch_name = cs.branch.clone().or(cs.branch_suggestion.clone());
    let Some(branch_path) = branch_name else {
        log::info!(
            "resolve_chain_integration_base_ref_from_parent_session: parent {} has no branch for chaining",
            parent_session_id
        );
        return Err(WorkflowError::ChangesetInvalid(NO_BRANCH_CHAIN_MSG.into()));
    };

    if cs.repo_path.is_none() {
        log::info!(
            "resolve_chain_integration_base_ref_from_parent_session: parent {} has branch but no repo_path for chaining",
            parent_session_id
        );
        return Err(WorkflowError::ChangesetInvalid(
            NO_REPO_PATH_CHAIN_MSG.into(),
        ));
    }

    let trimmed = branch_path.trim().trim_start_matches('/');
    if trimmed.is_empty() {
        return Err(WorkflowError::ChangesetInvalid(
            "parent session branch name is empty".into(),
        ));
    }

    let origin_ref = format!("origin/{trimmed}");
    validate_chain_pr_integration_base_ref(&origin_ref).map_err(WorkflowError::PlanDirInvalid)?;

    if let Some(ref parent_repo) = cs.repo_path {
        let parent_path = Path::new(parent_repo);
        let parent_canon =
            std::fs::canonicalize(parent_path).unwrap_or_else(|_| parent_path.to_path_buf());
        let child_canon = std::fs::canonicalize(child_project_repo)
            .unwrap_or_else(|_| child_project_repo.to_path_buf());
        if parent_canon != child_canon {
            log::info!(
                "resolve_chain_integration_base_ref_from_parent_session: repo mismatch parent={} child={}",
                parent_canon.display(),
                child_canon.display()
            );
            return Err(WorkflowError::PlanDirInvalid(format!(
                "parent session repository ({}) does not match selected project repository ({})",
                parent_canon.display(),
                child_canon.display()
            )));
        }
    }

    log::info!(
        "resolve_chain_integration_base_ref_from_parent_session: ok origin_ref={origin_ref}"
    );
    Ok(origin_ref)
}

/// Integrates a resolved chain base ref into session worktree bootstrap by delegating to
/// [`crate::setup_worktree_for_session_with_optional_chain_base`].
///
/// `sessions_root` and `parent_session_id` are retained for logging and future validation hooks.
pub fn integrate_chain_base_into_session_worktree_bootstrap(
    sessions_root: &Path,
    parent_session_id: &str,
    child_session_dir: &Path,
    child_project_repo: &Path,
    resolved_origin_ref: &str,
) -> Result<(), WorkflowError> {
    log::info!(
        "integrate_chain_base_into_session_worktree_bootstrap: sessions_root={} parent_session_id={} child_session_dir={} child_repo={} resolved_ref={}",
        sessions_root.display(),
        parent_session_id,
        child_session_dir.display(),
        child_project_repo.display(),
        resolved_origin_ref
    );
    validate_chain_pr_integration_base_ref(resolved_origin_ref)
        .map_err(WorkflowError::PlanDirInvalid)?;

    crate::setup_worktree_for_session_with_optional_chain_base(
        child_project_repo,
        child_session_dir,
        Some(resolved_origin_ref),
    )
    .map_err(WorkflowError::PlanDirInvalid)?;

    log::debug!(
        "integrate_chain_base_into_session_worktree_bootstrap: worktree setup complete for child_session_dir={}",
        child_session_dir.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::changeset::{write_changeset, Changeset, ChangesetState};
    use crate::session_lifecycle::unified_session_dir_path;
    use crate::workflow::ids::WorkflowState;
    use std::fs;
    use std::process::Command;

    fn tmp_sessions_parent_with_branch(
        label: &str,
        branch: Option<&str>,
    ) -> (std::path::PathBuf, String, std::path::PathBuf) {
        let base = std::env::temp_dir().join(format!(
            "tddy-session-chain-unit-{}-{}-{}",
            label.replace('/', "_"),
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&base);
        let repo = base.join("repo");
        fs::create_dir_all(&repo).unwrap();
        let repo_canon = repo.canonicalize().unwrap();
        let parent_id = "018faaaa-bbbb-7ccc-ddee-00000000aa01";
        let sessions_home = base.join("sessions-home");
        let parent_dir = unified_session_dir_path(&sessions_home, parent_id);
        fs::create_dir_all(&parent_dir).unwrap();
        let mut cs = Changeset {
            name: Some("unit-parent".into()),
            repo_path: Some(repo_canon.to_string_lossy().into_owned()),
            state: ChangesetState {
                current: WorkflowState::new("Planned"),
                ..Changeset::default().state
            },
            ..Changeset::default()
        };
        if let Some(b) = branch {
            cs.branch_suggestion = Some(b.to_string());
        }
        write_changeset(&parent_dir, &cs).unwrap();
        (sessions_home, parent_id.to_string(), repo_canon)
    }

    fn git(repo: &Path, args: &[&str]) {
        let o = Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&o.stderr)
        );
    }

    fn init_repo_with_origin_feature_x(repo: &Path) {
        fs::create_dir_all(repo).unwrap();
        git(repo, &["init"]);
        git(repo, &["config", "user.email", "test@test.com"]);
        git(repo, &["config", "user.name", "Test"]);
        fs::write(repo.join("README"), "initial").unwrap();
        git(repo, &["add", "README"]);
        git(repo, &["commit", "-m", "initial"]);
        git(repo, &["branch", "-M", "master"]);
        git(repo, &["remote", "add", "origin", repo.to_str().unwrap()]);
        git(repo, &["push", "-u", "origin", "master"]);
        git(repo, &["checkout", "-b", "feature/x"]);
        fs::write(repo.join("feat"), "x").unwrap();
        git(repo, &["add", "feat"]);
        git(repo, &["commit", "-m", "feat"]);
        git(repo, &["push", "-u", "origin", "feature/x"]);
        git(repo, &["checkout", "master"]);
    }

    /// Granular RED: success path must return `Ok` once the PlanDirInvalid gate is removed.
    #[test]
    fn unit_resolve_chain_returns_ok_when_parent_has_branch_and_matching_repo() {
        let (sessions_home, parent_id, repo) =
            tmp_sessions_parent_with_branch("ok", Some("feature/u1"));
        let got = resolve_chain_integration_base_ref_from_parent_session(
            &sessions_home,
            &parent_id,
            &repo,
        );
        let _ = fs::remove_dir_all(sessions_home.parent().unwrap());
        assert_eq!(
            got.expect("resolver must return Ok(origin/...) for valid parent branch + repo"),
            "origin/feature/u1"
        );
    }

    /// Granular RED: operator-facing copy for missing branch (matches integration acceptance).
    #[test]
    fn unit_resolve_chain_no_branch_includes_prd_acceptance_message() {
        let (sessions_home, parent_id, repo) = tmp_sessions_parent_with_branch("no-branch", None);
        let err = resolve_chain_integration_base_ref_from_parent_session(
            &sessions_home,
            &parent_id,
            &repo,
        )
        .expect_err("missing branch must error");
        let msg = err.to_string();
        let _ = fs::remove_dir_all(sessions_home.parent().unwrap());
        assert!(
            msg.contains(
                "PRD acceptance copy: parent session must record a branch before chaining (operators: push or persist branch name)"
            ),
            "unexpected message: {msg}"
        );
    }

    /// Worktree bootstrap integration delegates to `setup_worktree_for_session_with_optional_chain_base`.
    #[test]
    fn unit_integrate_chain_bootstrap_skeleton_succeeds() {
        let base =
            std::env::temp_dir().join(format!("tddy-chain-integ-skel-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let repo = base.join("repo");
        init_repo_with_origin_feature_x(&repo);
        let repo_canon = repo.canonicalize().unwrap();

        let child_session_dir = base.join("sessions").join("child-session-001");
        fs::create_dir_all(&child_session_dir).unwrap();
        let cs = Changeset {
            name: Some("chain-child".into()),
            worktree_suggestion: Some("chain-child-wt".into()),
            branch_suggestion: Some("feature/child-from-chain".into()),
            ..Changeset::default()
        };
        write_changeset(&child_session_dir, &cs).unwrap();

        integrate_chain_base_into_session_worktree_bootstrap(
            base.join("sessions-home").as_path(),
            "parent-sid",
            &child_session_dir,
            &repo_canon,
            "origin/feature/x",
        )
        .expect("integrate_chain_base_into_session_worktree_bootstrap must succeed with valid git fixture");

        let _ = fs::remove_dir_all(&base);
    }
}
