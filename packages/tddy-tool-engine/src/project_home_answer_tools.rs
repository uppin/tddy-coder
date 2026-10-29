//! `AnswerSessionPermission` and `AnswerSessionQuestion`: the project-home agent — or a spawn-capable session — answers the
//! permission prompt a session it started is waiting on (`#home-inbox` 6/8).
//!
//! The child's `PermissionRequest` hook is held open on the child's host (`AwaitQuestionAnswer`);
//! this tool wakes it with an option of the request — `allow_once`, `reject_once`, or `allow_always`
//! when Claude suggested a rule — or cancels it, and Claude proceeds as if a person had answered.
//!
//! `AnswerSessionQuestion` (`#home-inbox` 8/9) does the same for an `AskUserQuestion`: `session_id`,
//! `daemon_instance_id`, `request_id` and `answers`, one per question it answers — `question_index`,
//! the `selected` labels, and `other` free text — validated against the options the child recorded.
//! A permission is refused there, and a question here, each naming the other tool.
//!
//! The arguments mirror `AnswerAgentPermissionRequest` (`#coagents`): `session_id`,
//! `daemon_instance_id`, `request_id`, `option_id` or `cancelled`, plus a `message` for a rejection.
//!
//! A target tool: it takes [`TARGET_DAEMON_INSTANCE_ID`] and the daemon forwards it to the child's
//! host (`#start-parity` 1/9), so the port is only ever asked about this host's sessions. Who may
//! answer is checked there, against the child's changeset `orchestrator_session_id`.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tddy_discovery::subagent::PermissionAnswer;
use tddy_service::proto::activity::QuestionAnswer;
use tddy_worktree_service::session_target::{HomeCaller, SessionTarget};

use crate::catalog::ToolDef;
use crate::project_home_tools::{
    resolve_target, target_host_property, tool_def, ProjectHomePort, TARGET_DAEMON_INSTANCE_ID,
};
use crate::ToolOutcome;

pub const ANSWER_SESSION_PERMISSION: &str = "AnswerSessionPermission";
pub const ANSWER_SESSION_QUESTION: &str = "AnswerSessionQuestion";

/// Whether `name` is [`ANSWER_SESSION_PERMISSION`] or [`ANSWER_SESSION_QUESTION`].
pub fn is_project_home_answer_tool(name: &str) -> bool {
    name == ANSWER_SESSION_PERMISSION || name == ANSWER_SESSION_QUESTION
}

/// The tools' definitions: [`ANSWER_SESSION_PERMISSION`], then [`ANSWER_SESSION_QUESTION`].
pub fn project_home_answer_tool_defs() -> Vec<ToolDef> {
    vec![
        answer_session_permission_def(),
        answer_session_question_def(),
    ]
}

fn answer_session_permission_def() -> ToolDef {
    tool_def(
        ANSWER_SESSION_PERMISSION,
        "Answer the permission prompt a session you started is waiting on, as ReadSessionActivity shows it (its pending question, kind permission, with its request_id). Choose option_id allow_once, reject_once, or allow_always when the request offers it (Claude suggested a rule), or set cancelled to leave it to the person at the terminal. A rejection's message tells the agent why. Only the session that started the target may answer; the first answer wins and a second is refused naming who answered. A question (not a permission) is refused: answer it with AnswerSessionQuestion. Answers who answered, the option and when.",
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "The session whose permission prompt to answer, as ListProjectSessions reports it."
                },
                "request_id": {
                    "type": "string",
                    "description": "The pending question's request_id, as ReadSessionActivity reports it."
                },
                "option_id": {
                    "type": "string",
                    "description": "allow_once, reject_once, or allow_always when offered. Omit with cancelled."
                },
                "cancelled": {
                    "type": "boolean",
                    "description": "Answer the request as cancelled: no decision is made, and Claude shows its own prompt."
                },
                "message": {
                    "type": "string",
                    "description": "With a reject option: why, for the agent."
                },
                TARGET_DAEMON_INSTANCE_ID: target_host_property()
            },
            "required": ["session_id", "request_id"]
        }),
    )
}

fn answer_session_question_def() -> ToolDef {
    tool_def(
        ANSWER_SESSION_QUESTION,
        "Answer the question a session you started is waiting on, as ReadSessionActivity shows it (its pending question, kind select or multi_select, with its request_id and the questions it asks). Give one answer per question you answer: question_index (from 0), the selected option labels exactly as offered (one for a single-choice question, any number for a multi-select one), and other for free text beside or instead of the options. An index the question does not have, a label it does not offer, or several selections on a single-choice question is refused and the session keeps waiting. Only the session that started the target may answer; the first answer wins and a second is refused naming who answered. A permission (not a question) is refused: answer it with AnswerSessionPermission. Answers who answered, the answers and when.",
        json!({
            "type": "object",
            "properties": {
                "session_id": {
                    "type": "string",
                    "description": "The session whose question to answer, as ListProjectSessions reports it."
                },
                "request_id": {
                    "type": "string",
                    "description": "The pending question's request_id, as ReadSessionActivity reports it."
                },
                "answers": {
                    "type": "array",
                    "description": "One entry per question you answer.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "question_index": {
                                "type": "integer",
                                "minimum": 0,
                                "description": "Which question, by its position in the pending question's questions, from 0."
                            },
                            "selected": {
                                "type": "array",
                                "items": { "type": "string" },
                                "description": "The option labels chosen, exactly as offered."
                            },
                            "other": {
                                "type": "string",
                                "description": "Free text beside, or instead of, the options."
                            }
                        },
                        "required": ["question_index"]
                    }
                },
                TARGET_DAEMON_INSTANCE_ID: target_host_property()
            },
            "required": ["session_id", "request_id", "answers"]
        }),
    )
}

/// What the tool needs from the daemon beyond the target resolver: delivering an answer to a hook
/// waiting on this host.
#[async_trait]
pub trait ProjectHomeAnswerPort: ProjectHomePort {
    /// Deliver `home`'s `answer` to the permission prompt `target`'s hook waits on, with `message`
    /// for a rejection. An error names why it was refused: the caller is not the target's
    /// orchestrator, the request was already answered (by whom), no such request is pending, or it
    /// is a question rather than a permission.
    ///
    /// The default refuses, so a port that serves no answers need not name it.
    async fn answer_permission(
        &self,
        home: &HomeCaller,
        target: &SessionTarget,
        answer: PermissionAnswer,
        message: Option<String>,
    ) -> Result<Value, String> {
        // The daemon, which owns this host's `PendingAnswers`, implements this.
        let _ = (home, target, answer, message);
        Err("this host serves no permission answers".to_string())
    }

    /// Deliver `home`'s `answers` to the question `target`'s hook waits on under `request_id`
    /// (`#home-inbox` 8/9). An error names why it was refused: the caller is not the target's
    /// orchestrator, the request was already answered (by whom), no such request is pending, it is
    /// a permission rather than a question, it carries no `request_id`, or the answers do not fit
    /// the question (which index, which label).
    ///
    /// The default refuses, so a port that serves no answers need not name it.
    async fn answer_question(
        &self,
        home: &HomeCaller,
        target: &SessionTarget,
        request_id: &str,
        answers: Vec<QuestionAnswer>,
    ) -> Result<Value, String> {
        // The daemon, which owns this host's `PendingAnswers`, implements this.
        let _ = (home, target, request_id, answers);
        Err("this host serves no question answers".to_string())
    }
}

/// The tool's arguments.
#[derive(Debug, Deserialize)]
pub struct AnswerPermissionArgs {
    pub session_id: String,
    pub request_id: String,
    #[serde(default)]
    pub option_id: Option<String>,
    #[serde(default)]
    pub cancelled: bool,
    #[serde(default)]
    pub message: Option<String>,
}

/// [`ANSWER_SESSION_QUESTION`]'s arguments.
#[derive(Debug, Deserialize)]
pub struct AnswerQuestionArgs {
    pub session_id: String,
    pub request_id: String,
    pub answers: Vec<QuestionAnswerArg>,
}

/// One entry of [`AnswerQuestionArgs::answers`], as the agent writes it; [`QuestionAnswer`] on the
/// wire.
#[derive(Debug, Deserialize)]
pub struct QuestionAnswerArg {
    pub question_index: u32,
    #[serde(default)]
    pub selected: Vec<String>,
    #[serde(default)]
    pub other: Option<String>,
}

impl From<QuestionAnswerArg> for QuestionAnswer {
    fn from(arg: QuestionAnswerArg) -> Self {
        Self {
            question_index: arg.question_index,
            selected: arg.selected,
            other: arg.other,
        }
    }
}

/// [`ANSWER_SESSION_QUESTION`]: resolve the target, then deliver the answers through the port.
async fn execute_answer_session_question(
    port: &dyn ProjectHomeAnswerPort,
    home: &HomeCaller,
    args_json: &str,
) -> ToolOutcome {
    let args: AnswerQuestionArgs = match serde_json::from_str(args_json) {
        Ok(args) => args,
        Err(e) => {
            return ToolOutcome::err(format!("{ANSWER_SESSION_QUESTION}: invalid arguments: {e}"))
        }
    };
    let target = match resolve_target(port, home, &args.session_id).await {
        Ok(target) => target,
        Err(outcome) => return outcome,
    };
    let answers = args.answers.into_iter().map(QuestionAnswer::from).collect();
    match port
        .answer_question(home, &target, &args.request_id, answers)
        .await
    {
        Ok(answered) => ToolOutcome::ok(answered.to_string()),
        Err(e) => ToolOutcome::err(format!("{ANSWER_SESSION_QUESTION}: {e}")),
    }
}

/// Run [`ANSWER_SESSION_PERMISSION`] or [`ANSWER_SESSION_QUESTION`] for `home`.
///
/// Bad arguments, a target refusal and the port's refusal come back as an `is_error` outcome naming
/// what was refused; the role refusal is the daemon's and happens before this is called.
pub async fn execute_project_home_answer_tool(
    port: &dyn ProjectHomeAnswerPort,
    home: &HomeCaller,
    tool_name: &str,
    args_json: &str,
) -> ToolOutcome {
    if !is_project_home_answer_tool(tool_name) {
        return ToolOutcome::err(format!("unknown project-home answer tool: {tool_name}"));
    }
    if tool_name == ANSWER_SESSION_QUESTION {
        return execute_answer_session_question(port, home, args_json).await;
    }
    let args: AnswerPermissionArgs = match serde_json::from_str(args_json) {
        Ok(args) => args,
        Err(e) => return ToolOutcome::err(format!("{tool_name}: invalid arguments: {e}")),
    };
    let option_id = args.option_id.as_deref().filter(|id| !id.is_empty());
    let answer = match (option_id, args.cancelled) {
        (Some(option_id), false) => PermissionAnswer::Selected {
            request_id: args.request_id.clone(),
            option_id: option_id.to_string(),
        },
        (None, true) => PermissionAnswer::Cancelled {
            request_id: args.request_id.clone(),
        },
        (Some(_), true) => {
            return ToolOutcome::err(format!(
                "{tool_name}: give option_id or cancelled, not both"
            ))
        }
        (None, false) => {
            return ToolOutcome::err(format!(
                "{tool_name}: give an option_id, or set cancelled to leave it to the person"
            ))
        }
    };
    let target = match resolve_target(port, home, &args.session_id).await {
        Ok(target) => target,
        Err(outcome) => return outcome,
    };
    let message = args.message.filter(|message| !message.is_empty());
    match port.answer_permission(home, &target, answer, message).await {
        Ok(answered) => ToolOutcome::ok(answered.to_string()),
        Err(e) => ToolOutcome::err(format!("{tool_name}: {e}")),
    }
}
