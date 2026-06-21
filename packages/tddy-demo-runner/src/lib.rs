//! Demo goal runtime — orchestration of the QEMU PortForward demo cycle.
//!
//! Key types:
//! - [`tddy_vm::Vm`] trait: mockable VM boundary (boot / deploy / verify / forward / shutdown)
//! - [`tddy_vm::QemuVm`]: concrete QEMU impl
//! - [`tddy_vm::MockVm`]: test double
//! - [`DemoOrchestrator`]: drives the full PortForward demo cycle; posts Telegram link

pub mod orchestrator;

pub use orchestrator::{DemoOrchestrator, DemoResult, OrchestratorError, TelegramNotifier};
// Re-export tddy_vm types so downstream callers (tddy-daemon, tests) can use one import path.
pub use tddy_vm::{
    send_monitor_command, wait_for_ssh_port, MockVm, PortForward, QemuVm, QemuVmArgs, RunningVm,
    Vm, VmConfig, VmError,
};
// BootCall re-export for tests that check mock call recording.
pub use tddy_vm::mock::BootCall;
