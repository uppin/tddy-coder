//! What counts as an already-baked layer.
//!
//! The overlay is created in the first seconds of a bake that then runs for hours, and only
//! an in-process failure removes it again. A Ctrl-C, a panic or the OOM killer leaves a
//! half-provisioned overlay at the layer's published path — so "the file is there" cannot
//! be the cache key. The `0444` seal `build_cloud_init_image` applies *after* the guest
//! signals completion can, because nothing else applies it.
//!
//! Neither test bakes: the source image they name is not on the machine, which is exactly
//! how a cache hit is told apart from a cache miss without waiting hours for one.

use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_vm::cloud_init::CloudInitUserData;
use tddy_vm::library::set_readonly_file;
use tddy_vm_testkit::bake::{ensure_prepared_base, BakeSpec};
use tddy_vm_testkit::layout::TestkitLayout;
use tempfile::{tempdir, TempDir};

const LAYER_NAME: &str = "tddy-nix-base";

/// A layout over a throwaway repo root, with the library tree already created.
fn a_testkit_layout_in(repo_root: &TempDir) -> TestkitLayout {
    let layout = TestkitLayout::for_repo_root(repo_root.path());
    layout
        .library()
        .init()
        .expect("the library tree must be creatable");
    layout
}

/// The overlay a bake killed mid-provisioning leaves behind: present, and still writable
/// because nothing sealed it.
fn an_unsealed_overlay_in(layout: &TestkitLayout) -> PathBuf {
    let path = layout.prepared_base_path(LAYER_NAME);
    std::fs::write(&path, b"half a bake").expect("the overlay must be writable");
    path
}

/// The overlay a finished bake leaves behind: sealed `0444`.
fn a_sealed_layer_in(layout: &TestkitLayout) -> PathBuf {
    let path = an_unsealed_overlay_in(layout);
    set_readonly_file(&path).expect("a finished layer is sealed read-only");
    path
}

/// A bake of [`LAYER_NAME`] from an image that is not on this machine.
fn a_bake_from(source_image: &Path) -> BakeSpec {
    BakeSpec::new(LAYER_NAME, source_image, CloudInitUserData::default())
}

fn ignoring_progress() -> impl Fn(&str) + Sync {
    |_line: &str| {}
}

#[tokio::test]
async fn reports_a_sealed_layer_as_already_prepared_instead_of_baking_it_again() {
    // Given a finished, sealed layer where the bake would put it
    let repo_root = tempdir().unwrap();
    let layout = a_testkit_layout_in(&repo_root);
    let sealed = a_sealed_layer_in(&layout);

    // When that layer is ensured
    let ensured = ensure_prepared_base(
        &layout,
        a_bake_from(Path::new("/nowhere/debian-12.qcow2")),
        &ignoring_progress(),
    )
    .await;

    // Then it answers with the layer already on disk, never reaching the source image
    assert_eq!(ensured.unwrap(), sealed);
}

#[tokio::test]
async fn bakes_again_over_the_unsealed_overlay_an_interrupted_bake_left_behind() {
    // Given the writable overlay a bake killed mid-provisioning left at the layer's path
    let repo_root = tempdir().unwrap();
    let layout = a_testkit_layout_in(&repo_root);
    an_unsealed_overlay_in(&layout);

    // When that layer is ensured
    let ensured = ensure_prepared_base(
        &layout,
        a_bake_from(Path::new("/nowhere/debian-12.qcow2")),
        &ignoring_progress(),
    )
    .await;

    // Then the bake runs — reaching the source image and failing on it — rather than
    // handing back a layer that was never provisioned
    assert_eq!(
        ensured.unwrap_err().to_string(),
        "source image /nowhere/debian-12.qcow2 does not exist"
    );
}
