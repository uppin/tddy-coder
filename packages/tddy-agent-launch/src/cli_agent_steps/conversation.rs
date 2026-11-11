//! The conversation a CLI agent launch is bound to: minted, resumed or adopted, and the refusals
//! around it.

use crate::harness_from_spec::HarnessLaunch;
use std::path::Path;
use tddy_coding_agent_specs::slots::conversation::ConversationSlot;
use tddy_coding_agent_specs::{AgentEnvBase, Capability, CodingAgentSpec};
use tddy_core::adapters::{ConversationMint, ConversationMintError, LaunchAdapters};
use tddy_core::session_metadata::PROJECT_HOME_ROLE;
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
    } = start;
    let conversation = spec.conversation();
    match &conversation {
        ConversationSlot::DaemonSessionId => {
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
            minter
                .mint(ConversationMint {
                    binary,
                    workdir: worktree,
                    env: &env_base.agent_env(spec, std::env::vars_os(), env),
                    deadline,
                })
                .await
                .map(LaunchConversation::Minted)
                .map_err(|error| conversation_mint_refused(&spec.id, adapter, &error))
        }
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
        ConversationSlot::DaemonSessionId => LaunchConversation::SessionId(meta.session_id.clone()),
        ConversationSlot::Minted { adapter, .. } => match meta.cursor_chat_id.as_deref() {
            Some(chat) if !chat.trim().is_empty() => LaunchConversation::Minted(chat.to_string()),
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
            .is_none_or(|chat| chat.trim().is_empty()),
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
/// main (`PROJECT_HOME_ROLE`) of a spec that does not declare `project_home_main` is refused
/// `FAILED_PRECONDITION` naming the agent. An ordinary session (an empty role) is admitted.
pub(crate) fn refuse_a_role_the_agent_cannot_hold(
    spec: &CodingAgentSpec,
    session_role: &str,
) -> Result<(), Status> {
    if session_role == PROJECT_HOME_ROLE && !spec.has(Capability::ProjectHomeMain) {
        return Err(Status::failed_precondition(format!(
            "{} cannot run a project's main: its spec declares no project_home_main",
            spec.id
        )));
    }
    Ok(())
}
