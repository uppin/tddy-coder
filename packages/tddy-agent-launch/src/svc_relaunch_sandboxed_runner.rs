use tddy_session_agents::agent_roster;

use std::path::Path;

use std::path::PathBuf;

use tddy_rpc::Status;

use std::sync::Arc;

use tddy_core::provider_route::HarnessRoute;

use crate::claude_jail::ClaudeJail;
use crate::launch_ports::LaunchSessions;
use crate::session_acting_identity::SessionIdentity;
use crate::svc_start_sandboxed_claude_cli_session::{JailDirs, JailSession};

/// What the relaunched runner's env is built from.
pub struct RelaunchJailEnv<'a> {
    pub session_id: &'a str,
    pub worktree_path: &'a Path,
    pub specialized_defs: Vec<tddy_discovery::agent_def::SpecializedAgentDef>,
    pub egress_dir: &'a Path,
    pub scratch_tmp: PathBuf,
    pub scratch_home: &'a Path,
    pub tool_ipc_socket: &'a Path,
}

/// What the relaunched sandbox runner is spawned with.
pub struct RelaunchedRunnerSpawn<'a> {
    pub sandbox_root: PathBuf,
    pub egress_dir: &'a Path,
    pub scratch_dir: PathBuf,
    pub scratch_home: PathBuf,
    pub tool_ipc_socket: PathBuf,
    pub ready_marker: &'a Path,
    pub profile_path: PathBuf,
    pub loopback_allow_ports: Vec<u16>,
    pub runner_argv: Vec<String>,
    pub env: std::collections::BTreeMap<String, String>,
    /// The provider route the agent runs on, re-resolved for the resume; `None` for its vendor.
    pub route: Option<HarnessRoute>,
}

/// What bridging a relaunched jail into this daemon, and registering it, needs.
pub struct RelaunchedJailBridge<'a> {
    pub session_id: &'a str,
    pub session_dir: &'a Path,
    pub worktree_path: &'a Path,
    pub egress_dir: PathBuf,
    pub managed: Option<tddy_cli_sessions::session_toolcall::ManagedWorkflow>,
    pub session_env: Vec<(String, String)>,
    pub ready_marker: PathBuf,
    pub handle: tddy_sandbox::SandboxHandle,
}

/// A relaunched managed session's controller, its orchestration prompt file and its host-side env.
pub type RelaunchManagedEnv = (
    Option<tddy_cli_sessions::session_toolcall::ManagedWorkflow>,
    Option<PathBuf>,
    Vec<(String, String)>,
);

impl LaunchSessions {
    /// Spawn sandbox-runner + SessionChannel bridge for an existing session directory.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn relaunch_sandboxed_runner(
        &self,
        os_user: &str,
        session_id: &str,
        // The project and the caller's token the session was started with, which the jail's
        // launch names and its tools' account lookups read the vault with.
        project_id: &str,
        session_token: &str,
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
        // The project's account identity the relaunched jail's host-side commands and tools run
        // under, resolved by the caller from the session's own start token.
        identity: SessionIdentity,
        // The provider route the session's start recorded, re-resolved by the caller from the
        // owner's vault as it stands now; `None` for a session on its vendor.
        route: Option<HarnessRoute>,
    ) -> Result<u32, Status> {
        // The defs are re-resolved for what the jail's *seed* and the warm-up need — an endpoint to
        // wake and a registry to start from. What the main agent loses comes from the roster below,
        // never from these: a def edited since the attach must not change a running session's tools.
        let specialized_defs = self
            .agent_roster
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

        // This relaunch serves `claude-cli` alone, so the agent's allow-list is Claude Code's.
        relaunch_jail_dirs::refresh_relaunch_context_dir(
            worktree_path,
            agents,
            &context_dir,
            "claude-cli",
        )?;

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
            os_user,
            identity,
        )?;

        // The session's daemon-added environment, as its start gave it — but for the semantic
        // index, which a relaunch does not rebuild: `SemanticSearch` stays withdrawn.
        let mut env: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
        if !specialized_defs.is_empty() {
            env.extend(self.specialized_subagent_env(&specialized_defs)?);
        }
        env.extend(self.jail_daemon_identity_env());
        env.extend(self.lsp_tools_env(worktree_path));
        env.extend(self.restructure_tools_env());
        env.extend(self.config.claude_cli_renderer_env());

        let jail = JailSession {
            session_id,
            project_id,
            session_token,
            session_dir,
            worktree_path,
        };
        let jail_dirs = JailDirs {
            sandbox_root,
            egress_dir,
            scratch_dir,
            scratch_tmp,
            context_dir,
        };
        // The same launcher the start uses, rendering the same `claude` definition with `resume`
        // bound. The role was recorded by the start and rides in the session's metadata, which the
        // resume keeps.
        self.launch_jailed_claude(ClaudeJail {
            jail: &jail,
            dirs: &jail_dirs,
            model,
            resume,
            permission_mode,
            dangerously_skip_permissions: false,
            passthrough_args: Vec::new(),
            append_system_prompt_file: append_system_prompt_file.as_deref(),
            agents,
            env,
            session_env,
            managed,
            managed_codebase: false,
            route,
            role: "",
        })
        .await
    }
}

pub use crate::{relaunch_jail_dirs, relaunch_jail_steps};
