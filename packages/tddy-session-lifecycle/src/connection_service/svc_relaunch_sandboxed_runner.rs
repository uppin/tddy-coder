use crate::connection_service::agent_roster;

use std::path::Path;

use std::path::PathBuf;

use tddy_rpc::Status;

use std::sync::Arc;

use super::DaemonSessionHost;

/// What the relaunched runner's env is built from.
struct RelaunchJailEnv<'a> {
    session_id: &'a str,
    worktree_path: &'a Path,
    specialized_defs: Vec<tddy_discovery::agent_def::SpecializedAgentDef>,
    egress_dir: &'a Path,
    scratch_tmp: PathBuf,
    scratch_home: &'a Path,
    tool_ipc_socket: &'a Path,
}

/// What the relaunched sandbox runner is spawned with.
struct RelaunchedRunnerSpawn<'a> {
    sandbox_root: PathBuf,
    egress_dir: &'a Path,
    scratch_dir: PathBuf,
    scratch_home: PathBuf,
    tool_ipc_socket: PathBuf,
    ready_marker: &'a Path,
    profile_path: PathBuf,
    loopback_allow_ports: Vec<u16>,
    runner_argv: Vec<String>,
    env: std::collections::BTreeMap<String, String>,
}

/// What bridging a relaunched jail into this daemon, and registering it, needs.
struct RelaunchedJailBridge<'a> {
    session_id: &'a str,
    session_dir: &'a Path,
    worktree_path: &'a Path,
    egress_dir: PathBuf,
    managed: Option<crate::session_toolcall::ManagedWorkflow>,
    session_env: Vec<(String, String)>,
    ready_marker: PathBuf,
    handle: tddy_sandbox::SandboxHandle,
}

/// A relaunched managed session's controller, its orchestration prompt file and its host-side env.
type RelaunchManagedEnv = (
    Option<crate::session_toolcall::ManagedWorkflow>,
    Option<PathBuf>,
    Vec<(String, String)>,
);

impl DaemonSessionHost {
    /// Spawn sandbox-runner + SessionChannel bridge for an existing session directory.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn relaunch_sandboxed_runner(
        &self,
        session_id: &str,
        session_dir: &Path,
        worktree_path: &Path,
        model: &str,
        permission_mode: &str,
        // The session's **persisted** roster, not the names its start request carried: an agent
        // attached while the session ran is in the former and in neither the latter nor the jail's
        // previous seed, and a relaunch that read the request would hand the main agent back a tool
        // the operator had withdrawn from it (PRD AC25).
        agents: &[tddy_core::SessionAgentRecord],
        managed_recipe: Option<Arc<dyn tddy_core::workflow::recipe::WorkflowRecipe>>,
        // When true, spawn the runner with `--resume` so the jailed `claude` continues the existing
        // on-disk transcript (`--resume <id>`) instead of assigning the id to a fresh session
        // (`--session-id <id>`). The persistent sandbox claude HOME keeps the transcript across
        // daemon restarts, so a fresh `--session-id` would abort with "Session ID already in use".
        resume: bool,
    ) -> Result<u32, Status> {
        // The defs are re-resolved for what the jail's *seed* and the warm-up need — an endpoint to
        // wake and a registry to start from. What the main agent loses comes from the roster below,
        // never from these: a def edited since the attach must not change a running session's tools.
        let specialized_defs = self
            .resolve_specialized_agent_defs(&agent_roster::roster_agent_ids(agents))
            .await?;

        // The same readiness gate the start paths apply: a resumed session's subagents are only as
        // usable as a fresh one's if their endpoints are awake before the jail comes back up.
        // Without this, resume would hand the agent a subagent whose first call stalls on a cold
        // model. No fallback — the runner is not relaunched if warm-up fails.
        tddy_discovery::warmup::warm_up_agents(
            &specialized_defs,
            &self.config.agent_warmup_options(),
        )
        .await
        .map_err(|e| Status::failed_precondition(e.to_string()))?;

        let (sandbox_root, egress_dir, scratch_dir, scratch_tmp, context_dir) =
            relaunch_jail_dirs::prepare_relaunch_dirs(session_dir)?;

        relaunch_jail_dirs::refresh_relaunch_context_dir(worktree_path, agents, &context_dir)?;

        let tddy_tools_path = tddy_daemon_sandbox::sandbox_session::resolve_tddy_tools_path(
            self.config
                .claude_cli
                .as_ref()
                .and_then(|c| c.tddy_tools_path.as_deref()),
        );

        // Re-wire managed-workflow orchestration on resume of a managed sandboxed session; the
        // controller resumes at the goal persisted in changeset.yaml. The prompt goes into the
        // jail-visible context dir and the per-session env carries TDDY_SOCKET for host-side
        // `tddy-tools transition` (relayed via the Shell tool).
        let (managed, append_system_prompt_file, session_env) = self.relaunch_managed_workflow(
            session_id,
            session_dir,
            worktree_path,
            managed_recipe,
            &context_dir,
            &tddy_tools_path,
        )?;

        let (tddy_tools_path, sandbox_runner_path, claude_binary, scratch_home) =
            self.resolve_relaunch_binaries(tddy_tools_path);

        let tool_ipc_socket = tddy_sandbox::SandboxSpec::short_ipc_socket_path(session_id);
        let ready_marker = sandbox_root.join("sandbox.ready");
        let _ = std::fs::remove_file(&tool_ipc_socket);
        let _ = std::fs::remove_file(&ready_marker);
        let profile_path = sandbox_root.join("sandbox.sb");
        let perm = if permission_mode.trim().is_empty() {
            "auto"
        } else {
            permission_mode.trim()
        };

        let egress_shim_port = tddy_daemon_sandbox::sandbox_session::pick_free_loopback_port()
            .map_err(Status::internal)?;
        let loopback_allow_ports = vec![egress_shim_port];

        let mut runner_argv = vec![
            sandbox_runner_path,
            "--session-id".into(),
            session_id.to_string(),
            "--context-dir".into(),
            context_dir.to_string_lossy().to_string(),
            "--tool-ipc-socket".into(),
            tool_ipc_socket.to_string_lossy().to_string(),
            "--tddy-tools-path".into(),
            tddy_tools_path.clone(),
            "--ready-marker".into(),
            ready_marker.to_string_lossy().to_string(),
            "--claude-binary".into(),
            claude_binary,
            "--model".into(),
            model.to_string(),
            "--permission-mode".into(),
            perm.to_string(),
            "--egress-shim-port".into(),
            egress_shim_port.to_string(),
            "--stdio".into(),
        ];
        if resume {
            runner_argv.push("--resume".into());
        }
        if let Some(prompt_path) = &append_system_prompt_file {
            runner_argv.push("--append-system-prompt-file".into());
            runner_argv.push(prompt_path.to_string_lossy().to_string());
        }

        let env = self.relaunch_jail_env(RelaunchJailEnv {
            session_id,
            worktree_path,
            specialized_defs,
            egress_dir: &egress_dir,
            scratch_tmp,
            scratch_home: &scratch_home,
            tool_ipc_socket: &tool_ipc_socket,
        })?;

        let handle = self
            .spawn_relaunched_runner(RelaunchedRunnerSpawn {
                sandbox_root,
                egress_dir: &egress_dir,
                scratch_dir,
                scratch_home,
                tool_ipc_socket,
                ready_marker: &ready_marker,
                profile_path,
                loopback_allow_ports,
                runner_argv,
                env,
            })
            .await?;

        let pid = self
            .bridge_relaunched_jail(RelaunchedJailBridge {
                session_id,
                session_dir,
                worktree_path,
                egress_dir,
                managed,
                session_env,
                ready_marker,
                handle,
            })
            .await?;
        Ok(pid)
    }
}

mod relaunch_jail_steps;

mod relaunch_jail_dirs;
