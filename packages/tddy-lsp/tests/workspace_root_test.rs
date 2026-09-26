//! Which directory a language server is rooted at, for a target somewhere inside a tree.
//!
//! The rule is `cargo locate-project --workspace`'s, bounded by the repository: the nearest
//! `Cargo.toml` declaring `[workspace]`, else the nearest manifest (a package outside any workspace
//! is its own root), never looking past the directory holding `.git`.
//!
//! The layout every test stands in is the one that went wrong: a main checkout that is itself a
//! cargo workspace, with a second checkout of the same repository nested under `.worktrees/`. A
//! warm index rooted at the enclosing checkout answers — and applies — against the wrong tree.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use tddy_lsp::registry::workspace_root_for;

const A_WORKSPACE_MANIFEST: &str = "[workspace]\nresolver = \"2\"\nmembers = [\"packages/*\"]\n";
const A_PACKAGE_MANIFEST: &str = "[package]\nname = \"member\"\nversion = \"0.1.0\"\n";

/// A directory tree on disk, removed when the test ends.
///
/// Under the system temporary directory rather than cargo's `target/tmp`, because the latter sits
/// inside this repository's own workspace — exactly the kind of enclosing tree under test.
struct ATree {
    root: PathBuf,
}

fn a_tree() -> ATree {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "tddy-lsp-workspace-root-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).expect("the tree's root is created");
    ATree {
        root: root.canonicalize().expect("the tree's root resolves"),
    }
}

/// The main checkout: a workspace with one member, and a `.git` directory.
fn a_main_checkout_workspace() -> ATree {
    a_tree()
        .with_file("Cargo.toml", A_WORKSPACE_MANIFEST)
        .with_file("packages/member/Cargo.toml", A_PACKAGE_MANIFEST)
        .with_dir(".git")
}

impl ATree {
    fn with_file(self, relative: &str, text: &str) -> Self {
        let path = self.root.join(relative);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("directories");
        std::fs::write(path, text).expect("the file is written");
        self
    }

    fn with_dir(self, relative: &str) -> Self {
        std::fs::create_dir_all(self.root.join(relative)).expect("the directory is created");
        self
    }

    /// A linked worktree at `relative`: its own workspace, a member, and a `.git` *file*.
    fn with_nested_worktree_workspace(self, relative: &str) -> Self {
        self.with_file(&format!("{relative}/Cargo.toml"), A_WORKSPACE_MANIFEST)
            .with_file(
                &format!("{relative}/packages/member/Cargo.toml"),
                A_PACKAGE_MANIFEST,
            )
            .with_file(
                &format!("{relative}/.git"),
                "gitdir: ../../.git/worktrees/x\n",
            )
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }
}

impl Drop for ATree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn root_for(target: &Path) -> PathBuf {
    workspace_root_for(target).expect("every manifest in the tree is readable")
}

#[test]
fn roots_a_nested_worktree_at_its_own_workspace_not_the_enclosing_checkout() {
    // Given a checkout nested inside a checkout, each its own cargo workspace
    let tree = a_main_checkout_workspace().with_nested_worktree_workspace(".worktrees/x");

    // When the nested checkout's root is resolved
    let root = root_for(&tree.path(".worktrees/x"));

    // Then it is the nested checkout
    assert_eq!(root, tree.path(".worktrees/x"));
}

#[test]
fn lifts_a_member_of_a_nested_worktree_to_that_worktree_workspace() {
    // Given a checkout nested inside a checkout, each its own cargo workspace
    let tree = a_main_checkout_workspace().with_nested_worktree_workspace(".worktrees/x");

    // When a member crate of the nested checkout is resolved
    let root = root_for(&tree.path(".worktrees/x/packages/member"));

    // Then it is lifted to the nested workspace, and no further
    assert_eq!(root, tree.path(".worktrees/x"));
}

#[test]
fn lifts_a_member_crate_to_the_workspace_that_holds_it() {
    // Given a workspace with a member crate
    let tree = a_main_checkout_workspace();

    // When the member's directory is resolved
    let root = root_for(&tree.path("packages/member"));

    // Then two targets in one workspace share its root, and so a server
    assert_eq!(root, tree.root);
}

#[test]
fn does_not_cross_a_git_boundary_into_an_enclosing_workspace() {
    // Given a nested checkout whose root manifest is a lone package, not a workspace
    let tree = a_main_checkout_workspace()
        .with_file(".worktrees/y/Cargo.toml", A_PACKAGE_MANIFEST)
        .with_file(".worktrees/y/.git", "gitdir: ../../.git/worktrees/y\n");

    // When the nested checkout is resolved
    let root = root_for(&tree.path(".worktrees/y"));

    // Then its own repository is the limit — cargo would reach the outer workspace, and then refuse
    // this package as a member it does not list
    assert_eq!(root, tree.path(".worktrees/y"));
}

#[test]
fn roots_a_directory_holding_no_manifest_at_itself() {
    // Given a repository with no cargo manifest anywhere
    let tree = a_tree().with_dir(".git").with_dir("src");

    // When a directory inside it is resolved
    let root = root_for(&tree.path("src"));

    // Then the directory named is the root: there is no workspace to lift it to
    assert_eq!(root, tree.path("src"));
}
