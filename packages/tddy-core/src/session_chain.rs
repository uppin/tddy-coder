//! Resolve `origin/...` chain integration base from a parent workflow session (session chaining PRD).
//!
//! Callers integrate with worktree bootstrap via
//! [`integrate_chain_base_into_session_worktree_bootstrap`] and
//! [`crate::setup_worktree_for_session_with_optional_chain_base`].

use std::path::{Path, PathBuf};

use crate::changeset::read_changeset;
use crate::session_lifecycle::unified_session_dir_path;
use crate::worktree::{detect_default_remote_name, validate_chain_pr_integration_base_ref};
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

    // The remote is the child project's default — detected from the main worktree's upstream, with
    // `origin` only as the last-resort fallback. The parent's persisted branch name is local (no
    // remote prefix), so the remote-tracking ref the child bases off is `<remote>/<trimmed>`.
    let remote =
        detect_default_remote_name(child_project_repo).unwrap_or_else(|| "origin".to_string());
    let origin_ref = format!("{remote}/{trimmed}");
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

/// True when the parent session is a pr-stack orchestrator — a planning session carrying a
/// planned `stack` (or the `pr-stack` recipe) and therefore no git branch of its own.
pub fn parent_is_pr_stack_orchestrator(sessions_base: &Path, parent_session_id: &str) -> bool {
    let parent_dir = unified_session_dir_path(sessions_base, parent_session_id);
    match read_changeset(&parent_dir) {
        Ok(cs) => cs.recipe.as_deref() == Some("pr-stack") || cs.stack.is_some(),
        Err(_) => false,
    }
}

/// Locate the planned stack node a child spawn belongs to: the node in `stack_parent`'s stack
/// that owns the branch the child is about to create.
pub fn pr_stack_node_for_spawn(
    sessions_base: &Path,
    stack_parent: &str,
    new_branch_name: &str,
) -> Option<(PathBuf, crate::changeset::Stack, String)> {
    if !parent_is_pr_stack_orchestrator(sessions_base, stack_parent) {
        return None;
    }
    let branch = new_branch_name.trim();
    if branch.is_empty() {
        return None;
    }
    let parent_dir = unified_session_dir_path(sessions_base, stack_parent);
    let stack =
        crate::changeset::read_stack_with_resolved_branches(sessions_base, stack_parent).ok()??;
    let node_id = stack
        .nodes
        .iter()
        .find(|n| n.branch.as_deref() == Some(branch))
        .or_else(|| {
            stack
                .nodes
                .iter()
                .find(|n| n.branch.is_none() && n.branch_suggestion.as_deref() == Some(branch))
        })
        .map(|n| n.node_id.clone())?;
    Some((parent_dir, stack, node_id))
}

/// Resolve the integration base ref for a session spawned with an optional `stack_parent`.
///
/// A pr-stack orchestrator parent bases the child off the planned node's effective base via
/// [`crate::changeset::Stack::base_ref_for_spawn`]. A regular code-session parent bases off
/// `origin/<parent-branch>`. When there is no `stack_parent`, returns `Ok(None)` so the caller
/// uses the default integration base.
pub fn resolve_chain_base_ref(
    sessions_base: &Path,
    stack_parent: Option<&str>,
    repo_root: &Path,
    new_branch_name: &str,
) -> Result<Option<String>, String> {
    let Some(sp) = stack_parent else {
        return Ok(None);
    };
    if parent_is_pr_stack_orchestrator(sessions_base, sp) {
        let Some((_, stack, node_id)) = pr_stack_node_for_spawn(sessions_base, sp, new_branch_name)
        else {
            return Ok(None);
        };
        let default_base = crate::resolve_default_integration_base_ref(repo_root)
            .map_err(|e| format!("could not resolve default branch for pr-stack node: {e}"))?;
        let base = stack
            .base_ref_for_spawn(&node_id, &default_base)
            .map_err(|e| e.to_string())?;
        return Ok(Some(base));
    }
    resolve_chain_integration_base_ref_from_parent_session(sessions_base, sp, repo_root)
        .map(Some)
        .map_err(|e| format!("could not resolve stack parent branch: {e}"))
}

/// Which host resolves a spawn's PR-stack parent.
///
/// The variants are the three answers to one question — whose disk holds the parent's
/// `changeset.yaml` — and nothing else. What a caller does with each is its own business: the
/// daemon serves the first two itself and forwards the third.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StackParentRoute {
    /// There is no stack parent, so nothing is resolved and no host is asked.
    NoParent,
    /// This host's own sessions tree holds the parent.
    Local,
    /// Another daemon holds it; only that daemon can answer.
    OwnedByPeer { daemon_instance_id: String },
}

/// Decide which host resolves `stack_parent`, given the daemon that owns it.
///
/// Every resolver behind a `stack_parent` reads
/// `unified_session_dir_path(sessions_base, parent_session_id)` on the **local** filesystem, so a
/// parent that lives on another daemon is not "not yet planned" but simply absent — and the spawn
/// is refused over a session that exists perfectly well, one host over. The owning host has to be
/// named for the question to reach it, and this is the rule that reads that name.
///
/// A blank owning host is *not* an error and does not mean "nowhere": it is what every caller
/// predating the field sends, and what every single-host deployment sends, so it means this host —
/// today's behaviour, unchanged. A host id equal to this one's means the same thing, which matters
/// because a daemon that forwarded to itself would deadlock on its own RPC rather than answer.
///
/// A blank `stack_parent` is [`StackParentRoute::NoParent`] whatever host is named: an empty id
/// names no session, so there is nothing for any host to resolve.
///
/// Deliberately a function over three strings — the rule about which host owns a parent is worth
/// asserting without a live daemon, a common room or a LiveKit connection behind it.
pub fn classify_stack_parent_route(
    local_daemon_instance_id: &str,
    stack_parent: Option<&str>,
    stack_parent_daemon_instance_id: &str,
) -> StackParentRoute {
    let names_a_parent = stack_parent.is_some_and(|p| !p.trim().is_empty());
    if !names_a_parent {
        return StackParentRoute::NoParent;
    }
    let owner = stack_parent_daemon_instance_id.trim();
    if owner.is_empty() || owner == local_daemon_instance_id.trim() {
        return StackParentRoute::Local;
    }
    StackParentRoute::OwnedByPeer {
        daemon_instance_id: owner.to_string(),
    }
}

/// Select the worktree base ref for a session spawn: an explicit, non-empty operator-chosen
/// `selected_integration_base_ref` (sent from the web Start-session dialog's "Base branch"
/// selector) wins over the stack-parent-resolved chain base; an empty/whitespace override falls
/// through to the stack-parent resolution (today's behavior). The returned value, when `Some`,
/// is the ref handed to `setup_worktree_for_session_with_optional_chain_base`.
pub fn select_worktree_base_ref(
    explicit_selected_integration_base_ref: &str,
    chain_base_ref: Option<String>,
) -> Option<String> {
    let trimmed = explicit_selected_integration_base_ref.trim();
    if !trimmed.is_empty() {
        Some(trimmed.to_string())
    } else {
        chain_base_ref
    }
}

/// Resolve the chain integration base for session spawn, honoring runtime `stack_parent` over a
/// persisted `worktree_integration_base_ref` when both are present.
pub fn resolve_chain_base_for_session_spawn(
    sessions_base: &Path,
    stack_parent: Option<&str>,
    repo_root: &Path,
    new_branch_name: &str,
    persisted_worktree_integration_base_ref: Option<&str>,
) -> Result<Option<String>, String> {
    if let Some(sp) = stack_parent {
        return resolve_chain_base_ref(sessions_base, Some(sp), repo_root, new_branch_name);
    }
    if let Some(persisted) = persisted_worktree_integration_base_ref {
        return Ok(Some(persisted.to_string()));
    }
    Ok(None)
}

/// Transport-agnostic helper: bootstrap a child worktree using a parent session's branch
/// as the integration base, OR an explicitly supplied base ref.
///
/// Sets `worktree_integration_base_ref` + `effective_worktree_integration_base_ref` on
/// the child's `changeset.yaml` via `integrate_chain_base_into_session_worktree_bootstrap`.
///
/// `explicit_base`: when Some, use this ref directly (skipping parent-branch resolution).
/// When None, call `resolve_chain_integration_base_ref_from_parent_session` first.
// TODO: implement by lifting body of telegram_session_control::merge_chain_integration_base_with_explicit_operator_overrides
pub fn spawn_chain_child_worktree(
    _sessions_root: &Path,
    _parent_session_id: &str,
    _child_session_dir: &Path,
    _child_project_repo: &Path,
    _explicit_base: Option<&str>,
) -> Result<String, WorkflowError> {
    unimplemented!("spawn_chain_child_worktree: not yet implemented")
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
        // Given
        let (sessions_home, parent_id, repo) =
            tmp_sessions_parent_with_branch("ok", Some("feature/u1"));

        // When
        let got = resolve_chain_integration_base_ref_from_parent_session(
            &sessions_home,
            &parent_id,
            &repo,
        );
        let _ = fs::remove_dir_all(sessions_home.parent().unwrap());

        // Then
        assert_eq!(
            got.expect("resolver must return Ok(origin/...) for valid parent branch + repo"),
            "origin/feature/u1"
        );
    }

    /// Granular RED: operator-facing copy for missing branch (matches integration acceptance).
    #[test]
    fn unit_resolve_chain_no_branch_includes_prd_acceptance_message() {
        // Given
        let (sessions_home, parent_id, repo) = tmp_sessions_parent_with_branch("no-branch", None);

        // When
        let err = resolve_chain_integration_base_ref_from_parent_session(
            &sessions_home,
            &parent_id,
            &repo,
        )
        .expect_err("missing branch must error");
        let msg = err.to_string();
        let _ = fs::remove_dir_all(sessions_home.parent().unwrap());

        // Then
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
        // Given
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

        // When / Then
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
