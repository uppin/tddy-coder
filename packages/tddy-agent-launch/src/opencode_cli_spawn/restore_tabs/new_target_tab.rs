//! A new agent tab on its session's **target** (`#session-liveness` 8/8): the target variant of the
//! agent-tab rule ([`super::new_agent_tab`]). The agent is the target's own, named by its spec's
//! bare command — never resolved on this host (`HostCodingAgents::resolve`), so a daemon host
//! without the agent opens one — and a conversation its spec mints is minted on the target, in the
//! session's worktree there.

use std::path::Path;

use tddy_rpc::Status;

use super::new_tab::tab_conversation_is_resumable;
use crate::cli_agent_steps::conversation::{
    start_conversation, ConversationStart, ConversationTarget,
};

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
    let spec = coding_agents
        .agents()
        .for_session_type(harness)
        .ok_or_else(|| Status::invalid_argument(format!("unknown terminal kind {harness:?}")))?;
    let command = bare_command(coding_agents, &spec.id)?;
    // `on_target` lets a spec render what only a run on a target needs (Cursor's `--trust`), as a
    // start on a target renders it.
    let flags: &[&str] = if resume {
        &["on_target", "resume"]
    } else {
        &["on_target"]
    };
    let argv = spec
        .render_run_argv(
            Path::new(&command),
            &[
                ("session_id", conversation_id),
                ("chat_id", conversation_id),
            ],
            flags,
        )
        .map_err(|e| Status::failed_precondition(e.to_string()))?;
    // A remote shell forwards no environment: what the agent needs travels in its command, as a
    // start on a target carries it — the `pass_env` values this daemon holds, then `run.env`.
    let mut env = spec.passed_env(std::env::vars_os());
    env.extend(
        spec.run
            .env
            .iter()
            .map(|(name, value)| (name.clone(), value.clone())),
    );
    Ok(TargetAgentTab {
        kind: harness.to_string(),
        argv,
        env,
    })
}

/// The bare command of agent `id`'s binary, read from its spec in the registry: what the target's
/// login `PATH` resolves.
fn bare_command(coding_agents: &HostCodingAgents, id: &str) -> Result<String, Status> {
    coding_agents
        .registry()
        .get(id)
        .map(|def| def.binary.command.clone())
        .ok_or_else(|| Status::failed_precondition(format!("no tool {id:?} is registered")))
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
    let spec = coding_agents
        .agents()
        .for_session_type(harness)
        .ok_or_else(|| Status::invalid_argument(format!("unknown terminal kind {harness:?}")))?;
    let command = bare_command(coding_agents, &spec.id)?;
    let fresh_id = uuid::Uuid::now_v7().to_string();
    let conversation = start_conversation(ConversationStart {
        spec,
        binary: Path::new(&command),
        worktree: &target.worktree,
        env_base: coding_agents.agents().agent_env_base(),
        env: &[],
        session_id: &fresh_id,
        target: Some(ConversationTarget {
            shell: &target.shell,
            command: &command,
        }),
    })
    .await?;
    let conversation_id = conversation.usage_id().map_or(fresh_id, str::to_string);
    Ok(NewTargetAgentTab {
        agent: target_agent_tab(coding_agents, harness, &conversation_id, false)?,
        conversation_id,
        resumable: tab_conversation_is_resumable(spec),
    })
}
