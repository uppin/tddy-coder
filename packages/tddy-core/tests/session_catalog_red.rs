//! Unit / integration tests for the per-session catalog store, its package index, its query
//! semantics, and the populate task + block-until-populate behaviour.
//!
//! Feature: docs/ft/coder/session-catalog.md
//! Changeset: docs/dev/1-WIP/session-catalog-db.md

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tddy_core::session_actions::{ActionListResult, ActionSummary, DiscoveryQuery};
use tddy_core::session_catalog::read::catalog_db_path;
use tddy_core::session_catalog::{
    BuildCatalogProvider, BuildTargetCatalogEntry, CatalogEntry, CatalogEntryKind, SessionCatalog,
};
use tddy_task::{TaskRegistry, TaskStatus};
use tempfile::TempDir;
use tokio::time::timeout;

// ─── Builders ───────────────────────────────────────────────────────────────

/// Test-side package derivation (mirrors the fixture's intent; the production `project_package`
/// is exercised separately in `entry.rs`).
fn package_of_target(id: &str) -> String {
    id.split(':').next().unwrap_or("").to_string()
}

fn package_of_manifest(path: &str) -> String {
    match path.rfind('/') {
        Some(i) => path[..i].to_string(),
        None => String::new(),
    }
}

struct CatalogEntryBuilder(CatalogEntry);

/// Start a catalog entry with valid defaults (a top-level action manifest).
fn a_catalog_entry() -> CatalogEntryBuilder {
    CatalogEntryBuilder(CatalogEntry {
        kind: CatalogEntryKind::ActionManifest,
        id: "action-1".into(),
        package: String::new(),
        summary: "Summary".into(),
        path: "action-1".into(),
        has_input_schema: false,
        has_output_schema: false,
        source_path: None,
    })
}

impl CatalogEntryBuilder {
    fn build_target(mut self, id: &str) -> Self {
        self.0.kind = CatalogEntryKind::BuildTarget;
        self.0.id = id.into();
        self.0.path = id.into();
        self.0.package = package_of_target(id);
        self
    }

    fn action_manifest(mut self, path: &str) -> Self {
        self.0.kind = CatalogEntryKind::ActionManifest;
        self.0.id = path.into();
        self.0.path = path.into();
        self.0.package = package_of_manifest(path);
        self
    }

    fn with_id(mut self, id: &str) -> Self {
        self.0.id = id.into();
        self
    }

    fn with_summary(mut self, summary: &str) -> Self {
        self.0.summary = summary.into();
        self
    }

    fn build(self) -> CatalogEntry {
        self.0
    }
}

// ─── Temp catalog harness (store-level, no populate task) ─────────────────────

struct TempCatalog {
    _base: TempDir,
    session_dir: PathBuf,
    catalog: Arc<SessionCatalog>,
}

async fn a_temp_catalog() -> TempCatalog {
    let base = tempfile::tempdir().expect("tempdir");
    let session_dir = base.path().join("sessions").join("s-red");
    fs::create_dir_all(&session_dir).expect("mkdir session");
    let catalog = SessionCatalog::open(&catalog_db_path(&session_dir))
        .await
        .expect("open catalog");
    TempCatalog {
        _base: base,
        session_dir,
        catalog,
    }
}

impl TempCatalog {
    fn db_path(&self) -> PathBuf {
        catalog_db_path(&self.session_dir)
    }

    async fn rebuild(&self, entries: &[CatalogEntry]) {
        self.catalog.rebuild(entries).await.expect("rebuild");
    }

    async fn list(&self, query: &DiscoveryQuery) -> ActionListResult {
        self.catalog.list(query).await.expect("list")
    }

    async fn list_for_package(&self, package: &str) -> ActionListResult {
        self.catalog
            .list_for_package(package)
            .await
            .expect("list_for_package")
    }
}

// ─── Worktree-open harness (populate task) ────────────────────────────────────

struct FakeBuildProvider {
    targets: Vec<BuildTargetCatalogEntry>,
}

impl BuildCatalogProvider for FakeBuildProvider {
    fn discover(&self, _repo_root: &Path) -> Result<Vec<BuildTargetCatalogEntry>, String> {
        Ok(self.targets.clone())
    }
}

fn a_build_target_dto(id: &str, name: &str) -> BuildTargetCatalogEntry {
    BuildTargetCatalogEntry {
        id: id.into(),
        name: name.into(),
        package: package_of_target(id),
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

/// A populated session (manifests on disk + injected build targets), with the catalog handle.
struct PopulatedSession {
    _base: TempDir,
    catalog: Arc<SessionCatalog>,
}

async fn a_populated_session() -> PopulatedSession {
    let base = tempfile::tempdir().expect("tempdir");
    let session_dir = base.path().join("session");
    let repo_root = base.path().join("repo");
    let data_dir = base.path().join("data");
    for d in [&session_dir, &repo_root, &data_dir] {
        fs::create_dir_all(d).expect("mkdir");
    }
    write_action_manifest(&session_dir, "packages/foo/build", "foo-build", "Build foo");
    write_action_manifest(&session_dir, "packages/bar/lint", "bar-lint", "Lint bar");

    let provider: Arc<dyn BuildCatalogProvider> = Arc::new(FakeBuildProvider {
        targets: vec![
            a_build_target_dto("packages/foo:binary", "Foo binary"),
            a_build_target_dto("packages/foo:test", "Foo tests"),
        ],
    });
    let registry = TaskRegistry::new();
    let catalog = SessionCatalog::open_and_populate(
        &session_dir,
        Some(&repo_root),
        &data_dir,
        &registry,
        "session-red",
        Some(provider),
    )
    .await
    .expect("open_and_populate");
    PopulatedSession {
        _base: base,
        catalog,
    }
}

// ─── Assertions ───────────────────────────────────────────────────────────────

trait ActionSummarySliceAssertions {
    fn assert_paths_exactly(&self, expected: &[&str]) -> &Self;
    fn assert_contains_exactly(&self, expected: &[&str]) -> &Self;
    fn assert_ids_exactly(&self, expected: &[&str]) -> &Self;
}

impl ActionSummarySliceAssertions for Vec<ActionSummary> {
    fn assert_paths_exactly(&self, expected: &[&str]) -> &Self {
        let actual: Vec<&str> = self.iter().map(|s| s.path.as_str()).collect();
        assert_eq!(actual, expected, "entry paths (in order) mismatch");
        self
    }

    fn assert_contains_exactly(&self, expected: &[&str]) -> &Self {
        let mut actual: Vec<&str> = self.iter().map(|s| s.path.as_str()).collect();
        actual.sort_unstable();
        let mut want: Vec<&str> = expected.to_vec();
        want.sort_unstable();
        assert_eq!(actual, want, "entry path set mismatch");
        self
    }

    fn assert_ids_exactly(&self, expected: &[&str]) -> &Self {
        let actual: Vec<&str> = self.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(actual, expected, "entry ids (in order) mismatch");
        self
    }
}

// ─── Schema / generated column ────────────────────────────────────────────────

#[tokio::test]
async fn the_generated_package_column_is_derived_from_the_entry_json() {
    // Given — one build-target entry whose JSON `package` is `packages/foo`.
    let tc = a_temp_catalog().await;
    tc.rebuild(&[a_catalog_entry()
        .build_target("packages/foo:binary")
        .build()])
        .await;

    // When — querying by the projected (generated) package column.
    let found = tc.list_for_package("packages/foo").await;

    // Then — the entry is retrievable via the column derived from its JSON.
    found.actions.assert_paths_exactly(&["packages/foo:binary"]);
}

#[tokio::test]
async fn the_catalog_db_is_created_at_the_session_catalog_db_path() {
    // Given / When
    let tc = a_temp_catalog().await;

    // Then — the file is created at `<session_dir>/catalog.db`.
    assert_eq!(tc.db_path().file_name().unwrap(), "catalog.db");
    assert!(tc.db_path().starts_with(&tc.session_dir));
    assert!(
        tc.db_path().exists(),
        "open must create the catalog db file"
    );
}

// ─── Unification + per-package listing ────────────────────────────────────────

#[tokio::test]
async fn populating_from_manifest_and_build_target_sources_yields_the_unified_catalog() {
    // Given — two action manifests and two build targets in one catalog.
    let tc = a_temp_catalog().await;
    tc.rebuild(&[
        a_catalog_entry()
            .action_manifest("packages/foo/build")
            .build(),
        a_catalog_entry()
            .action_manifest("packages/bar/lint")
            .build(),
        a_catalog_entry()
            .build_target("packages/foo:binary")
            .build(),
        a_catalog_entry().build_target("packages/foo:test").build(),
    ])
    .await;

    // When
    let all = tc.list(&DiscoveryQuery::default()).await;

    // Then — both kinds coexist, sorted ascending by path.
    all.actions.assert_paths_exactly(&[
        "packages/bar/lint",
        "packages/foo/build",
        "packages/foo:binary",
        "packages/foo:test",
    ]);
    assert_eq!(all.total, 4);
}

#[tokio::test]
async fn listing_targets_for_a_package_returns_exactly_the_entries_projected_to_that_package() {
    // Given — entries across `packages/foo` and `packages/bar`.
    let tc = a_temp_catalog().await;
    tc.rebuild(&[
        a_catalog_entry()
            .action_manifest("packages/foo/build")
            .build(),
        a_catalog_entry()
            .build_target("packages/foo:binary")
            .build(),
        a_catalog_entry().build_target("packages/foo:test").build(),
        a_catalog_entry()
            .action_manifest("packages/bar/lint")
            .build(),
        a_catalog_entry()
            .build_target("packages/bar:binary")
            .build(),
    ])
    .await;

    // When
    let foo = tc.list_for_package("packages/foo").await;

    // Then — exactly the `packages/foo` entries (set), nothing from `packages/bar`.
    foo.actions.assert_contains_exactly(&[
        "packages/foo/build",
        "packages/foo:binary",
        "packages/foo:test",
    ]);
}

// ─── DiscoveryQuery: filter + substring + pagination ──────────────────────────

#[tokio::test]
async fn filtering_by_package_prefix_returns_only_entries_within_that_package() {
    // Given
    let tc = a_temp_catalog().await;
    tc.rebuild(&[
        a_catalog_entry()
            .action_manifest("packages/foo/build")
            .build(),
        a_catalog_entry()
            .build_target("packages/foo:binary")
            .build(),
        a_catalog_entry()
            .action_manifest("packages/bar/lint")
            .build(),
    ])
    .await;

    // When
    let result = tc
        .list(&DiscoveryQuery {
            path_prefix: Some("packages/foo".into()),
            ..DiscoveryQuery::default()
        })
        .await;

    // Then
    result
        .actions
        .assert_paths_exactly(&["packages/foo/build", "packages/foo:binary"]);
    assert_eq!(result.total, 2);
}

#[tokio::test]
async fn filtering_by_query_substring_matches_id_and_summary_case_insensitively() {
    // Given — one entry whose summary contains "BINARY" (upper) and one that does not.
    let tc = a_temp_catalog().await;
    tc.rebuild(&[
        a_catalog_entry()
            .build_target("packages/foo:binary")
            .with_summary("Foo BINARY")
            .build(),
        a_catalog_entry()
            .action_manifest("packages/bar/lint")
            .with_summary("Lint bar")
            .build(),
    ])
    .await;

    // When — a lowercase substring query.
    let result = tc
        .list(&DiscoveryQuery {
            query: Some("binary".into()),
            ..DiscoveryQuery::default()
        })
        .await;

    // Then — only the binary entry matches (case-insensitive).
    result
        .actions
        .assert_paths_exactly(&["packages/foo:binary"]);
    assert_eq!(result.total, 1);
}

#[tokio::test]
async fn paginating_with_limit_and_offset_returns_the_expected_window_and_total() {
    // Given — five entries with known ascending paths.
    let tc = a_temp_catalog().await;
    tc.rebuild(&[
        a_catalog_entry().action_manifest("a/one").build(),
        a_catalog_entry().action_manifest("a/two").build(),
        a_catalog_entry().action_manifest("a/three").build(),
        a_catalog_entry().action_manifest("a/four").build(),
        a_catalog_entry().action_manifest("a/five").build(),
    ])
    .await;

    // When — the third and fourth entries (ascending: five, four, one, three, two).
    let result = tc
        .list(&DiscoveryQuery {
            limit: Some(2),
            offset: 2,
            ..DiscoveryQuery::default()
        })
        .await;

    // Then — the window is the 3rd+4th sorted paths; total is the full count before pagination.
    result.actions.assert_paths_exactly(&["a/one", "a/three"]);
    assert_eq!(result.total, 5);
}

#[tokio::test]
async fn combining_a_package_filter_a_query_substring_and_pagination_applies_all_three() {
    // Given — foo entries (some matching "svc") plus a bar entry that must never appear.
    let tc = a_temp_catalog().await;
    tc.rebuild(&[
        a_catalog_entry()
            .action_manifest("packages/foo/svc-build")
            .with_summary("svc build")
            .build(),
        a_catalog_entry()
            .action_manifest("packages/foo/svc-lint")
            .with_summary("svc lint")
            .build(),
        a_catalog_entry()
            .action_manifest("packages/foo/docs")
            .with_summary("docs")
            .build(),
        a_catalog_entry()
            .action_manifest("packages/bar/svc-run")
            .with_summary("svc run")
            .build(),
    ])
    .await;

    // When — package scope + substring + a one-item window at offset 1.
    let result = tc
        .list(&DiscoveryQuery {
            path_prefix: Some("packages/foo".into()),
            query: Some("svc".into()),
            limit: Some(1),
            offset: 1,
        })
        .await;

    // Then — filter-before-pagination: 2 foo entries match "svc"; the window is the 2nd of them.
    result
        .actions
        .assert_paths_exactly(&["packages/foo/svc-lint"]);
    assert_eq!(result.total, 2);
}

#[tokio::test]
async fn rescanning_removes_stale_entries_and_reflects_updated_summaries() {
    // Given — a first scan seeding A and B.
    let tc = a_temp_catalog().await;
    tc.rebuild(&[
        a_catalog_entry()
            .action_manifest("packages/foo/a")
            .with_id("a")
            .with_summary("A original")
            .build(),
        a_catalog_entry()
            .action_manifest("packages/foo/b")
            .with_id("b")
            .build(),
    ])
    .await;

    // When — a re-scan drops B, edits A's summary, and adds C.
    tc.rebuild(&[
        a_catalog_entry()
            .action_manifest("packages/foo/a")
            .with_id("a")
            .with_summary("A edited")
            .build(),
        a_catalog_entry()
            .action_manifest("packages/foo/c")
            .with_id("c")
            .build(),
    ])
    .await;
    let result = tc.list(&DiscoveryQuery::default()).await;

    // Then — B is gone, A's summary is updated, C is present.
    result.actions.assert_ids_exactly(&["a", "c"]);
    assert_eq!(result.actions[0].summary, "A edited");
}

// ─── Populate task + block-until-populate ─────────────────────────────────────

#[tokio::test]
async fn the_populate_task_reaches_completed_after_scanning_the_worktree() {
    // Given — a session whose worktree-open populate flow has been started.
    let session = a_populated_session().await;
    let handle = session
        .catalog
        .populate_handle()
        .expect("populate handle must exist after open_and_populate");

    // When — awaiting the task to a terminal status (bounded).
    let mut rx = handle.status_watch();
    timeout(Duration::from_secs(5), async {
        while !rx.borrow().is_terminal() {
            if rx.changed().await.is_err() {
                break;
            }
        }
    })
    .await
    .expect("populate task must reach a terminal status");

    // Then — it completed successfully.
    assert!(
        matches!(handle.status(), TaskStatus::Completed { .. }),
        "populate task must be Completed, was {:?}",
        handle.status()
    );
}

#[tokio::test]
async fn the_first_list_query_blocks_until_populate_completes_then_returns_the_populated_set() {
    // Given — a populate flow started (the first read must await it).
    let session = a_populated_session().await;

    // When — the first list query (which internally blocks until populate is terminal).
    let all = timeout(
        Duration::from_secs(5),
        session.catalog.list(&DiscoveryQuery::default()),
    )
    .await
    .expect("first list must not hang past the populate task")
    .expect("list must succeed");

    // Then — it returns the fully populated unified set.
    all.actions.assert_paths_exactly(&[
        "packages/bar/lint",
        "packages/foo/build",
        "packages/foo:binary",
        "packages/foo:test",
    ]);
}

#[tokio::test]
async fn a_second_list_query_after_populate_returns_the_same_set_without_reblocking() {
    // Given — a populated session where the first list has already completed.
    let session = a_populated_session().await;
    let first = session
        .catalog
        .list(&DiscoveryQuery::default())
        .await
        .expect("first list");

    // When — a second list runs; populate is already terminal (sticky), so it must not re-block.
    let second = timeout(
        Duration::from_secs(5),
        session.catalog.list(&DiscoveryQuery::default()),
    )
    .await
    .expect("second list must return immediately without re-blocking")
    .expect("second list");

    // Then — the populate task is terminal and both reads return the identical set.
    let handle = session.catalog.populate_handle().expect("populate handle");
    assert!(
        handle.status().is_terminal(),
        "populate status must be terminal after the first read"
    );
    let first_paths: Vec<&str> = first.actions.iter().map(|s| s.path.as_str()).collect();
    second.actions.assert_paths_exactly(&first_paths);
}
