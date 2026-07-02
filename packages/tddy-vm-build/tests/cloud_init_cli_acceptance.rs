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

/// Resolve the base image path from `TDDY_CLOUDINIT_BASE_IMAGE`, or `None` if unset.
fn configured_base_image() -> Option<PathBuf> {
    std::env::var(BASE_IMAGE_ENV).ok().map(PathBuf::from)
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
    let user_data_path = dir.path().join("user-data.yaml");
    std::fs::write(&user_data_path, a_minimal_user_data_yaml()).unwrap();

    let mut cmd = tddy_vm_build_bin();
    cmd.arg("cloud-init")
        .arg("--name")
        .arg(name)
        .arg("--base-image")
        .arg(base_image)
        .arg("--library-root")
        .arg(&library_root)
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

    let prepared_base_dir = library_root.join("images").join("02-prepared-base");
    (dir, library_root, prepared_base_dir)
}

#[test]
#[ignore = "production test: boots a real QEMU VM to bake cloud-init, ~1-3 min; requires \
            TDDY_CLOUDINIT_BASE_IMAGE (see module docs); run with --ignored"]
#[serial(cloud_init_qemu_vm)]
fn the_cloud_init_subcommand_produces_a_valid_chained_qcow2_pair_in_the_prepared_base_dir() {
    let Some(base_image) = configured_base_image() else {
        eprintln!(
            "{BASE_IMAGE_ENV} not set — skipping production test (see module docs to run it)"
        );
        return;
    };

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
#[ignore = "production test: boots a real QEMU VM to bake cloud-init, ~1-3 min; requires \
            TDDY_CLOUDINIT_BASE_IMAGE (see module docs); run with --ignored"]
#[serial(cloud_init_qemu_vm)]
fn the_cloud_init_subcommand_imports_the_raw_base_image_into_01_base() {
    let Some(base_image) = configured_base_image() else {
        eprintln!(
            "{BASE_IMAGE_ENV} not set — skipping production test (see module docs to run it)"
        );
        return;
    };

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
    let Some(base_image) = configured_base_image() else {
        eprintln!(
            "{BASE_IMAGE_ENV} not set — skipping production test (see module docs to run it)"
        );
        return;
    };

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
    let Some(base_image) = configured_base_image() else {
        eprintln!(
            "{BASE_IMAGE_ENV} not set — skipping production test (see module docs to run it)"
        );
        return;
    };

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
