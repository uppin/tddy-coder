use crate::git_ops::rebase_onto;

use super::init_repo_with_commit;

/// A repository whose own checkout stays on `master`, with `feature` checked out in a linked
/// worktree of its own — a node's worktree — forked from the initial commit, and `new-base` one
/// commit ahead of that fork point.
struct ANodeInItsOwnWorktree {
    pub(crate) _tmp: tempfile::TempDir,
    pub(crate) root: std::path::PathBuf,
    pub(crate) worktree: std::path::PathBuf,
    pub(crate) fork_point: String,
}

fn git_in(dir: &std::path::Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("git {args:?}: {e}"));
    assert!(
        out.status.success(),
        "git {args:?} in {dir:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn a_node_in_its_own_worktree() -> ANodeInItsOwnWorktree {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("repo");
    std::fs::create_dir(&root).unwrap();
    let fork_point = init_repo_with_commit(&root);
    git_in(&root, &["checkout", "-q", "-b", "new-base"]);
    std::fs::write(root.join("base.txt"), "base change").unwrap();
    git_in(&root, &["add", "."]);
    git_in(&root, &["commit", "-q", "-m", "new base change"]);
    git_in(&root, &["checkout", "-q", "master"]);
    let worktree = tmp.path().join("node");
    git_in(
        &root,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feature",
            worktree.to_str().unwrap(),
            &fork_point,
        ],
    );
    std::fs::write(worktree.join("feature.txt"), "feature change").unwrap();
    git_in(&worktree, &["add", "."]);
    git_in(&worktree, &["commit", "-q", "-m", "feature change"]);
    ANodeInItsOwnWorktree {
        _tmp: tmp,
        root,
        worktree,
        fork_point,
    }
}

impl ANodeInItsOwnWorktree {
    pub(crate) fn tip(&self, reference: &str) -> String {
        git_in(&self.root, &["rev-parse", reference])
    }
}

#[test]
fn rebase_onto_runs_in_the_worktree_that_holds_the_branch() {
    // Given `feature` checked out in a node's own worktree, the repository's checkout on master
    let node = a_node_in_its_own_worktree();

    // When it is rebased onto new-base, from the repository
    let result = rebase_onto(&node.root, "new-base", &node.fork_point, "feature");

    // Then it succeeds, the node's worktree holds the rebased branch with new-base's work on
    // disk, and the repository's own checkout is still on master
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(
        git_in(&node.worktree, &["rev-parse", "HEAD~1"]),
        node.tip("new-base")
    );
    assert!(node.worktree.join("base.txt").exists());
    assert_eq!(
        git_in(&node.root, &["rev-parse", "--abbrev-ref", "HEAD"]),
        "master"
    );
}

#[test]
fn rebase_onto_refuses_a_dirty_node_worktree_and_leaves_it_untouched() {
    // Given `feature` in a node's own worktree with an uncommitted edit
    let node = a_node_in_its_own_worktree();
    std::fs::write(node.worktree.join("feature.txt"), "not committed").unwrap();
    let before = node.tip("feature");

    // When it is rebased onto new-base
    let result = rebase_onto(&node.root, "new-base", &node.fork_point, "feature");

    // Then it is refused naming the uncommitted file, and neither the branch nor the edit moved
    let refusal = result
        .expect_err("a dirty worktree is not rebased")
        .to_string();
    assert!(refusal.contains("feature.txt"), "{refusal}");
    assert_eq!(node.tip("feature"), before);
    assert_eq!(
        std::fs::read_to_string(node.worktree.join("feature.txt")).unwrap(),
        "not committed"
    );
}
