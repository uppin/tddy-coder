//! Unit tests for the testkit's on-disk cache layout.
//!
//! Pure path arithmetic — no filesystem, no VM — so these run in the default `./test` suite.

use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_vm_testkit::layout::{repo_root_from_manifest_dir, TestkitLayout};
use tddy_vm_testkit::VmArch;

fn a_layout() -> TestkitLayout {
    TestkitLayout::for_repo_root(PathBuf::from("/repo"))
}

#[test]
fn resolves_the_repo_root_from_the_crates_own_manifest_directory() {
    // Given this crate's compile-time manifest directory
    let manifest_dir = Path::new("/repo/packages/tddy-vm-testkit");

    // When the repo root is derived from it
    let root = repo_root_from_manifest_dir(manifest_dir);

    // Then it is the repo, not the package — `default_tddy_data_dir()` hands back the
    // *relative* `tmp/.tddy`, and `cargo test` runs with the CWD set to the package
    // directory, so anything CWD-relative would scatter caches into
    // `packages/<pkg>/tmp/.tddy`
    assert_eq!(root, PathBuf::from("/repo"));
}

#[test]
fn resolves_the_repo_root_of_a_worktree_to_that_worktree() {
    // Given the crate compiled inside a git worktree
    let manifest_dir = Path::new("/repo/.worktrees/feat-x/packages/tddy-vm-testkit");

    // When the repo root is derived from it
    let root = repo_root_from_manifest_dir(manifest_dir);

    // Then it is the worktree's own root, so two worktrees never share one image cache
    assert_eq!(root, PathBuf::from("/repo/.worktrees/feat-x"));
}

#[test]
fn caches_images_under_the_same_dev_data_dir_the_web_dev_script_uses() {
    // Given the layout for a repo
    let layout = a_layout();

    // When the library root is asked for
    let library_root = layout.library_root();

    // Then it is the repo's own `tmp/.tddy` — gitignored, and the directory `./web-dev`
    // already populates
    assert_eq!(library_root, PathBuf::from("/repo/tmp/.tddy"));
}

#[test]
fn keeps_binaries_built_for_an_aarch64_guest_in_an_aarch64_directory() {
    // Given the layout for a repo
    let layout = a_layout();

    // When the distribution directory for an aarch64 guest is asked for
    let dist = layout.dist_dir_for(VmArch::Aarch64);

    // Then it names the guest platform, not the host's — these are Linux binaries built in a
    // VM because a macOS host cannot produce them
    assert_eq!(dist, PathBuf::from("/repo/tmp/.tddy/dist/linux-aarch64"));
}

#[test]
fn keeps_binaries_built_for_an_x86_64_guest_in_an_x86_64_directory() {
    // Given the layout for a repo
    let layout = a_layout();

    // When the distribution directory for an x86_64 guest is asked for
    let dist = layout.dist_dir_for(VmArch::X86_64);

    // Then it is named for that architecture too: the guest always runs the *host's*
    // architecture, so a hardcoded `linux-aarch64` would misname every binary an x86_64
    // developer builds
    assert_eq!(dist, PathBuf::from("/repo/tmp/.tddy/dist/linux-x86_64"));
}

#[test]
fn collects_the_hosts_own_binaries_in_the_directory_named_for_the_host_architecture() {
    // Given the layout for a repo
    let layout = a_layout();

    // When the distribution directory is asked for without naming an architecture
    let dist = layout.dist_dir();

    // Then it is the host architecture's own directory — the builder guest is emulated at
    // the host's architecture so its output can run on the test host
    assert_eq!(dist, layout.dist_dir_for(VmArch::host()));
}

#[test]
fn gives_the_builder_guest_an_overlay_that_outlives_a_single_run() {
    // Given the layout for a repo
    let layout = a_layout();

    // When the builder VM's directory is asked for twice
    let first = layout.builder_vm_name();
    let second = layout.builder_vm_name();

    // Then it is the same name both times, so `/opt/tddy/target` survives and the next
    // `./release` is incremental instead of a cold rebuild
    assert_eq!(first, second);
    assert_eq!(first, "tddy-builder");
}

#[test]
fn gives_each_test_run_its_own_disposable_guest_name() {
    // Given the layout for a repo
    let layout = a_layout();

    // When a test-host VM name is minted for two different runs
    let first = layout.test_host_vm_name(4242);
    let second = layout.test_host_vm_name(4243);

    // Then they differ, so a fresh overlay is created per run and the cgroup state under
    // assertion is never inherited from the previous one
    assert_ne!(first, second);
    assert_eq!(first, "tddy-test-4242");
}
