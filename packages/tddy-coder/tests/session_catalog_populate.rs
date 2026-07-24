//! Integration: the real `tddy-build`-backed catalog provider feeds the populate task, producing
//! an on-disk `catalog.db` that unifies action manifests and auto-discovered `BUILD.yaml` targets.
//!
//! Feature: docs/ft/coder/session-catalog.md
//! Changeset: docs/dev/1-WIP/session-catalog-db.md

use std::fs;
use std::time::Duration;

use tddy_core::session_actions::DiscoveryQuery;
use tddy_core::session_catalog::read::catalog_db_path;
use tddy_core::session_catalog::{store, PopulateCatalogTask, SessionCatalog};
use tddy_task::{TaskRegistry, TaskStatus};
use tokio::time::timeout;

/// The coder registers a `BUILD.yaml`-backed provider; populating a session then writes a catalog
/// that a fresh reader sees as the union of the session's action manifests and the repo's build
/// targets (sorted ascending by path).
#[tokio::test]
async fn populating_with_the_real_build_provider_unifies_manifests_and_build_yaml_targets() {
    // Given — the real provider registered, a repo with a BUILD.yaml, and a session-overlay manifest.
    tddy_bsp::register_catalog_provider();

    let base = tempfile::tempdir().expect("tempdir");
    let session_dir = base.path().join("session");
    let repo_root = base.path().join("repo");
    let data_dir = base.path().join("data");
    for dir in [&session_dir, &repo_root, &data_dir] {
        fs::create_dir_all(dir).expect("mkdir");
    }

    fs::create_dir_all(repo_root.join("packages/foo")).expect("mkdir pkg");
    fs::write(
        repo_root.join("packages/foo/BUILD.yaml"),
        "schema_version: 1\n\
         targets:\n\
         \x20 - id: \"packages/foo:binary\"\n\
         \x20   name: Foo binary\n",
    )
    .expect("write BUILD.yaml");

    let overlay = session_dir.join("actions/packages/foo");
    fs::create_dir_all(&overlay).expect("mkdir overlay");
    fs::write(
        overlay.join("build.yaml"),
        "version: 1\nid: foo-build\nsummary: Build foo\narchitecture: native\ncommand: ['true']\n",
    )
    .expect("write manifest");

    // When — run the populate task exactly as the coder trigger does (real provider from the
    // process-global registry), and wait for it to commit.
    let pool = store::open_pool(&catalog_db_path(&session_dir))
        .await
        .expect("open pool");
    let task = PopulateCatalogTask {
        pool,
        session_dir: session_dir.clone(),
        repo_root: Some(repo_root.clone()),
        tddy_data_dir: data_dir.clone(),
        build_provider: tddy_core::session_catalog::build_catalog_provider(),
    };
    let registry = TaskRegistry::new();
    let handle = registry
        .spawn(task, "session_catalog_populate", "session-populate", vec![])
        .await;
    let mut status = handle.status_watch();
    timeout(Duration::from_secs(5), async {
        let _ = status.wait_for(TaskStatus::is_terminal).await;
    })
    .await
    .expect("populate task must terminate");
    assert!(
        matches!(handle.status(), TaskStatus::Completed { .. }),
        "populate must complete, was {:?}",
        handle.status()
    );

    // Then — a fresh reader of `catalog.db` sees the manifest and the build target unified.
    let catalog = SessionCatalog::open(&catalog_db_path(&session_dir))
        .await
        .expect("open catalog");
    let all = catalog
        .list(&DiscoveryQuery::default())
        .await
        .expect("list catalog");
    let paths: Vec<&str> = all.actions.iter().map(|a| a.path.as_str()).collect();
    assert_eq!(paths, vec!["packages/foo/build", "packages/foo:binary"]);
    assert_eq!(all.total, 2);
}
