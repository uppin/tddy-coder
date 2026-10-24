//! A permission request the main agent handed to the user, answered from the web.
//!
//! PRD: docs/ft/web/1-WIP/PRD-2026-10-24-coagents-web-approvals.md

use tddy_discovery::subagent::PermissionAnswer;

use crate::parked_permissions::ParkedPermissions;

/// Who answered a parked permission request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnsweredBy {
    /// The main agent, through `subagent_resume`.
    MainAgent,
    /// A user, by login, through `AnswerAgentPermission`.
    User(String),
}

impl std::fmt::Display for AnsweredBy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MainAgent => write!(f, "the main agent"),
            Self::User(login) => write!(f, "{login}"),
        }
    }
}

/// Answering a parked request from either side, once: the first answer wins, and a later one is
/// refused naming who answered first.
pub trait PermissionAnswering: Send + Sync {
    fn answer_as(&self, answer: &PermissionAnswer, by: AnsweredBy) -> Result<(), String>;

    /// Who answered `request_id`, once it has been answered.
    fn answered_by(&self, request_id: &str) -> Option<AnsweredBy>;
}

impl PermissionAnswering for ParkedPermissions {
    fn answer_as(&self, _answer: &PermissionAnswer, _by: AnsweredBy) -> Result<(), String> {
        // TODO(web-approvals): implement
        Err("TODO(web-approvals): PermissionAnswering::answer_as is not implemented".to_string())
    }

    fn answered_by(&self, _request_id: &str) -> Option<AnsweredBy> {
        // TODO(web-approvals): implement
        None
    }
}
