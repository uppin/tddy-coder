//! A VM & Image Library rooted at a path named relative to the working directory.
//!
//! This is the default `tddy-vm-build cloud-init` runs under, not an exotic one:
//! `default_library_root()` hands back `tddy_core::output::default_tddy_data_dir()`, which
//! is the *relative* `tmp/.tddy` in a debug build, and the daemon's configured data dir can
//! be relative too.
//!
//! This binary holds exactly one test on purpose. It sets the process working directory,
//! which every thread of a test binary shares, so it must have no neighbours to disturb.

use std::path::Path;

use pretty_assertions::assert_eq;
use tddy_vm::cloud_init::create_chained_overlay;
use tempfile::tempdir;

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

/// A real, standalone qcow2 at `path` — the whole image a chain starts from.
fn a_base_image_at(path: &Path) {
    std::fs::create_dir_all(path.parent().expect("a base image lives in a directory"))
        .expect("the base image directory must be creatable");
    let output = std::process::Command::new("qemu-img")
        .args(["create", "-f", "qcow2"])
        .arg(path)
        .arg("64M")
        .output()
        .expect("qemu-img must be on PATH");
    assert!(
        output.status.success(),
        "qemu-img create failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
async fn chains_a_layer_onto_a_parent_under_a_library_root_named_relative_to_the_working_directory()
{
    // Given a working directory holding a library at the relative root `tmp/.tddy`
    let dir = tempdir().unwrap();
    std::env::set_current_dir(dir.path()).expect("the working directory must be settable");
    let base = Path::new("tmp/.tddy/images/01-base/debian-12.qcow2");
    a_base_image_at(base);
    let overlay = Path::new("tmp/.tddy/images/02-prepared-base/tddy-nix-base.qcow2");

    // When a layer is chained onto it through those same relative paths
    create_chained_overlay(overlay, base, "64M")
        .await
        .expect("a relative library root must resolve against the working directory");

    // Then the layer lands beside its siblings, naming its parent one tier up — not under
    // a second copy of the root re-resolved from the overlay's own directory
    assert_eq!(backing_file_of(overlay), "../01-base/debian-12.qcow2");
}
