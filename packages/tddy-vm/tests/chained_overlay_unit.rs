//! Creating the delta overlay a layer bakes into.
//!
//! The overlay is the one artifact of a bake that cannot be produced somewhere convenient
//! and moved, so it is created at its final, published path — where the *previous* bake of
//! the same name already sealed a file `0444`. `qemu-img create` opens its output
//! `O_WRONLY|O_CREAT|O_TRUNC`, which a sealed file refuses.
//!
//! Shelling out to a real `qemu-img` is the point: what the tool does with the arguments it
//! is handed is exactly what an argv assertion cannot see.

use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_vm::cloud_init::create_chained_overlay;
use tddy_vm::library::set_readonly_file;
use tempfile::tempdir;

/// A real, standalone qcow2 at `path` — the whole image a chain starts from.
fn a_base_image_at(path: &Path) -> PathBuf {
    std::fs::create_dir_all(path.parent().expect("a base image lives in a directory"))
        .expect("the base image directory must be creatable");
    run_qemu_img(&["create", "-f", "qcow2", &path.display().to_string(), "64M"]);
    path.to_path_buf()
}

/// The parent `qemu-img` records for the image at `path`, exactly as it is written into the
/// qcow2 header.
fn backing_file_of(path: &Path) -> String {
    let output = std::process::Command::new("qemu-img")
        .args(["info", "--output=json"])
        .arg(path)
        .output()
        .expect("qemu-img must be on PATH");
    assert!(
        output.status.success(),
        "qemu-img info {} failed: {}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let info: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("qemu-img info must emit JSON");
    info["backing-filename"]
        .as_str()
        .expect("the overlay must record a parent")
        .to_string()
}

fn run_qemu_img(args: &[&str]) {
    let output = std::process::Command::new("qemu-img")
        .args(args)
        .output()
        .expect("qemu-img must be on PATH");
    assert!(
        output.status.success(),
        "qemu-img {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn chains_a_new_layer_onto_the_parent_it_names_one_tier_up() {
    // Given a base image in 01-base and a layer to build in 02-prepared-base
    let dir = tempdir().unwrap();
    let base = a_base_image_at(&dir.path().join("images/01-base/debian-12.qcow2"));
    let overlay = dir
        .path()
        .join("images/02-prepared-base/tddy-nix-base.qcow2");

    // When the layer's overlay is created
    create_chained_overlay(&overlay, &base, "64M")
        .await
        .expect("the layer's overlay must be creatable");

    // Then it holds a relative reference to that parent, so the whole library relocates
    // as a unit
    assert_eq!(backing_file_of(&overlay), "../01-base/debian-12.qcow2");
}

#[tokio::test]
async fn re_bakes_a_layer_over_the_sealed_overlay_the_previous_bake_left_at_that_path() {
    // Given a layer already baked and sealed 0444 at its final path
    let dir = tempdir().unwrap();
    let base = a_base_image_at(&dir.path().join("images/01-base/debian-12.qcow2"));
    let overlay = dir
        .path()
        .join("images/02-prepared-base/tddy-nix-base.qcow2");
    create_chained_overlay(&overlay, &base, "64M")
        .await
        .expect("the first bake's overlay must be creatable");
    set_readonly_file(&overlay).expect("a finished layer is sealed read-only");

    // When the same layer is baked again
    create_chained_overlay(&overlay, &base, "64M")
        .await
        .expect("a re-bake must not be blocked by the seal the last one left behind");

    // Then the sealed file was replaced by a fresh delta of the same parent
    assert_eq!(backing_file_of(&overlay), "../01-base/debian-12.qcow2");
}
