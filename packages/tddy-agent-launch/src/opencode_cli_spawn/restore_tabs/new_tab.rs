//! A new agent tab and the pieces of its launch chain every start and restore shares: the argv a tab
//! runs, the `agent` node it records, and whether a resume can name its conversation.

use std::path::Path;

use tddy_coding_agent_specs::slots::conversation::ConversationSlot;
use tddy_coding_agent_specs::{CodingAgentSpec, SpecError};
use tddy_core::tab_roster::LaunchNode;
use tddy_rpc::Status;

use super::plan::RESUMABLE_PROPERTY;
use crate::cli_agent_steps::conversation::{start_conversation, ConversationStart};
use crate::spec_launch::HostCodingAgents;

/// The argv of an agent tab of `spec` holding `conversation_id`, run as `binary`: `resume` set,
/// it continues that conversation (claude `--resume <id>`, cursor `--resume <chat id>`); unset, it
/// starts it (claude `--session-id <id>`).
pub fn agent_tab_argv(
    spec: &CodingAgentSpec,
    binary: &Path,
    conversation_id: &str,
    resume: bool,
) -> Result<Vec<String>, SpecError> {
    let flags: &[&str] = if resume { &["resume"] } else { &[] };
    spec.render_run_argv(
        binary,
        &[
            ("session_id", conversation_id),
            ("chat_id", conversation_id),
        ],
        flags,
    )
}

/// Whether a tab of `spec` can resume a given conversation: its conversation is the daemon's session
/// id or one the agent minted. A worktree's own (`--continue`) or a resume token read off an exit
/// is not the tab's to name.
///
/// TODO(opencode-cli): a tab resumes its worktree's last conversation, which is main's, not its own.
/// TODO(muse-cli): same as opencode: `--continue` is per worktree.
/// TODO(warp-cli): a tab does not capture the exit-hook token a resume needs.
pub fn tab_conversation_is_resumable(spec: &CodingAgentSpec) -> bool {
    matches!(
        spec.conversation(),
        ConversationSlot::DaemonSessionId { .. } | ConversationSlot::Minted { .. }
    )
}

/// The `agent` launch node of a tab of `harness` in `conversation_id`, marked `resumable: false`
/// when its harness cannot resume a given conversation.
pub fn agent_launch_node(harness: &str, conversation_id: &str, resumable: bool) -> LaunchNode {
    let mut node = LaunchNode::agent(harness, conversation_id);
    if !resumable {
        node.properties
            .insert(RESUMABLE_PROPERTY.to_string(), "false".to_string());
    }
    node
}

/// A new agent tab, ready to start: what it runs, the conversation it holds and whether a resume can
/// name that conversation.
pub struct NewAgentTab {
    pub argv: Vec<String>,
    pub conversation_id: String,
    pub resumable: bool,
}

/// Settle a new agent tab of session type `kind` in `worktree`: its conversation — the spec's own
/// rule, a fresh id for a `daemon_session_id` one, a chat the agent mints for a `minted` one — and
/// the argv that starts it. A tab is never the session's own conversation.
pub async fn new_agent_tab(
    coding_agents: &HostCodingAgents,
    kind: &str,
    worktree: &Path,
) -> Result<NewAgentTab, Status> {
    let spec = coding_agents
        .agents()
        .for_session_type(kind)
        .ok_or_else(|| Status::invalid_argument(format!("unknown terminal kind {kind:?}")))?;
    // The binary is the agent spec's, resolved on this host (`#agent-tools 19/19`): its
    // `binary.env`, then the host's search, refusing what is found nowhere.
    let agent = coding_agents
        .resolve(&spec.id, None)
        .map_err(|e| Status::failed_precondition(e.to_string()))?;
    let fresh_id = uuid::Uuid::now_v7().to_string();
    let conversation = start_conversation(ConversationStart {
        spec: &agent.spec,
        binary: &agent.tool.binary,
        worktree,
        env_base: &agent.env_base,
        env: &[],
        session_id: &fresh_id,
        target: None,
    })
    .await?;
    let conversation_id = conversation.usage_id().map_or(fresh_id, str::to_string);
    let argv = agent_tab_argv(&agent.spec, &agent.tool.binary, &conversation_id, false)
        .map_err(|e| Status::failed_precondition(e.to_string()))?;
    Ok(NewAgentTab {
        argv,
        conversation_id,
        resumable: tab_conversation_is_resumable(&agent.spec),
    })
}
