//! A coding agent (Claude Code, Codex, OpenCode over ACP) as a roster entry: how its catalog row
//! becomes a record, how a managed session admits it, and how its conversation is opened.
//!
//! The session itself, the catalog and the confinement table live in `tddy-coding-subagent`; this
//! module is the daemon-side glue that reads the session, the config and the launcher port.
//!
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-10-24-coagents-acp-subagent.md

use std::path::Path;

use tddy_coding_subagent::{
    coding_agent_defs_in, confinement_for, AgentTransport, CodingAgentDef, CodingAgentLaunch,
    CodingAgentSession, Confinement,
};
use tddy_core::{AgentKindRecord, SessionAgentRecord};
use tddy_core::{McpServerSpec, OpenSession};
use tddy_daemon_kernel::config::DaemonConfig;
use tddy_rpc::Status;
use tddy_service::proto::types::AgentKind;

use crate::agent_host_callbacks::AgentRoster;
use crate::agent_roster::qualified_agent_id;

/// The `name` the tddy-tools MCP server is registered under in a managed coding agent's session.
const TDDY_TOOLS_SERVER: &str = "tddy-tools";

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

/// The persisted kind a wire `kind` stands for; an unknown number reads as the default, a model.
#[must_use]
pub fn agent_kind_from_wire(kind: i32) -> AgentKindRecord {
    if kind == AgentKind::CodingAgent as i32 {
        AgentKindRecord::CodingAgent
    } else {
        AgentKindRecord::Model
    }
}

/// Refuse a coding agent whose harness cannot have its native tools withdrawn, for a managed
/// session — whose main agent has none, so its subagent must not either. Never launched
/// unconfined.
pub fn refuse_unconfinable_harness(
    session_id: &str,
    session_dir: &Path,
    record: &SessionAgentRecord,
) -> Result<(), Status> {
    let Some(harness) = coding_harness_of(record) else {
        return Ok(());
    };
    let metadata = read_metadata(session_id, session_dir)?;
    refuse_if_managed_and_unconfinable(
        session_id,
        &record.agent_id,
        harness,
        metadata.managed_codebase,
    )
}

fn coding_harness_of(record: &SessionAgentRecord) -> Option<&str> {
    match record.kind {
        AgentKindRecord::CodingAgent => Some(record.harness.as_deref().unwrap_or_default()),
        AgentKindRecord::Model => None,
    }
}

fn refuse_if_managed_and_unconfinable(
    session_id: &str,
    agent_id: &str,
    harness: &str,
    managed: bool,
) -> Result<(), Status> {
    if managed && confinement_for(harness) == Confinement::CannotWithdraw {
        return Err(Status::failed_precondition(format!(
            "coding agent '{agent_id}' runs the '{harness}' harness, whose native file-system and \
             shell tools cannot be withdrawn, and session '{session_id}' is a managed codebase: \
             its main agent has none, so a subagent must not either. Attach a coding agent on a \
             harness that can be confined (claude-acp, opencode) or a model agent."
        )));
    }
    Ok(())
}

fn read_metadata(
    session_id: &str,
    session_dir: &Path,
) -> Result<tddy_core::SessionMetadata, Status> {
    tddy_core::read_session_metadata(session_dir).map_err(|e| {
        Status::not_found(format!(
            "session '{session_id}' has no readable metadata at {}: {e}",
            session_dir.display()
        ))
    })
}

impl AgentRoster {
    /// The coding agent this daemon offers under `name`, if any.
    pub(crate) fn coding_agent_def(&self, name: &str) -> Option<CodingAgentDef> {
        coding_agent_defs_for(&self.tddy_data_dir, &self.config)
            .into_iter()
            .find(|def| def.name == name)
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
        if system_prompt.is_some() {
            return Err(Status::invalid_argument(format!(
                "coding agent '{}' keeps its own system prompt; `system_prompt` is not supported \
                 for a coding agent",
                record.agent_id
            )));
        }
        let launcher = self.coding_agent_launcher.clone().ok_or_else(|| {
            Status::failed_precondition(format!(
                "coding agent '{}' cannot start: this daemon has no coding-agent launcher \
                 configured",
                record.agent_id
            ))
        })?;
        let github_user = (self.user_resolver)(session_token)
            .ok_or_else(|| Status::unauthenticated("invalid or expired session"))?;
        let session_os_user = self
            .config
            .os_user_for_github(&github_user)
            .ok_or_else(|| {
                Status::permission_denied(format!(
                "GitHub user {github_user} has no users[] entry on this daemon; a coding agent \
                 runs as the session's OS user"
            ))
            })?;
        let def = self.coding_agent_def(&record.name).ok_or_else(|| {
            Status::invalid_argument(format!(
                "coding agent '{}' resolves to no harness profile or ACP allowed agent on this \
                 daemon any more",
                record.agent_id
            ))
        })?;
        let metadata = read_metadata(session_id, session_dir)?;
        refuse_if_managed_and_unconfinable(
            session_id,
            &record.agent_id,
            &def.harness,
            metadata.managed_codebase,
        )?;
        let worktree = metadata
            .repo_path
            .as_deref()
            .map(std::path::PathBuf::from)
            .ok_or_else(|| {
                Status::failed_precondition(format!(
                    "session '{session_id}' has no worktree for coding agent '{}' to work on",
                    record.agent_id
                ))
            })?;
        let mcp_servers = if metadata.managed_codebase {
            vec![tddy_tools_server(session_id)]
        } else {
            Vec::new()
        };
        let launch = CodingAgentLaunch {
            def: def.clone(),
            session_id: session_id.to_string(),
            session_os_user,
            worktree: worktree.clone(),
            managed: metadata.managed_codebase,
            mcp_servers,
            session_token: session_token.to_string(),
        };
        let transport = launcher
            .launch_transport(&launch)
            .await
            .map_err(|e| Status::failed_precondition(e.to_string()))?;
        let mcp_servers = launcher
            .mcp_servers_for(&launch)
            .await
            .map_err(|e| Status::failed_precondition(e.to_string()))?;
        // A managed session's agent loses its native tools: through `_meta` for the harness that
        // takes them there; the others get theirs through the launcher's environment.
        let meta = metadata
            .managed_codebase
            .then(|| tddy_coding_subagent::withdrawal_session_meta(&def.harness))
            .flatten();
        let open = OpenSession {
            cwd: worktree,
            resume: None,
            model: Some(def.model.clone()),
            mcp_servers,
            meta,
        };
        let session = match transport {
            AgentTransport::Command(command) => {
                CodingAgentSession::start(command, open, def.model).await
            }
            AgentTransport::Stream(stream) => {
                CodingAgentSession::start_over(stream, open, def.model).await
            }
        }
        .map_err(|e| Status::failed_precondition(e.to_string()))?;
        Ok(Box::new(session))
    }
}

/// `tddy-tools --mcp`, the MCP server a managed coding agent reaches the session's exec tools
/// through. The launcher resolves the binary and completes the transport environment.
fn tddy_tools_server(session_id: &str) -> McpServerSpec {
    McpServerSpec {
        name: TDDY_TOOLS_SERVER.to_string(),
        command: std::path::PathBuf::from("tddy-tools"),
        args: vec!["--mcp".to_string()],
        env: vec![("TDDY_REMOTE_SESSION_ID".to_string(), session_id.to_string())],
    }
}
