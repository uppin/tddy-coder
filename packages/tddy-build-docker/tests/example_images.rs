//! Exercises the docker recipe plugin on a real, interdependent image set:
//! deps-first ordering, lowered argv (incl. --iidfile), and — when a docker daemon
//! is reachable — real `docker build` plus action-cache hit/miss.

use std::path::PathBuf;
use std::sync::Arc;

use tddy_build::discovery::discover_build_manifests;
use tddy_build::executor::{execute_target, ExecuteOptions};
use tddy_build::graph::BuildGraph;
use tddy_build::plugin::PluginRegistry;
use tddy_build_docker::DockerPlugin;

fn example_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/images")
}

fn docker_up() -> bool {
    std::process::Command::new("docker")
        .arg("info")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn registry() -> PluginRegistry {
    let mut r = PluginRegistry::new();
    r.register(Arc::new(DockerPlugin));
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
        if from
            .file_name()
            .map(|n| n == ".tddy-build")
            .unwrap_or(false)
        {
            continue;
        }
        let to = dst.join(entry.file_name());
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
fn docker_targets_depend_on_base_first() {
    // Given
    let graph = load(&example_root());

    // When
    let order = graph.build_order("api:image").expect("order");
    let pos = |id: &str| order.iter().position(|t| t == id).expect("present");

    // Then
    assert!(pos("base:image") < pos("api:image"));
}

#[test]
fn docker_plugin_lowers_iidfile_argv() {
    // Given
    let graph = load(&example_root());

    // When
    let actions = graph.actions_for("base:image", &registry()).expect("lower");

    // Then
    assert_eq!(
        actions[0].command,
        vec![
            "docker",
            "build",
            "-f",
            "base/Dockerfile",
            "-t",
            "example-base",
            "--iidfile",
            ".tddy-build/iid/base.txt",
            "base"
        ]
    );
}

#[tokio::test]
async fn docker_images_build_and_cache_when_daemon_available() {
    if !docker_up() {
        eprintln!("SKIP: docker daemon not reachable");
        return;
    }

    // Given
    let dir = staged();
    let opts = ExecuteOptions::default();
    let reg = registry();
    let graph = load(dir.path());

    // When
    // base + api build (deps-first); api is FROM example-base.
    let record = execute_target(
        dir.path(),
        &graph,
        "api:image",
        &opts,
        tddy_build::BuildMode::Compile,
        &reg,
    )
    .await
    .expect("docker build");

    // Then
    assert_eq!(
        record.actions[0].exit_code, 0,
        "stderr: {}",
        record.actions[0].stderr
    );
    assert!(
        dir.path().join(".tddy-build/iid/api.txt").exists(),
        "iidfile written"
    );

    // When (rerun base alone)
    let second = execute_target(
        dir.path(),
        &graph,
        "base:image",
        &opts,
        tddy_build::BuildMode::Compile,
        &reg,
    )
    .await
    .expect("second base");

    // Then
    assert!(second.actions[0].cached, "rerun is a cache hit");

    // When (edit the base Dockerfile)
    std::fs::write(
        dir.path().join("base/Dockerfile"),
        "FROM busybox:latest\nRUN echo \"base v2\" > /base.txt\n",
    )
    .expect("edit dockerfile");
    let third = execute_target(
        dir.path(),
        &graph,
        "base:image",
        &opts,
        tddy_build::BuildMode::Compile,
        &reg,
    )
    .await
    .expect("third base");

    // Then
    assert!(
        !third.actions[0].cached,
        "dockerfile edit invalidates the cache"
    );
}
