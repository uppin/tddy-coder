use std::path::Path;

use tddy_core::changeset::PrInternalStatus;

use std::path::PathBuf;

use super::journal_path;

use super::write_journal;

use super::node_of;

use serde_json::{json, Value};

use super::GitEnv;

pub(crate) fn merge_node(env: &GitEnv, node_id: &str) -> Result<Value, String> {
    use tddy_pr_stack::op_journal::{MergePhase, StackOpJournal};
    let stack = env.stack()?;
    let node = node_of(&stack, node_id)?.clone();
    let gh = env.github_client()?;
    let number = env.pull_number(&stack, &node, &*gh)?;
    let dependents: Vec<String> = stack
        .nodes
        .iter()
        .filter(|candidate| candidate.parents.iter().any(|parent| parent == node_id))
        .map(|candidate| candidate.node_id.clone())
        .collect();
    let mut journal = StackOpJournal {
        op_id: uuid::Uuid::new_v4().to_string(),
        merged_node_id: node_id.to_string(),
        merge_phase: MergePhase::Planned,
        dependents,
    };
    write_journal(env, &journal)?;
    let sha = gh.merge_pr(number).map_err(|e| format!("merge pr: {e}"))?;
    journal.merge_phase = MergePhase::PrMerged { sha: sha.clone() };
    write_journal(env, &journal)?;
    env.set_phase(node_id, "merged")?;
    let _ = std::fs::remove_file(journal_path(env));
    Ok(json!({ "merged": true, "sha": sha }))
}

pub(crate) fn close_node(env: &GitEnv, node_id: &str) -> Result<Value, String> {
    let stack = env.stack()?;
    let node = node_of(&stack, node_id)?.clone();
    let gh = env.github_client()?;
    let number = env.pull_number(&stack, &node, &*gh)?;
    gh.close_pr(number).map_err(|e| format!("close pr: {e}"))?;
    env.set_phase(node_id, "closed")?;
    Ok(json!({ "closed": true }))
}

pub(crate) fn repoint_node(
    env: &GitEnv,
    node_id: &str,
    new_base: Option<&str>,
) -> Result<Value, String> {
    let stack = env.stack()?;
    let node = node_of(&stack, node_id)?.clone();
    let project = env.project()?;
    let repo_root = PathBuf::from(&project.main_repo_path);
    let remote = env.remote_name(&project, &repo_root);
    let default_branch = env.default_branch(&project);
    let (base_branch, base_ref) = match new_base.map(str::trim).filter(|base| !base.is_empty()) {
        Some(named) => {
            let branch = named
                .strip_prefix(&format!("{remote}/"))
                .or_else(|| named.strip_prefix("origin/"))
                .unwrap_or(named)
                .to_string();
            (branch.clone(), branch)
        }
        None => env.effective_base(&stack, node_id, &remote, &default_branch),
    };
    if node.branch.is_none() {
        return Ok(json!({ "node_id": node_id, "new_base": base_branch }));
    }
    env.realign(&node, &base_branch, &base_ref)?;
    Ok(json!({ "node_id": node_id, "new_base": base_branch }))
}

pub(crate) fn set_parents(
    env: &GitEnv,
    node_id: &str,
    parents: &[String],
) -> Result<Value, String> {
    let stack = env.stack()?;
    let node = node_of(&stack, node_id)?.clone();
    // Validate before anything is written, as the plan writers do.
    let mut seen: Vec<&String> = Vec::new();
    for parent in parents {
        if parent == node_id {
            return Err(format!("node '{parent}' cannot be its own parent"));
        }
        if stack.node(parent).is_none() {
            return Err(format!("dangling parent ref: {parent}"));
        }
        if seen.contains(&parent) {
            return Err(format!("parent '{parent}' is named more than once"));
        }
        seen.push(parent);
    }
    let mut candidate = stack.clone();
    if let Some(candidate_node) = candidate.nodes.iter_mut().find(|n| n.node_id == node_id) {
        candidate_node.parents = parents.to_vec();
    }
    candidate
        .topo_order()
        .map_err(|e| format!("cycle detected: {e}"))?;
    // Refuse a dirty or running node before the plan moves.
    if let Some(branch) = node.branch.as_deref() {
        let project = env.project()?;
        let repo_root = PathBuf::from(&project.main_repo_path);
        let _ = branch;
        let worktree = env.worktree_for(&node, &repo_root)?;
        env.refuse_dirty_or_running(&node, &worktree)?;
    }
    env.store
        .update(&env.project_id, &env.stack_id, |stack| {
            if let Some(node) = stack.nodes.iter_mut().find(|n| n.node_id == node_id) {
                node.parents = parents.to_vec();
            }
        })
        .map_err(|e| e.to_string())?;
    if node.branch.is_some() {
        let updated = env.stack()?;
        let updated_node = node_of(&updated, node_id)?.clone();
        let project = env.project()?;
        let repo_root = PathBuf::from(&project.main_repo_path);
        let remote = env.remote_name(&project, &repo_root);
        let default_branch = env.default_branch(&project);
        let (base_branch, base_ref) =
            env.effective_base(&updated, node_id, &remote, &default_branch);
        env.realign(&updated_node, &base_branch, &base_ref)?;
    }
    Ok(json!({ "node_id": node_id, "parents": parents }))
}

pub(crate) fn resolve_conflicts(env: &GitEnv, node_id: &str) -> Result<Value, String> {
    let stack = env.stack()?;
    let node = node_of(&stack, node_id)?.clone();
    let branch = node.branch.clone().ok_or_else(|| {
        format!("node '{node_id}' owns no branch yet, so it has no conflicts to find")
    })?;
    let project = env.project()?;
    let repo_root = PathBuf::from(&project.main_repo_path);
    let remote = env.remote_name(&project, &repo_root);
    let default_branch = env.default_branch(&project);
    let (_, base_ref) = env.effective_base(&stack, node_id, &remote, &default_branch);
    let worktree = env.worktree_for(&node, &repo_root)?;
    let conflicts = merge_tree_conflicts(&worktree, &branch, &base_ref)?;
    env.store
        .update(&env.project_id, &env.stack_id, |stack| {
            if let Some(node) = stack.nodes.iter_mut().find(|n| n.node_id == node_id) {
                if conflicts.is_empty() {
                    if node
                        .internal_status
                        .as_ref()
                        .is_some_and(|status| status.kind == "has-conflicts")
                    {
                        node.internal_status = None;
                    }
                } else {
                    node.internal_status = Some(PrInternalStatus {
                        kind: "has-conflicts".to_string(),
                        note: None,
                        source: "override".to_string(),
                    });
                }
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(json!({ "conflicts": conflicts }))
}

/// The paths `branch` and `base_ref` conflict on, probed with `git merge-tree --write-tree`:
/// no merge is started, so the worktree's HEAD, index and files are left exactly as they were.
fn merge_tree_conflicts(
    worktree: &Path,
    branch: &str,
    base_ref: &str,
) -> Result<Vec<String>, String> {
    let output = std::process::Command::new("git")
        .args([
            "merge-tree",
            "--write-tree",
            "--name-only",
            branch,
            base_ref,
        ])
        .current_dir(worktree)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|e| format!("git merge-tree in {}: {e}", worktree.display()))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if output.status.success() {
        return Ok(Vec::new());
    }
    if output.status.code() != Some(1) {
        return Err(format!(
            "git merge-tree {branch} {base_ref} in {}: {}",
            worktree.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let mut paths: Vec<String> = stdout
        .lines()
        .skip(1)
        .take_while(|line| !line.trim().is_empty())
        .map(|line| line.trim().trim_matches('"').to_string())
        .filter(|line| !line.is_empty())
        .collect();
    paths.sort();
    paths.dedup();
    Ok(paths)
}
