//! Rebuilding the jail a sandboxed `workspace` session lost mid-call.
//!
//! The jail is an in-process handle to a `tddy-sandbox-runner` child, and a handle whose channel
//! broke stays broken for the life of the daemon — every later tool call of that session is
//! refused, because answering from the host checkout the session was jailed away from would be
//! worse than failing. Replacing the jail is the third option, and this is where it happens.
//!
//! Reached only from a [`ToolDispatchOutcome::TransportFailed`](tddy_daemon_sandbox::workspace_tool_sandbox::ToolDispatchOutcome::TransportFailed):
//! a tool that ran and exited non-zero leaves the jail alone.
//!
//! Feature: `docs/ft/daemon/remote-codebase-mode.md` § Workspace tool sandbox.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex as StdMutex};

use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon_sandbox::workspace_tool_sandbox::{
    WorkspaceSandbox, WorkspaceSandboxProvisioner, WorkspaceSandboxRegistry, WorkspaceSandboxSpec,
};
use tddy_rpc::Status;

use crate::workspace_session;

/// What `session_id`'s jail is built over: the session's own directory, and the checkout that is
/// the only part of this host inside it.
///
/// One definition for the start that first provisions the jail and the dispatch that rebuilds it,
/// so a replacement confines exactly what the original did. Read from `.session.yaml` rather than
/// taken from the caller, for the same reason every other routing decision about a jailed session
/// is.
pub(crate) fn workspace_sandbox_spec(
    sessions_base: &Path,
    session_id: &str,
) -> Result<WorkspaceSandboxSpec, Status> {
    Ok(WorkspaceSandboxSpec {
        session_id: session_id.to_string(),
        session_dir: unified_session_dir_path(sessions_base, session_id),
        worktree_path: workspace_session::resolve_worktree_root_for_session(
            sessions_base,
            session_id,
        )?,
    })
}

/// Rebuilds a dead workspace jail — once per death, however many tool calls witnessed it.
///
/// Shared across every clone of the session host, because that is what makes "once" true: two
/// concurrent tool calls that both watch the same jail die must produce one replacement between
/// them, not two runners over one checkout.
#[derive(Default)]
pub(crate) struct JailRelaunch {
    /// The session ids being rebuilt right now, each with the gate its rebuilders queue on.
    ///
    /// Per session rather than one gate for the daemon: provisioning a jail waits for the runner
    /// to come up (`JAIL_READY_TIMEOUT`, 120s), so a single gate would stall an unrelated
    /// session's tool call for two minutes behind someone else's repair.
    gates: StdMutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl JailRelaunch {
    /// Replace `spec.session_id`'s jail with a fresh one and register it, answering with the jail
    /// the next call should use.
    ///
    /// `dead` is the jail the caller's own call failed in. A caller that queued behind someone
    /// else's rebuild finds a jail it has never used and is handed that one instead of building a
    /// second: the gate makes the rebuild exclusive, and the identity check makes it happen once.
    ///
    /// The error is prose because it is reported to an agent, never matched on.
    pub(crate) async fn rebuild(
        &self,
        sandboxes: &WorkspaceSandboxRegistry,
        provisioner: &dyn WorkspaceSandboxProvisioner,
        spec: &WorkspaceSandboxSpec,
        dead: &Arc<dyn WorkspaceSandbox>,
    ) -> Result<Arc<dyn WorkspaceSandbox>, String> {
        let gate = self.gate_for(&spec.session_id);
        let _rebuilding = gate.lock().await;
        let rebuilt = self
            .rebuild_under_gate(sandboxes, provisioner, spec, dead)
            .await;
        self.forget_unused_gate(&spec.session_id, &gate);
        rebuilt
    }

    async fn rebuild_under_gate(
        &self,
        sandboxes: &WorkspaceSandboxRegistry,
        provisioner: &dyn WorkspaceSandboxProvisioner,
        spec: &WorkspaceSandboxSpec,
        dead: &Arc<dyn WorkspaceSandbox>,
    ) -> Result<Arc<dyn WorkspaceSandbox>, String> {
        match sandboxes.get(&spec.session_id).await {
            // Someone else's rebuild, finished while this caller waited on the gate. Its jail is
            // the session's jail now, and building another would orphan one of them.
            Some(registered) if !Arc::ptr_eq(&registered, dead) => return Ok(registered),
            Some(_) => {}
            // The session gave its jail up while the call was in flight — a delete, or a teardown
            // after a failed start. Rebuilding would resurrect a jail for a session that is going
            // away, and leave its runner behind when it does.
            None => {
                return Err(format!(
                    "session {}: its jail was withdrawn while the call was in flight",
                    spec.session_id
                ))
            }
        }

        // Out of the registry and stopped *before* the replacement is built: the jail is a live
        // `tddy-sandbox-runner` holding the checkout open, and two of them over one worktree is a
        // worse state than none. A failed rebuild therefore leaves the session with no jail,
        // which every later call refuses — the same refusal this path exists to repair, and the
        // right one when the repair itself cannot be made.
        if let Some(stopping) = sandboxes.remove(&spec.session_id).await {
            // `stop` kills the runner and waits for it, so it goes to the blocking pool for the
            // same reason the shutdown sweep does.
            if let Err(e) = tokio::task::spawn_blocking(move || stopping.stop()).await {
                log::error!(
                    "session {}: the jail being replaced could not be stopped ({e}); its runner \
                     may be orphaned onto this host, and the replacement is built regardless",
                    spec.session_id
                );
            }
        }

        let rebuilt = provisioner
            .provision(spec)
            .await
            .map_err(|e| e.to_string())?;
        sandboxes
            .insert(spec.session_id.clone(), Arc::clone(&rebuilt))
            .await;
        log::info!(
            "session {}: its jail died mid-call and was rebuilt over {}",
            spec.session_id,
            spec.worktree_path.display()
        );
        Ok(rebuilt)
    }

    fn gate_for(&self, session_id: &str) -> Arc<tokio::sync::Mutex<()>> {
        Arc::clone(
            self.gates
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .entry(session_id.to_string())
                .or_default(),
        )
    }

    /// Drop the gate of a session nobody else is queued on, so a daemon that repairs many
    /// sessions over its life does not keep a mutex per session forever.
    ///
    /// Called while the gate is still held, which is what makes the count decisive: every other
    /// rebuilder holds a clone of this `Arc`, and takes the map's lock to get one, so a count of
    /// two under that lock is this caller plus the map and nobody else. A caller arriving after
    /// the entry is gone makes a fresh gate and finds the rebuilt jail already registered.
    fn forget_unused_gate(&self, session_id: &str, gate: &Arc<tokio::sync::Mutex<()>>) {
        let mut gates = self
            .gates
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if Arc::strong_count(gate) == 2 {
            gates.remove(session_id);
        }
    }
}
