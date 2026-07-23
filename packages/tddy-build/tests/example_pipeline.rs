//! Exercises the engine's built-in script/tool/group types as a real, runnable
//! multi-package pipeline: discovery, cross-package deps, real execution, the action
//! cache (hit on rerun, miss on input edit), and cycle detection.

use std::path::PathBuf;

use tddy_build::discovery::discover_build_manifests;
use tddy_build::executor::{execute_target, ExecuteOptions};
use tddy_build::graph::BuildGraph;
use tddy_build::plugin::PluginRegistry;

fn example_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/pipeline")
}

/// Copy the committed example into a fresh tempdir so builds never dirty the repo.
fn staged() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    copy_dir(&example_root(), dir.path());
    dir
}

fn copy_dir(src: &std::path::Path, dst: &std::path::Path) {
    for entry in std::fs::read_dir(src).expect("read_dir") {
        let entry = entry.expect("entry");
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            std::fs::create_dir_all(&to).expect("mkdir");
            copy_dir(&from, &to);
        } else {
            std::fs::copy(&from, &to).expect("copy");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = std::fs::metadata(&from).unwrap().permissions().mode();
                std::fs::set_permissions(&to, std::fs::Permissions::from_mode(mode)).unwrap();
            }
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
fn targets_reference_each_other_and_resolve_deps_first() {
    // Given
    let graph = load(&example_root());

    // When
    let order = graph.build_order("app:build").expect("build order");
    let pos = |id: &str| order.iter().position(|t| t == id).expect("present");

    // Then
    assert!(pos("codegen:gen") < pos("lib:build"));
    assert!(pos("lib:build") < pos("app:build"));
}

#[tokio::test]
async fn pipeline_builds_successfully_through_a_tool_target() {
    // Given
    let dir = staged();
    let graph = load(dir.path());

    // When
    let record = execute_target(
        dir.path(),
        &graph,
        "app:build",
        &ExecuteOptions::default(),
        tddy_build::BuildMode::Compile,
        &PluginRegistry::new(),
    )
    .await
    .expect("build app");

    // Then
    assert_eq!(record.actions[0].exit_code, 0);
    let app = std::fs::read_to_string(dir.path().join("app.txt")).expect("app.txt");
    assert!(app.contains("STAMPED:"), "tool stub must run, got: {app:?}");
    assert!(
        app.contains("generated"),
        "pipeline output threads through, got: {app:?}"
    );
}

#[tokio::test]
async fn cache_hits_on_rerun_and_misses_after_input_edit() {
    // Given
    let dir = staged();
    let opts = ExecuteOptions::default();
    let registry = PluginRegistry::new();
    let graph = load(dir.path());

    // When
    let first = execute_target(
        dir.path(),
        &graph,
        "codegen:gen",
        &opts,
        tddy_build::BuildMode::Compile,
        &registry,
    )
    .await
    .expect("first");

    // Then
    assert!(!first.actions[0].cached, "first run executes");

    // When
    let second = execute_target(
        dir.path(),
        &graph,
        "codegen:gen",
        &opts,
        tddy_build::BuildMode::Compile,
        &registry,
    )
    .await
    .expect("second");

    // Then
    assert!(second.actions[0].cached, "rerun is a cache hit");

    // Given — edit a declared input → fingerprint changes → miss.
    std::fs::write(dir.path().join("codegen/seed.txt"), "seed-changed").expect("edit seed");

    // When
    let third = execute_target(
        dir.path(),
        &graph,
        "codegen:gen",
        &opts,
        tddy_build::BuildMode::Compile,
        &registry,
    )
    .await
    .expect("third");

    // Then
    assert!(
        !third.actions[0].cached,
        "edited input invalidates the cache"
    );
}

#[test]
fn group_membership_orders_the_whole_pipeline() {
    // Given
    let graph = load(&example_root());

    // When
    let order = graph.build_order("pipeline:all").expect("group order");

    // Then
    for member in ["codegen:gen", "lib:build", "app:build"] {
        assert!(
            order.contains(&member.to_string()),
            "{member} in group order"
        );
    }
}

#[test]
fn engine_detects_self_loop_and_multi_node_cycles() {
    // Given — self-loop: a target depending on itself.
    let self_loop = r#"
schema_version: 1
targets:
  - id: "a:t"
    name: A
    deps: ["a:t"]
    config: { type: script, command: ["true"] }
"#;
    let m = tddy_build::load_build_manifest(self_loop).expect("parse self loop");

    // When / Then
    assert!(
        BuildGraph::from_manifests(vec![m]).is_err(),
        "self-loop is a cycle"
    );

    // Given — three-node cycle: a -> b -> c -> a.
    let three = r#"
schema_version: 1
targets:
  - id: "a:t"
    name: A
    deps: ["b:t"]
    config: { type: script, command: ["true"] }
  - id: "b:t"
    name: B
    deps: ["c:t"]
    config: { type: script, command: ["true"] }
  - id: "c:t"
    name: C
    deps: ["a:t"]
    config: { type: script, command: ["true"] }
"#;
    let m = tddy_build::load_build_manifest(three).expect("parse 3-cycle");

    // When / Then
    assert!(
        BuildGraph::from_manifests(vec![m]).is_err(),
        "3-node cycle is rejected"
    );
}
