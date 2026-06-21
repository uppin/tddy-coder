//! Unit tests for `list_built_images_in`.

use std::path::PathBuf;
use tddy_vm::build::list_built_images_in;
use tempfile::tempdir;
use tokio::fs;

/// Create a file at `base/build-<name>/images/rootfs.qcow2` with `size_bytes` of content.
async fn make_qcow2(base: &std::path::Path, name: &str, size_bytes: usize) -> PathBuf {
    let img_dir = base.join(name).join("images");
    fs::create_dir_all(&img_dir).await.unwrap();
    let path = img_dir.join("rootfs.qcow2");
    fs::write(&path, vec![0u8; size_bytes]).await.unwrap();
    path
}

#[tokio::test]
async fn returns_qcow2_images_from_build_dirs() {
    let dir = tempdir().unwrap();
    let base = dir.path();

    make_qcow2(base, "build-1000", 1024).await;
    make_qcow2(base, "build-2000", 2048).await;

    let records = list_built_images_in(base).await;

    assert_eq!(
        records.len(),
        2,
        "must find exactly 2 qcow2 images; got {:?}",
        records.iter().map(|r| &r.name).collect::<Vec<_>>()
    );
    // Verify size bytes are captured
    let total_sizes: u64 = records.iter().map(|r| r.size_bytes).sum();
    assert_eq!(total_sizes, 1024 + 2048, "size_bytes must match file sizes");
}

#[tokio::test]
async fn ignores_non_qcow2_files() {
    let dir = tempdir().unwrap();
    let base = dir.path();

    // One legit qcow2
    make_qcow2(base, "build-3000", 512).await;

    // Stray non-.qcow2 file inside images/
    let img_dir = base.join("build-3000").join("images");
    fs::write(img_dir.join("rootfs.ext2"), b"not a qcow2")
        .await
        .unwrap();

    // A .qcow2 directly in the build dir (not inside images/)
    fs::write(base.join("build-3000").join("rootfs.qcow2"), b"stray")
        .await
        .unwrap();

    let records = list_built_images_in(base).await;

    assert_eq!(records.len(), 1, "must return only the images/*.qcow2 file");
    assert_eq!(records[0].size_bytes, 512);
}

#[tokio::test]
async fn skips_build_dirs_with_no_images_subdir() {
    let dir = tempdir().unwrap();
    let base = dir.path();

    // A build dir with no images/ subdirectory
    fs::create_dir_all(base.join("build-4000")).await.unwrap();
    fs::write(base.join("build-4000").join(".config"), b"BR2_x86_64=y")
        .await
        .unwrap();

    // A valid one for contrast
    make_qcow2(base, "build-5000", 256).await;

    let records = list_built_images_in(base).await;

    assert_eq!(
        records.len(),
        1,
        "build dir without images/ must be skipped"
    );
}

#[tokio::test]
async fn returns_empty_when_disks_dir_does_not_exist() {
    let records = list_built_images_in(std::path::Path::new("/nonexistent/path/disks")).await;
    assert!(
        records.is_empty(),
        "must return empty vec when disks dir does not exist"
    );
}

#[tokio::test]
async fn records_include_name_from_build_dir() {
    let dir = tempdir().unwrap();
    let base = dir.path();

    make_qcow2(base, "build-9999", 128).await;

    let records = list_built_images_in(base).await;

    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].name, "build-9999",
        "name must be the build dir name, got {:?}",
        records[0].name
    );
}

#[tokio::test]
async fn records_sorted_newest_first_by_modified_time() {
    let dir = tempdir().unwrap();
    let base = dir.path();

    // Create two images with different timestamps by writing them sequentially
    // and touching the second one to ensure its mtime > first.
    make_qcow2(base, "build-100", 64).await;
    // Small delay to ensure different mtime
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    make_qcow2(base, "build-200", 64).await;

    let records = list_built_images_in(base).await;

    assert_eq!(records.len(), 2);
    assert!(
        records[0].modified_unix_ms >= records[1].modified_unix_ms,
        "records must be sorted newest-first: first={}, second={}",
        records[0].modified_unix_ms,
        records[1].modified_unix_ms
    );
    assert_eq!(
        records[0].name, "build-200",
        "newest build dir must be first"
    );
}
