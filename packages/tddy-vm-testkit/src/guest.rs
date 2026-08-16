//! A booted guest, with a serial console, an SSH channel, and teardown that survives a
//! panicking test.
//!
//! The one booted-guest fixture in the workspace: `packages/tddy-vm/tests/common/mod.rs`
//! assembles its own overlay and seed ISO and then boots *this* through
//! [`BootedGuest::boot_config`], rather than keeping a second copy of the teardown.
//!
//! The `Drop` guard is the part that cannot be skipped: unlike a testcontainers-backed
//! testkit, nothing stops a QEMU process on its own, so an assertion firing mid-test
//! would otherwise orphan an emulator still holding this guest's forwarded ports — and
//! the *next* run would then fail to boot for a reason that has nothing to do with the
//! code under test.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use tddy_vm::cloud_init::NinePShare;
use tddy_vm::qemu::{scp_to_guest, uefi_firmware_for, QemuVm};
use tddy_vm::serial_shell::SerialConsole;
use tddy_vm::vm::{RunningVm, VerifyResult, VmConfig, VmLogin};
use tddy_vm::vm_manifest::VmManifest;
use tddy_vm::Vm;

use crate::recipes::GUEST_PASSWORD;

/// How long a guest gets to reach a login prompt.
pub const BOOT_TIMEOUT: Duration = Duration::from_secs(300);

/// How long a graceful powerdown gets to actually release the port.
pub const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(120);

/// How long sshd gets to start answering after a boot.
pub const SSH_READY_TIMEOUT: Duration = Duration::from_secs(300);

/// How long an ordinary command gets before it is treated as hung. Generous, because a
/// command that has not returned is indistinguishable from one still working — but finite,
/// so a wedged guest fails the test instead of the suite.
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(300);

/// The gap between attempts while polling.
const SSH_POLL_INTERVAL: Duration = Duration::from_secs(2);

/// The command a readiness probe runs: it does nothing, so its exit code says only whether
/// sshd answered and authenticated.
const SSH_READINESS_PROBE: &str = "true";

/// What one command run in a guest produced: its two streams, kept apart, and the status it
/// exited with.
///
/// The separation is the point. Concatenated into one string — which is what this used to
/// be handed — "assert on what the tool printed" cannot be stated at all: sshd's host-key
/// banner, a `sudo` warning and the tool's own diagnostics land in the same text as the
/// answer, so the assertion passes or fails for reasons unrelated to the behaviour under
/// test.
#[derive(Debug, Clone)]
pub struct GuestCommandOutput {
    command: String,
    stdout: String,
    stderr: String,
    exit_code: i32,
}

impl GuestCommandOutput {
    /// Report `result` as the outcome of running `command` in the guest.
    pub fn from_ssh(command: &str, result: VerifyResult) -> Self {
        Self {
            command: command.to_string(),
            stdout: result.stdout,
            stderr: result.stderr,
            exit_code: result.exit_code,
        }
    }

    /// What the command answered, byte for byte.
    pub fn stdout(&self) -> &str {
        &self.stdout
    }

    /// What the command reported alongside its answer.
    pub fn stderr(&self) -> &str {
        &self.stderr
    }

    /// The status the command exited with in the guest.
    pub fn exit_code(&self) -> i32 {
        self.exit_code
    }

    /// Assert the command succeeded, reporting both streams if it did not.
    pub fn assert_succeeded(&self) -> &Self {
        assert_eq!(self.exit_code, 0, "{self}");
        self
    }

    /// Assert the command's answer is exactly `expected`, as a reader would have seen it.
    ///
    /// Surrounding whitespace is not part of the answer: the newline every shell appends and
    /// the column padding a tool like `ps` emits are artefacts of how the text was printed,
    /// not of what was said. Everything between the first and last non-space character is
    /// compared exactly.
    pub fn assert_stdout_line(&self, expected: &str) -> &Self {
        assert_eq!(
            self.stdout.trim(),
            expected,
            "`{}` printed {:?} on stdout, expected {expected:?}{}",
            self.command,
            self.stdout,
            self.stderr_note()
        );
        self
    }

    /// ` (stderr: …)`, or nothing at all when the command wrote none — so a failure message
    /// carries the diagnostic that explains it without inventing one when there is none.
    fn stderr_note(&self) -> String {
        if self.stderr.is_empty() {
            return String::new();
        }
        format!(" (stderr: {:?})", self.stderr)
    }
}

impl std::fmt::Display for GuestCommandOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "`{}` exited {} in the guest\nstdout:\n{}\nstderr:\n{}",
            self.command, self.exit_code, self.stdout, self.stderr
        )
    }
}

/// A running guest under the testkit's control.
pub struct BootedGuest {
    qemu: QemuVm,
    running: Option<RunningVm>,
    /// The serial console, for everything that happens before sshd exists.
    pub console: SerialConsole,
    ssh_host_port: u16,
}

impl BootedGuest {
    /// Boot the VM named by `manifest`, which must already have an overlay in `library`.
    ///
    /// `nine_p_shares` is why this exists rather than `VmManager::start`, which hardcodes
    /// `seed_iso: None` and `nine_p_shares: vec![]` (`registry.rs:286-287`) and so cannot
    /// carry a share into a guest at all.
    pub async fn boot(
        library: &tddy_vm::VmLibrary,
        manifest: &VmManifest,
        nine_p_shares: Vec<NinePShare>,
    ) -> Result<Self> {
        let vm_dir = library.vm_dir(&manifest.name);
        let overlay = vm_dir.join(format!("{}.qcow2", manifest.name));

        // Arch-aware: None on x86_64, whose q35 machine boots the image's own
        // bootloader through SeaBIOS. Hand-assembling the pair here instead
        // attaches a 64 MiB vars store — the aarch64 `virt` pflash convention —
        // which QEMU rejects on x86, where combined system firmware is capped
        // at 8 MiB.
        let firmware = uefi_firmware_for(manifest.run.arch, &vm_dir, &manifest.name)
            .context("resolving UEFI firmware")?;

        let config = VmConfig {
            qcow2_path: overlay.display().to_string(),
            extra_hostfwd: manifest.run.port_forwards.clone(),
            ssh_host_port: manifest.run.ssh_host_port,
            arch: manifest.run.arch,
            accel: manifest.run.accel,
            memory: manifest.run.memory.clone(),
            cpus: manifest.run.cpus,
            firmware,
            login: VmLogin {
                username: manifest.login.username.clone(),
                private_key_path: manifest.login.ssh_private_key.clone(),
            },
            // The VM's own NoCloud seed, written next to its overlay by
            // `VmLibrary::create_vm`. Without it the guest authorizes only the key the
            // prepared base was *baked* with, and `manifest.login.ssh_private_key` — a
            // fresh per-VM key — opens nothing.
            seed_iso: Some(
                library
                    .vm_seed_iso_path(&manifest.name)
                    .display()
                    .to_string(),
            ),
            nine_p_shares,
        };

        Self::boot_config(&config)
            .await
            .map_err(|e| anyhow!("booting {}: {e}", manifest.name))
    }

    /// Boot a config the caller assembled itself, with the serial console routed to a pipe
    /// this process drives.
    ///
    /// The seam under [`Self::boot`], for a guest whose overlay, seed ISO and firmware were
    /// not produced by [`tddy_vm::VmLibrary::create_vm`] — a bare cloud image booted with a
    /// one-off seed, say. Everything after the boot is identical, teardown guard included,
    /// which is the point of sharing the type rather than the code.
    pub async fn boot_config(config: &VmConfig) -> Result<Self> {
        let qemu = QemuVm::new();
        let booted = qemu
            .boot_with_serial_console(config)
            .await
            .map_err(|e| anyhow!("booting a guest on port {}: {e}", config.ssh_host_port))?;

        Ok(Self {
            qemu,
            running: Some(booted.vm),
            console: booted.console,
            ssh_host_port: config.ssh_host_port,
        })
    }

    /// The host port the guest's SSH is forwarded to.
    pub fn ssh_host_port(&self) -> u16 {
        self.ssh_host_port
    }

    /// Wait for the guest to reach a login prompt on the serial console.
    ///
    /// Distinct from [`Self::wait_for_ssh_ready`]: this is the earliest signal a guest is up
    /// at all, and it needs no networking, no sshd and no authorized key.
    pub async fn wait_for_login_prompt(&mut self, timeout: Duration) -> Result<()> {
        self.console
            .wait_for_login_prompt(timeout)
            .await
            .map_err(|e| anyhow!("waiting for a login prompt on the serial console: {e}"))
    }

    fn running(&self) -> Result<&RunningVm> {
        self.running
            .as_ref()
            .ok_or_else(|| anyhow!("guest is no longer running"))
    }

    /// Log in on the serial console, then stop the kernel writing to it.
    ///
    /// The quiesce is not cosmetic. The console is shared between the login shell and the
    /// kernel log, so a `printk` landing mid-command is captured as another line of that
    /// command's output. `dmesg -n 1` limits the console to panics and leaves everything
    /// readable via `dmesg`/journald, so nothing is lost.
    pub async fn login_on_console(&mut self, username: &str) -> Result<()> {
        self.console
            .login(username, GUEST_PASSWORD, BOOT_TIMEOUT)
            .await
            .map_err(|e| anyhow!("serial console login as {username}: {e}"))?;
        let quiesced = self
            .console
            .run_command("sudo dmesg -n 1", Duration::from_secs(60))
            .await
            .map_err(|e| anyhow!("quiescing the kernel console: {e}"))?;
        if quiesced.exit_code != 0 {
            return Err(anyhow!(
                "quiescing the kernel console failed: {:?}",
                quiesced.stdout_lines
            ));
        }
        Ok(())
    }

    /// Run a command on the serial console, failing on a non-zero exit.
    ///
    /// The console, not SSH, is what long build steps run over: it needs no networking and
    /// it is the only channel that still works when a step has just restarted the guest's
    /// network or swapped its kernel.
    pub async fn run_on_console(
        &mut self,
        command: &str,
        timeout: Duration,
    ) -> Result<Vec<String>> {
        let output = self
            .console
            .run_command(command, timeout)
            .await
            .map_err(|e| anyhow!("running `{command}` on the serial console: {e}"))?;
        if output.exit_code != 0 {
            return Err(anyhow!(
                "`{command}` exited {} in the guest:\n{}",
                output.exit_code,
                output.stdout_lines.join("\n")
            ));
        }
        Ok(output.stdout_lines)
    }

    /// Wait until sshd in the guest actually answers.
    ///
    /// This, and not a port check, is what "SSH is up" means here: QEMU's slirp networking
    /// accepts a forwarded connection whether or not anything in the guest is listening, so
    /// the host-side port is open from the moment QEMU starts. The probe is a command whose
    /// only job is to exit zero, retried because the first attempts after boot routinely
    /// land before sshd has finished starting.
    ///
    /// Every non-idempotent step runs *after* this returns, exactly once — which is the
    /// whole point of separating the two.
    pub async fn wait_for_ssh_ready(&self, timeout: Duration) -> Result<()> {
        let probe = self
            .run_over_ssh_until_success(SSH_READINESS_PROBE, timeout)
            .await?;
        if probe.exit_code() != 0 {
            return Err(anyhow!(
                "sshd never answered on host port {} within {timeout:?}: {probe}",
                self.ssh_host_port,
            ));
        }
        Ok(())
    }

    /// Run a command over SSH **once**, whatever it exits with, failing if it has not
    /// finished within `timeout`.
    ///
    /// The default for anything that changes the guest. Re-running a failed `./install` is
    /// not a readiness probe: it repeats work that already half-happened and it reports only
    /// the last attempt, so the failure that mattered — the first one — is the one nobody
    /// ever sees. Call [`Self::wait_for_ssh_ready`] once after boot instead.
    pub async fn run_over_ssh_once(
        &self,
        command: &str,
        timeout: Duration,
    ) -> Result<GuestCommandOutput> {
        tokio::time::timeout(timeout, self.ssh_attempt(command))
            .await
            .map_err(|_| anyhow!("`{command}` had not finished in the guest after {timeout:?}"))?
    }

    /// Run a command over SSH once and require it to succeed, reporting what it printed.
    ///
    /// The ad-hoc channel a test asserts on: one execution, stdout and stderr kept apart, so
    /// `guest.run_over_ssh("tddy-tools …").await?.assert_stdout_line("…")` asserts on the
    /// tool's answer and nothing else.
    pub async fn run_over_ssh(&self, command: &str) -> Result<GuestCommandOutput> {
        let result = self.run_over_ssh_once(command, COMMAND_TIMEOUT).await?;
        if result.exit_code() != 0 {
            return Err(anyhow!("{result}"));
        }
        Ok(result)
    }

    /// Run a command over SSH repeatedly until it succeeds or `timeout` elapses, reporting
    /// the last attempt either way.
    ///
    /// Only for a command whose *purpose* is to poll a condition into existence — "is the
    /// daemon active yet" — so that running it again is both harmless and the point. For
    /// anything else use [`Self::run_over_ssh_once`].
    pub async fn run_over_ssh_until_success(
        &self,
        command: &str,
        timeout: Duration,
    ) -> Result<GuestCommandOutput> {
        retry_until_successful(timeout, SSH_POLL_INTERVAL, || self.ssh_attempt(command)).await
    }

    /// One SSH round trip, reported whatever it exits with.
    async fn ssh_attempt(&self, command: &str) -> Result<GuestCommandOutput> {
        let result = self
            .qemu
            .verify(self.running()?, command)
            .await
            .map_err(|e| anyhow!("ssh `{command}`: {e}"))?;
        Ok(GuestCommandOutput::from_ssh(command, result))
    }

    /// Copy host files into the guest over the forwarded SSH port.
    pub async fn copy_in(&self, local_paths: &[PathBuf], remote_dest: &str) -> Result<()> {
        scp_to_guest(self.running()?, local_paths, remote_dest)
            .await
            .map_err(|e| anyhow!("copying {local_paths:?} to {remote_dest}: {e}"))
    }

    /// Shut the guest down gracefully and wait for it to actually go.
    ///
    /// The wait matters: `system_powerdown` is an asynchronous ACPI request, so without it
    /// the next run races a process that has not released the port yet.
    pub async fn shutdown(mut self) -> Result<()> {
        let Some(running) = self.running.take() else {
            return Ok(());
        };
        let (pid, monitor_socket) = (running.pid, running.monitor_socket.clone());
        let port = self.ssh_host_port;

        // `running` has to move into `shutdown`, which disarms the Drop guard — so every
        // failure from here on re-does by hand what the guard would have done, rather than
        // orphaning the VM on precisely the path the guard exists for.
        if let Err(e) = self.qemu.shutdown(running).await {
            force_kill(pid, &monitor_socket);
            return Err(anyhow!("guest did not shut down gracefully: {e}"));
        }
        // Drain the console while waiting. If the test worked over SSH, nothing has read the
        // console since the guest booted, and a guest blocked on a full console pipe cannot
        // complete its shutdown at all — see `SerialConsole::drain_for`. The drain finishing
        // first means the console closed, which is QEMU exiting, so re-check the port rather
        // than treating it as a failure.
        let released = tokio::select! {
            released = wait_for_port_release(port, SHUTDOWN_TIMEOUT) => released,
            _ = self.console.drain_for(SHUTDOWN_TIMEOUT) => {
                wait_for_port_release(port, Duration::from_secs(5)).await
            }
        };
        if !released {
            force_kill(pid, &monitor_socket);
            return Err(anyhow!(
                "guest accepted the powerdown but never released port {port}"
            ));
        }
        let _ = std::fs::remove_file(&monitor_socket);
        Ok(())
    }
}

impl Drop for BootedGuest {
    fn drop(&mut self) {
        let Some(running) = self.running.take() else {
            return;
        };
        force_kill(running.pid, &running.monitor_socket);
    }
}

/// Run `attempt` until it reports a zero exit code, giving up once `timeout` has elapsed
/// and reporting the last outcome either way.
///
/// Extracted from [`BootedGuest`] because the retry *policy* is the thing worth pinning
/// down, and a `BootedGuest` needs a real QEMU process to exist at all. The first attempt is
/// made immediately; every later one is separated by `poll_interval`.
pub async fn retry_until_successful<F, Fut>(
    timeout: Duration,
    poll_interval: Duration,
    mut attempt: F,
) -> Result<GuestCommandOutput>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<GuestCommandOutput>>,
{
    let deadline = tokio::time::Instant::now() + timeout;
    let mut last = attempt().await?;
    while last.exit_code() != 0 && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(poll_interval).await;
        last = attempt().await?;
    }
    Ok(last)
}

/// Last-resort teardown. Best-effort by nature — it runs from `Drop` and from panic
/// paths, where there is nobody left to report a failure to.
fn force_kill(pid: u32, monitor_socket: &str) {
    let _ = std::process::Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status();
    let _ = std::fs::remove_file(monitor_socket);
}

/// Poll until nothing is listening on `port`, reporting whether that happened in time.
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

/// A read-only host directory exported into a guest.
pub fn read_only_share(host_path: &Path, mount_tag: &str) -> NinePShare {
    NinePShare {
        host_path: host_path.display().to_string(),
        mount_tag: mount_tag.to_string(),
        writable: false,
    }
}

/// A writable host directory exported into a guest.
///
/// The argv builder has always supported this (`qemu.rs:329` drops `readonly=on`), but
/// nothing in the workspace had used it — every call site passed `writable: false`. It is
/// how the builder guest hands its output back to a host that cannot compile it.
pub fn writable_share(host_path: &Path, mount_tag: &str) -> NinePShare {
    NinePShare {
        host_path: host_path.display().to_string(),
        mount_tag: mount_tag.to_string(),
        writable: true,
    }
}
