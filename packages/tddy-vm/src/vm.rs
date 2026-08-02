//! `Vm` trait — the mockable boundary between the VM manager and the concrete
//! VM runtime (QEMU or, in tests, `MockVm`).

use crate::cloud_init::NinePShare;

/// A single host ↔ guest port mapping for QEMU slirp `hostfwd`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PortForward {
    pub host_port: u16,
    pub guest_port: u16,
}

/// The guest architecture, which picks both the `qemu-system-*` emulator and the machine
/// type — aarch64 has no default machine, so this is never implicit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VmArch {
    Aarch64,
    X86_64,
}

impl VmArch {
    /// The architecture of the host this binary was built for — the only architecture that
    /// can be hardware-accelerated, and therefore the default a manifest records.
    ///
    /// Panics on an architecture this crate has no QEMU target for; `std::env::consts::ARCH`
    /// is a compile-time constant, so that is a build-configuration error rather than
    /// something a caller can hit at runtime on a supported platform.
    pub fn host() -> Self {
        match std::env::consts::ARCH {
            "aarch64" => Self::Aarch64,
            "x86_64" => Self::X86_64,
            other => panic!("no QEMU system emulator is wired up for host architecture {other}"),
        }
    }
}

/// The QEMU accelerator to run the guest under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VmAccel {
    /// Apple's Hypervisor.framework (macOS).
    Hvf,
    /// Linux KVM.
    Kvm,
    /// QEMU's software emulation — correct everywhere, an order of magnitude slower.
    Tcg,
}

impl VmAccel {
    /// The best accelerator this host offers. An explicit constructor used to populate a
    /// manifest: the launcher itself never guesses, it emits whatever the manifest says.
    pub fn host_default() -> Self {
        if cfg!(target_os = "macos") {
            Self::Hvf
        } else if cfg!(target_os = "linux") && std::path::Path::new("/dev/kvm").exists() {
            Self::Kvm
        } else {
            Self::Tcg
        }
    }
}

/// The UEFI firmware pair a guest boots through: read-only code plus writable variables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UefiFirmware {
    /// The read-only `edk2-<arch>-code.fd` firmware image.
    pub code_path: String,
    /// The per-VM writable 64 MiB variables store.
    pub vars_path: String,
}

/// How the host logs in to the guest over SSH.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmLogin {
    pub username: String,
    /// The per-VM private key. When `None`, SSH falls back to whatever the ambient agent
    /// offers.
    pub private_key_path: Option<String>,
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
    /// Guest architecture — selects the emulator binary and machine type.
    pub arch: VmArch,
    /// Accelerator to run under.
    pub accel: VmAccel,
    /// Guest RAM, in QEMU `-m` syntax (e.g. `"2048M"`).
    pub memory: String,
    /// Number of vCPUs.
    pub cpus: u32,
    /// UEFI firmware pair, or `None` for a guest that boots via its own BIOS.
    pub firmware: Option<UefiFirmware>,
    /// SSH login policy for reaching the guest once it is up.
    pub login: VmLogin,
    /// A NoCloud seed ISO to attach as a CD-ROM for cloud-init's first boot.
    pub seed_iso: Option<String>,
    /// Host directories exported into the guest over virtio-9p.
    pub nine_p_shares: Vec<NinePShare>,
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
    /// SSH login policy carried over from the config, so `deploy`/`verify` reach the guest
    /// as the right user with the right key.
    pub login: VmLogin,
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

/// A shared `Vm` is itself a `Vm`, so a caller can hand ownership of a backend to something
/// that takes `Box<dyn Vm>` (e.g. [`crate::registry::VmManager`]) while keeping its own
/// handle on it — how a test inspects what the manager asked the backend to boot.
#[async_trait::async_trait]
impl<T: Vm + ?Sized> Vm for std::sync::Arc<T> {
    async fn boot(&self, config: &VmConfig) -> Result<RunningVm, VmError> {
        (**self).boot(config).await
    }

    async fn deploy(&self, vm: &RunningVm, steps: &[String]) -> Result<(), VmError> {
        (**self).deploy(vm, steps).await
    }

    async fn verify(&self, vm: &RunningVm, command: &str) -> Result<VerifyResult, VmError> {
        (**self).verify(vm, command).await
    }

    async fn forward(
        &self,
        vm: &RunningVm,
        port_forward: &PortForward,
    ) -> Result<ForwardHandle, VmError> {
        (**self).forward(vm, port_forward).await
    }

    async fn shutdown(&self, vm: RunningVm) -> Result<(), VmError> {
        (**self).shutdown(vm).await
    }
}
