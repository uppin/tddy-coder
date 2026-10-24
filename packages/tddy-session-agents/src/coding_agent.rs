//! A coding agent (Claude Code, Codex, OpenCode over ACP) as a roster entry: how its catalog row
//! becomes a record, how a managed session admits it, and how its conversation is opened.
//!
//! The session itself, the catalog and the confinement table live in `tddy-coding-subagent`; this
//! module is the daemon-side glue that reads the session, the config and the launcher port.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-10-24-coagents-acp-subagent.md

use std::path::{Path, PathBuf};

use tddy_coding_subagent::{
    coding_agent_defs_in, refuse_unless_confinable, withdrawal_session_meta, AgentTransport,
    CodingAgentDef, CodingAgentLaunch, CodingAgentLauncher, CodingAgentSession,
};
use tddy_core::OpenSession;
use tddy_core::{AgentKindRecord, SessionAgentRecord};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::Status;
use tddy_service::proto::types::AgentKind;

use crate::agent_host_callbacks::AgentRoster;
use crate::agent_roster::qualified_agent_id;
use crate::seed_codebase::read_session_metadata;

/// The coding agents this daemon offers: ACP harness profiles under `<tddyhome>/agents`, then its
/// ACP `allowed_agents` ids.
pub fn coding_agent_defs_for(tddy_data_dir: &Path, config: &DaemonConfig) -> Vec<CodingAgentDef> {
    let allowed: Vec<(String, Option<String>)> = config
        .allowed_agents()
        .iter()
        .map(|agent| (agent.id.clone(), agent.label.clone()))
        .collect();
    coding_agent_defs_in(&tddy_data_dir.join("agents"), &allowed)
}

/// The roster entry attaching `def` produces. A coding agent replaces no tool and carries no tool
/// list: its own tools are its harness's.
pub fn coding_agent_record(
    def: &CodingAgentDef,
    daemon_instance_id: &str,
) -> Result<SessionAgentRecord, tddy_core::AgentIdError> {
    Ok(SessionAgentRecord {
        agent_id: qualified_agent_id(&def.name, daemon_instance_id)?,
        name: def.name.clone(),
        daemon_instance_id: daemon_instance_id.to_string(),
        label: def.label.clone().filter(|s| !s.trim().is_empty()),
        model: def.model.clone(),
        replaces: Vec::new(),
        tools: Vec::new(),
        codebase_session_id: None,
        kind: AgentKindRecord::CodingAgent,
        harness: Some(def.harness.clone()),
    })
}

/// The wire spelling of a record's kind.
#[must_use]
pub fn agent_kind_to_wire(kind: AgentKindRecord) -> AgentKind {
    match kind {
        AgentKindRecord::Model => AgentKind::Model,
        AgentKindRecord::CodingAgent => AgentKind::CodingAgent,
    }
}

/// The persisted kind a wire `kind` stands for. A number this daemon does not know is refused
/// naming it — read as a model, a peer's newer kind would be attached as something it is not.
pub fn agent_kind_from_wire(kind: i32) -> Result<AgentKindRecord, Status> {
    match AgentKind::try_from(kind) {
        Ok(AgentKind::Model) => Ok(AgentKindRecord::Model),
        Ok(AgentKind::CodingAgent) => Ok(AgentKindRecord::CodingAgent),
        Err(_) => Err(Status::invalid_argument(format!(
            "agent kind {kind} is not one this daemon knows (model = {}, coding agent = {})",
            AgentKind::Model as i32,
            AgentKind::CodingAgent as i32
        ))),
    }
}

/// Refuse a coding agent whose harness cannot have its native tools withdrawn, for a managed
/// session — whose main agent has none, so its subagent must not either. Never launched
/// unconfined. `managed` is the session's `managed_codebase`, as its caller already read it.
pub fn refuse_unconfinable_harness(
    session_id: &str,
    record: &SessionAgentRecord,
    managed: bool,
) -> Result<(), Status> {
    match record.kind {
        AgentKindRecord::CodingAgent => refuse_in_managed_session(
            session_id,
            &record.agent_id,
            record.harness.as_deref().unwrap_or_default(),
            managed,
        ),
        AgentKindRecord::Model => Ok(()),
    }
}

fn refuse_in_managed_session(
    session_id: &str,
    agent_id: &str,
    harness: &str,
    managed: bool,
) -> Result<(), Status> {
    refuse_unless_confinable(harness, managed).map_err(|reason| {
        Status::failed_precondition(format!(
            "coding agent '{agent_id}' cannot join session '{session_id}', a managed codebase: \
             {reason}"
        ))
    })
}

/// Refuse a `system_prompt` for a coding agent, which keeps its own.
fn refuse_system_prompt(
    record: &SessionAgentRecord,
    system_prompt: Option<&str>,
) -> Result<(), Status> {
    match system_prompt {
        Some(_) => Err(Status::invalid_argument(format!(
            "coding agent '{}' keeps its own system prompt; `system_prompt` is not supported \
             for a coding agent",
            record.agent_id
        ))),
        None => Ok(()),
    }
}

/// The worktree a coding agent of `session_id` works on: the session's own checkout.
fn session_worktree(
    session_id: &str,
    agent_id: &str,
    metadata: &tddy_core::SessionMetadata,
) -> Result<PathBuf, Status> {
    metadata
        .repo_path
        .as_deref()
        .map(PathBuf::from)
        .ok_or_else(|| {
            Status::failed_precondition(format!(
                "session '{session_id}' has no worktree for coding agent '{agent_id}' to work on"
            ))
        })
}

/// Start the agent `launch` names through `launcher` and open its ACP session. The MCP servers
/// are settled first, so a refusal there never leaves a process started as another user.
async fn start_coding_agent_session(
    launcher: &dyn CodingAgentLauncher,
    launch: &CodingAgentLaunch,
) -> Result<CodingAgentSession, Status> {
    let refused =
        |e: tddy_discovery::subagent::SubagentError| Status::failed_precondition(e.to_string());
    let mcp_servers = launcher.mcp_servers_for(launch).await.map_err(refused)?;
    let transport = launcher.launch_transport(launch).await.map_err(refused)?;
    // A managed session's agent loses its native tools: through `_meta` for the harness that
    // takes them there; the others get theirs through the launcher's environment.
    let open = OpenSession {
        cwd: launch.worktree.clone(),
        resume: None,
        model: Some(launch.def.model.clone()),
        mcp_servers,
        meta: launch
            .managed
            .then(|| withdrawal_session_meta(&launch.def.harness))
            .flatten(),
    };
    let model = launch.def.model.clone();
    match transport {
        AgentTransport::Command(command) => CodingAgentSession::start(command, open, model).await,
        AgentTransport::Stream(stream) => CodingAgentSession::start_over(stream, open, model).await,
    }
    .map_err(refused)
}

impl AgentRoster {
    /// The coding agent this daemon offers under `name`, if any.
    pub(crate) fn coding_agent_def(&self, name: &str) -> Option<CodingAgentDef> {
        coding_agent_defs_for(&self.tddy_data_dir, &self.config)
            .into_iter()
            .find(|def| def.name == name)
    }

    /// The GitHub user `session_token` authenticates.
    pub(crate) fn session_github_user(&self, session_token: &str) -> Result<String, Status> {
        (self.user_resolver)(session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))
    }

    /// The OS user `session_token`'s caller runs as on this daemon — what a coding agent runs as.
    fn session_os_user(&self, session_token: &str) -> Result<String, Status> {
        let github_user = self.session_github_user(session_token)?;
        self.config.os_user_for_github(&github_user).ok_or_else(|| {
            Status::permission_denied(format!(
                "GitHub user {github_user} has no users[] entry on this daemon; a coding agent \
                 runs as the session's OS user"
            ))
        })
    }

    /// Everything a launch of `record` for `session_id` needs, read from the caller, the daemon's
    /// offer and the session's metadata — refused when a managed session cannot confine it.
    fn coding_agent_launch(
        &self,
        session_id: &str,
        session_dir: &Path,
        record: &SessionAgentRecord,
        session_token: &str,
    ) -> Result<CodingAgentLaunch, Status> {
        let session_os_user = self.session_os_user(session_token)?;
        let def = self.coding_agent_def(&record.name).ok_or_else(|| {
            Status::invalid_argument(format!(
                "coding agent '{}' resolves to no harness profile or ACP allowed agent on this \
                 daemon any more",
                record.agent_id
            ))
        })?;
        let metadata = read_session_metadata(session_id, session_dir)?;
        let managed = metadata.managed_codebase;
        refuse_in_managed_session(session_id, &record.agent_id, &def.harness, managed)?;
        Ok(CodingAgentLaunch {
            worktree: session_worktree(session_id, &record.agent_id, &metadata)?,
            def,
            session_id: session_id.to_string(),
            session_os_user,
            managed,
            session_token: session_token.to_string(),
        })
    }

    /// A conversation with a coding agent, started through the daemon's launcher as the session's
    /// OS user on the session's worktree. Without a launcher this refuses naming that — a coding
    /// agent is never started some other way.
    pub(crate) async fn open_coding_agent_session(
        &self,
        session_id: &str,
        session_dir: &Path,
        record: &SessionAgentRecord,
        session_token: &str,
        system_prompt: Option<&str>,
    ) -> Result<Box<dyn tddy_discovery::subagent::SubagentSession>, Status> {
        refuse_system_prompt(record, system_prompt)?;
        let launcher = self.coding_agent_launcher.clone().ok_or_else(|| {
            Status::failed_precondition(format!(
                "coding agent '{}' cannot start: this daemon has no coding-agent launcher \
                 configured",
                record.agent_id
            ))
        })?;
        let launch = self.coding_agent_launch(session_id, session_dir, record, session_token)?;
        let session = start_coding_agent_session(launcher.as_ref(), &launch).await?;
        Ok(Box::new(session))
    }
}
