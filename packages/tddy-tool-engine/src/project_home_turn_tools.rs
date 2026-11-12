//! `SendSessionMessage` and `ResumeSession`: an orchestrator drives another session's turns the way
//! `/yolo-pr-stack` drives a subagent — it hands a running session a new turn, and resumes a stopped
//! one, optionally with a first message (`#orchestrators` 4/11).
//!
//! - **`SendSessionMessage`** gives a running session's agent one new turn, delivered the way the
//!   agent's YAML spec declares (`coding_agent.turn_input`): a terminal paste followed by the submit
//!   key for a terminal agent, the ACP prompt for an ACP agent. It takes **no terminal control
//!   lease**: a person holding control in the browser keeps it. A session whose agent declares no
//!   `turn_input` is refused naming the agent; a stopped one is refused naming `ResumeSession`; one
//!   waiting on a permission prompt or a question is refused naming the answer tools.
//! - **`ResumeSession`** does what the web's Resume does, then delivers `message`, if any, as the
//!   resumed agent's first turn once its terminal is ready. A session that is already running
//!   answers `running` and is left alone.
//!
//! Both are target tools: they take
//! [`TARGET_DAEMON_INSTANCE_ID`](crate::project_home_tools::TARGET_DAEMON_INSTANCE_ID) and the
//! daemon forwards a call naming another host to that host whole, so the port below is only ever
//! asked about this host's sessions.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.
//! PRD: docs/ft/daemon/1-WIP/PRD-2026-11-10-session-turn-control.md.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tddy_worktree_service::session_target::{HomeCaller, SessionTarget};

use crate::catalog::ToolDef;
use crate::project_home_tools::{
    resolve_target, target_host_property, tool_def, ProjectHomePort, TARGET_DAEMON_INSTANCE_ID,
};
use crate::ToolOutcome;

pub const SEND_SESSION_MESSAGE: &str = "SendSessionMessage";
pub const RESUME_SESSION: &str = "ResumeSession";

/// Whether `name` is [`SEND_SESSION_MESSAGE`] or [`RESUME_SESSION`].
pub fn is_project_home_turn_tool(name: &str) -> bool {
    matches!(name, SEND_SESSION_MESSAGE | RESUME_SESSION)
}

/// How a message reached the session's agent: the `delivered_via` of a `SendSessionMessage` answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnDelivery {
    /// Pasted into the session's main terminal, then the submit key its spec names.
    TerminalPaste,
    /// Sent as a prompt over the session's ACP connection.
    AcpPrompt,
}

impl TurnDelivery {
    /// The word the answer spells it with: `terminal_paste` or `acp_prompt`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TerminalPaste => "terminal_paste",
            Self::AcpPrompt => "acp_prompt",
        }
    }
}

/// What a `ResumeSession` did: the `outcome` of its answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeOutcome {
    /// The session was stopped and is now resumed (and its message, if any, delivered).
    Resumed,
    /// The session was already running; nothing was done.
    Running,
}

impl ResumeOutcome {
    /// The word the answer spells it with: `resumed` or `running`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Resumed => "resumed",
            Self::Running => "running",
        }
    }
}

/// The tools' definitions.
pub fn project_home_turn_tool_defs() -> Vec<ToolDef> {
    vec![
        tool_def(
            SEND_SESSION_MESSAGE,
            "Give a running session of this project a new turn: text is delivered to its agent the way the agent's spec declares (pasted into its terminal and submitted, or sent as an ACP prompt), as if its operator had typed it. Takes no terminal control: a person watching the session keeps control, and you need not ClaimSessionTerminal. Answers session_id and delivered_via (terminal_paste or acp_prompt). A stopped session is refused: resume it with ResumeSession. A session waiting on a permission prompt or a question is refused: answer it with AnswerSessionPermission or AnswerSessionQuestion. A session whose agent declares no way to take a turn is refused naming the agent. Read the result with ReadSessionActivity, ReadSessionTranscript or ReadSessionTerminal, or wait for its turn to end with WaitProjectNotifications.",
            json!({
                "type": "object",
                "properties": {
                    "session_id": {
                        "type": "string",
                        "description": "The session of this project to message, as ListProjectSessions reports it."
                    },
                    "text": {
                        "type": "string",
                        "minLength": 1,
                        "description": "The message, as the agent's operator would type it. It is submitted for you."
                    },
                    TARGET_DAEMON_INSTANCE_ID: target_host_property()
                },
                "required": ["session_id", "text"]
            }),
        ),
        tool_def(
            RESUME_SESSION,
            "Resume a stopped session of this project, as the web UI's Resume does, and optionally give it message as its first turn once its agent is ready. Answers session_id and outcome: resumed, or running when the session was already running, in which case nothing is done (use SendSessionMessage to give a running session a turn).",
            json!({
                "type": "object",
                "properties": {
                    "session_id": {
                        "type": "string",
                        "description": "The session of this project to resume, as ListProjectSessions reports it."
                    },
                    "message": {
                        "type": "string",
                        "description": "A first turn for the resumed agent. Omit to resume without one."
                    },
                    TARGET_DAEMON_INSTANCE_ID: target_host_property()
                },
                "required": ["session_id"]
            }),
        ),
    ]
}

/// What the tools need from the daemon beyond the target resolver: delivering a turn to a session on
/// this host, and resuming one.
#[async_trait]
pub trait ProjectHomeTurnPort: ProjectHomePort {
    /// Deliver `text` to `target`'s agent as one new turn, by the `turn_input` its agent's spec
    /// declares, without changing who holds its terminal's control lease. An error names why
    /// nothing was delivered: the agent declares no `turn_input` (naming the agent), the session is
    /// stopped (naming `ResumeSession`), or it waits on a permission prompt or a question (naming
    /// the answer tools).
    ///
    /// The default refuses, so a port that delivers nothing need not name it.
    async fn send_session_message(
        &self,
        target: &SessionTarget,
        text: &str,
    ) -> Result<TurnDelivery, String> {
        let _ = (target, text);
        Err(NO_TURN_DELIVERY.to_string())
    }

    /// Resume `target` as `ResumeSession` does, then deliver `message`, if any, as its first turn
    /// once its agent's terminal is ready. [`ResumeOutcome::Running`] for a session that is already
    /// running, which is left alone. An error names why it was not resumed.
    ///
    /// The default refuses, so a port that resumes nothing need not name it.
    async fn resume_session(
        &self,
        target: &SessionTarget,
        message: Option<&str>,
    ) -> Result<ResumeOutcome, String> {
        let _ = (target, message);
        Err(NO_RESUME.to_string())
    }
}

/// The refusal of a port that delivers no turns.
pub const NO_TURN_DELIVERY: &str = "this host delivers no session messages";
/// The refusal of a port that resumes no sessions.
pub const NO_RESUME: &str = "this host resumes no sessions";

/// Run [`SEND_SESSION_MESSAGE`] or [`RESUME_SESSION`] for `home`.
///
/// Bad arguments, a target refusal and a refusal of the port come back as an `is_error` outcome
/// naming what was refused; the role refusal is the daemon's and happens before this is called.
pub async fn execute_project_home_turn_tool(
    port: &dyn ProjectHomeTurnPort,
    home: &HomeCaller,
    tool_name: &str,
    args_json: &str,
) -> ToolOutcome {
    if !is_project_home_turn_tool(tool_name) {
        return ToolOutcome::err(format!("unknown project-home turn tool: {tool_name}"));
    }
    let args: Value = match serde_json::from_str(args_json) {
        Ok(v) => v,
        Err(e) => return ToolOutcome::err(format!("invalid args_json: {e}")),
    };
    if tool_name == SEND_SESSION_MESSAGE {
        send_session_message(port, home, args).await
    } else {
        resume_session(port, home, args).await
    }
}

#[derive(Deserialize)]
struct SendArgs {
    session_id: String,
    text: String,
}

#[derive(Deserialize)]
struct ResumeArgs {
    session_id: String,
    #[serde(default)]
    message: Option<String>,
}

/// `SendSessionMessage`: refuse an empty text, resolve the target, deliver. Answers
/// `{session_id, delivered_via}`.
async fn send_session_message(
    port: &dyn ProjectHomeTurnPort,
    home: &HomeCaller,
    args: Value,
) -> ToolOutcome {
    let args: SendArgs = match serde_json::from_value(args) {
        Ok(a) => a,
        Err(e) => {
            return ToolOutcome::err(format!("{SEND_SESSION_MESSAGE}: invalid arguments: {e}"))
        }
    };
    if args.text.trim().is_empty() {
        return ToolOutcome::err(format!(
            "{SEND_SESSION_MESSAGE}: text is empty: nothing to send"
        ));
    }
    let target = match resolve_target(port, home, &args.session_id).await {
        Ok(t) => t,
        Err(outcome) => return outcome,
    };
    match port.send_session_message(&target, &args.text).await {
        Ok(delivery) => ToolOutcome::ok(
            json!({ "session_id": target.session_id, "delivered_via": delivery.as_str() })
                .to_string(),
        ),
        Err(e) => ToolOutcome::err(format!("{SEND_SESSION_MESSAGE}: {e}")),
    }
}

/// `ResumeSession`: resolve the target, resume it, with the first message if one was given.
/// Answers `{session_id, outcome}`. An empty message is no message.
async fn resume_session(
    port: &dyn ProjectHomeTurnPort,
    home: &HomeCaller,
    args: Value,
) -> ToolOutcome {
    let args: ResumeArgs = match serde_json::from_value(args) {
        Ok(a) => a,
        Err(e) => return ToolOutcome::err(format!("{RESUME_SESSION}: invalid arguments: {e}")),
    };
    let target = match resolve_target(port, home, &args.session_id).await {
        Ok(t) => t,
        Err(outcome) => return outcome,
    };
    let message = args.message.as_deref().filter(|m| !m.trim().is_empty());
    match port.resume_session(&target, message).await {
        Ok(outcome) => ToolOutcome::ok(
            json!({ "session_id": target.session_id, "outcome": outcome.as_str() }).to_string(),
        ),
        Err(e) => ToolOutcome::err(format!("{RESUME_SESSION}: {e}")),
    }
}
