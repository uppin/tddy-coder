use crate::vm::{PortForward, Vm, VmError};
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
#[allow(dead_code)]
struct VmHandle {
    state: VmState,
}

pub struct VmManager {
    state_file: PathBuf,
    backend: Arc<dyn Vm>,
    vms: Mutex<HashMap<String, (VmSpec, VmHandle)>>,
}

impl VmManager {
    pub fn new(state_file: &Path, backend: Box<dyn Vm>) -> Self {
        Self {
            state_file: state_file.to_path_buf(),
            backend: Arc::from(backend),
            vms: Mutex::new(HashMap::new()),
        }
    }
    pub async fn define(&self, spec: VmSpec) -> Result<(), VmError> {
        let _ = spec;
        unimplemented!()
    }
    pub async fn list(&self) -> Vec<(VmSpec, VmState)> {
        unimplemented!()
    }
    pub async fn start(&self, name: &str) -> Result<(), VmError> {
        let _ = name;
        unimplemented!()
    }
    pub async fn stop(&self, name: &str) -> Result<(), VmError> {
        let _ = name;
        unimplemented!()
    }
    pub async fn status(&self, name: &str) -> Result<VmState, VmError> {
        let _ = name;
        unimplemented!()
    }
    pub async fn remove(&self, name: &str) -> Result<(), VmError> {
        let _ = name;
        unimplemented!()
    }
}
