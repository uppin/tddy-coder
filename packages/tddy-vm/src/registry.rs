use crate::vm::{PortForward, Vm, VmConfig, VmError};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VmSpec {
    pub name: String,
    pub build_target: Option<String>,
    pub image_path: Option<String>,
    pub port_forwards: Vec<PortForward>,
    pub ssh_host_port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VmState {
    Defined,
    Booting,
    Running,
    Stopped,
    Error(String),
}

/// Handle to a running VM kept in the registry.
struct VmHandle {
    state: VmState,
    running: Option<crate::vm::RunningVm>,
}

pub struct VmManager {
    state_file: PathBuf,
    backend: Arc<dyn Vm>,
    vms: Mutex<HashMap<String, (VmSpec, VmHandle)>>,
}

impl VmManager {
    pub fn new(state_file: &Path, backend: Box<dyn Vm>) -> Self {
        let vms = if state_file.exists() {
            if let Ok(json) = std::fs::read_to_string(state_file) {
                if let Ok(specs) = serde_json::from_str::<Vec<VmSpec>>(&json) {
                    specs
                        .into_iter()
                        .map(|spec| {
                            (
                                spec.name.clone(),
                                (
                                    spec,
                                    VmHandle {
                                        state: VmState::Defined,
                                        running: None,
                                    },
                                ),
                            )
                        })
                        .collect()
                } else {
                    HashMap::new()
                }
            } else {
                HashMap::new()
            }
        } else {
            HashMap::new()
        };
        Self {
            state_file: state_file.to_path_buf(),
            backend: Arc::from(backend),
            vms: Mutex::new(vms),
        }
    }

    pub async fn define(&self, spec: VmSpec) -> Result<(), VmError> {
        let mut vms = self.vms.lock().await;
        if vms.contains_key(&spec.name) {
            return Err(VmError::AlreadyExists(spec.name.clone()));
        }
        vms.insert(
            spec.name.clone(),
            (
                spec,
                VmHandle {
                    state: VmState::Defined,
                    running: None,
                },
            ),
        );
        self.persist(&vms);
        Ok(())
    }

    pub async fn list(&self) -> Vec<(VmSpec, VmState)> {
        let vms = self.vms.lock().await;
        vms.values()
            .map(|(spec, handle)| (spec.clone(), handle.state.clone()))
            .collect()
    }

    pub async fn start(&self, name: &str) -> Result<(), VmError> {
        // Extract what we need and update state to Booting, then release lock
        let config = {
            let mut vms = self.vms.lock().await;
            let (spec, handle) = vms
                .get_mut(name)
                .ok_or_else(|| VmError::NotFound(name.to_string()))?;

            if matches!(handle.state, VmState::Booting | VmState::Running) {
                return Err(VmError::InvalidState(format!(
                    "VM '{}' is already {:?}",
                    name, handle.state
                )));
            }

            let image_path = if let Some(path) = spec.image_path.clone() {
                path
            } else if spec.build_target.is_some() {
                return Err(VmError::InvalidState(
                    "build_target not yet supported by start; use image_path".to_string(),
                ));
            } else {
                return Err(VmError::InvalidState(
                    "no image_path or build_target set on spec".to_string(),
                ));
            };

            handle.state = VmState::Booting;

            let extra_hostfwd = spec.port_forwards.clone();
            let ssh_host_port = spec.ssh_host_port;

            VmConfig {
                qcow2_path: image_path,
                extra_hostfwd,
                ssh_host_port,
            }
        };

        // Call async backend without holding the lock
        let running_vm = self.backend.boot(&config).await?;

        // Re-lock and update state
        let mut vms = self.vms.lock().await;
        let (_, handle) = vms
            .get_mut(name)
            .ok_or_else(|| VmError::NotFound(name.to_string()))?;
        handle.state = VmState::Running;
        handle.running = Some(running_vm);

        Ok(())
    }

    pub async fn stop(&self, name: &str) -> Result<(), VmError> {
        // Take the RunningVm out and verify state, then release lock
        let running_vm = {
            let mut vms = self.vms.lock().await;
            let (_, handle) = vms
                .get_mut(name)
                .ok_or_else(|| VmError::NotFound(name.to_string()))?;

            if handle.state != VmState::Running {
                return Err(VmError::InvalidState(format!(
                    "VM '{}' is not running",
                    name
                )));
            }

            handle.running.take().ok_or_else(|| {
                VmError::InvalidState(format!("VM '{}' has no running handle", name))
            })?
        };

        // Call async backend without holding the lock
        match self.backend.shutdown(running_vm).await {
            Ok(()) => {
                let mut vms = self.vms.lock().await;
                if let Some((_, handle)) = vms.get_mut(name) {
                    handle.state = VmState::Stopped;
                }
                Ok(())
            }
            Err(e) => {
                let mut vms = self.vms.lock().await;
                if let Some((_, handle)) = vms.get_mut(name) {
                    handle.state = VmState::Error(e.to_string());
                }
                Err(e)
            }
        }
    }

    pub async fn status(&self, name: &str) -> Result<VmState, VmError> {
        let vms = self.vms.lock().await;
        let (_, handle) = vms
            .get(name)
            .ok_or_else(|| VmError::NotFound(name.to_string()))?;
        Ok(handle.state.clone())
    }

    pub async fn remove(&self, name: &str) -> Result<(), VmError> {
        let mut vms = self.vms.lock().await;
        let (_, handle) = vms
            .get(name)
            .ok_or_else(|| VmError::NotFound(name.to_string()))?;

        if handle.state == VmState::Running {
            return Err(VmError::InvalidState(
                "cannot remove a running VM".to_string(),
            ));
        }

        vms.remove(name);
        self.persist(&vms);
        Ok(())
    }

    fn persist(&self, vms: &HashMap<String, (VmSpec, VmHandle)>) {
        let specs: Vec<&VmSpec> = vms.values().map(|(spec, _)| spec).collect();
        if let Ok(json) = serde_json::to_string_pretty(&specs) {
            if let Err(e) = std::fs::write(&self.state_file, json) {
                log::error!(
                    "VmManager: failed to persist state to {}: {e}",
                    self.state_file.display()
                );
            }
        }
    }
}
