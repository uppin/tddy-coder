//! Production test for the `tddy-vm-build cloud-init` subcommand: imports an immutable
//! base cloud image into the VM & Image Library and produces a cloud-init-provisioned,
//! chained qcow2 delta overlay in `images/02-prepared-base/` — the "ready to use" image
//! this feature exists to build.
//!
//! ## Real QEMU boot (production test — manual trigger only)
//!
//! Boots a real `qemu-system-x86_64` VM to bake cloud-init into the overlay (see
//! `packages/tddy-vm/tests/cloud_init_acceptance.rs` for the underlying pipeline this
//! CLI wraps).
//!
//! This is a production test: it never runs on its own. `#[ignore]`d (excluded from
//! `./test`/`./verify`/plain `cargo test`) *and* gated on `TDDY_CLOUDINIT_BASE_IMAGE`
//! pointing at a real cloud-init-compatible qcow2 image (e.g. a Debian genericcloud
//! image) — the same config the CLI's own `--base-image` flag reads. There is no
//! bundled or auto-downloaded image; a developer must supply one explicitly to run
//! this test.
//!
//! Run explicitly with:
//! ```text
//! TDDY_CLOUDINIT_BASE_IMAGE=/path/to/base.qcow2 \
//!   cargo test -p tddy-vm-build --test cloud_init_cli_acceptance -- --ignored --nocapture
//! ```

use assert_cmd::cargo::cargo_bin_cmd;
use serial_test::serial;
use std::path::PathBuf;
use tempfile::tempdir;

fn tddy_vm_build_bin() -> assert_cmd::Command {
    cargo_bin_cmd!("tddy-vm-build")
}

/// The env var this production test reads its base image path from — the same config
/// knob the CLI's own `--base-image` flag reads.
const BASE_IMAGE_ENV: &str = "TDDY_CLOUDINIT_BASE_IMAGE";

/// The base image path, or a panic naming what is missing.
///
/// Not `Option` and an early return: a production test with no image configured has not
/// been satisfied, it has been skipped — and a skip that returns normally is reported as a
/// pass, which is exactly what a mistyped path or a failed download produces. `./vm-tests`
/// is where a *deliberate* absence is reported, once, before anything runs.
fn require_base_image() -> PathBuf {
    let raw = std::env::var(BASE_IMAGE_ENV).unwrap_or_default();
    let trimmed = raw.trim();
    assert!(
        !trimmed.is_empty(),
        "{BASE_IMAGE_ENV} is not set, so this production test has no image to bake from. \
         Point it at a cloud image already on disk — exported, or in the repo-root .env — \
         and run these tests with ./vm-tests. Nothing is ever downloaded."
    );
    PathBuf::from(trimmed)
}

fn a_minimal_user_data_yaml() -> &'static str {
    "hostname: cloud-init-cli-demo\n\
     users:\n  \
       - name: tddy\n    \
         shell: /bin/bash\n    \
         sudo: \"ALL=(ALL) NOPASSWD:ALL\"\n    \
         ssh_authorized_keys:\n      \
           - \"{{SSH_PUBLIC_KEY}}\"\n"
}

/// Run the `cloud-init` subcommand against the configured base image, writing into a
/// fresh library root under a per-test temp dir named `name`. Returns the library root
/// and the resolved `images/02-prepared-base/` directory on success.
///
/// Each behavior claim below is its own `#[test]` (matching the granularity this
/// crate's own `cloud_init_acceptance.rs` uses — split by semantic claim, not by
/// individual `assert!`), so this helper re-runs the full CLI invocation, including its
/// real QEMU boot, once per test rather than sharing a single run's outputs.
fn run_cloud_init_cli(
    name: &str,
    base_image: PathBuf,
    ssh_host_port: &str,
) -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempdir().unwrap();
    let library_root = dir.path().join("library");
    run_cloud_init_cli_in(
        &library_root,
        name,
        &["--base-image", &base_image.display().to_string()],
        ssh_host_port,
    );

    let prepared_base_dir = library_root.join("images").join("02-prepared-base");
    (dir, library_root, prepared_base_dir)
}

/// One `cloud-init` invocation against an explicit library root, with `layer_parent` naming
/// the new layer's parent — `["--base-image", <path>]` to start a chain from a pristine
/// cloud image, `["--parent-layer", <name>]` to continue an existing one.
fn run_cloud_init_cli_in(
    library_root: &std::path::Path,
    name: &str,
    layer_parent: &[&str; 2],
    ssh_host_port: &str,
) {
    let user_data_path = library_root.with_file_name(format!("{name}-user-data.yaml"));
    std::fs::create_dir_all(user_data_path.parent().unwrap()).unwrap();
    std::fs::write(&user_data_path, a_minimal_user_data_yaml()).unwrap();

    let mut cmd = tddy_vm_build_bin();
    cmd.arg("cloud-init")
        .arg("--name")
        .arg(name)
        .args(layer_parent)
        .arg("--library-root")
        .arg(library_root)
        .arg("--user-data")
        .arg(&user_data_path)
        .arg("--disk-size")
        .arg("10G")
        .arg("--memory")
        .arg("1024M")
        .arg("--cpus")
        .arg("1")
        .arg("--ssh-host-port")
        .arg(ssh_host_port)
        .arg("--timeout-secs")
        .arg("180");
    cmd.assert().success();
}

/// The backing reference `qemu-img info` reports as recorded in `image` — without the
/// ` (actual path: …)` annotation qemu-img adds for a relative one, which describes where the
/// file sits today rather than what the image records.
fn backing_file_of(image: &std::path::Path) -> String {
    let output = std::process::Command::new("qemu-img")
        .arg("info")
        .arg(image)
        .output()
        .expect("qemu-img info must run");
    let info = String::from_utf8_lossy(&output.stdout);
    let reported = info
        .lines()
        .find_map(|line| line.strip_prefix("backing file: "))
        .unwrap_or_else(|| panic!("qemu-img info reported no backing file:\n{info}"));
    reported
        .split(" (actual path:")
        .next()
        .unwrap_or(reported)
        .trim()
        .to_string()
}

#[test]
#[ignore = "production test: boots a real QEMU VM to bake cloud-init, ~1-3 min; requires \
            TDDY_CLOUDINIT_BASE_IMAGE (see module docs); run with --ignored"]
#[serial(cloud_init_qemu_vm)]
fn the_cloud_init_subcommand_produces_a_valid_chained_qcow2_pair_in_the_prepared_base_dir() {
    let base_image = require_base_image();

    // Given the cloud-init subcommand run against the configured base image
    let (_dir, _library_root, prepared_base_dir) =
        run_cloud_init_cli("cli-demo-pair", base_image, "2297");

    // Then it produces a delta overlay chained onto an immutable base, both valid qcow2
    // files under images/02-prepared-base/ in the library
    let overlay_path = prepared_base_dir.join("cli-demo-pair.qcow2");
    let base_path = prepared_base_dir.join("cli-demo-pair-base.qcow2");
    assert!(
        overlay_path.exists(),
        "provisioned overlay must exist at {}",
        overlay_path.display()
    );
    assert!(
        base_path.exists(),
        "immutable base must exist at {}",
        base_path.display()
    );
    let magic = std::fs::read(&overlay_path).expect("overlay must be readable");
    assert_eq!(&magic[..4], b"QFI\xfb", "overlay must be a qcow2 image");
}

#[test]
#[ignore = "production test: boots two real QEMU VMs to bake two chained layers, ~2-6 min; \
            requires TDDY_CLOUDINIT_BASE_IMAGE (see module docs); run with --ignored"]
#[serial(cloud_init_qemu_vm)]
fn the_cloud_init_subcommand_chains_a_second_layer_onto_an_already_prepared_one() {
    let base_image = require_base_image();

    // Given a first layer already baked from the configured base image
    let dir = tempdir().unwrap();
    let library_root = dir.path().join("library");
    run_cloud_init_cli_in(
        &library_root,
        "cli-chain-parent",
        &["--base-image", &base_image.display().to_string()],
        "2301",
    );

    // When a second layer is baked naming that layer as its parent
    run_cloud_init_cli_in(
        &library_root,
        "cli-chain-child",
        &["--parent-layer", "cli-chain-parent"],
        "2302",
    );

    // Then the child is a delta over the parent layer, referenced as the sibling it is —
    // the case that used to be rejected outright, because importing a delta into
    // images/01-base/ would strand the relative reference it resolves its own parent through
    let child = library_root
        .join("images")
        .join("02-prepared-base")
        .join("cli-chain-child.qcow2");
    assert_eq!(backing_file_of(&child), "cli-chain-parent.qcow2");
}

#[test]
#[ignore = "production test: boots a real QEMU VM to bake cloud-init, ~1-3 min; requires \
            TDDY_CLOUDINIT_BASE_IMAGE (see module docs); run with --ignored"]
#[serial(cloud_init_qemu_vm)]
fn the_cloud_init_subcommand_imports_the_raw_base_image_into_01_base() {
    let base_image = require_base_image();

    // Given the cloud-init subcommand run against the configured base image
    let (_dir, library_root, _prepared_base_dir) =
        run_cloud_init_cli("cli-demo-import", base_image, "2298");

    // Then the raw base image was catalogued into images/01-base/, separate from the
    // provisioned pair
    let imported_base = library_root
        .join("images")
        .join("01-base")
        .join("cli-demo-import.qcow2");
    assert!(
        imported_base.exists(),
        "imported base must exist at {}",
        imported_base.display()
    );
}

#[test]
#[ignore = "production test: boots a real QEMU VM to bake cloud-init, ~1-3 min; requires \
            TDDY_CLOUDINIT_BASE_IMAGE (see module docs); run with --ignored"]
#[serial(cloud_init_qemu_vm)]
fn the_cloud_init_subcommand_locks_both_halves_of_the_produced_pair_read_only() {
    let base_image = require_base_image();

    // Given the cloud-init subcommand run against the configured base image
    let (_dir, _library_root, prepared_base_dir) =
        run_cloud_init_cli("cli-demo-readonly", base_image, "2299");

    // Then both halves of the produced pair are locked read-only, protecting the
    // prepared base from accidental mutation
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let overlay_path = prepared_base_dir.join("cli-demo-readonly.qcow2");
        let base_path = prepared_base_dir.join("cli-demo-readonly-base.qcow2");
        let overlay_mode = std::fs::metadata(&overlay_path)
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(overlay_mode & 0o777, 0o444);
        let base_mode = std::fs::metadata(&base_path).unwrap().permissions().mode();
        assert_eq!(base_mode & 0o777, 0o444);
    }
}

#[test]
#[ignore = "production test: boots a real QEMU VM to bake cloud-init, ~1-3 min; requires \
            TDDY_CLOUDINIT_BASE_IMAGE (see module docs); run with --ignored"]
#[serial(cloud_init_qemu_vm)]
fn the_cloud_init_subcommand_keeps_scratch_artifacts_out_of_the_flat_prepared_base_dir() {
    let base_image = require_base_image();

    // Given the cloud-init subcommand run against the configured base image
    let (_dir, _library_root, prepared_base_dir) =
        run_cloud_init_cli("cli-demo-scratch", base_image, "2300");

    // Then the seed ISO, generated SSH keypair, and boot log live in a per-image
    // scratch subdirectory, not scattered directly in 02-prepared-base/ alongside the
    // qcow2 pair
    let scratch_dir = prepared_base_dir.join("cli-demo-scratch");
    assert!(
        scratch_dir.join("cli-demo-scratch-seed.iso").exists(),
        "seed ISO must be in the per-image scratch subdirectory"
    );
    assert!(
        scratch_dir.join("id_cli-demo-scratch").exists(),
        "generated SSH private key must be in the per-image scratch subdirectory"
    );
    assert!(
        scratch_dir.join("cli-demo-scratch-boot.log").exists(),
        "boot log must be in the per-image scratch subdirectory"
    );
    assert!(
        !prepared_base_dir.join("cli-demo-scratch-seed.iso").exists(),
        "seed ISO must not be left directly in 02-prepared-base/"
    );
}
