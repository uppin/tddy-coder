//! The conversation a CLI agent launch is bound to: minted, resumed or adopted, and the refusals
//! around it.

use crate::harness_from_spec::HarnessLaunch;
use std::path::Path;
use std::time::Duration;
use tddy_coding_agent_specs::slots::conversation::ConversationSlot;
use tddy_coding_agent_specs::{AgentEnvBase, Capability, CodingAgentSpec};
use tddy_core::adapters::{
    AgentOnTarget, ConversationMint, ConversationMintError, LaunchAdapters, TargetAgentRun,
};
use tddy_core::session_metadata::is_orchestrator_role;
use tddy_core::SessionMetadata;
use tddy_rpc::Status;

/// The conversation a launch binds, from the spec's `coding_agent.conversation`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LaunchConversation {
    /// The daemon's session id (`session_id`).
    SessionId(String),
    /// A minted id (`chat_id`), recorded on the session (`cursor_chat_id`).
    Minted(String),
    /// The worktree's own (`--continue`); nothing to bind.
    Worktree,
    /// The token the spec's resume adapter read off the last run, when one was recorded.
    ResumeToken(Option<String>),
}

impl LaunchConversation {
    /// `launch` with this conversation's template parameter set.
    pub(crate) fn bound<'a>(&'a self, launch: HarnessLaunch<'a>) -> HarnessLaunch<'a> {
        match self {
            Self::SessionId(id) => HarnessLaunch {
                session_id: Some(id),
                ..launch
            },
            Self::Minted(id) => HarnessLaunch {
                chat_id: Some(id),
                ..launch
            },
            Self::ResumeToken(token) => HarnessLaunch {
                resume_token: token.as_deref(),
                ..launch
            },
            Self::Worktree => launch,
        }
    }

    /// The main conversation's id, as the usage context records it: the session id, the minted
    /// id or the recorded resume token. A worktree conversation has none.
    pub(crate) fn usage_id(&self) -> Option<&str> {
        match self {
            Self::SessionId(id) | Self::Minted(id) => Some(id),
            Self::ResumeToken(token) => token.as_deref(),
            Self::Worktree => None,
        }
    }

    /// The id to record on the session's `.session.yaml` (`cursor_chat_id`): the minted one only.
    pub(crate) fn recorded_chat_id(&self) -> Option<&str> {
        match self {
            Self::Minted(id) => Some(id),
            _ => None,
        }
    }
}

/// What a first start's conversation is settled from.
pub(crate) struct ConversationStart<'a> {
    pub spec: &'a CodingAgentSpec,
    /// The agent's resolved binary, which a `minted` adapter runs.
    pub binary: &'a Path,
    pub worktree: &'a Path,
    /// The base set the mint's environment keeps from the daemon's: the agent's host registry's
    /// (`#agent-tools 28/28`).
    pub env_base: &'a AgentEnvBase,
    /// What the mint's launch sets on purpose (the session's git identity). The mint starts from
    /// `env_base`'s agent environment with these on top, never from the daemon's whole environment.
    pub env: &'a [(String, String)],
    pub session_id: &'a str,
    /// The session's target, when it runs on one: a minted conversation is minted through its
    /// shell — in the worktree there, under the target's own environment — instead of on this
    /// host. `None` for a start on this host.
    pub target: Option<ConversationTarget<'a>>,
}

/// The target a conversation is minted on.
#[derive(Clone, Copy)]
pub(crate) struct ConversationTarget<'a> {
    /// The shell that reaches the target.
    pub shell: &'a std::sync::Arc<dyn tddy_remote_shell::RemoteShell>,
    /// The agent's bare command (`ResolvedTool::command`): what the target's login shell finds on
    /// its own `PATH`. [`ConversationStart::binary`] is a path of this host, and means nothing there.
    pub command: &'a str,
}

/// A target's own agent, run through its shell in the session's worktree there.
struct AgentThroughShell<'a> {
    shell: &'a dyn tddy_remote_shell::RemoteShell,
    worktree: &'a Path,
    command: &'a str,
    env: Vec<(String, String)>,
}

#[async_trait::async_trait]
impl AgentOnTarget for AgentThroughShell<'_> {
    async fn run(&self, args: &[&str], deadline: Duration) -> Result<TargetAgentRun, String> {
        let argv: Vec<String> = std::iter::once(self.command)
            .chain(args.iter().copied())
            .map(str::to_string)
            .collect();
        let command =
            crate::remote_agent_launch::target_agent_one_shot(self.worktree, &argv, &self.env);
        let output = self
            .shell
            .exec(tddy_remote_shell::ExecRequest::new(command).within(deadline))
            .await
            .map_err(|error| error.to_string())?;
        Ok(TargetAgentRun {
            status: Some(output.status),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

/// The conversation a first start binds: the session id, a freshly minted id (bounded by the
/// slot's mint deadline, refused through [`conversation_mint_refused`]), or nothing to bind.
pub(crate) async fn start_conversation(
    start: ConversationStart<'_>,
) -> Result<LaunchConversation, Status> {
    let ConversationStart {
        spec,
        binary,
        env_base,
        worktree,
        env,
        session_id,
        target,
    } = start;
    let conversation = spec.conversation();
    match &conversation {
        ConversationSlot::DaemonSessionId { .. } => {
            Ok(LaunchConversation::SessionId(session_id.to_string()))
        }
        ConversationSlot::Worktree => Ok(LaunchConversation::Worktree),
        ConversationSlot::ResumeToken => Ok(LaunchConversation::ResumeToken(None)),
        ConversationSlot::Minted { adapter, .. } => {
            let minter = LaunchAdapters::builtin()
                .conversation_id(adapter)
                .ok_or_else(|| {
                    Status::failed_precondition(format!(
                        "{} names conversation adapter {adapter}, which this build does not have",
                        spec.id
                    ))
                })?;
            let deadline = conversation
                .mint_deadline()
                .expect("a minted conversation has a mint deadline");
            let minted = match target {
                // The agent runs where the session's worktree is: through the target's shell, as
                // the target's own login runs it, never by a path of this host.
                Some(target) => {
                    let mut launch_env = spec.passed_env(std::env::vars_os());
                    launch_env.extend(env.iter().cloned());
                    let agent = AgentThroughShell {
                        shell: target.shell.as_ref(),
                        worktree,
                        command: target.command,
                        env: launch_env,
                    };
                    minter.mint_on_target(&agent, deadline).await
                }
                None => {
                    minter
                        .mint(ConversationMint {
                            binary,
                            workdir: worktree,
                            env: &env_base.agent_env(spec, std::env::vars_os(), env),
                            deadline,
                        })
                        .await
                }
            };
            minted
                .map(LaunchConversation::Minted)
                .map_err(|error| conversation_mint_refused(&spec.id, adapter, &error))
        }
    }
}

/// The id of the conversation `meta`'s agent is in now, as `spec`'s `coding_agent.conversation`
/// identifies it (`#orchestrators` 6/11): for a `daemon_session_id` conversation the newest id in
/// `agent_conversations` — the one the agent rotated to on `/clear` — or the session's own id when
/// the record lists none; the recorded minted id; the recorded resume token; `None` for a worktree
/// conversation. What a resume binds and what the transcript and usage readers open.
#[must_use]
pub fn current_conversation_id<'a>(
    meta: &'a SessionMetadata,
    spec: &CodingAgentSpec,
) -> Option<&'a str> {
    match spec.conversation() {
        ConversationSlot::DaemonSessionId { .. } => Some(
            meta.agent_conversations
                .last()
                .map_or(meta.session_id.as_str(), |c| c.id.as_str()),
        ),
        ConversationSlot::Minted { .. } => meta.cursor_chat_id.as_deref(),
        ConversationSlot::ResumeToken => meta.harness_resume_token.as_deref(),
        ConversationSlot::Worktree => None,
    }
}

/// The conversation a resume of `meta` binds: the session id, the recorded minted id, the worktree,
/// or the recorded resume token. A minted conversation the session never recorded is refused here;
/// [`resumed_or_adopted_conversation`] adopts one instead.
pub(crate) fn resumed_conversation(
    spec: &CodingAgentSpec,
    meta: &SessionMetadata,
) -> Result<LaunchConversation, Status> {
    Ok(match spec.conversation() {
        ConversationSlot::DaemonSessionId { .. } => LaunchConversation::SessionId(
            current_conversation_id(meta, spec)
                .unwrap_or(meta.session_id.as_str())
                .to_string(),
        ),
        ConversationSlot::Minted { adapter, .. } => match meta.cursor_chat_id.as_deref() {
            Some(chat) if is_recorded_chat(chat) => LaunchConversation::Minted(chat.to_string()),
            _ => {
                return Err(Status::failed_precondition(format!(
                    "{} recorded no conversation minted by {adapter}: it cannot be resumed",
                    spec.id
                )))
            }
        },
        ConversationSlot::Worktree => LaunchConversation::Worktree,
        ConversationSlot::ResumeToken => {
            LaunchConversation::ResumeToken(meta.harness_resume_token.clone())
        }
    })
}

/// Whether `chat` is a minted id worth resuming: not blank, and free of the terminal control
/// characters a target's terminal once leaked into it (a session that recorded `"\e[?25h"` from
/// `coder ssh` mints a fresh chat on resume instead of handing Cursor that).
fn is_recorded_chat(chat: &str) -> bool {
    !chat.trim().is_empty() && !chat.chars().any(char::is_control)
}

/// [`resumed_conversation`], but a session recorded without the conversation its spec mints — one
/// started before the conversation was minted at start — adopts a fresh one now, which the resume
/// records: every later resume then continues it instead of starting over again.
pub(crate) async fn resumed_or_adopted_conversation(
    start: ConversationStart<'_>,
    meta: &SessionMetadata,
) -> Result<LaunchConversation, Status> {
    let recorded_nothing = match start.spec.conversation() {
        ConversationSlot::Minted { .. } => meta
            .cursor_chat_id
            .as_deref()
            .is_none_or(|chat| !is_recorded_chat(chat)),
        _ => false,
    };
    if !recorded_nothing {
        return resumed_conversation(start.spec, meta);
    }
    let session_id = start.session_id.to_string();
    let adopted = start_conversation(start).await?;
    log::info!(
        target: "tddy_daemon::cursor_cli_spawn",
        "resume of {session_id}: no conversation on record; adopted a minted one for this and every later resume"
    );
    Ok(adopted)
}

/// The refusal of a start whose conversation `adapter` did not mint an id for `agent_id`:
/// `DEADLINE_EXCEEDED` for a mint past its deadline, `FAILED_PRECONDITION` otherwise, naming the
/// agent and the adapter.
pub(crate) fn conversation_mint_refused(
    agent_id: &str,
    adapter: &str,
    error: &ConversationMintError,
) -> Status {
    let message = format!("{agent_id} could not mint its conversation with {adapter}: {error}");
    match error {
        ConversationMintError::TimedOut { .. } => Status::deadline_exceeded(message),
        ConversationMintError::Failed(_) => Status::failed_precondition(message),
    }
}

/// A starting session's standing role, refused for an agent that cannot hold it: a project-home
/// main or orchestrator (`is_orchestrator_role`) of a spec that does not declare
/// `project_home_main` is refused `FAILED_PRECONDITION` naming the agent. An ordinary session (an empty role) is admitted.
pub(crate) fn refuse_a_role_the_agent_cannot_hold(
    spec: &CodingAgentSpec,
    session_role: &str,
) -> Result<(), Status> {
    if is_orchestrator_role(session_role) && !spec.has(Capability::ProjectHomeMain) {
        return Err(Status::failed_precondition(format!(
            "{} cannot run a project's main: its spec declares no project_home_main",
            spec.id
        )));
    }
    Ok(())
}
