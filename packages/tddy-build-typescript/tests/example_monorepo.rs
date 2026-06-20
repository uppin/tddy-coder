//! Exercises the typescript recipe plugin on a real, interdependent bun monorepo:
//! deps-first ordering, real `bun run build`, and the action cache. Skips when bun
//! is unavailable.

use std::path::PathBuf;
use std::sync::Arc;

use tddy_build::discovery::discover_build_manifests;
use tddy_build::executor::{execute_target, ExecuteOptions};
use tddy_build::graph::BuildGraph;
use tddy_build::plugin::PluginRegistry;
use tddy_build_typescript::TypeScriptPlugin;

fn example_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/monorepo")
}

fn bun_available() -> bool {
    std::process::Command::new("bun")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn registry() -> PluginRegistry {
    let mut r = PluginRegistry::new();
    r.register(Arc::new(TypeScriptPlugin));
    r
}

fn staged() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    copy_dir(&example_root(), dir.path());
    dir
}

fn copy_dir(src: &std::path::Path, dst: &std::path::Path) {
    for entry in std::fs::read_dir(src).expect("read_dir") {
        let entry = entry.expect("entry");
        let from = entry.path();
        let name = entry.file_name();
        if name == "dist" || name == "node_modules" {
            continue;
        }
        let to = dst.join(&name);
        if from.is_dir() {
            std::fs::create_dir_all(&to).expect("mkdir");
            copy_dir(&from, &to);
        } else {
            std::fs::copy(&from, &to).expect("copy");
        }
    }
}

fn load(root: &std::path::Path) -> BuildGraph {
    let manifests = discover_build_manifests(root)
        .expect("discover")
        .into_iter()
        .map(|(_, m)| m)
        .collect();
    BuildGraph::from_manifests(manifests).expect("graph")
}

#[test]
fn ts_targets_depend_on_each_other_deps_first() {
    // Given
    let graph = load(&example_root());

    // When
    let order = graph.build_order("web:build").expect("order");
    let pos = |id: &str| order.iter().position(|t| t == id).expect("present");

    // Then
    assert!(pos("shared:build") < pos("ui:build"));
    assert!(pos("ui:build") < pos("web:build"));
}

#[test]
fn ts_plugin_lowers_expected_bun_argv_and_workdir() {
    // Given
    let graph = load(&example_root());

    // When
    let actions = graph
        .actions_for("shared:build", &registry())
        .expect("lower");

    // Then
    assert_eq!(actions[0].command, vec!["bun", "run", "build"]);
    assert_eq!(actions[0].working_dir, "packages/shared");
}

#[tokio::test]
async fn ts_monorepo_builds_with_real_bun() {
    if !bun_available() {
        eprintln!("SKIP: bun not available");
        return;
    }

    // Given
    let dir = staged();
    let graph = load(dir.path());

    // When
    let record = execute_target(
        dir.path(),
        &graph,
        "web:build",
        &ExecuteOptions::default(),
        &registry(),
    )
    .await
    .expect("bun build");

    // Then
    assert_eq!(
        record.actions[0].exit_code, 0,
        "stderr: {}",
        record.actions[0].stderr
    );
    assert!(dir.path().join("apps/web/dist").exists(), "dist produced");
}

#[tokio::test]
async fn ts_cache_hits_then_misses_after_source_edit() {
    if !bun_available() {
        eprintln!("SKIP: bun not available");
        return;
    }

    // Given
    let dir = staged();
    let opts = ExecuteOptions::default();
    let reg = registry();
    let graph = load(dir.path());

    // When
    let first = execute_target(dir.path(), &graph, "shared:build", &opts, &reg)
        .await
        .expect("first");

    // Then
    assert!(!first.actions[0].cached);

    // When (rerun without changes)
    let second = execute_target(dir.path(), &graph, "shared:build", &opts, &reg)
        .await
        .expect("second");

    // Then
    assert!(second.actions[0].cached, "rerun is a cache hit");

    // When (source file edited)
    std::fs::write(
        dir.path().join("packages/shared/src/index.ts"),
        "export const greeting = \"hello again\";\n",
    )
    .expect("edit source");
    let third = execute_target(dir.path(), &graph, "shared:build", &opts, &reg)
        .await
        .expect("third");

    // Then
    assert!(
        !third.actions[0].cached,
        "source edit invalidates the cache"
    );
}
