use std::path::PathBuf;
use std::sync::Arc;

use tddy_build::executor::{execute_target, ExecuteOptions};
use tddy_build::graph::BuildGraph;
use tddy_build::plugin::PluginRegistry;
use tddy_build_buildroot::BuildrootPlugin;

fn fake_buildroot_src() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/fake-buildroot")
}

fn registry() -> PluginRegistry {
    let mut r = PluginRegistry::new();
    r.register(Arc::new(BuildrootPlugin));
    r
}

fn staged() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let br_dir = dir.path().join("external/buildroot");
    std::fs::create_dir_all(&br_dir).expect("mkdir");
    std::fs::copy(fake_buildroot_src().join("Makefile"), br_dir.join("Makefile"))
        .expect("copy Makefile");
    dir
}

fn load_graph(output_dir: &str) -> BuildGraph {
    let yaml = format!(
        "schema_version: 1\ntargets:\n  - id: \"my-os:rootfs\"\n    config:\n      type: buildroot_image\n      defconfig: qemu_x86_64_defconfig\n      buildroot_dir: external/buildroot\n      output_dir: {output_dir}\n"
    );
    let manifest = tddy_build::load_build_manifest(&yaml).expect("parse");
    BuildGraph::from_manifests(vec![manifest]).expect("graph")
}

#[tokio::test]
async fn buildroot_defconfig_action_creates_config_file() {
    // Given
    let dir = staged();
    let graph = load_graph("build/br-out");

    // When
    let record = execute_target(
        dir.path(),
        &graph,
        "my-os:rootfs",
        &ExecuteOptions::default(),
        &registry(),
    )
    .await
    .expect("execute");

    // Then
    assert_eq!(
        record.actions[0].exit_code,
        0,
        "defconfig stderr: {}",
        record.actions[0].stderr
    );
    assert!(
        dir.path().join("build/br-out/.config").exists(),
        ".config must be created by defconfig action"
    );
}

#[tokio::test]
async fn buildroot_build_action_creates_rootfs_image() {
    // Given
    let dir = staged();
    let graph = load_graph("build/br-out");

    // When
    let record = execute_target(
        dir.path(),
        &graph,
        "my-os:rootfs",
        &ExecuteOptions::default(),
        &registry(),
    )
    .await
    .expect("execute");

    // Then
    assert_eq!(
        record.actions[1].exit_code,
        0,
        "build stderr: {}",
        record.actions[1].stderr
    );
    assert!(
        dir.path().join("build/br-out/images/rootfs.ext4").exists(),
        "rootfs.ext4 must be produced"
    );
}

#[tokio::test]
async fn buildroot_cache_hits_on_rerun() {
    // Given
    let dir = staged();
    let opts = ExecuteOptions::default();
    let reg = registry();
    let graph = load_graph("build/br-out");
    execute_target(dir.path(), &graph, "my-os:rootfs", &opts, &reg)
        .await
        .expect("first run");

    // When
    let second = execute_target(dir.path(), &graph, "my-os:rootfs", &opts, &reg)
        .await
        .expect("second run");

    // Then
    assert!(second.actions[1].cached, "rerun must be a cache hit");
}

#[tokio::test]
async fn buildroot_cache_miss_after_makefile_edit() {
    // Given
    let dir = staged();
    let opts = ExecuteOptions::default();
    let reg = registry();
    // Use srcs to track the Makefile
    let yaml = "schema_version: 1\ntargets:\n  - id: \"my-os:rootfs\"\n    config:\n      type: buildroot_image\n      defconfig: qemu_x86_64_defconfig\n      buildroot_dir: external/buildroot\n      output_dir: build/br-out\n      srcs: [\"external/buildroot/Makefile\"]\n";
    let manifest = tddy_build::load_build_manifest(yaml).expect("parse");
    let graph = BuildGraph::from_manifests(vec![manifest]).expect("graph");
    execute_target(dir.path(), &graph, "my-os:rootfs", &opts, &reg)
        .await
        .expect("first run");

    // When
    // Touch the tracked Makefile to change its mtime
    let makefile = dir.path().join("external/buildroot/Makefile");
    let contents = std::fs::read(&makefile).expect("read");
    std::fs::write(&makefile, contents).expect("rewrite");
    let third = execute_target(dir.path(), &graph, "my-os:rootfs", &opts, &reg)
        .await
        .expect("third run");

    // Then
    assert!(
        !third.actions[1].cached,
        "Makefile edit must invalidate the cache"
    );
}
