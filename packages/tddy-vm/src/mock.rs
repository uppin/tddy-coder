//! `MockVm` — test double for `Vm`.
//!
//! Records all calls (deploy steps, verify commands, forward requests) and
//! returns configurable results. Used in acceptance tests.

use crate::vm::{ForwardHandle, PortForward, RunningVm, VerifyResult, Vm, VmConfig, VmError};
use std::sync::Mutex;

/// Recorded call to `deploy`.
#[derive(Debug, Clone)]
pub struct DeployCall {
    pub ssh_host_port: u16,
    pub steps: Vec<String>,
}

/// Recorded call to `verify`.
#[derive(Debug, Clone)]
pub struct VerifyCall {
    pub ssh_host_port: u16,
    pub command: String,
}

/// Recorded call to `forward`.
#[derive(Debug, Clone)]
pub struct ForwardCall {
    pub host_port: u16,
    pub guest_port: u16,
}

/// Recorded call to `boot`.
#[derive(Debug, Clone)]
pub struct BootCall {
    pub qcow2_path: String,
    pub ssh_host_port: u16,
}

/// Test double for `Vm`.
///
/// Configure it before use:
/// ```
/// use tddy_vm::MockVm;
/// let vm = MockVm::new();
/// // All methods succeed by default; the mock records calls.
/// ```
#[derive(Default)]
pub struct MockVm {
    pub boot_calls: Mutex<Vec<BootCall>>,
    pub deploy_calls: Mutex<Vec<DeployCall>>,
    pub verify_calls: Mutex<Vec<VerifyCall>>,
    pub forward_calls: Mutex<Vec<ForwardCall>>,
    /// Override the share URL returned by `forward`. Defaults to `http://localhost:<host_port>`.
    pub forward_url_override: Option<String>,
    /// If `true`, `verify` returns a failing `VerifyResult`. Default `false`.
    pub verify_fails: bool,
}

impl MockVm {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn boot_calls(&self) -> Vec<BootCall> {
        self.boot_calls.lock().unwrap().clone()
    }

    pub fn deploy_calls(&self) -> Vec<DeployCall> {
        self.deploy_calls.lock().unwrap().clone()
    }

    pub fn verify_calls(&self) -> Vec<VerifyCall> {
        self.verify_calls.lock().unwrap().clone()
    }

    pub fn forward_calls(&self) -> Vec<ForwardCall> {
        self.forward_calls.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl Vm for MockVm {
    async fn boot(&self, config: &VmConfig) -> Result<RunningVm, VmError> {
        self.boot_calls.lock().unwrap().push(BootCall {
            qcow2_path: config.qcow2_path.clone(),
            ssh_host_port: config.ssh_host_port,
        });
        Ok(RunningVm {
            ssh_host_port: config.ssh_host_port,
            monitor_socket: "/tmp/tddy-mock-monitor.sock".to_string(),
            pid: 99999,
        })
    }

    async fn deploy(&self, vm: &RunningVm, steps: &[String]) -> Result<(), VmError> {
        self.deploy_calls.lock().unwrap().push(DeployCall {
            ssh_host_port: vm.ssh_host_port,
            steps: steps.to_vec(),
        });
        Ok(())
    }

    async fn verify(&self, vm: &RunningVm, command: &str) -> Result<VerifyResult, VmError> {
        self.verify_calls.lock().unwrap().push(VerifyCall {
            ssh_host_port: vm.ssh_host_port,
            command: command.to_string(),
        });
        if self.verify_fails {
            return Ok(VerifyResult {
                success: false,
                output: "mock verify: forced failure".to_string(),
                exit_code: 1,
            });
        }
        Ok(VerifyResult {
            success: true,
            output: "mock verify: ok".to_string(),
            exit_code: 0,
        })
    }

    async fn forward(
        &self,
        _vm: &RunningVm,
        port_forward: &PortForward,
    ) -> Result<ForwardHandle, VmError> {
        self.forward_calls.lock().unwrap().push(ForwardCall {
            host_port: port_forward.host_port,
            guest_port: port_forward.guest_port,
        });
        let share_url = self
            .forward_url_override
            .clone()
            .unwrap_or_else(|| format!("http://localhost:{}", port_forward.host_port));
        Ok(ForwardHandle {
            host_port: port_forward.host_port,
            guest_port: port_forward.guest_port,
            share_url,
        })
    }

    async fn shutdown(&self, _vm: RunningVm) -> Result<(), VmError> {
        Ok(())
    }
}
