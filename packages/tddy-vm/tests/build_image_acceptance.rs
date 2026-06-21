//! build_vm_image acceptance test — gated on qemu-img availability.
//! Fails until build_vm_image is implemented.

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

    // Given — a fake repo root with a BUILD.yaml stub target
    let dir = tempdir().unwrap();
    let repo_root = dir.path();

    // (A real BUILD.yaml with a qemu build target would be here.)
    // For the acceptance test we verify the API surface compiles and the
    // call fails predictably (unimplemented) until /green implements it.
    let result = build_vm_image(repo_root, "qemu-minimal").await;

    // Then — currently unimplemented; error expected until /green
    // The assertion below verifies the return type is Result<PathBuf, VmError>
    // and that when implemented it will return a valid path.
    assert!(
        result.is_ok(),
        "build_vm_image must succeed for a valid build target: {:?}",
        result.err()
    );

    let image_path: PathBuf = result.unwrap();
    let magic = std::fs::read(&image_path)
        .expect("produced image must be readable");
    // qcow2 magic bytes: QFI\xfb
    assert_eq!(&magic[..4], b"QFI\xfb", "produced image must be qcow2 format");
}
