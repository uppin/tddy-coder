//! QEMU concrete implementation of `DemoVm`.
//!
//! `QemuDemoVm` boots `qemu-system-x86_64` (nix-provided) with:
//! - virtio drive from the qcow2 image
//! - user-mode networking (slirp) with `hostfwd` specs for SSH + app ports
//! - optional VNC (`-vnc :<n>`) for the deferred ScreenShare mode
//! - QEMU monitor unix socket for graceful shutdown
//!
//! `QemuVmArgs` assembles the argv vector from a `DemoVmConfig` so the arg-builder logic
//! is unit-testable independently of process spawning.

use crate::vm::{DemoVm, DemoVmConfig, DemoVmError, ForwardHandle, RunningVm, VerifyResult};
use tddy_workflow_recipes::parser::PortMap;

/// Assembles `qemu-system-x86_64` argument vectors from a `DemoVmConfig`.
///
/// This struct is the pure, unit-testable core of the QEMU runner — no process spawning.
pub struct QemuVmArgs;

impl QemuVmArgs {
    /// Build the full argv for `qemu-system-x86_64` from the given config.
    ///
    /// The SSH forward (`tcp::<ssh_host_port>-:22`) is always included first so the
    /// orchestrator can reach the guest regardless of `extra_hostfwd`.
    ///
    /// # Example output
    /// ```text
    /// qemu-system-x86_64
    ///   -drive file=<qcow2>,if=virtio,format=qcow2
    ///   -m 512M
    ///   -nographic
    ///   -netdev user,id=net0,hostfwd=tcp::2222-:22,hostfwd=tcp::8080-:80
    ///   -device virtio-net-pci,netdev=net0
    ///   -monitor unix:/tmp/tddy-demo-<pid>.sock,server,nowait
    ///   -serial file:/tmp/tddy-demo-<pid>.serial
    /// ```
    pub fn build(config: &DemoVmConfig) -> Vec<String> {
        let netdev = Self::netdev_arg(config);
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
            "unix:/tmp/tddy-demo-monitor.sock,server,nowait".to_string(),
            "-serial".to_string(),
            "file:/tmp/tddy-demo-serial.log".to_string(),
        ]
    }

    /// Format a single `hostfwd` spec from a `PortMap`.
    ///
    /// Returns `"tcp::<host_port>-:<guest_port>"` (the slirp `-netdev user,hostfwd=` value format).
    pub fn hostfwd_spec(port_map: &PortMap) -> String {
        format!("tcp::{}-:{}", port_map.host_port, port_map.guest_port)
    }

    /// Build the combined `-netdev` argument including all hostfwd specs.
    ///
    /// SSH forward (`tcp::<ssh_host_port>-:22`) is prepended; `extra_hostfwd` follows.
    pub fn netdev_arg(config: &DemoVmConfig) -> String {
        let mut arg = format!("user,id=net0,hostfwd=tcp::{}-:22", config.ssh_host_port);
        for port_map in &config.extra_hostfwd {
            arg.push_str(&format!(",hostfwd={}", Self::hostfwd_spec(port_map)));
        }
        arg
    }
}

/// QEMU concrete implementation of [`DemoVm`].
///
/// Boots `qemu-system-x86_64` (resolved via `$PATH`, provided by the nix dev shell),
/// deploys via SSH over the slirp hostfwd port, and verifies the app via SSH command.
pub struct QemuDemoVm;

#[async_trait::async_trait]
impl DemoVm for QemuDemoVm {
    async fn boot(&self, _config: &DemoVmConfig) -> Result<RunningVm, DemoVmError> {
        // TODO(demo-qemu): spawn qemu-system-x86_64, wait for SSH to become ready
        Err(DemoVmError::NotImplemented("QemuDemoVm::boot".into()))
    }

    async fn deploy(&self, _vm: &RunningVm, _steps: &[String]) -> Result<(), DemoVmError> {
        // TODO(demo-qemu): SSH into guest on vm.ssh_host_port, run each step
        Err(DemoVmError::NotImplemented("QemuDemoVm::deploy".into()))
    }

    async fn verify(&self, _vm: &RunningVm, _command: &str) -> Result<VerifyResult, DemoVmError> {
        // TODO(demo-qemu): SSH into guest, run command, return result
        Err(DemoVmError::NotImplemented("QemuDemoVm::verify".into()))
    }

    async fn forward(
        &self,
        _vm: &RunningVm,
        _port_map: &PortMap,
    ) -> Result<ForwardHandle, DemoVmError> {
        // TODO(demo-qemu): validate hostfwd connectivity, build share URL
        Err(DemoVmError::NotImplemented("QemuDemoVm::forward".into()))
    }

    async fn shutdown(&self, _vm: RunningVm) -> Result<(), DemoVmError> {
        // TODO(demo-qemu): send `system_powerdown` to monitor socket
        Err(DemoVmError::NotImplemented("QemuDemoVm::shutdown".into()))
    }
}
