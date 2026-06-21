//! `Vm` trait — the mockable boundary between the VM manager and the concrete
//! VM runtime (QEMU or, in tests, `MockVm`).

/// A single host ↔ guest port mapping for QEMU slirp `hostfwd`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PortForward {
    pub host_port: u16,
    pub guest_port: u16,
}

/// Configuration needed to boot a VM instance.
#[derive(Debug, Clone)]
pub struct VmConfig {
    /// Path to the qcow2 image to boot.
    pub qcow2_path: String,
    /// Host ↔ guest port maps (beyond the SSH forward which is always added at `tcp::2222-:22`).
    pub extra_hostfwd: Vec<PortForward>,
    /// Base SSH port on the host (default `2222`).
    pub ssh_host_port: u16,
}

/// A handle to a successfully booted VM.
#[derive(Debug)]
pub struct RunningVm {
    /// The SSH port on the host side (typically the `ssh_host_port` from the config).
    pub ssh_host_port: u16,
    /// Monitor socket path (used for graceful shutdown via QEMU monitor `system_powerdown`).
    pub monitor_socket: String,
    /// Child process ID of the qemu-system process.
    pub pid: u32,
}

/// Handle to an active port-forward from host to guest.
#[derive(Debug)]
pub struct ForwardHandle {
    pub host_port: u16,
    pub guest_port: u16,
    /// Shareable URL for a port-forward: `http://localhost:<host_port>`.
    pub share_url: String,
}

/// Result of running the verify command inside the guest.
#[derive(Debug)]
pub struct VerifyResult {
    pub success: bool,
    pub output: String,
    pub exit_code: i32,
}

/// Errors from VM operations.
#[derive(Debug, thiserror::Error)]
pub enum VmError {
    #[error("VM boot failed: {0}")]
    BootFailed(String),
    #[error("SSH deploy failed: {0}")]
    DeployFailed(String),
    #[error("Verify command failed: {0}")]
    VerifyFailed(String),
    #[error("Port forward failed: {0}")]
    ForwardFailed(String),
    #[error("Shutdown failed: {0}")]
    ShutdownFailed(String),
    #[error("Not implemented: {0}")]
    NotImplemented(String),
    #[error("VM not found: {0}")]
    NotFound(String),
    #[error("VM already exists: {0}")]
    AlreadyExists(String),
    #[error("Invalid state for operation: {0}")]
    InvalidState(String),
    #[error("VM image build failed: {0}")]
    BuildFailed(String),
}

/// Mockable boundary for a VM.
///
/// The caller drives the VM through:
/// `boot` → `deploy` → `verify` → `forward` → (use the link) → `shutdown`.
#[async_trait::async_trait]
pub trait Vm: Send + Sync {
    /// Boot the VM from the given config. Returns a `RunningVm` handle when SSH is ready.
    async fn boot(&self, config: &VmConfig) -> Result<RunningVm, VmError>;

    /// Run the given deploy commands inside the guest via SSH.
    async fn deploy(&self, vm: &RunningVm, steps: &[String]) -> Result<(), VmError>;

    /// Run a verification command inside the guest and return the result.
    async fn verify(&self, vm: &RunningVm, command: &str) -> Result<VerifyResult, VmError>;

    /// Activate the port-forward mapping and return a `ForwardHandle` with the share URL.
    ///
    /// For QEMU slirp, the port is already forwarded by the `-netdev` arg; this method
    /// validates connectivity and builds the URL.
    async fn forward(
        &self,
        vm: &RunningVm,
        port_forward: &PortForward,
    ) -> Result<ForwardHandle, VmError>;

    /// Shut down the VM gracefully (QEMU monitor `system_powerdown`).
    async fn shutdown(&self, vm: RunningVm) -> Result<(), VmError>;
}
