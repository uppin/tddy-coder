use std::sync::Arc;

use tddy_build::executor::{execute_target, ExecuteOptions};
use tddy_build::graph::BuildGraph;
use tddy_build::plugin::PluginRegistry;
use tddy_build_qemu::QemuPlugin;

fn registry() -> PluginRegistry {
    let mut r = PluginRegistry::new();
    r.register(Arc::new(QemuPlugin));
    r
}

fn staged() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let images = dir.path().join("build/br-out/images");
    std::fs::create_dir_all(&images).expect("mkdir");
    // 1 MiB zero-filled raw "disk image" — valid input for qemu-img convert -f raw
    std::fs::write(images.join("rootfs.ext4"), vec![0u8; 1024 * 1024])
        .expect("write raw image");
    dir
}

fn load_graph() -> BuildGraph {
    let yaml = "schema_version: 1\ntargets:\n  - id: \"my-os:qcow2\"\n    config:\n      type: qemu_disk_image\n      input: build/br-out/images/rootfs.ext4\n      srcs: [\"build/br-out/images/rootfs.ext4\"]\n";
    let manifest = tddy_build::load_build_manifest(yaml).expect("parse");
    BuildGraph::from_manifests(vec![manifest]).expect("graph")
}

#[tokio::test]
async fn qemu_disk_image_converts_raw_to_qcow2() {
    let dir = staged();
    let record = execute_target(
        dir.path(),
        &load_graph(),
        "my-os:qcow2",
        &ExecuteOptions::default(),
        &registry(),
    )
    .await
    .expect("execute");
    assert_eq!(
        record.actions[0].exit_code,
        0,
        "stderr: {}",
        record.actions[0].stderr
    );
    let qcow2_path = dir.path().join("build/br-out/images/rootfs.qcow2");
    assert!(qcow2_path.exists(), "qcow2 output must be created");

    // Verify qcow2 magic header: first 4 bytes are b"QFI\xfb"
    let header = std::fs::read(&qcow2_path).expect("read qcow2");
    assert_eq!(&header[..4], b"QFI\xfb", "output must be a valid qcow2 image");
}

#[tokio::test]
async fn qemu_disk_image_cache_hits_on_rerun() {
    let dir = staged();
    let opts = ExecuteOptions::default();
    let reg = registry();
    let graph = load_graph();

    execute_target(dir.path(), &graph, "my-os:qcow2", &opts, &reg)
        .await
        .expect("first run");

    let second = execute_target(dir.path(), &graph, "my-os:qcow2", &opts, &reg)
        .await
        .expect("second run");
    assert!(second.actions[0].cached, "rerun must be a cache hit");
}

#[tokio::test]
async fn qemu_disk_image_cache_miss_after_input_change() {
    let dir = staged();
    let opts = ExecuteOptions::default();
    let reg = registry();
    let graph = load_graph();

    execute_target(dir.path(), &graph, "my-os:qcow2", &opts, &reg)
        .await
        .expect("first run");

    // Overwrite the raw image with different content to change its fingerprint
    let raw_path = dir.path().join("build/br-out/images/rootfs.ext4");
    std::fs::write(&raw_path, vec![1u8; 1024 * 1024]).expect("modify raw image");

    let third = execute_target(dir.path(), &graph, "my-os:qcow2", &opts, &reg)
        .await
        .expect("third run");
    assert!(
        !third.actions[0].cached,
        "modified input must invalidate the cache"
    );
}
