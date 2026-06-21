//! build_vm_image acceptance test — gated on qemu-img availability.

use std::path::PathBuf;
use tddy_vm::build::build_vm_image;
use tempfile::tempdir;

fn qemu_img_available() -> bool {
    std::process::Command::new("qemu-img")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[tokio::test]
async fn build_vm_image_produces_qcow2() {
    if !qemu_img_available() {
        eprintln!("qemu-img not found — skipping build_vm_image test");
        return;
    }

    // Given — a staged repo with a raw image and a BUILD.yaml
    let dir = tempdir().unwrap();
    let repo_root = dir.path();

    // Stage a 1 MiB zero-filled raw disk image
    let images_dir = repo_root.join("build/br-out/images");
    std::fs::create_dir_all(&images_dir).unwrap();
    std::fs::write(images_dir.join("rootfs.ext4"), vec![0u8; 1024 * 1024]).unwrap();

    // Write a BUILD.yaml with a qemu_disk_image target
    let build_yaml = "schema_version: 1\ntargets:\n  - id: \"qemu-minimal:qcow2\"\n    config:\n      type: qemu_disk_image\n      input: build/br-out/images/rootfs.ext4\n      srcs: [\"build/br-out/images/rootfs.ext4\"]\n";
    std::fs::write(repo_root.join("BUILD.yaml"), build_yaml).unwrap();

    // When — build_vm_image is called with the staged target
    let result = build_vm_image(repo_root, "qemu-minimal:qcow2").await;

    // Then — a valid qcow2 image is produced
    assert!(
        result.is_ok(),
        "build_vm_image must succeed: {:?}",
        result.err()
    );
    let image_path: PathBuf = result.unwrap();
    assert!(
        image_path.exists(),
        "qcow2 output must exist at {}",
        image_path.display()
    );
    let magic = std::fs::read(&image_path).expect("produced image must be readable");
    assert_eq!(
        &magic[..4],
        b"QFI\xfb",
        "produced image must be qcow2 format"
    );
}
