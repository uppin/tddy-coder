use crate::vm::{PortForward, Vm, VmError};
use std::path::Path;

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

pub struct VmManager {/* internals TBD — must have Mutex<HashMap>, Box<dyn Vm>, state-file path */}

impl VmManager {
    pub fn new(state_file: &Path, backend: Box<dyn Vm>) -> Self {
        let _ = (state_file, backend);
        unimplemented!()
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
