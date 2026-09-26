//! Where an exec tool actually runs on this daemon: a sandboxed session's jail, a session's own
//! checkout, or the agent clone this daemon hosts for another daemon's session.
//!
//! Shared by the session host — a roster agent's own turn loop runs its tools through here — and by
//! `tddy-daemon-rpc`'s exec-tool family, so the RPC and the agent loop take exactly one path.

use std::path::Path;
use std::sync::Arc;

use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_daemon_sandbox::workspace_tool_sandbox::{
    ToolDispatchOutcome, WorkspaceSandbox, WorkspaceSandboxProvisioner, WorkspaceSandboxRegistry,
};
use tddy_sandbox_runner::ExecuteToolResponse;
use tddy_service::proto::exec_tools::ExecuteToolRequest;
use tddy_task::TaskRegistry;

use super::jail_relaunch::{self, JailRelaunch};
use super::{agent_roster, ExecToolRoute};
use crate::session_agent_clone::{HostedAgentClones, HostedClone};
use crate::tool_engine;

/// The three places this daemon runs a tool, and the registry every run is recorded in.
///
/// Shared, not copied: `Clone` hands out the same task registry, jails and hosted clones, so a
/// tool run through a clone of this is one the rest of the daemon can see and cancel.
#[derive(Clone)]
pub struct LocalExecTools {
    task_registry: TaskRegistry,
    workspace_sandboxes: Arc<WorkspaceSandboxRegistry>,
    /// What builds a jail, held here so a session whose jail died mid-call can be given a new one
    /// on the spot — this is the only layer beneath all three ways a tool call arrives.
    workspace_sandbox_provisioner: Arc<dyn WorkspaceSandboxProvisioner>,
    /// Shared, so two calls that watch the same jail die rebuild it once between them.
    jail_relaunch: Arc<JailRelaunch>,
    hosted_agent_clones: Arc<HostedAgentClones>,
}

impl LocalExecTools {
    #[must_use]
    pub(crate) fn new(
        task_registry: TaskRegistry,
        workspace_sandboxes: Arc<WorkspaceSandboxRegistry>,
        workspace_sandbox_provisioner: Arc<dyn WorkspaceSandboxProvisioner>,
        jail_relaunch: Arc<JailRelaunch>,
        hosted_agent_clones: Arc<HostedAgentClones>,
    ) -> Self {
        Self {
            task_registry,
            workspace_sandboxes,
            workspace_sandbox_provisioner,
            jail_relaunch,
            hosted_agent_clones,
        }
    }

    /// Where `session_id`'s tools run: its own jail, or the checkout on this host.
    ///
    /// Read from what this daemon persisted about the session rather than from the request, because
    /// the request is the caller's claim and the metadata is the session's. A `workspace` session
    /// that recorded `sandbox: true` is served by the jail registered for it and by nothing else.
    async fn exec_tool_route(&self, session_dir: &Path, session_id: &str) -> ExecToolRoute {
        let meta = match tddy_core::read_session_metadata(session_dir) {
            Ok(meta) => meta,
            // The callers all resolved this session's worktree out of this same file moments ago, so
            // an unreadable one here is a transient failure rather than a session that is not
            // sandboxed — and "assume unconfined" is the wrong guess to make about a jail.
            Err(e) => {
                return ExecToolRoute::Refused(format!(
                    "session {session_id}: cannot tell whether this session is sandboxed \
                     (.session.yaml unreadable: {e}); refusing to run its tools on the host"
                ))
            }
        };
        let sandboxed_workspace =
            meta.session_type.as_deref() == Some("workspace") && meta.sandbox == Some(true);
        if !sandboxed_workspace {
            return ExecToolRoute::HostWorktree;
        }
        match self.workspace_sandboxes.get(session_id).await {
            Some(jail) => ExecToolRoute::Jail(jail),
            None => ExecToolRoute::Refused(format!(
                "session {session_id} is sandboxed and this daemon holds no jail for it; \
                 refusing to run its tools on the host worktree"
            )),
        }
    }

    /// Run one tool call for `req`'s session and durably record it.
    ///
    /// A tool failure is carried in the returned response, never raised as an RPC error: only
    /// routing and auth failures are RPC errors, so an agent can tell "the tool said no" from "the
    /// call never reached the tool".
    ///
    /// The single choke point for every exec tool this daemon serves out of its own sessions —
    /// `ExecuteTool`, `StreamExecuteTool`, and a roster agent's own loop
    /// ([`DaemonSessionHost::local_agent_codebase_access`](super::DaemonSessionHost::local_agent_codebase_access)) — which is why a sandboxed workspace session is
    /// routed to its jail here rather than three times over. `worktree_root` is where the tool runs
    /// when it runs on this host; inside the jail the same checkout is mounted at that very path.
    pub async fn run_exec_tool_locally(
        &self,
        req: &ExecuteToolRequest,
        sessions_base: &Path,
        worktree_root: &Path,
    ) -> ExecuteToolResponse {
        let session_dir = unified_session_dir_path(sessions_base, &req.session_id);
        let response = match self.exec_tool_route(&session_dir, &req.session_id).await {
            ExecToolRoute::HostWorktree => {
                let meta = tddy_core::read_session_metadata(&session_dir).ok();
                let ssh_host = meta
                    .as_ref()
                    .and_then(|m| m.ssh_config_host.as_deref())
                    .unwrap_or("");
                let shell = tool_engine::session_shell(worktree_root.to_path_buf(), ssh_host);
                let outcome = tool_engine::execute_tool_on_shell(
                    shell.as_ref(),
                    &req.tool_name,
                    &req.args_json,
                    &self.task_registry,
                    &req.session_id,
                )
                .await;
                ExecuteToolResponse {
                    result_json: outcome.result_json,
                    is_error: outcome.is_error,
                    error_message: outcome.error_message,
                    job_id: outcome.job_id,
                    job_running: outcome.job_running,
                }
            }
            ExecToolRoute::Jail(jail) => match jail.execute_tool(req).await {
                ToolDispatchOutcome::Ran(response) => response,
                // The jail died, not the tool — the one failure that is repairable here.
                ToolDispatchOutcome::TransportFailed(reason) => {
                    self.retry_in_a_rebuilt_jail(req, sessions_base, &jail, &reason)
                        .await
                }
            },
            ExecToolRoute::Refused(reason) => refused(reason),
        };

        // Durably record the tool call (non-fatal on failure). One log for both routes: which side
        // of the jail boundary a call ran on does not change that it is the session's tool call.
        let record = tddy_tool_engine::tool_call_log::ToolCallRecord {
            task_id: response.job_id.clone(),
            tool_name: req.tool_name.clone(),
            args_json: req.args_json.clone(),
            result_json: response.result_json.clone(),
            is_error: response.is_error,
            error_message: response.error_message.clone(),
            job_running: response.job_running,
            created_unix_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        };
        if let Err(e) = tddy_tool_engine::tool_call_log::append_tool_call(&session_dir, &record) {
            log::warn!(
                "tool_call_log: failed to persist tool call for session {}: {}",
                req.session_id,
                e
            );
        }

        response
    }

    /// Rebuild the jail a call just died in, and run that call in the replacement.
    ///
    /// Exactly once, and only for a transport failure. A jail whose channel broke refuses every
    /// later call of that session for the life of the daemon
    /// (`docs/ft/daemon/remote-codebase-mode.md` § Workspace tool sandbox), so the
    /// choice here is between one rebuild and a session that can no longer run a tool. A second
    /// failure is not a transient: the replacement is a freshly spawned runner, and a host that
    /// cannot keep one alive will not keep the third one alive either — so it is reported, and
    /// the caller decides.
    ///
    /// What it never becomes is a way onto the host worktree. A rebuild that cannot be made, or a
    /// replacement that dies the same way, answers with the failure — the session asked to be
    /// confined, and the one outcome nobody can see afterwards is a tool that ran unconfined.
    async fn retry_in_a_rebuilt_jail(
        &self,
        req: &ExecuteToolRequest,
        sessions_base: &Path,
        dead: &Arc<dyn WorkspaceSandbox>,
        reason: &str,
    ) -> ExecuteToolResponse {
        let rebuilt = match jail_relaunch::workspace_sandbox_spec(sessions_base, &req.session_id) {
            Ok(spec) => {
                self.jail_relaunch
                    .rebuild(
                        &self.workspace_sandboxes,
                        self.workspace_sandbox_provisioner.as_ref(),
                        &spec,
                        dead,
                    )
                    .await
            }
            Err(status) => Err(status.message().to_string()),
        };
        let rebuilt = match rebuilt {
            Ok(jail) => jail,
            Err(e) => {
                return refused(format!(
                    "{reason}, and its jail could not be rebuilt ({e}); refusing to run it on the \
                     host worktree instead"
                ))
            }
        };
        match rebuilt.execute_tool(req).await {
            ToolDispatchOutcome::Ran(response) => response,
            ToolDispatchOutcome::TransportFailed(again) => refused(format!(
                "{again}, in a jail rebuilt moments earlier because the first one failed the same \
                 way; refusing to run it on the host worktree instead"
            )),
        }
    }

    /// The clone this daemon hosts for `session_id`, when it holds one.
    ///
    /// What makes an exec tool addressed at this daemon for another daemon's session resolvable at
    /// all: the session lives elsewhere, so the ordinary "resolve the worktree from my own sessions
    /// base" would find nothing.
    pub fn hosted_clone_for(&self, session_id: &str) -> Option<Arc<HostedClone>> {
        self.hosted_agent_clones.get(session_id)
    }

    /// Serve one exec tool for a session whose checkout this daemon holds as an agent clone.
    ///
    /// This is where the read/write split actually happens, so the agent's own turn loop and an
    /// exec-tool RPC addressed here take exactly one path. A read is answered from the clone with no
    /// round trip — which is the entire reason for placing an agent on this host — and a mutation is
    /// proxied to the facilitating daemon's authoritative worktree.
    ///
    /// Which tree is worked is settled by the clone link rather than by the caller: a hosted clone is
    /// a checkout this daemon built for exactly one session on exactly one peer, at the request of a
    /// `StartSession` it already authenticated, so the request cannot select any other tree and the
    /// OS user the tools run as was settled then. *Who may drive them* is settled by the caller's
    /// session token, which every path into here — the RPC handlers and this daemon's own agent turn
    /// loop — establishes before this is reached: the mutating half proxies to the facilitating
    /// daemon under the clone's stored credential, so an unauthenticated caller reaching here would
    /// be writing into another host's authoritative worktree under a credential it never held.
    ///
    /// TODO(session-agent-roster): narrow that to a session-scoped tool token — audience = this
    /// clone's session, exec-tool methods only — which is the same credential the split placement's
    /// trust model already wants and `docs/dev/TODO.md` already records.
    pub async fn run_hosted_clone_tool(
        &self,
        req: &ExecuteToolRequest,
        clone: &HostedClone,
    ) -> ExecuteToolResponse {
        if !agent_roster::agent_tool_reads_the_clone(&req.tool_name) {
            return match clone
                .execute_tool_on_facilitator(&req.tool_name, &req.args_json)
                .await
            {
                Ok(result_json) => ExecuteToolResponse {
                    result_json,
                    is_error: false,
                    error_message: String::new(),
                    job_id: String::new(),
                    job_running: false,
                },
                // Carried in the response rather than raised, exactly as a locally-run tool failure
                // is: the agent asked for a tool and the tool did not happen, which is a tool result
                // and not a transport failure.
                Err(status) => ExecuteToolResponse {
                    result_json: serde_json::json!({ "error": status.message() }).to_string(),
                    is_error: true,
                    error_message: status.message().to_string(),
                    job_id: String::new(),
                    job_running: false,
                },
            };
        }
        let outcome = tool_engine::execute_tool(
            &clone.worktree_path,
            &req.tool_name,
            &req.args_json,
            &self.task_registry,
            &req.session_id,
        )
        .await;
        ExecuteToolResponse {
            result_json: outcome.result_json,
            is_error: outcome.is_error,
            error_message: outcome.error_message,
            job_id: outcome.job_id,
            job_running: outcome.job_running,
        }
    }
}

/// A tool call this daemon would not run, answered as the failure it is.
///
/// Carried in the response rather than raised, exactly as a tool's own failure is: the agent
/// asked for a tool, and what it needs to read is why it did not happen.
fn refused(reason: String) -> ExecuteToolResponse {
    log::warn!("exec tool: {reason}");
    ExecuteToolResponse {
        result_json: String::new(),
        is_error: true,
        error_message: reason,
        job_id: String::new(),
        job_running: false,
    }
}
