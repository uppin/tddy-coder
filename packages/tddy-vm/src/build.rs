use crate::vm::VmError;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tddy_build::discovery::discover_build_manifests;
use tddy_build::executor::{execute_target, ExecuteOptions};
use tddy_build::graph::BuildGraph;
use tddy_build::plugin::PluginRegistry;
use tddy_build_qemu::QemuPlugin;

/// Build a VM image from the given build target using the tddy-build system.
/// Returns the path to the produced qcow2 image.
pub async fn build_vm_image(repo_root: &Path, build_target: &str) -> Result<PathBuf, VmError> {
    // Discover BUILD.yaml manifests from repo_root
    let discovered =
        discover_build_manifests(repo_root).map_err(|e| VmError::BuildFailed(e.to_string()))?;
    if discovered.is_empty() {
        return Err(VmError::BuildFailed(format!(
            "no BUILD.yaml found under {}",
            repo_root.display()
        )));
    }

    let manifests = discovered.into_iter().map(|(_, m)| m).collect();
    let graph =
        BuildGraph::from_manifests(manifests).map_err(|e| VmError::BuildFailed(e.to_string()))?;

    // Set up plugin registry with QemuPlugin
    let mut registry = PluginRegistry::new();
    registry.register(Arc::new(QemuPlugin));

    // Get output path before executing (from the action plan)
    let actions = graph
        .actions_for(build_target, &registry)
        .map_err(|e| VmError::BuildFailed(e.to_string()))?;
    let output_path = actions
        .first()
        .and_then(|a| a.outputs.first())
        .map(|o| repo_root.join(&o.path))
        .ok_or_else(|| {
            VmError::BuildFailed(format!("target '{}' has no output actions", build_target))
        })?;

    // Execute the target
    execute_target(
        repo_root,
        &graph,
        build_target,
        &ExecuteOptions::default(),
        &registry,
    )
    .await
    .map_err(|e| VmError::BuildFailed(e.to_string()))?;

    Ok(output_path)
}
