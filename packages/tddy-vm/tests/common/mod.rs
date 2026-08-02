#![allow(dead_code)] // each test binary uses a subset of these helpers.
//! Shared support for the real-boot VM acceptance tests.
//!
//! These helpers compose `tddy-vm`'s public API into page objects so each test reads as
//! Given / When / Then rather than as QEMU plumbing. Everything here is production-test
//! support: it boots real `qemu-system-*` processes.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tddy_vm::cloud_init::{
    iso_tool_command, render_meta_data, render_user_data_without_completion, CloudInitUser,
    CloudInitUserData, IsoTool, NinePShare,
};
use tddy_vm::qemu::{ensure_uefi_vars_file, resolve_uefi_code_path, QemuVm};
use tddy_vm::vm::{PortForward, UefiFirmware, VmAccel, VmArch, VmConfig, VmLogin};
use tddy_vm::vm_manifest::{LoginPolicy, RunPolicy, VmManifest};

/// The env var these production tests read their base image path from — the same knob the
/// `tddy-vm-build cloud-init` CLI's `--base-image` flag reads. There is no bundled or
/// auto-downloaded image.
pub const BASE_IMAGE_ENV: &str = "TDDY_CLOUDINIT_BASE_IMAGE";

/// The env var naming an already-baked tddy-host prepared base, for the tests that consume
/// one instead of spending hours baking it.
pub const PREPARED_BASE_ENV: &str = "TDDY_TDDY_HOST_PREPARED_BASE";

/// Resolve the base image path from [`BASE_IMAGE_ENV`], or `None` when unset.
pub fn configured_base_image() -> Option<PathBuf> {
    std::env::var(BASE_IMAGE_ENV).ok().map(PathBuf::from)
}

/// Resolve the prepared-base path from [`PREPARED_BASE_ENV`], or `None` when unset.
pub fn configured_prepared_base() -> Option<PathBuf> {
    std::env::var(PREPARED_BASE_ENV).ok().map(PathBuf::from)
}

/// The guest login these tests drive the serial console with.
pub const GUEST_USERNAME: &str = "tddy";
pub const GUEST_PASSWORD: &str = "tddy-acceptance";

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
        let vars = self.work_dir.join(format!("{}-vars.fd", self.name));

        create_overlay(&self.base_image, &overlay, &self.run.disk_size).await;
        write_seed_iso(&self.work_dir, &seed_iso, &self.name, &self.user_data).await;
        ensure_uefi_vars_file(&vars).expect("UEFI vars file must be creatable");

        let firmware = UefiFirmware {
            code_path: resolve_uefi_code_path(self.run.arch)
                .expect("UEFI firmware must be resolvable for the acceptance tests")
                .display()
                .to_string(),
            vars_path: vars.display().to_string(),
        };

        let config = VmConfig {
            qcow2_path: overlay.display().to_string(),
            extra_hostfwd: self.run.port_forwards.clone(),
            ssh_host_port: self.run.ssh_host_port,
            arch: self.run.arch,
            accel: self.run.accel,
            memory: self.run.memory.clone(),
            cpus: self.run.cpus,
            firmware: Some(firmware),
            login: VmLogin {
                username: self.login.username.clone(),
                private_key_path: self.login.ssh_private_key.clone(),
            },
            seed_iso: Some(seed_iso.display().to_string()),
            nine_p_shares: self.nine_p_shares.clone(),
        };

        let qemu = QemuVm::new();
        let booted = qemu
            .boot_with_serial_console(&config)
            .await
            .expect("guest must boot");

        BootedGuest {
            qemu,
            running: Some(booted.vm),
            console: booted.console,
            ssh_host_port: self.run.ssh_host_port,
        }
    }
}

/// A running guest with a driveable serial console.
pub struct BootedGuest {
    qemu: QemuVm,
    running: Option<tddy_vm::vm::RunningVm>,
    pub console: tddy_vm::serial_shell::SerialConsole,
    pub ssh_host_port: u16,
}

impl BootedGuest {
    /// Log in on the serial console as the acceptance-test user, then stop the kernel
    /// printing to that console.
    ///
    /// The quiesce is not cosmetic. `ttyAMA0` is shared between the login shell and the
    /// kernel log, so a `printk` landing mid-command is captured as another line of that
    /// command's output — observed for real as `[ 7.790602] 9p: Installing v9fs 9p2000 file
    /// system support` arriving in the middle of a `mount`. Any test asserting on exact
    /// command output is otherwise only deterministic when no kernel message happens to
    /// coincide with it, which is a flake waiting to happen rather than a passing test.
    ///
    /// `dmesg -n 1` limits console output to panics and leaves everything still readable via
    /// `dmesg`/journald, so nothing is actually lost.
    pub async fn login_on_console(&mut self) {
        self.console
            .login(GUEST_USERNAME, GUEST_PASSWORD, Duration::from_secs(180))
            .await
            .expect("serial console must reach a login prompt and accept credentials");
        let quiesced = self
            .console
            .run_command("sudo dmesg -n 1", Duration::from_secs(60))
            .await
            .expect("the kernel console level must be settable over the serial console");
        assert_eq!(
            quiesced.exit_code, 0,
            "quiescing the kernel console failed: {:?}",
            quiesced.stdout_lines
        );
    }

    /// Run a command in the guest over SSH, as the manifest's `LoginPolicy` user with its
    /// per-VM private key. This is how a real caller reaches a provisioned VM; the serial
    /// console is for guests that have no working login yet.
    ///
    /// Retries until the command succeeds or `timeout` elapses. A plain port check is not a
    /// readiness signal here: QEMU's slirp networking accepts a forwarded connection
    /// immediately whether or not anything in the guest is listening, so the first attempt
    /// typically lands before sshd has finished starting and fails the banner exchange.
    pub async fn run_over_ssh_within(
        &self,
        command: &str,
        timeout: Duration,
    ) -> tddy_vm::vm::VerifyResult {
        use tddy_vm::Vm;
        let running = self.running.as_ref().expect("guest must still be running");
        let deadline = tokio::time::Instant::now() + timeout;
        let mut last = self
            .qemu
            .verify(running, command)
            .await
            .expect("SSH command must execute");
        while last.exit_code != 0 && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_secs(2)).await;
            last = self
                .qemu
                .verify(running, command)
                .await
                .expect("SSH command must execute");
        }
        last
    }

    /// [`Self::run_over_ssh_within`] with the budget the acceptance tests use for a guest
    /// that has just booted.
    pub async fn run_over_ssh(&self, command: &str) -> tddy_vm::vm::VerifyResult {
        self.run_over_ssh_within(command, Duration::from_secs(180))
            .await
    }

    /// Shut the guest down gracefully via the QEMU monitor and wait for it to actually go.
    ///
    /// The wait matters: `system_powerdown` is an asynchronous ACPI request, so without it
    /// the enclosing `tempdir()` deletes the guest's disk out from under a still-running
    /// QEMU, and the next test on the same port races a process that has not released it.
    pub async fn shutdown(mut self) {
        use tddy_vm::Vm;
        let running = self.running.take().expect("guest must still be running");
        let (pid, monitor_socket) = (running.pid, running.monitor_socket.clone());
        let ssh_host_port = self.ssh_host_port;

        // `running` has to move into `shutdown`, which disarms the Drop guard — so any
        // failure from here on re-does by hand what the guard would have done, rather than
        // orphaning the VM on precisely the path the guard exists for.
        if let Err(e) = self.qemu.shutdown(running).await {
            force_kill(pid, &monitor_socket);
            panic!("guest must shut down gracefully: {e}");
        }
        if !wait_for_port_release(ssh_host_port, Duration::from_secs(90)).await {
            force_kill(pid, &monitor_socket);
            panic!("guest accepted the powerdown but never released port {ssh_host_port}");
        }
        let _ = std::fs::remove_file(&monitor_socket);
    }
}

impl Drop for BootedGuest {
    /// Kill the VM if the test never reached its `shutdown()` — an assertion that fires
    /// mid-test would otherwise orphan a QEMU process still holding this guest's forwarded
    /// ports, and the next run of the same test would fail to boot for an unrelated reason.
    fn drop(&mut self) {
        let Some(running) = self.running.take() else {
            return;
        };
        force_kill(running.pid, &running.monitor_socket);
    }
}

/// Last-resort teardown: kill the emulator and clear its monitor socket.
///
/// Best-effort by nature — it runs from `Drop` and from panic paths, where there is nobody
/// left to report a failure to. It is only ever reached for a VM this process spawned and
/// has not yet shut down, so the PID is still ours to kill.
fn force_kill(pid: u32, monitor_socket: &str) {
    let _ = std::process::Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status();
    let _ = std::fs::remove_file(monitor_socket);
}

/// Poll until nothing is listening on `port`, returning whether that happened in time.
async fn wait_for_port_release(port: u16, timeout: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    while tokio::time::Instant::now() < deadline {
        if tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_err()
        {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    false
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

/// Boot a VM that already exists in `library` (overlay + manifest written by
/// [`tddy_vm::library::VmLibrary::create_vm`]), with its serial console routed to a pipe
/// this process drives.
pub async fn boot_library_vm(
    library: &tddy_vm::library::VmLibrary,
    manifest: &VmManifest,
    boot_timeout: Duration,
) -> BootedGuest {
    let vm_dir = library.vm_dir(&manifest.name);
    let overlay = vm_dir.join(format!("{}.qcow2", manifest.name));
    let vars = vm_dir.join(format!("{}-vars.fd", manifest.name));
    ensure_uefi_vars_file(&vars).expect("UEFI vars file must be creatable");

    let firmware = UefiFirmware {
        code_path: resolve_uefi_code_path(manifest.run.arch)
            .expect("UEFI firmware must be resolvable for the acceptance tests")
            .display()
            .to_string(),
        vars_path: vars.display().to_string(),
    };

    let config = VmConfig {
        qcow2_path: overlay.display().to_string(),
        extra_hostfwd: manifest.run.port_forwards.clone(),
        ssh_host_port: manifest.run.ssh_host_port,
        arch: manifest.run.arch,
        accel: manifest.run.accel,
        memory: manifest.run.memory.clone(),
        cpus: manifest.run.cpus,
        firmware: Some(firmware),
        login: VmLogin {
            username: manifest.login.username.clone(),
            private_key_path: manifest.login.ssh_private_key.clone(),
        },
        seed_iso: None,
        nine_p_shares: vec![],
    };

    let qemu = QemuVm::new();
    let mut booted = qemu
        .boot_with_serial_console(&config)
        .await
        .expect("library VM must boot");
    booted
        .console
        .wait_for_login_prompt(boot_timeout)
        .await
        .expect("library VM must reach a login prompt");

    BootedGuest {
        qemu,
        running: Some(booted.vm),
        console: booted.console,
        ssh_host_port: manifest.run.ssh_host_port,
    }
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
