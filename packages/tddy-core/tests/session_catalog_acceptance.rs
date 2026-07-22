//! Acceptance test for the per-session catalog: opening a worktree scans the session's action
//! manifests and its auto-discovered build targets into one queryable SQLite catalog.
//!
//! Feature: docs/ft/coder/session-catalog.md
//! Changeset: docs/dev/1-WIP/session-catalog-db.md

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tddy_core::session_actions::{ActionSummary, DiscoveryQuery};
use tddy_core::session_catalog::{BuildCatalogProvider, BuildTargetCatalogEntry, SessionCatalog};
use tddy_task::TaskRegistry;
use tokio::time::timeout;

/// An in-memory build-target source standing in for `tddy-coder`'s `BUILD.yaml` provider.
struct FakeBuildProvider {
    targets: Vec<BuildTargetCatalogEntry>,
}

impl BuildCatalogProvider for FakeBuildProvider {
    fn discover(&self, _repo_root: &Path) -> Result<Vec<BuildTargetCatalogEntry>, String> {
        Ok(self.targets.clone())
    }
}

fn a_build_target(id: &str, name: &str, package: &str) -> BuildTargetCatalogEntry {
    BuildTargetCatalogEntry {
        id: id.into(),
        name: name.into(),
        package: package.into(),
        source_path: "/repo/BUILD.yaml".into(),
    }
}

fn write_action_manifest(session_dir: &Path, rel_no_ext: &str, id: &str, summary: &str) {
    let path = session_dir
        .join("actions")
        .join(format!("{rel_no_ext}.yaml"));
    fs::create_dir_all(path.parent().unwrap()).expect("mkdir manifest parent");
    fs::write(
        &path,
        format!(
            "version: 1\nid: {id}\nsummary: {summary}\narchitecture: native\ncommand: ['true']\n"
        ),
    )
    .expect("write manifest");
}

trait ActionSummarySliceAssertions {
    fn assert_paths_exactly(&self, expected: &[&str]) -> &Self;
}

impl ActionSummarySliceAssertions for Vec<ActionSummary> {
    fn assert_paths_exactly(&self, expected: &[&str]) -> &Self {
        let actual: Vec<&str> = self.iter().map(|s| s.path.as_str()).collect();
        assert_eq!(actual, expected, "catalog entry paths (ascending) mismatch");
        self
    }
}

/// Opening a worktree populates the catalog with both the session's action manifests and its
/// auto-discovered build targets, and the catalog is then queryable — the full list is sorted
/// ascending by path, and a per-package query returns exactly that package's entries.
#[tokio::test]
async fn opening_a_worktree_scans_and_exposes_a_queryable_targets_catalog() {
    // Given — a session with two action manifests, a repo, and a build-target source with two
    // targets under `packages/foo`.
    let base = tempfile::tempdir().expect("tempdir");
    let session_dir = base.path().join("session");
    let repo_root = base.path().join("repo");
    let tddy_data_dir = base.path().join("data");
    fs::create_dir_all(&session_dir).expect("mkdir session");
    fs::create_dir_all(&repo_root).expect("mkdir repo");
    fs::create_dir_all(&tddy_data_dir).expect("mkdir data");

    write_action_manifest(&session_dir, "packages/foo/build", "foo-build", "Build foo");
    write_action_manifest(&session_dir, "packages/bar/lint", "bar-lint", "Lint bar");

    let provider: Arc<dyn BuildCatalogProvider> = Arc::new(FakeBuildProvider {
        targets: vec![
            a_build_target("packages/foo:binary", "Foo binary", "packages/foo"),
            a_build_target("packages/foo:test", "Foo tests", "packages/foo"),
        ],
    });
    let registry = TaskRegistry::new();

    // When — the worktree-open populate flow runs, then the catalog is queried.
    let catalog = SessionCatalog::open_and_populate(
        &session_dir,
        Some(&repo_root),
        &tddy_data_dir,
        &registry,
        "session-accept",
        Some(provider),
    )
    .await
    .expect("open_and_populate must succeed");

    // 5s ceiling: the first list blocks until the populate task completes; a hang fails loudly.
    let all = timeout(
        Duration::from_secs(5),
        catalog.list(&DiscoveryQuery::default()),
    )
    .await
    .expect("catalog list must not hang past the populate task")
    .expect("catalog list must succeed");
    let foo = timeout(
        Duration::from_secs(5),
        catalog.list_for_package("packages/foo"),
    )
    .await
    .expect("per-package list must not hang")
    .expect("per-package list must succeed");

    // Then — the unified catalog holds both manifests and both targets, sorted ascending by path.
    all.actions.assert_paths_exactly(&[
        "packages/bar/lint",
        "packages/foo/build",
        "packages/foo:binary",
        "packages/foo:test",
    ]);
    assert_eq!(all.total, 4, "total must count every unified entry");

    // And — the per-package query returns exactly the `packages/foo` entries.
    foo.actions.assert_paths_exactly(&[
        "packages/foo/build",
        "packages/foo:binary",
        "packages/foo:test",
    ]);
}
