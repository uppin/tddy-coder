use crate::library::VmLibrary;
use crate::vm::{PortForward, Vm, VmConfig, VmError};
use crate::vm_manifest::{LoginPolicy, RunPolicy, VmManifest};
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

impl VmHandle {
    /// A freshly defined, not-yet-started handle.
    fn defined() -> Self {
        Self {
            state: VmState::Defined,
            running: None,
        }
    }
}

/// Where a [`VmManager`]'s specs are persisted: the original single shared JSON file,
/// or the new per-VM manifest library (source of truth going forward — see
/// [`VmManager::from_library`]).
enum Storage {
    Json(PathBuf),
    Library(VmLibrary),
}

pub struct VmManager {
    storage: Storage,
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
            storage: Storage::Json(state_file.to_path_buf()),
            backend: Arc::from(backend),
            vms: Mutex::new(vms),
        }
    }

    /// Construct a `VmManager` backed by a [`VmLibrary`] — per-VM `manifest.yaml` files
    /// are the source of truth, superseding the single shared JSON state file.
    /// `VmSpec` remains the in-memory/RPC DTO; specs are mapped to/from
    /// [`crate::vm_manifest::VmManifest`] when reading or writing the library.
    pub fn from_library(library: VmLibrary, backend: Box<dyn Vm>) -> Self {
        let vms = Self::load_from_library(&library);
        Self {
            storage: Storage::Library(library),
            backend: Arc::from(backend),
            vms: Mutex::new(vms),
        }
    }

    fn load_from_library(library: &VmLibrary) -> HashMap<String, (VmSpec, VmHandle)> {
        let manifests = library.list_manifests().unwrap_or_else(|e| {
            log::error!(
                "VmManager: failed to load manifests from library {}: {e}",
                library.root().display()
            );
            Vec::new()
        });
        manifests
            .into_iter()
            .map(|manifest| {
                let spec = vm_spec_from_manifest(library, &manifest);
                (spec.name.clone(), (spec, VmHandle::defined()))
            })
            .collect()
    }

    pub async fn define(&self, spec: VmSpec) -> Result<(), VmError> {
        let mut vms = self.vms.lock().await;
        if vms.contains_key(&spec.name) {
            return Err(VmError::AlreadyExists(spec.name.clone()));
        }
        match &self.storage {
            Storage::Json(path) => {
                vms.insert(spec.name.clone(), (spec, VmHandle::defined()));
                self.persist_json(path, &vms);
            }
            Storage::Library(library) => {
                self.write_spec_to_library(library, &spec);
                vms.insert(spec.name.clone(), (spec, VmHandle::defined()));
            }
        }
        Ok(())
    }

    fn write_spec_to_library(&self, library: &VmLibrary, spec: &VmSpec) {
        let manifest = vm_manifest_from_spec(spec);
        if let Err(e) = library.write_manifest(&manifest) {
            log::error!(
                "VmManager: failed to write manifest for '{}' to library {}: {e}",
                spec.name,
                library.root().display()
            );
        }
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
        match &self.storage {
            Storage::Json(path) => self.persist_json(path, &vms),
            Storage::Library(library) => self.remove_from_library(library, name),
        }
        Ok(())
    }

    fn remove_from_library(&self, library: &VmLibrary, name: &str) {
        if let Err(e) = library.remove_vm(name) {
            log::error!(
                "VmManager: failed to remove '{name}' from library {}: {e}",
                library.root().display()
            );
        }
    }

    fn persist_json(&self, state_file: &Path, vms: &HashMap<String, (VmSpec, VmHandle)>) {
        let specs: Vec<&VmSpec> = vms.values().map(|(spec, _)| spec).collect();
        if let Ok(json) = serde_json::to_string_pretty(&specs) {
            if let Err(e) = std::fs::write(state_file, json) {
                log::error!(
                    "VmManager: failed to persist state to {}: {e}",
                    state_file.display()
                );
            }
        }
    }
}

/// Map a loaded [`VmManifest`] to the in-memory/RPC [`VmSpec`] DTO. A manifest with
/// `prepared_base` set (rather than a direct `image_path`) resolves to the overlay
/// `create_vm` would have produced at `vm/<name>/<name>.qcow2` — the manifest itself
/// does not duplicate that derived path.
fn vm_spec_from_manifest(library: &VmLibrary, manifest: &VmManifest) -> VmSpec {
    let image_path = manifest.image_path.clone().or_else(|| {
        manifest.prepared_base.as_ref().map(|_| {
            library
                .vm_dir(&manifest.name)
                .join(format!("{}.qcow2", manifest.name))
                .to_string_lossy()
                .into_owned()
        })
    });
    VmSpec {
        name: manifest.name.clone(),
        build_target: None,
        image_path,
        port_forwards: manifest.run.port_forwards.clone(),
        ssh_host_port: manifest.run.ssh_host_port,
    }
}

/// Map a [`VmSpec`] to a [`VmManifest`] for persistence via `VmManager::define`'s
/// library-mode path. This is the direct-`image_path` shape (mirrors the JSON-backed
/// contract); the richer `prepared_base`-driven shape is populated separately by
/// [`VmLibrary::create_vm`], not through this generic spec mapping.
fn vm_manifest_from_spec(spec: &VmSpec) -> VmManifest {
    VmManifest {
        name: spec.name.clone(),
        prepared_base: None,
        image_path: spec.image_path.clone(),
        run: RunPolicy {
            memory: "2048M".to_string(),
            cpus: 2,
            disk_size: "20G".to_string(),
            ssh_host_port: spec.ssh_host_port,
            port_forwards: spec.port_forwards.clone(),
        },
        login: LoginPolicy {
            // SSH-as-root matches this crate's existing `QemuVm` convention (see qemu.rs).
            username: "root".to_string(),
            ssh_private_key: None,
            ssh_public_key: None,
        },
    }
}
