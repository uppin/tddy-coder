//! ProcessRuntime-backed session action execution.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use dashmap::DashMap;
use serde_json::{json, Value};
use tddy_actions::{
    action_spec_from_session_manifest, ActionSpec, ProcessRuntime, SessionManifestFields,
};
use tddy_task::{TaskHandle, TaskId, TaskRegistry, TaskStatus};

use super::error::SessionActionsError;
use super::invoke::finalize_invocation_record;
use super::manifest::ActionManifest;

fn tokio_runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("session action tokio runtime")
    })
}

pub(crate) fn block_on<F: Future>(f: F) -> F::Output {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        return tokio::task::block_in_place(|| handle.block_on(f));
    }
    tokio_runtime().block_on(f)
}

static SESSION_REGISTRIES: OnceLock<DashMap<PathBuf, Arc<TaskRegistry>>> = OnceLock::new();

fn registries() -> &'static DashMap<PathBuf, Arc<TaskRegistry>> {
    SESSION_REGISTRIES.get_or_init(DashMap::new)
}

/// Per-session task registry for async jobs (`wait` / `stop` reuse the same registry).
pub fn session_task_registry(session_dir: &Path) -> Arc<TaskRegistry> {
    let key = session_dir
        .canonicalize()
        .unwrap_or_else(|_| session_dir.to_path_buf());
    registries()
        .entry(key)
        .or_insert_with(|| Arc::new(TaskRegistry::new()))
        .value()
        .clone()
}

/// Build a session-action [`ActionSpec`] from a parsed manifest.
pub fn action_manifest_to_spec(m: &ActionManifest, working_dir: Option<PathBuf>) -> ActionSpec {
    action_spec_from_session_manifest(SessionManifestFields {
        version: m.version,
        id: m.id.clone(),
        summary: m.summary.clone(),
        architecture: m.architecture.clone(),
        command: m.command.clone(),
        input_schema: m.input_schema.clone(),
        output_schema: m.output_schema.clone(),
        result_kind: m.result_kind.clone(),
        output_path_arg: m.output_path_arg.clone(),
        working_dir,
    })
}

pub(crate) async fn spawn_manifest_task(
    registry: &TaskRegistry,
    spec: ActionSpec,
    session_key: &str,
) -> Result<Arc<TaskHandle>, SessionActionsError> {
    ProcessRuntime::spawn(registry, spec, session_key)
        .await
        .map_err(|e| SessionActionsError::CommandSpawn {
            program: "session-action".into(),
            detail: e.to_string(),
        })
}

pub(crate) async fn wait_task_terminal(handle: &TaskHandle) {
    let mut rx = handle.status_watch();
    loop {
        if rx.borrow().is_terminal() {
            return;
        }
        if rx.changed().await.is_err() {
            return;
        }
    }
}

pub(crate) fn write_channel_logs(
    handle: &TaskHandle,
    stdout_path: &Path,
    stderr_path: &Path,
) -> Result<(), SessionActionsError> {
    if let Some(ch) = handle.channel("stdout") {
        std::fs::write(stdout_path, ch.replay_capture())?;
    }
    if let Some(ch) = handle.channel("stderr") {
        std::fs::write(stderr_path, ch.replay_capture())?;
    }
    Ok(())
}

pub(crate) fn manifest_record_from_handle(
    manifest: &ActionManifest,
    handle: &TaskHandle,
) -> Result<Value, SessionActionsError> {
    let stdout = handle
        .channel("stdout")
        .map(|ch| String::from_utf8_lossy(&ch.replay_capture()).into_owned())
        .unwrap_or_default();
    let stderr = handle
        .channel("stderr")
        .map(|ch| String::from_utf8_lossy(&ch.replay_capture()).into_owned())
        .unwrap_or_default();
    let exit_code = match handle.status() {
        TaskStatus::Completed { exit_code } => exit_code.unwrap_or(-1),
        TaskStatus::Cancelled => -1,
        TaskStatus::Failed { .. } => -1,
        _ => -1,
    };
    let mut record = json!({
        "exit_code": exit_code,
        "stdout": stdout,
        "stderr": stderr,
    });
    finalize_invocation_record(manifest, &mut record)?;
    Ok(record)
}

/// Run a manifest synchronously via [`ProcessRuntime`].
pub fn run_manifest_blocking(
    manifest: &ActionManifest,
    working_dir: PathBuf,
) -> Result<Value, SessionActionsError> {
    let spec = action_manifest_to_spec(manifest, Some(working_dir));
    let registry = TaskRegistry::new();
    let handle = block_on(spawn_manifest_task(&registry, spec, &manifest.id))?;
    block_on(wait_task_terminal(&handle));
    manifest_record_from_handle(manifest, &handle)
}

/// Start a manifest asynchronously; returns the task handle and session registry.
pub fn start_manifest_async(
    session_dir: &Path,
    manifest: &ActionManifest,
    working_dir: PathBuf,
) -> Result<(Arc<TaskHandle>, Arc<TaskRegistry>), SessionActionsError> {
    let spec = action_manifest_to_spec(manifest, Some(working_dir));
    let registry = session_task_registry(session_dir);
    let handle = block_on(spawn_manifest_task(registry.as_ref(), spec, &manifest.id))?;
    Ok((handle, registry))
}

/// Schedule log mirroring once the task reaches a terminal state.
pub fn schedule_async_log_mirror(
    handle: Arc<TaskHandle>,
    stdout_path: PathBuf,
    stderr_path: PathBuf,
) {
    tokio_runtime().spawn(async move {
        wait_task_terminal(&handle).await;
        let _ = write_channel_logs(&handle, &stdout_path, &stderr_path);
    });
}

/// Poll task status from the session registry.
pub fn task_status_for_job(
    registry: &TaskRegistry,
    task_id: &str,
) -> Result<Option<TaskStatus>, SessionActionsError> {
    Ok(block_on(async {
        registry.get_by_str(task_id).await.map(|h| h.status())
    }))
}

/// Request cancellation via the session task registry.
pub fn cancel_task_in_registry(registry: &TaskRegistry, task_id: &str) -> bool {
    block_on(registry.cancel_task(&TaskId(task_id.to_string())))
}
