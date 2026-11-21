//! How a session started by an orchestrator reports its progress and status back (`#orchestra`
//! 2/9).
//!
//! Each child gets guidance naming its orchestrator and exactly one back channel, chosen by where
//! it runs and what both agents can do:
//!
//! - **native** — a child whose agent has native peer messaging (`coding_agent.peer_name`), on the
//!   same host as an orchestrator that has it too, messages the orchestrator by its peer name;
//! - **tddy-tools** — any other local child runs `tddy-tools send` to the orchestrator's session;
//! - **hook log** — a child on a target (Coder, SSH) has no `tddy-tools`: it appends a
//!   [`REMOTE_MESSAGE_EVENT`](tddy_core::REMOTE_MESSAGE_EVENT) line to its worktree's hook log, which
//!   the daemon already follows, and the follower posts it to the orchestrator.
//!
//! A session's peer name is a function of its id ([`native_peer_name_of`]), so nothing extra is
//! recorded: whoever knows a session id and its agent knows how to address it natively.

use tddy_coding_agent_specs::CodingAgentSpec;

/// The report a target child's hook-log line carried, addressed by the session that wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteMessage {
    /// The child session whose hook log held the line.
    pub session_id: String,
    /// The child's OS user, which the message is posted as.
    pub os_user: String,
    /// The report's text, as the child wrote it.
    pub text: String,
}

/// The one channel a child is told to report back over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackChannel {
    /// Message the orchestrator natively by its peer name.
    NativePeer { orchestrator_peer_name: String },
    /// `tddy-tools send` to the orchestrator's session id.
    ToolsSend,
    /// Append a message line to the hook log: `command` is the shell command the child pipes
    /// `{"text": "..."}` into.
    HookLog { command: String },
}

/// Where a child runs, for choosing its channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChildPlacement<'a> {
    /// On the orchestrator's host.
    Local,
    /// On a target: its hook token and its hook log's path there.
    Target {
        hook_token: &'a str,
        hook_log_path: &'a str,
    },
}

/// What [`back_channel_for`] decides from.
#[derive(Debug, Clone)]
pub struct BackChannelInputs<'a> {
    pub orchestrator_session_id: &'a str,
    /// The orchestrator's agent.
    pub orchestrator_agent: &'a CodingAgentSpec,
    /// The child's agent.
    pub child_agent: &'a CodingAgentSpec,
    pub placement: ChildPlacement<'a>,
}

/// The name a session of an agent with native peer messaging is launched under, and addressed by.
pub fn native_peer_name_of(session_id: &str) -> String {
    // TODO(child-back-channel): implement
    let _ = session_id;
    unimplemented!("native_peer_name_of")
}

/// The channel a child described by `inputs` reports back over.
pub fn back_channel_for(inputs: &BackChannelInputs<'_>) -> BackChannel {
    // TODO(child-back-channel): implement
    let _ = inputs;
    unimplemented!("back_channel_for")
}

/// The guidance appended to a child's system prompt: who its orchestrator is, to report progress
/// and status to it periodically, and the exact way to do so over `channel`.
pub fn back_channel_guidance(orchestrator_session_id: &str, channel: &BackChannel) -> String {
    // TODO(child-back-channel): implement
    let _ = (orchestrator_session_id, channel);
    unimplemented!("back_channel_guidance")
}
