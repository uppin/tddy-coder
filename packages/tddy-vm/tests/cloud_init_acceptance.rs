//! Cloud-init image build production test — boots a real `qemu-system-x86_64` VM
//! against a developer-supplied base cloud image.
//!
//! ## Real QEMU boot (production test — manual trigger only)
//!
//! Chains a delta overlay onto a real cloud image, bakes a NoCloud cloud-init seed into the
//! overlay by actually booting `qemu-system-x86_64` and watching the serial console for a
//! completion token, then asserts the guest shut itself down and the overlay is a valid
//! qcow2 that still names the image it was baked from.
//!
//! [`a_baked_prepared_base_boots_as_a_vm_that_answers_over_ssh`] closes the loop the other
//! two leave open: they inspect the artifact the bake produced, and it *uses* it — creates a
//! VM from the baked layer, boots that VM, and logs into it over SSH. A bake that produced
//! an unbootable image, or one whose cloud-init never created the account, passes both
//! inspections and fails this.
//!
//! This is a production test: it never runs on its own. `#[ignore]`d (excluded from
//! `./test`/`./verify`/plain `cargo test`) *and* gated on `TDDY_CLOUDINIT_BASE_IMAGE`
//! — the same config the `tddy-vm-build cloud-init` CLI reads for `--base-image` —
//! pointing at a real cloud-init-compatible qcow2 image (e.g. a Debian genericcloud
//! image). There is no bundled or auto-downloaded image; a developer must supply one
//! explicitly to run this test.
//!
//! Run explicitly with:
//! ```text
//! TDDY_CLOUDINIT_BASE_IMAGE=/path/to/base.qcow2 \
//!   cargo test -p tddy-vm --test cloud_init_acceptance -- --ignored --nocapture
//! ```

use serial_test::serial;
use std::path::PathBuf;
use std::time::Duration;
use tddy_vm::cloud_init::{
    build_cloud_init_image, CloudInitBuildOptions, CloudInitUser, CloudInitUserData, IsoTool,
};
use tddy_vm::qemu::{ensure_uefi_vars_file, resolve_uefi_code_path};
use tddy_vm::vm_manifest::{LoginPolicy, RunPolicy, VmManifest};
use tddy_vm::{UefiFirmware, VmAccel, VmArch, VmLibrary};
use tddy_vm_testkit::BootedGuest;
use tempfile::tempdir;

/// The env var this production test reads its base image path from — the same config
/// knob the `tddy-vm-build cloud-init` CLI's `--base-image` flag reads.
const BASE_IMAGE_ENV: &str = "TDDY_CLOUDINIT_BASE_IMAGE";

/// The account [`a_minimal_cloud_init_user_data`] provisions, named once so the VM that
/// later logs into a baked layer cannot ask for an account the bake never created.
const PROVISIONED_USERNAME: &str = "tddy";

/// Virtual size of every overlay these tests build. A qcow2 layer may not be smaller than
/// the image it chains onto, so the bake and the VM overlay stacked on top of it share one
/// figure rather than each picking their own.
const DISK_SIZE: &str = "10G";

/// Name of the layer [`a_baked_prepared_base_boots_as_a_vm_that_answers_over_ssh`] bakes,
/// which is both the filename under `images/02-prepared-base/` and the `prepared_base` its
/// VM manifest chains onto — one name, so the two cannot disagree.
const PREPARED_BASE_NAME: &str = "cloud-init-bootable";

/// Boot budget for the VM created from the freshly baked layer.
///
/// A bare cloud image reaches a login prompt in ~17 s on an accelerated host
/// (`vm_boot_control_acceptance.rs`); this guest additionally runs cloud-init again to apply
/// its own login seed. 300 s is an order of magnitude over the observed figure, so a merely
/// slow boot does not fail the suite while a guest that never comes up still does.
const VM_BOOT_TIMEOUT: Duration = Duration::from_secs(300);

/// Resolve the base image path from `TDDY_CLOUDINIT_BASE_IMAGE`, or `None` if unset.
fn configured_base_image() -> Option<PathBuf> {
    std::env::var(BASE_IMAGE_ENV).ok().map(PathBuf::from)
}

/// The UEFI firmware pair the bake boots through on this host, with its writable variables
/// store placed in `work_dir`.
///
/// `None` on x86_64, whose `q35` machine boots the image's own bootloader through SeaBIOS;
/// the aarch64 `virt` machine has no BIOS at all, so firmware there is mandatory.
fn a_host_firmware(work_dir: &std::path::Path, name: &str) -> Option<UefiFirmware> {
    let arch = VmArch::host();
    if arch == VmArch::X86_64 {
        return None;
    }
    let vars = work_dir.join(format!("{name}-vars.fd"));
    ensure_uefi_vars_file(&vars).expect("UEFI vars file must be creatable");
    Some(UefiFirmware {
        code_path: resolve_uefi_code_path(arch)
            .expect("UEFI firmware must be resolvable")
            .display()
            .to_string(),
        vars_path: vars.display().to_string(),
    })
}

fn a_minimal_cloud_init_user_data() -> CloudInitUserData {
    CloudInitUserData {
        hostname: Some("cloud-init-acceptance".to_string()),
        users: vec![CloudInitUser {
            name: PROVISIONED_USERNAME.to_string(),
            shell: Some("/bin/bash".to_string()),
            sudo: Some("ALL=(ALL) NOPASSWD:ALL".to_string()),
            ssh_authorized_keys: vec!["{{SSH_PUBLIC_KEY}}".to_string()],
            plain_text_passwd: None,
            lock_passwd: None,
        }],
        packages: vec![],
        runcmd: vec![],
        write_files: vec![],
        bootcmd: vec![],
    }
}

#[tokio::test]
#[ignore = "production test: boots a real QEMU VM to bake cloud-init, ~1-3 min; requires \
            TDDY_CLOUDINIT_BASE_IMAGE (see module docs); run with --ignored"]
#[serial(cloud_init_qemu_vm)]
async fn builds_a_ready_to_use_provisioned_qcow2_by_baking_cloud_init_into_an_overlay() {
    let Some(base_image_src) = configured_base_image() else {
        eprintln!(
            "{BASE_IMAGE_ENV} not set — skipping production test (see module docs to run it)"
        );
        return;
    };

    // Given a scratch directory, a place for the finished layer, and a minimal
    // provisioning spec
    let dir = tempdir().unwrap();
    let overlay_output = a_layer_directory_in(dir.path()).join("cloud-init-demo.qcow2");
    let opts = CloudInitBuildOptions {
        name: "cloud-init-demo".to_string(),
        base_image_src: base_image_src.clone(),
        overlay_output: overlay_output.clone(),
        output_dir: dir.path().to_path_buf(),
        user_data: a_minimal_cloud_init_user_data(),
        disk_size: "10G".to_string(),
        memory: "1024M".to_string(),
        cpus: 1,
        ssh_host_port: 2299,
        timeout: Duration::from_secs(180),
        iso_tool: IsoTool::Xorriso,
        ssh_public_key: None,
        arch: VmArch::host(),
        accel: VmAccel::host_default(),
        firmware: a_host_firmware(dir.path(), "cloud-init-demo"),
        nine_p_shares: vec![],
    };

    // When building the cloud-init image
    let result = build_cloud_init_image(&opts, &|line| eprintln!("{line}")).await;

    // Then it succeeds and returns a provisioned overlay at the path it was told to build,
    // a real qcow2
    let overlay_path = result.expect("cloud-init image build must succeed");
    assert_eq!(overlay_path, overlay_output);
    let magic = std::fs::read(&overlay_path).expect("overlay must be readable");
    assert_eq!(&magic[..4], b"QFI\xfb", "overlay must be a qcow2 image");

    // And the layer holds its own delta rather than a copy of what it derives from. The
    // exact byte count of a baked delta depends on what cloud-init installed, so this
    // compares against the one thing that is certain: a copy could not be smaller
    assert!(
        size_on_disk(&overlay_path) < size_on_disk(&base_image_src),
        "a delta must be smaller than the image it chains onto: {} vs {}",
        size_on_disk(&overlay_path),
        size_on_disk(&base_image_src)
    );
}

#[tokio::test]
#[ignore = "production test: boots a real QEMU VM to bake cloud-init, ~1-3 min; requires \
            TDDY_CLOUDINIT_BASE_IMAGE (see module docs); run with --ignored"]
#[serial(cloud_init_qemu_vm)]
async fn the_overlay_records_the_image_it_was_baked_from_as_a_relative_backing_file() {
    let Some(base_image_src) = configured_base_image() else {
        eprintln!(
            "{BASE_IMAGE_ENV} not set — skipping production test (see module docs to run it)"
        );
        return;
    };

    // Given a completed cloud-init build
    let dir = tempdir().unwrap();
    let overlay_output = a_layer_directory_in(dir.path()).join("cloud-init-backing.qcow2");
    let opts = CloudInitBuildOptions {
        name: "cloud-init-backing".to_string(),
        base_image_src: base_image_src.clone(),
        overlay_output,
        output_dir: dir.path().to_path_buf(),
        user_data: a_minimal_cloud_init_user_data(),
        disk_size: "10G".to_string(),
        memory: "1024M".to_string(),
        cpus: 1,
        ssh_host_port: 2298,
        timeout: Duration::from_secs(180),
        iso_tool: IsoTool::Xorriso,
        ssh_public_key: None,
        arch: VmArch::host(),
        accel: VmAccel::host_default(),
        firmware: a_host_firmware(dir.path(), "cloud-init-backing"),
        nine_p_shares: vec![],
    };
    let overlay_path = build_cloud_init_image(&opts, &|line| eprintln!("{line}"))
        .await
        .expect("cloud-init image build must succeed");

    // When inspecting the overlay's backing file via qemu-img info
    let backing = backing_file_of(&overlay_path);

    // Then the reference is relative, so the layer travels with its ancestors when the
    // library moves
    assert!(
        !PathBuf::from(&backing).is_absolute(),
        "the backing reference must be relative, got: {backing}"
    );

    // And it resolves — against the overlay's own directory, which is how qcow2 reads it —
    // back to the exact image the bake chained onto
    let overlay_dir = overlay_path
        .parent()
        .expect("the overlay must live in a directory");
    assert_eq!(
        overlay_dir
            .join(&backing)
            .canonicalize()
            .expect("the backing reference must resolve to a real file"),
        base_image_src
            .canonicalize()
            .expect("the base image must still be readable")
    );
}

#[tokio::test]
#[ignore = "production test: bakes cloud-init into an overlay and then boots a VM off it, \
            ~2-5 min; requires TDDY_CLOUDINIT_BASE_IMAGE (see module docs); run with \
            --ignored"]
#[serial(cloud_init_qemu_vm)]
async fn a_baked_prepared_base_boots_as_a_vm_that_answers_over_ssh() {
    let Some(base_image_src) = configured_base_image() else {
        eprintln!(
            "{BASE_IMAGE_ENV} not set — skipping production test (see module docs to run it)"
        );
        return;
    };

    // Given a library whose prepared base was baked from the supplied cloud image. The bake
    // is the subject here, so it runs for real rather than being stood in for: nothing but a
    // bake produces the layer the rest of this test boots.
    let dir = tempdir().unwrap();
    let library = VmLibrary::new(dir.path().join("library"));
    library.init().expect("the library tree must be creatable");
    let opts = CloudInitBuildOptions {
        name: PREPARED_BASE_NAME.to_string(),
        base_image_src,
        overlay_output: library
            .prepared_base_dir()
            .join(format!("{PREPARED_BASE_NAME}.qcow2")),
        output_dir: dir.path().join("bake"),
        user_data: a_minimal_cloud_init_user_data(),
        disk_size: DISK_SIZE.to_string(),
        memory: "1024M".to_string(),
        cpus: 1,
        ssh_host_port: 2297,
        timeout: Duration::from_secs(180),
        iso_tool: IsoTool::Xorriso,
        ssh_public_key: None,
        arch: VmArch::host(),
        accel: VmAccel::host_default(),
        firmware: a_host_firmware(dir.path(), PREPARED_BASE_NAME),
        nine_p_shares: vec![],
    };
    build_cloud_init_image(&opts, &|line| eprintln!("{line}"))
        .await
        .expect("cloud-init image build must succeed");

    // When a VM created from that base is booted.
    //
    // The manifest is read back rather than reused: `create_vm` generates the per-VM keypair
    // and login seed itself and records their paths in the manifest it persists, so the
    // manifest that comes out of the library is the only one that knows which private key
    // opens this guest.
    let requested = a_vm_manifest("cloud-init-vm", PREPARED_BASE_NAME, 2296);
    library
        .create_vm(&requested)
        .await
        .expect("a VM must be creatable from the freshly baked prepared base");
    let manifest = library
        .read_manifest(&requested.name)
        .expect("create_vm must have persisted a manifest naming the per-VM key");

    let guest = BootedGuest::boot(&library, &manifest, vec![])
        .await
        .expect("a VM created from the baked prepared base must boot");

    // Then sshd in the guest answers. Slirp accepts the forwarded connection from the moment
    // QEMU starts, so reaching the port proves nothing; only a command the guest ran does.
    guest
        .wait_for_ssh_ready(VM_BOOT_TIMEOUT)
        .await
        .expect("sshd must start answering in a guest baked from this cloud-init layer");

    // And it authenticates as the account the bake's cloud-init provisioned — which is what
    // separates an image that merely boots from one that was actually provisioned
    guest
        .run_over_ssh("id -un")
        .await
        .expect("ssh must authenticate with the per-VM key and run the command")
        .assert_stdout_line(PROVISIONED_USERNAME);

    guest.shutdown().await.expect("guest must shut down");
}

/// The manifest of a throwaway VM chained onto `prepared_base`, logging in as the account
/// the bake provisioned.
///
/// Sized like the boot-control suite's guests — 2 vCPUs and 2 GiB — because this one has to
/// run cloud-init a second time to apply its own login seed before sshd will take the key.
fn a_vm_manifest(name: &str, prepared_base: &str, ssh_host_port: u16) -> VmManifest {
    VmManifest {
        name: name.to_string(),
        prepared_base: Some(prepared_base.to_string()),
        image_path: None,
        run: RunPolicy {
            memory: "2048M".to_string(),
            cpus: 2,
            disk_size: DISK_SIZE.to_string(),
            ssh_host_port,
            port_forwards: vec![],
            arch: VmArch::host(),
            accel: VmAccel::host_default(),
        },
        login: LoginPolicy {
            username: PROVISIONED_USERNAME.to_string(),
            ssh_private_key: None,
            ssh_public_key: None,
        },
    }
}

/// A library-shaped `images/02-prepared-base/` directory under `root`, so a layer is built
/// where a layer belongs — and, more importantly, is built where it will stay.
fn a_layer_directory_in(root: &std::path::Path) -> PathBuf {
    let dir = root.join("images/02-prepared-base");
    std::fs::create_dir_all(&dir).expect("the layer directory must be creatable");
    dir
}

/// Bytes `image` occupies, which for a qcow2 delta is its own content only.
fn size_on_disk(image: &std::path::Path) -> u64 {
    std::fs::metadata(image)
        .expect("the image must exist")
        .len()
}

/// The backing reference `qemu-img info` reports as recorded in `image` — without the
/// ` (actual path: …)` annotation qemu-img adds for a relative one, which describes where
/// the file sits today rather than what the image records.
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
