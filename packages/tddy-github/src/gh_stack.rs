//! `gh stack` membership (`#workqueue` 9/14) — optional: read only where the host's `gh` has the
//! `gh-stack` extension; without it the lineage is complete except for this annotation.

use std::path::Path;
use std::process::Command;

/// The branches of the GitHub stack `branch` belongs to, bottom first; `None` when the extension is
/// missing, the branch is in no stack, or `gh` fails.
///
/// Runs `gh stack view --json` in `repo_dir` (a read of the local stack tracking; nothing is written)
/// off the async runtime's threads.
pub async fn gh_stack_membership(
    gh_bin: &Path,
    repo_dir: &Path,
    branch: &str,
) -> Option<Vec<String>> {
    let (gh_bin, repo_dir) = (gh_bin.to_path_buf(), repo_dir.to_path_buf());
    let output = tokio::task::spawn_blocking(move || {
        Command::new(gh_bin)
            .args(["stack", "view", "--json"])
            .current_dir(repo_dir)
            .output()
    })
    .await
    .ok()?
    .ok()?;
    if !output.status.success() {
        return None;
    }
    members_of(&String::from_utf8_lossy(&output.stdout), branch)
}

/// The stack's branch names out of `gh stack view --json`, when `branch` is one of them.
fn members_of(json: &str, branch: &str) -> Option<Vec<String>> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let branches: Vec<String> = value
        .get("branches")?
        .as_array()?
        .iter()
        .filter_map(|entry| {
            entry
                .as_str()
                .or_else(|| entry.get("name").and_then(|n| n.as_str()))
                .or_else(|| entry.get("branch").and_then(|n| n.as_str()))
                .map(str::to_string)
        })
        .collect();
    branches.iter().any(|b| b == branch).then_some(branches)
}

#[cfg(test)]
mod tests {
    use super::members_of;

    #[test]
    fn a_branch_in_the_listed_stack_gets_the_whole_stack_bottom_first() {
        let json = r#"{"trunk":"master","branches":[{"name":"a"},{"name":"b"}]}"#;
        assert_eq!(
            members_of(json, "b"),
            Some(vec!["a".to_string(), "b".to_string()])
        );
    }

    #[test]
    fn a_branch_outside_the_listed_stack_has_no_membership() {
        assert_eq!(members_of(r#"{"branches":[{"name":"a"}]}"#, "z"), None);
        assert_eq!(members_of("not json", "a"), None);
    }
}
