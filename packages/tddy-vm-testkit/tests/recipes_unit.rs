//! Unit tests for the image chain: one shared Nix-prepared parent, two children.
//!
//! Pure document rendering — no VM, no filesystem — so these run in the default `./test`
//! suite. What they pin is the *division of labour* between the three recipes, which is
//! the whole reason there are three.

use pretty_assertions::assert_eq;
use tddy_vm::cloud_init::CloudInitUserData;
use tddy_vm_testkit::recipes::{
    builder_user_data, deployed_binaries, install_bundle_paths, nix_base_user_data,
    test_host_user_data, ALICE_USERNAME, TDDY_SERVICE_USERNAME,
};

/// Every `runcmd` entry joined, so a test can ask what the guest is told to do.
///
/// Asserted non-empty at the point of use: every "must not contain X" check below would
/// otherwise pass vacuously against a recipe that provisions nothing at all.
fn provisioning_script(user_data: &CloudInitUserData) -> String {
    let script = user_data.runcmd.join("\n");
    assert!(
        !script.trim().is_empty(),
        "a recipe with no provisioning steps would satisfy every negative assertion here"
    );
    script
}

/// The kernel-swap command the builder recipe borrows from `tddy_vm::tddy_host`.
///
/// Taken from the function itself rather than restating its text: the literal lives in
/// another crate, so a reword there would silently disarm three assertions that spell it
/// out by hand.
fn the_kernel_swap_command() -> String {
    tddy_vm::tddy_host::ninep_capable_kernel_command()
}

fn usernames(user_data: &CloudInitUserData) -> Vec<String> {
    user_data.users.iter().map(|u| u.name.clone()).collect()
}

#[test]
fn installs_nix_once_in_the_parent_both_images_derive_from() {
    // Given the three recipes
    let parent = provisioning_script(&nix_base_user_data());
    let builder = provisioning_script(&builder_user_data());
    let test_host = provisioning_script(&test_host_user_data());

    // Then only the shared parent pays for the Nix install — installing it is the
    // expensive half of both bakes, and doing it once is the difference between one long
    // wait and two
    assert!(
        parent.contains("nixos.org/nix/install"),
        "the shared parent must install Nix: {parent}"
    );
    assert!(
        !builder.contains("nixos.org/nix/install"),
        "the builder must inherit Nix, not reinstall it: {builder}"
    );
    assert!(
        !test_host.contains("nixos.org/nix/install"),
        "the test host must inherit Nix, not reinstall it: {test_host}"
    );
}

#[test]
fn creates_both_os_accounts_once_in_the_shared_parent() {
    // Given the shared parent recipe
    let user_data = nix_base_user_data();

    // When its accounts are read
    let accounts = usernames(&user_data);

    // Then both exist: the headline claim under test is that a session for `alice` runs
    // as `alice` while the daemon runs as `tddy`, which needs two real OS users — and
    // both children inherit them
    assert_eq!(
        accounts,
        vec![
            TDDY_SERVICE_USERNAME.to_string(),
            ALICE_USERNAME.to_string()
        ]
    );
}

#[test]
fn gives_only_the_builder_a_kernel_that_can_mount_the_working_copy() {
    // Given the recipes for the shared parent and both children
    let parent = provisioning_script(&nix_base_user_data());
    let builder = provisioning_script(&builder_user_data());
    let test_host = provisioning_script(&test_host_user_data());

    // Then only the builder swaps Debian's -cloud kernel for the generic flavour. It has
    // to: the cloud kernel ships no 9p modules at all, and 9p is how the working copy
    // gets in and the built binaries get out
    assert!(
        builder.contains(&the_kernel_swap_command()),
        "the builder must install a 9p-capable kernel: {builder}"
    );

    // And neither the parent nor the test host touches the kernel. Swapping it would
    // diverge the kernel under test from the one a real Debian cloud host runs — and the
    // thing under test here *is* kernel behaviour, which is why binaries reach the test
    // host by scp instead of over 9p
    assert!(
        !parent.contains(&the_kernel_swap_command()),
        "the shared parent must keep the stock kernel: {parent}"
    );
    assert!(
        !test_host.contains(&the_kernel_swap_command()),
        "the test host must keep the stock kernel: {test_host}"
    );
}

#[test]
fn warms_the_workspace_dev_shell_only_in_the_builder() {
    // Given both children
    let builder = provisioning_script(&builder_user_data());
    let test_host = provisioning_script(&test_host_user_data());

    // Then only the builder realises the flake's dev shell, so its first real `./release`
    // is a compile rather than a download — and the guest under assertion gains nothing
    // that could compile anything
    assert!(
        builder.contains("./dev true"),
        "the builder must warm the dev shell: {builder}"
    );
    assert!(
        !test_host.contains("./dev"),
        "the test host must carry no warmed toolchain: {test_host}"
    );
}

#[test]
fn stages_no_tddy_binaries_at_bake_time() {
    // Given the test-host recipe
    let script = provisioning_script(&test_host_user_data());

    // Then it installs nothing — the binaries do not exist when this bakes. They are
    // scp'd in per run, which is what makes re-testing a code change a boot instead of a
    // re-bake
    assert!(
        !script.contains("./install"),
        "the test host must not install at bake time: {script}"
    );
}

#[test]
fn exports_everything_the_install_script_reads_from_a_checkout() {
    // Given the set of paths the builder copies back to the host

    // When it is listed
    let paths = install_bundle_paths();

    // Then it covers every `${ROOT_DIR}/...` the install script dereferences under
    // `--systemd --headless`, so the guest can run the real `./install` rather than a
    // reimplementation of it
    assert_eq!(
        paths,
        vec![
            ("install", "install"),
            ("daemon.yaml.production", "daemon.yaml.production"),
            ("supervisor.yaml.production", "supervisor.yaml.production"),
            (
                "packages/tddy-daemon/apparmor/tddy-daemon",
                "apparmor-tddy-daemon"
            ),
        ]
    );
}

#[test]
fn deploys_every_binary_the_install_script_requires() {
    // Given the install script's own list, read from the script rather than restated
    let script = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .expect("the testkit must sit two levels under the repo root")
            .join("install"),
    )
    .expect("./install must be readable");
    let required: Vec<String> = script
        .lines()
        .find(|line| line.starts_with("INSTALLED_BINARIES=("))
        .expect("./install must declare INSTALLED_BINARIES")
        .trim_start_matches("INSTALLED_BINARIES=(")
        .trim_end_matches(')')
        .split_whitespace()
        .map(str::to_string)
        .collect();

    // When the binaries the builder deploys are listed
    let deployed = deployed_binaries();

    // Then every one the script will look for is there — including `tddy-supervisor`, which
    // the script prepends in `--systemd` mode. A short list is not caught until the guest
    // is booted and the install exits 1 on the first missing path, which is minutes and one
    // bake too late
    for name in required
        .iter()
        .chain(std::iter::once(&"tddy-supervisor".to_string()))
    {
        assert!(
            deployed.contains(&name.as_str()),
            "./install requires {name}, which the deployed set does not carry: {deployed:?}"
        );
    }
}

#[test]
fn stages_no_bundle_file_under_a_name_that_would_overwrite_a_binary() {
    // Given the flat directory the builder writes everything into
    let staged_bundle_names: Vec<&str> = install_bundle_paths()
        .into_iter()
        .map(|(_, staged)| staged)
        .collect();

    // Then no bundle file lands under a binary's name. The AppArmor profile is the trap:
    // its basename in the checkout *is* `tddy-daemon`, so flattening it naively would
    // overwrite the daemon binary with a text file — and the guest would install cleanly
    // and only fail later, unable to exec its own daemon
    for binary in deployed_binaries() {
        assert!(
            !staged_bundle_names.contains(&binary),
            "bundle file would overwrite the {binary} binary: {staged_bundle_names:?}"
        );
    }
}

#[test]
fn deploys_the_jailed_payload_alongside_the_binaries_that_spawn_it() {
    // Given the set of binaries deployed to the test host

    // When it is listed
    let binaries = deployed_binaries();

    // Then `tddy-sandbox-runner` is among them even though `./release` does not build it:
    // it is the payload every sandbox spawn execs, so a jail fails without it on disk
    assert!(
        binaries.contains(&"tddy-sandbox-runner"),
        "the jailed payload must be deployed: {binaries:?}"
    );
    assert!(
        binaries.contains(&"tddy-supervisor") && binaries.contains(&"tddy-daemon"),
        "the supervised stack must be deployed: {binaries:?}"
    );
}
