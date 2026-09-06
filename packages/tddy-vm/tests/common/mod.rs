#![allow(dead_code)] // each test binary uses a subset of these helpers.
//! Shared support for the real-boot VM acceptance tests.
//!
//! Only what `tddy-vm-testkit` has no answer for lives here. The booted guest itself — the
//! serial console, the SSH channel, the graceful shutdown and the `Drop` guard that kills a
//! QEMU orphaned by a panicking test — comes from [`tddy_vm_testkit::BootedGuest`], which
//! `tddy-vm` takes as a dev-dependency. (Cargo permits that cycle: the testkit depends on
//! this crate's library, and only this crate's *tests* depend back on the testkit.)
//!
//! What remains is [`TestGuestBuilder`], which has no testkit equivalent because it bakes a
//! per-test seed ISO from a **raw** cloud image with the non-halting renderer — the bake
//! renderer's completion script powers the guest off as soon as cloud-init finishes, which
//! is right for baking a layer and fatal for a guest a test is about to drive.

use std::path::{Path, PathBuf};

use tddy_vm::cloud_init::{
    iso_tool_command, render_meta_data, render_user_data_without_completion, CloudInitUser,
    CloudInitUserData, IsoTool, NinePShare,
};
use tddy_vm::qemu::uefi_firmware_for;
use tddy_vm::vm::{PortForward, VmAccel, VmArch, VmConfig, VmLogin};
use tddy_vm::vm_manifest::{LoginPolicy, RunPolicy, VmManifest};
use tddy_vm_testkit::recipes::{GUEST_PASSWORD, TDDY_SERVICE_USERNAME};
use tddy_vm_testkit::BootedGuest;

/// The base image path, or a panic naming the variable that is missing.
///
/// Re-exported from the testkit rather than restated, so the env var these tests read and
/// the one the testkit reads cannot drift. A production test with no image configured has
/// not been satisfied, it has been skipped — and a skip that returns normally is reported
/// as a pass, which is why this panics. See `tddy_vm_testkit::require_env_path`.
pub use tddy_vm_testkit::require_base_image;

/// The env var naming an already-baked tddy-host prepared base, for the tests that consume
/// one instead of spending hours baking it.
pub const PREPARED_BASE_ENV: &str = "TDDY_TDDY_HOST_PREPARED_BASE";

/// The prepared-base path, or a panic naming the variable that is missing — the
/// [`require_base_image`] of the tests that consume an already-baked base.
pub fn require_prepared_base() -> PathBuf {
    tddy_vm_testkit::require_env_path(PREPARED_BASE_ENV)
}

/// The guest login these tests drive the serial console with — the testkit's own service
/// account and password, so one credential opens every guest this workspace boots.
pub const GUEST_USERNAME: &str = TDDY_SERVICE_USERNAME;

/// A cloud-init spec whose user can log in on the serial console with a password — the
/// Debian cloud images ship no password at all, so the serial-console tests cannot log in
/// without this.
pub fn a_console_loginable_user_data(hostname: &str) -> CloudInitUserData {
    CloudInitUserData {
        hostname: Some(hostname.to_string()),
        users: vec![CloudInitUser {
            name: GUEST_USERNAME.to_string(),
            shell: Some("/bin/bash".to_string()),
            sudo: Some("ALL=(ALL) NOPASSWD:ALL".to_string()),
            ssh_authorized_keys: vec!["{{SSH_PUBLIC_KEY}}".to_string()],
            plain_text_passwd: Some(GUEST_PASSWORD.to_string()),
            lock_passwd: Some(false),
        }],
        packages: vec![],
        runcmd: vec![],
        write_files: vec![],
        bootcmd: vec![],
    }
}

/// A run policy sized for the acceptance tests, targeting the host's own architecture with
/// hardware acceleration.
pub fn an_acceptance_run_policy(ssh_host_port: u16) -> RunPolicy {
    RunPolicy {
        memory: "2048M".to_string(),
        cpus: 2,
        disk_size: "8G".to_string(),
        ssh_host_port,
        port_forwards: vec![],
        arch: VmArch::host(),
        accel: VmAccel::host_default(),
    }
}

/// Builder for a real, booted guest under test.
pub struct TestGuestBuilder {
    base_image: PathBuf,
    work_dir: PathBuf,
    name: String,
    user_data: CloudInitUserData,
    nine_p_shares: Vec<NinePShare>,
    run: RunPolicy,
    login: LoginPolicy,
}

/// Start describing a guest booted from `base_image`, with scratch artifacts under
/// `work_dir`.
pub fn a_test_guest(base_image: &Path, work_dir: &Path, name: &str) -> TestGuestBuilder {
    TestGuestBuilder {
        base_image: base_image.to_path_buf(),
        work_dir: work_dir.to_path_buf(),
        name: name.to_string(),
        user_data: a_console_loginable_user_data(name),
        nine_p_shares: vec![],
        run: an_acceptance_run_policy(2222),
        login: LoginPolicy {
            username: GUEST_USERNAME.to_string(),
            ssh_private_key: None,
            ssh_public_key: None,
        },
    }
}

impl TestGuestBuilder {
    pub fn with_user_data(mut self, user_data: CloudInitUserData) -> Self {
        self.user_data = user_data;
        self
    }

    pub fn with_ssh_host_port(mut self, port: u16) -> Self {
        self.run.ssh_host_port = port;
        self
    }

    pub fn with_read_only_nine_p_share(mut self, host_path: &Path, mount_tag: &str) -> Self {
        self.nine_p_shares.push(NinePShare {
            host_path: host_path.display().to_string(),
            mount_tag: mount_tag.to_string(),
            writable: false,
        });
        self
    }

    pub fn with_ssh_key_login(mut self, private_key: &Path, public_key: &Path) -> Self {
        self.login.ssh_private_key = Some(private_key.display().to_string());
        self.login.ssh_public_key = Some(public_key.display().to_string());
        self
    }

    /// Build the overlay, seed ISO, and UEFI vars, then boot the guest with its serial
    /// console routed to a pipe this process drives.
    pub async fn boot(self) -> BootedGuest {
        let overlay = self.work_dir.join(format!("{}.qcow2", self.name));
        let seed_iso = self.work_dir.join(format!("{}-seed.iso", self.name));

        create_overlay(&self.base_image, &overlay, &self.run.disk_size).await;
        write_seed_iso(&self.work_dir, &seed_iso, &self.name, &self.user_data).await;

        // `uefi_firmware_for` rather than a hand-assembled pair, because it is
        // arch-aware and this is not: it returns None on x86_64, whose q35
        // machine boots the image's own bootloader through SeaBIOS. Attaching
        // firmware there fails, since the writable vars store is 64 MiB — the
        // aarch64 `virt` pflash convention — and QEMU caps combined x86 system
        // firmware at 8 MiB.
        let firmware = uefi_firmware_for(self.run.arch, &self.work_dir, &self.name)
            .expect("UEFI firmware must be resolvable for the acceptance tests");

        let config = VmConfig {
            qcow2_path: overlay.display().to_string(),
            extra_hostfwd: self.run.port_forwards.clone(),
            ssh_host_port: self.run.ssh_host_port,
            arch: self.run.arch,
            accel: self.run.accel,
            memory: self.run.memory.clone(),
            cpus: self.run.cpus,
            firmware,
            login: VmLogin {
                username: self.login.username.clone(),
                private_key_path: self.login.ssh_private_key.clone(),
            },
            seed_iso: Some(seed_iso.display().to_string()),
            nine_p_shares: self.nine_p_shares.clone(),
        };

        BootedGuest::boot_config(&config)
            .await
            .expect("guest must boot")
    }
}

async fn create_overlay(base: &Path, overlay: &Path, disk_size: &str) {
    let args = tddy_vm::library::vm_overlay_create_argv(base, overlay, disk_size);
    let status = tokio::process::Command::new("qemu-img")
        .args(&args)
        .status()
        .await
        .expect("qemu-img must be on PATH");
    assert!(status.success(), "qemu-img create must succeed: {args:?}");
}

async fn write_seed_iso(
    work_dir: &Path,
    seed_iso: &Path,
    name: &str,
    user_data: &CloudInitUserData,
) {
    let nocloud = work_dir.join(format!("{name}-seed"));
    std::fs::create_dir_all(&nocloud).expect("seed dir must be creatable");

    // Deliberately not the bake renderer: its completion script halts the guest as soon as
    // cloud-init finishes, which would power the VM off mid-test.
    std::fs::write(
        nocloud.join("user-data"),
        render_user_data_without_completion(user_data, ""),
    )
    .expect("user-data must be writable");
    std::fs::write(nocloud.join("meta-data"), render_meta_data(name, name))
        .expect("meta-data must be writable");

    let (program, args) = iso_tool_command(IsoTool::Xorriso, seed_iso, &nocloud);
    let status = tokio::process::Command::new(&program)
        .args(&args)
        .status()
        .await
        .expect("an ISO tool must be on PATH");
    assert!(status.success(), "seed ISO creation must succeed");
}

/// Build the manifest a tddy-host VM is created from.
pub fn a_tddy_host_manifest(name: &str, prepared_base: &str, ssh_host_port: u16) -> VmManifest {
    VmManifest {
        name: name.to_string(),
        prepared_base: Some(prepared_base.to_string()),
        image_path: None,
        run: RunPolicy {
            port_forwards: vec![PortForward {
                host_port: ssh_host_port + 1000,
                guest_port: 8080,
            }],
            ..an_acceptance_run_policy(ssh_host_port)
        },
        login: LoginPolicy {
            username: GUEST_USERNAME.to_string(),
            ssh_private_key: None,
            ssh_public_key: None,
        },
    }
}
