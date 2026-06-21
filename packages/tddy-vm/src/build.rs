use crate::vm::VmError;
use std::path::{Path, PathBuf};

/// Build a VM image from the given build target using the tddy-build system.
/// Returns the path to the produced qcow2 image.
pub async fn build_vm_image(repo_root: &Path, build_target: &str) -> Result<PathBuf, VmError> {
    let _ = (repo_root, build_target);
    unimplemented!("build_vm_image: not yet implemented")
}
