//! QEMU concrete implementation of `Vm`.
//!
//! `QemuVm` boots `qemu-system-x86_64` (nix-provided) with:
//! - virtio drive from the qcow2 image
//! - user-mode networking (slirp) with `hostfwd` specs for SSH + app ports
//! - optional VNC (`-vnc :<n>`) for the deferred ScreenShare mode
//! - QEMU monitor unix socket for graceful shutdown
//!
//! `QemuVmArgs` assembles the argv vector from a `VmConfig` so the arg-builder logic
//! is unit-testable independently of process spawning.

use std::time::Duration;

use tokio::io::AsyncWriteExt;

use crate::vm::{ForwardHandle, PortForward, RunningVm, VerifyResult, Vm, VmConfig, VmError};

/// Poll `host:port` via TCP every 100 ms until either a connection succeeds or `timeout`
/// elapses.
///
/// Returns `Ok(())` on the first successful connection.
/// Returns `Err(VmError::BootFailed(...))` when the timeout expires without a
/// successful connection.
pub async fn wait_for_ssh_port(host: &str, port: u16, timeout: Duration) -> Result<(), VmError> {
    let addr = format!("{host}:{port}");
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        match tokio::net::TcpStream::connect(&addr).await {
            Ok(_) => return Ok(()),
            Err(_) => {
                if tokio::time::Instant::now() >= deadline {
                    return Err(VmError::BootFailed(format!(
                        "timed out waiting for SSH port {port} on {host}"
                    )));
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

/// Connect to the QEMU monitor Unix socket at `socket_path`, write `"{command}\n"`, then
/// close the connection.
///
/// Returns `Err(VmError::ShutdownFailed(...))` if the socket cannot be reached or the
/// write fails.
pub async fn send_monitor_command(socket_path: &str, command: &str) -> Result<(), VmError> {
    let mut stream = tokio::net::UnixStream::connect(socket_path)
        .await
        .map_err(|e| VmError::ShutdownFailed(format!("connect to monitor socket: {e}")))?;
    let msg = format!("{command}\n");
    stream
        .write_all(msg.as_bytes())
        .await
        .map_err(|e| VmError::ShutdownFailed(format!("write to monitor socket: {e}")))?;
    stream
        .flush()
        .await
        .map_err(|e| VmError::ShutdownFailed(format!("flush monitor socket: {e}")))?;
    Ok(())
}

/// Assembles `qemu-system-x86_64` argument vectors from a `VmConfig`.
///
/// This struct is the pure, unit-testable core of the QEMU runner — no process spawning.
pub struct QemuVmArgs;

impl QemuVmArgs {
    /// Build the full argv for `qemu-system-x86_64` from the given config.
    ///
    /// The SSH forward (`tcp::<ssh_host_port>-:22`) is always included first so the
    /// caller can reach the guest regardless of `extra_hostfwd`.
    ///
    /// # Example output
    /// ```text
    /// qemu-system-x86_64
    ///   -drive file=<qcow2>,if=virtio,format=qcow2
    ///   -m 512M
    ///   -nographic
    ///   -netdev user,id=net0,hostfwd=tcp::2222-:22,hostfwd=tcp::8080-:80
    ///   -device virtio-net-pci,netdev=net0
    ///   -monitor unix:/tmp/tddy-vm-<port>.sock,server,nowait
    ///   -serial file:/tmp/tddy-vm-serial-<port>.log
    /// ```
    pub fn build(config: &VmConfig) -> Vec<String> {
        let netdev = Self::netdev_arg(config);
        let monitor = format!(
            "unix:{},server,nowait",
            Self::monitor_socket_path(config.ssh_host_port)
        );
        let serial = format!("file:/tmp/tddy-vm-serial-{}.log", config.ssh_host_port);
        vec![
            "-drive".to_string(),
            format!("file={},if=virtio,format=qcow2", config.qcow2_path),
            "-m".to_string(),
            "512M".to_string(),
            "-nographic".to_string(),
            "-netdev".to_string(),
            netdev,
            "-device".to_string(),
            "virtio-net-pci,netdev=net0".to_string(),
            "-monitor".to_string(),
            monitor,
            "-serial".to_string(),
            serial,
        ]
    }

    /// Derive the monitor Unix socket path from the SSH host port so concurrent VM
    /// instances (each with a unique SSH port) don't collide.
    pub fn monitor_socket_path(ssh_host_port: u16) -> String {
        format!("/tmp/tddy-vm-monitor-{ssh_host_port}.sock")
    }

    /// Format a single `hostfwd` spec from a `PortForward`.
    ///
    /// Returns `"tcp::<host_port>-:<guest_port>"` (the slirp `-netdev user,hostfwd=` value format).
    pub fn hostfwd_spec(port_forward: &PortForward) -> String {
        format!(
            "tcp::{}-:{}",
            port_forward.host_port, port_forward.guest_port
        )
    }

    /// Build the combined `-netdev` argument including all hostfwd specs.
    ///
    /// SSH forward (`tcp::<ssh_host_port>-:22`) is prepended; `extra_hostfwd` follows.
    pub fn netdev_arg(config: &VmConfig) -> String {
        let mut arg = format!("user,id=net0,hostfwd=tcp::{}-:22", config.ssh_host_port);
        for port_forward in &config.extra_hostfwd {
            arg.push_str(&format!(",hostfwd={}", Self::hostfwd_spec(port_forward)));
        }
        arg
    }
}

/// QEMU concrete implementation of [`Vm`].
///
/// Boots `qemu-system-x86_64` (resolved via `$PATH`, provided by the nix dev shell),
/// deploys via SSH over the slirp hostfwd port, and verifies the app via SSH command.
pub struct QemuVm;

/// Build the SSH argument list for connecting to a guest at the given host port.
///
/// Options suppress host-key prompts (`StrictHostKeyChecking=no`,
/// `UserKnownHostsFile=/dev/null`), prevent interactive password prompts
/// (`BatchMode=yes`), cap the connect wait (`ConnectTimeout=10`), and silence
/// the "Warning: Permanently added" banner (`LogLevel=ERROR`).
fn ssh_opts(ssh_host_port: u16) -> Vec<String> {
    vec![
        "-p".into(),
        ssh_host_port.to_string(),
        "-o".into(),
        "StrictHostKeyChecking=no".into(),
        "-o".into(),
        "UserKnownHostsFile=/dev/null".into(),
        "-o".into(),
        "BatchMode=yes".into(),
        "-o".into(),
        "ConnectTimeout=10".into(),
        "-o".into(),
        "LogLevel=ERROR".into(),
    ]
}

#[async_trait::async_trait]
impl Vm for QemuVm {
    /// Boot the VM from the given config.
    ///
    /// Spawns `qemu-system-x86_64` with args from [`QemuVmArgs::build`] and waits
    /// up to 5 minutes for the guest SSH port to become reachable before returning.
    /// The QEMU process is detached (not killed when the `Child` is dropped) so it
    /// outlives this call and runs until [`shutdown`][Self::shutdown] is called.
    async fn boot(&self, config: &VmConfig) -> Result<RunningVm, VmError> {
        let monitor_socket = QemuVmArgs::monitor_socket_path(config.ssh_host_port);
        let args = QemuVmArgs::build(config);

        let child = tokio::process::Command::new("qemu-system-x86_64")
            .args(&args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| VmError::BootFailed(format!("spawn qemu-system-x86_64: {e}")))?;

        let pid = child
            .id()
            .ok_or_else(|| VmError::BootFailed("qemu exited immediately after spawn".into()))?;

        // Drop the Child without awaiting: the process runs as a detached daemon
        // until shutdown() sends system_powerdown to the monitor socket.
        drop(child);

        wait_for_ssh_port("127.0.0.1", config.ssh_host_port, Duration::from_secs(300)).await?;

        Ok(RunningVm {
            ssh_host_port: config.ssh_host_port,
            monitor_socket,
            pid,
        })
    }

    /// Run each deploy step inside the guest via SSH.
    ///
    /// Steps are executed sequentially as `root@127.0.0.1`. The first step that
    /// exits non-zero returns `Err(DeployFailed)` with the step text and exit status.
    async fn deploy(&self, vm: &RunningVm, steps: &[String]) -> Result<(), VmError> {
        for step in steps {
            let status = tokio::process::Command::new("ssh")
                .args(ssh_opts(vm.ssh_host_port))
                .arg("root@127.0.0.1")
                .arg(step)
                .status()
                .await
                .map_err(|e| VmError::DeployFailed(format!("ssh spawn error: {e}")))?;

            if !status.success() {
                return Err(VmError::DeployFailed(format!(
                    "step `{step}` failed with {status}"
                )));
            }
        }
        Ok(())
    }

    /// Run `command` inside the guest via SSH and return its output and exit code.
    ///
    /// Both stdout and stderr are captured and concatenated into `VerifyResult::output`.
    /// A non-zero exit code sets `success = false` but does **not** return `Err` — the
    /// caller decides whether to treat verification failure as fatal.
    async fn verify(&self, vm: &RunningVm, command: &str) -> Result<VerifyResult, VmError> {
        let output = tokio::process::Command::new("ssh")
            .args(ssh_opts(vm.ssh_host_port))
            .arg("root@127.0.0.1")
            .arg(command)
            .output()
            .await
            .map_err(|e| VmError::VerifyFailed(format!("ssh spawn error: {e}")))?;

        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        if !output.stderr.is_empty() {
            text.push_str(&String::from_utf8_lossy(&output.stderr));
        }

        Ok(VerifyResult {
            success: output.status.success(),
            output: text,
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    async fn forward(
        &self,
        _vm: &RunningVm,
        port_forward: &PortForward,
    ) -> Result<ForwardHandle, VmError> {
        let addr = format!("127.0.0.1:{}", port_forward.host_port);
        tokio::time::timeout(
            Duration::from_secs(1),
            tokio::net::TcpStream::connect(&addr),
        )
        .await
        .map_err(|_| {
            VmError::ForwardFailed(format!(
                "timed out connecting to host port {}",
                port_forward.host_port
            ))
        })?
        .map_err(|e| {
            VmError::ForwardFailed(format!(
                "host port {} not reachable: {e}",
                port_forward.host_port
            ))
        })?;

        Ok(ForwardHandle {
            host_port: port_forward.host_port,
            guest_port: port_forward.guest_port,
            share_url: format!("http://localhost:{}", port_forward.host_port),
        })
    }

    /// Gracefully shut down the VM by sending `system_powerdown` to the QEMU monitor socket.
    async fn shutdown(&self, vm: RunningVm) -> Result<(), VmError> {
        send_monitor_command(&vm.monitor_socket, "system_powerdown").await
    }
}
