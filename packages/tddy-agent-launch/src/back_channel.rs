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

use std::path::Path;

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
        hook_log_path: String,
    },
}

impl<'a> ChildPlacement<'a> {
    /// Where a child runs: on a target when `target_worktree` names its worktree there, reporting
    /// over that worktree's hook log, which its `hook_token` authenticates; `None` for a target
    /// child with no hook token, whose log nobody follows and which therefore has no channel.
    pub fn of(target_worktree: Option<&Path>, hook_token: Option<&'a str>) -> Option<Self> {
        let Some(worktree) = target_worktree else {
            return Some(Self::Local);
        };
        // FIXME(red): still the relative path the guidance has always named.
        let _ = worktree;
        hook_token
            .filter(|token| !token.is_empty())
            .map(|hook_token| Self::Target {
                hook_token,
                hook_log_path: tddy_core::REMOTE_HOOK_LOG.to_string(),
            })
    }
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
    format!("tddy-{session_id}")
}

/// The channel a child described by `inputs` reports back over.
pub fn back_channel_for(inputs: &BackChannelInputs<'_>) -> BackChannel {
    match &inputs.placement {
        ChildPlacement::Target {
            hook_token,
            hook_log_path,
        } => hook_log_channel(hook_token, hook_log_path),
        ChildPlacement::Local
            if inputs.child_agent.peer_name().is_some()
                && inputs.orchestrator_agent.peer_name().is_some() =>
        {
            BackChannel::NativePeer {
                orchestrator_peer_name: native_peer_name_of(inputs.orchestrator_session_id),
            }
        }
        ChildPlacement::Local => BackChannel::ToolsSend,
    }
}

/// The hook-log channel: [`tddy_core::remote_hook_command`] for the message event, run by an explicit
/// POSIX `sh`.
///
/// The agent runs the guidance's line in its own login shell, and the line pipes the report into
/// the command. A hook's command reads its payload as its own stdin, but here the command is the
/// last element of a pipeline, and zsh — a Coder workspace's and macOS's login shell — runs that
/// element in the current shell, expanding the command's `$(cat …)` before stdin is the pipe: the
/// report reached the log empty and was dropped. Under `sh -c` the command's stdin is the pipe in
/// every shell.
fn hook_log_channel(hook_token: &str, hook_log_path: &str) -> BackChannel {
    let command =
        tddy_core::remote_hook_command(tddy_core::REMOTE_MESSAGE_EVENT, hook_token, hook_log_path);
    BackChannel::HookLog {
        command: format!(
            "sh -c {}",
            crate::remote_hook_follower::shell_quote(&command)
        ),
    }
}

/// The guidance a child is given — in its system prompt, or ahead of its first prompt when its agent
/// has no system-prompt slot ([`first_prompt_with`]): who its orchestrator is, to report progress
/// and status to it periodically, and the exact way to do so over `channel`.
pub fn back_channel_guidance(orchestrator_session_id: &str, channel: &BackChannel) -> String {
    let how = match channel {
        BackChannel::NativePeer {
            orchestrator_peer_name,
        } => format!(
            "Your orchestrator is a Claude Code session on this host, named \
             `{orchestrator_peer_name}`. Message it by that name with Claude Code's peer \
             messaging."
        ),
        BackChannel::ToolsSend => format!(
            "Send it a message with:\n\n```bash\ntddy-tools send --to {orchestrator_session_id} \
             \"<your progress and status>\"\n```\n\nThe command exits non-zero and says why when \
             the message was not sent."
        ),
        BackChannel::HookLog { command } => format!(
            "This machine has no `tddy-tools`. Send it a message by piping a JSON object whose \
             `text` is your message into this command, from the directory you were started in:\n\n\
             ```bash\necho '{{\"text\": \"<your progress and status>\"}}' | {command}\n```\n\n\
             The text must be valid JSON (escape quotes and newlines). The line is appended to a \
             log the daemon follows; the command prints nothing when it is written."
        ),
    };
    format!(
        "## Reporting to your orchestrator\n\nThis session was started by an orchestrator, \
         session `{orchestrator_session_id}`. Report your progress and status to it periodically: \
         when you start, after each milestone, when you are blocked, and when you finish. Keep each \
         report to a few lines.\n\n{how}"
    )
}

/// The first prompt a child is started with: `guidance` — the back channel its system prompt could
/// not carry — ahead of the `task` it was given; the task alone when there is no guidance.
pub fn first_prompt_with(guidance: Option<&str>, task: &str) -> String {
    match (guidance.map(str::trim_end), task.trim()) {
        (None, _) => task.to_string(),
        (Some(guidance), "") => guidance.to_string(),
        (Some(guidance), _) => format!("{guidance}\n\n## Your task\n\n{task}"),
    }
}

/// The orchestrator a starting session reports to.
#[derive(Debug, Clone)]
pub struct ReportsTo<'a> {
    pub orchestrator_session_id: String,
    /// The orchestrator's agent, when it is a coding agent this host knows. An orchestrator whose
    /// agent is unknown offers no native peer messaging, so a local child reports with
    /// `tddy-tools send`.
    pub orchestrator_agent: Option<&'a CodingAgentSpec>,
}

impl ReportsTo<'_> {
    /// The guidance for a child running `child_agent` at `placement`.
    pub fn guidance_for(
        &self,
        child_agent: &CodingAgentSpec,
        placement: ChildPlacement<'_>,
    ) -> String {
        let channel = match (self.orchestrator_agent, placement) {
            (Some(orchestrator_agent), placement) => back_channel_for(&BackChannelInputs {
                orchestrator_session_id: &self.orchestrator_session_id,
                orchestrator_agent,
                child_agent,
                placement,
            }),
            (None, ChildPlacement::Local) => BackChannel::ToolsSend,
            (
                None,
                ChildPlacement::Target {
                    hook_token,
                    hook_log_path,
                },
            ) => hook_log_channel(hook_token, &hook_log_path),
        };
        back_channel_guidance(&self.orchestrator_session_id, &channel)
    }

    /// The guidance for a child running `child_agent`, on a target when `target_worktree` names its
    /// worktree there, with its `hook_token`; `None` for a target child with no hook token
    /// ([`ChildPlacement::of`]).
    pub fn guidance_at(
        &self,
        child_agent: &CodingAgentSpec,
        target_worktree: Option<&Path>,
        hook_token: Option<&str>,
    ) -> Option<String> {
        ChildPlacement::of(target_worktree, hook_token)
            .map(|placement| self.guidance_for(child_agent, placement))
    }
}

/// The arguments that launch `session_id` of `spec` under its native peer name: the spec's
/// `peer_name` flag and [`native_peer_name_of`] `session_id`, for an orchestrator (`role`) or a child
/// that reports to one, running on the orchestrator's host. None for any other session, and for an
/// agent without the slot.
pub fn peer_name_args(
    spec: &CodingAgentSpec,
    session_id: &str,
    on_target: bool,
    reports_to_an_orchestrator: bool,
    role: &str,
) -> Vec<String> {
    let named = !on_target
        && (reports_to_an_orchestrator || tddy_core::session_metadata::is_orchestrator_role(role));
    spec.peer_name()
        .filter(|_| named)
        .map(|slot| vec![slot.flag.clone(), native_peer_name_of(session_id)])
        .unwrap_or_default()
}
