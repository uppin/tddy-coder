//! Unit tests for the cloud-init → library path-mapping helper
//! (`tddy_vm::cloud_init::cloud_init_library_paths`) — routes a cloud-init build's input
//! (the imported base) and its one output (the provisioned delta overlay) into the correct
//! library directories instead of a bare caller-chosen `--output-dir`.
//! Fails until `cloud_init_library_paths` is implemented.

use std::path::PathBuf;
use tddy_vm::cloud_init::cloud_init_library_paths;
use tddy_vm::library::VmLibrary;

#[test]
fn routes_the_downloaded_input_base_into_01_base_under_the_base_image_name() {
    // Given a library rooted at a fixed path and a distinct base-image name
    let library = VmLibrary::new(PathBuf::from("/data/.tddy"));

    // When resolving the cloud-init library paths for a build named
    // "debian-12-nodejs" that is derived from the base image "debian-12"
    let paths = cloud_init_library_paths(&library, "debian-12", "debian-12-nodejs");

    // Then the input base lives in images/01-base, named after the base image — not
    // the derived build
    assert_eq!(
        paths.base_image_in_01_base,
        PathBuf::from("/data/.tddy/images/01-base/debian-12.qcow2")
    );
}

#[test]
fn routes_the_provisioned_overlay_into_02_prepared_base_under_the_derived_build_name() {
    // Given the same library and names as above
    let library = VmLibrary::new(PathBuf::from("/data/.tddy"));

    // When resolving the cloud-init library paths
    let paths = cloud_init_library_paths(&library, "debian-12", "debian-12-nodejs");

    // Then the delta lands in 02-prepared-base named after the derived build — one image,
    // chained onto the 01-base import above rather than carrying a copy of it
    assert_eq!(
        paths.prepared_overlay_output,
        PathBuf::from("/data/.tddy/images/02-prepared-base/debian-12-nodejs.qcow2")
    );
}

#[test]
fn a_shared_base_image_name_resolves_to_the_same_01_base_path_for_different_builds() {
    // Given two different derived-build names that both reference the same base image
    // (mirrors makers-lt: "debian-12-nodejs" and "debian-12-docker" both derive from
    // the single imported "debian-12" base)
    let library = VmLibrary::new(PathBuf::from("/data/.tddy"));
    let nodejs_paths = cloud_init_library_paths(&library, "debian-12", "debian-12-nodejs");
    let docker_paths = cloud_init_library_paths(&library, "debian-12", "debian-12-docker");

    // When comparing the resolved input-base path across both builds
    // Then it is identical — the shared base is imported into 01-base exactly once
    assert_eq!(
        nodejs_paths.base_image_in_01_base,
        docker_paths.base_image_in_01_base
    );
}
