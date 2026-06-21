//! Demo goal runtime — QEMU VM lifecycle, SSH deploy, port-forward, orchestration.
//!
//! Key types:
//! - [`DemoVm`] trait: mockable VM boundary (boot / deploy / verify / forward / shutdown)
//! - [`QemuDemoVm`]: concrete impl using nix-provided `qemu-system-x86_64` + slirp hostfwd
//! - [`MockDemoVm`]: test double that records all calls
//! - [`DemoOrchestrator`]: drives the full PortForward demo cycle; posts Telegram link

pub mod mock;
pub mod orchestrator;
pub mod qemu;
pub mod vm;

pub use mock::{BootCall, MockDemoVm};
pub use orchestrator::{DemoOrchestrator, DemoResult, TelegramNotifier};
pub use qemu::{send_monitor_command, wait_for_ssh_port, QemuDemoVm, QemuVmArgs};
pub use vm::{DemoVm, DemoVmConfig, DemoVmError, ForwardHandle, RunningVm, VerifyResult};
