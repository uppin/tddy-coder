//! Pure path arithmetic for backing references between layers.
//!
//! A qcow2 resolves a relative backing path against **the directory containing the
//! referencing image**, not the process CWD. Every case below is therefore expressed as
//! "from the child's directory, name the parent" — which is also why a relative-backed
//! overlay must be created in its final location and can never be moved afterwards.

use std::path::Path;

use pretty_assertions::assert_eq;
use tddy_vm::cloud_init::{relative_backing_path, CloudInitLibraryPaths};
use tddy_vm::library::VmLibrary;

#[test]
fn names_a_parent_in_the_same_directory_by_bare_filename() {
    // Given a parent and child sharing one directory
    let child_dir = Path::new("/lib/images/02-prepared-base");
    let parent = Path::new("/lib/images/02-prepared-base/tddy-nix-base.qcow2");

    // When the backing reference is computed
    let backing = relative_backing_path(child_dir, parent).unwrap();

    // Then it is the bare filename, the shortest reference that survives relocation
    assert_eq!(backing, "tddy-nix-base.qcow2");
}

#[test]
fn names_a_parent_one_tier_up_by_walking_out_of_the_child_directory() {
    // Given a child in 02-prepared-base and its parent in 01-base
    let child_dir = Path::new("/lib/images/02-prepared-base");
    let parent = Path::new("/lib/images/01-base/debian-12.qcow2");

    // When the backing reference is computed
    let backing = relative_backing_path(child_dir, parent).unwrap();

    // Then it walks out and back down, keeping the whole library relocatable as a unit
    assert_eq!(backing, "../01-base/debian-12.qcow2");
}

#[test]
fn names_a_parent_two_tiers_up_without_collapsing_the_walk() {
    // Given a per-VM overlay nested a further level down
    let child_dir = Path::new("/lib/vm/tddy-test-42");
    let parent = Path::new("/lib/images/02-prepared-base/tddy-test-host.qcow2");

    // When the backing reference is computed
    let backing = relative_backing_path(child_dir, parent).unwrap();

    // Then every level of the walk is present
    assert_eq!(
        backing,
        "../../images/02-prepared-base/tddy-test-host.qcow2"
    );
}

#[test]
fn keeps_a_trailing_slash_on_the_child_directory_from_changing_the_answer() {
    // Given the same directory written with a trailing separator
    let parent = Path::new("/lib/images/01-base/debian-12.qcow2");

    // When the backing reference is computed both ways
    let plain = relative_backing_path(Path::new("/lib/images/02-prepared-base"), parent).unwrap();
    let trailing =
        relative_backing_path(Path::new("/lib/images/02-prepared-base/"), parent).unwrap();

    // Then both name the parent correctly. The literal is asserted on each side rather than
    // just that the two agree: two identically wrong answers would satisfy equality alone
    assert_eq!(plain, "../01-base/debian-12.qcow2");
    assert_eq!(trailing, "../01-base/debian-12.qcow2");
}

#[test]
fn refuses_to_name_a_parent_when_one_path_is_absolute_and_the_other_is_relative() {
    // Given a child directory named absolutely and a parent named relative to the working
    // directory — the shape a half-resolved library root produces
    let child_dir = Path::new("/lib/images/02-prepared-base");
    let parent = Path::new("tmp/.tddy/images/01-base/debian-12.qcow2");

    // When the backing reference is computed
    let result = relative_backing_path(child_dir, parent);

    // Then it is refused by name rather than answered with a walk that resolves to
    // nothing: the two paths are measured against different origins, so no number of `..`
    // steps relates them
    assert_eq!(
        result.unwrap_err().to_string(),
        "VM image build failed: cannot name tmp/.tddy/images/01-base/debian-12.qcow2 \
         relative to /lib/images/02-prepared-base: one is absolute and the other is relative"
    );
}

#[test]
fn routes_a_provisioned_layer_into_02_prepared_base_without_a_second_half() {
    // Given a library and a layer name
    let library = VmLibrary::new("/lib");

    // When the layer's library paths are resolved
    let paths: CloudInitLibraryPaths =
        tddy_vm::cloud_init::cloud_init_library_paths(&library, "debian-12", "tddy-nix-base");

    // Then the imported pristine image and the single provisioned delta are named, and
    // nothing else — there is no flattened `-base.qcow2` half, because the layer chains
    // onto the import rather than copying it
    assert_eq!(
        paths.base_image_in_01_base,
        Path::new("/lib/images/01-base/debian-12.qcow2")
    );
    assert_eq!(
        paths.prepared_overlay_output,
        Path::new("/lib/images/02-prepared-base/tddy-nix-base.qcow2")
    );
}
