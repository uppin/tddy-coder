//! A new agent tab on its session's **target** (`#session-liveness` 8/8): the target variant of the
//! agent-tab rule ([`super::new_agent_tab`]). The agent is the target's own, named by its spec's
//! bare command — never resolved on this host (`HostCodingAgents::resolve`), so a daemon host
//! without the agent opens one — and a conversation its spec mints is minted on the target, in the
//! session's worktree there.

use tddy_rpc::Status;

use crate::remote_agent_launch::TargetAgentTab;
use crate::spec_launch::HostCodingAgents;
use crate::tab_target_shell::TabTarget;

/// A new agent tab on a target, ready to launch
/// ([`crate::remote_agent_launch::launch_agent_tab_on_target`]): what it runs there, the
/// conversation it holds and whether a resume can name that conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTargetAgentTab {
    pub agent: TargetAgentTab,
    pub conversation_id: String,
    pub resumable: bool,
}

/// What an agent tab of `harness` holding `conversation_id` runs on a target: its spec's argv with
/// the spec's bare command as `{binary}` — `resume` set, continuing that conversation (claude
/// `--resume <id>`, cursor `--resume <chat id>`); unset, starting it (claude `--session-id <id>`) —
/// and the environment the remote command carries. Nothing is resolved on this host.
///
/// `INVALID_ARGUMENT` for a harness no coding agent runs; `FAILED_PRECONDITION` for a spec whose
/// argv cannot be rendered.
pub fn target_agent_tab(
    coding_agents: &HostCodingAgents,
    harness: &str,
    conversation_id: &str,
    resume: bool,
) -> Result<TargetAgentTab, Status> {
    // TODO(target-agent-tab): implement
    let _ = (coding_agents, harness, conversation_id, resume);
    todo!("what an agent tab runs on a target")
}

/// Settle a new agent tab of `harness` on `target`: its conversation — the spec's own rule, a fresh
/// id for a `daemon_session_id` one, a chat the target's agent mints in `target.worktree` for a
/// `minted` one (`ConversationStart { target: Some(..) }`) — and what starts it there
/// ([`target_agent_tab`]). A tab is never the session's own conversation.
pub async fn new_target_agent_tab(
    coding_agents: &HostCodingAgents,
    harness: &str,
    target: &TabTarget,
) -> Result<NewTargetAgentTab, Status> {
    // TODO(target-agent-tab): implement
    let _ = (coding_agents, harness, target);
    todo!("settle a new agent tab on the session's target")
}
