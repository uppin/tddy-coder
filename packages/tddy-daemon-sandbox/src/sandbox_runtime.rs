//! Sandbox action runtime — orchestrates plan build + confined spawn.

use std::path::PathBuf;
use std::sync::Arc;

use tddy_actions::{ActionSpec, SandboxRequest};
use tddy_task::{TaskHandle, TaskRegistry};

pub use crate::sandbox_action::spawn_sandbox_action;

/// Attach sandbox request from StartAction params when `sandbox` is true.
pub fn attach_sandbox_request(
    mut spec: ActionSpec,
    output_dir: PathBuf,
    recipe: Option<String>,
    extra_read_paths: Vec<PathBuf>,
    stdin: Option<String>,
) -> ActionSpec {
    let output_dir = std::fs::canonicalize(&output_dir).unwrap_or(output_dir);
    let extra_read_paths = extra_read_paths
        .into_iter()
        .map(|p| std::fs::canonicalize(&p).unwrap_or(p))
        .collect();
    spec.sandbox = Some(SandboxRequest {
        output_dir,
        extra_read_paths,
        recipe,
        stdin,
    });
    spec
}

/// Spawn an action declared with a [`SandboxRequest`] inside a platform sandbox.
pub struct SandboxRuntime;

impl SandboxRuntime {
    pub async fn spawn(
        registry: &TaskRegistry,
        spec: ActionSpec,
        session_id: impl Into<String>,
    ) -> Result<Arc<TaskHandle>, String> {
        spawn_sandbox_action(registry, spec, session_id).await
    }
}
