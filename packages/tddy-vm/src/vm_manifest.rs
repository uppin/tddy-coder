//! `VmManifest` — the per-VM manifest persisted as `vm/<name>/manifest.yaml` in the
//! [`crate::library::VmLibrary`]. Captures how to run a VM (resources, port forwards),
//! its login policy (SSH username/keys), and which prepared base its mutable overlay is
//! backed by — mirroring `~/Code/makers-lt`'s `build.ts` manifests and this crate's
//! existing [`crate::registry::VmSpec`] persistence shape.

use crate::vm::PortForward;
use serde::{Deserialize, Serialize};

/// The full manifest for one VM in the library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmManifest {
    pub name: String,

    /// Name of a prepared base image in `images/02-prepared-base/` (without the
    /// `.qcow2` extension) that this VM's mutable overlay is backed by. Mutually
    /// exclusive with `image_path` — mirrors `VmSpec::build_target`/`image_path`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prepared_base: Option<String>,

    /// Path to an existing qcow2 image to run directly, unmanaged by the library
    /// (e.g. a build-target output or an externally supplied image).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_path: Option<String>,

    pub run: RunPolicy,
    pub login: LoginPolicy,
}

/// How the VM is run: resources, disk sizing for a prepared-base-derived overlay, and
/// network port forwards.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunPolicy {
    pub memory: String,
    pub cpus: u32,
    pub disk_size: String,
    pub ssh_host_port: u16,
    #[serde(default)]
    pub port_forwards: Vec<PortForward>,
}

/// The VM's login policy: the SSH username and, for prepared-base-derived VMs, the
/// keypair placed alongside the manifest (paths relative to `vm/<name>/`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginPolicy {
    pub username: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_private_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_public_key: Option<String>,
}
