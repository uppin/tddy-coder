//! `DescribeSessionStartOptions`: the per-host choices behind `StartSession`'s arguments, answered
//! from the same RPCs the web's New-session form fills its pickers from (`#start-parity` 5/9).
//!
//! The closed lists — session types, recipes, permission modes, hosts — are enums in `StartSession`'s
//! own schema. What depends on a host and on other choices (the agents a host offers, the models of
//! one agent or provider account, the branches of the project's checkout there, the sessions that can
//! parent or seed a stack) is asked for here, when the agent needs it, rather than frozen into a
//! schema that would go stale.
//!
//! The two stack pickers are filters over the listed sessions. [`pr_stack_orchestrators`] and
//! [`stack_base_session_candidates`] mirror the form's `prStackOrchestrators` and
//! `stackBaseSessionCandidates` (`tddy-web/src/utils/stackParents.ts`); the web's
//! `stackParentsParity.cy.ts` runs those over the fixture `tests/start_options.rs` runs these over.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

use serde::{Deserialize, Serialize};

use crate::catalog::ToolDef;

pub const DESCRIBE_SESSION_START_OPTIONS: &str = "DescribeSessionStartOptions";

/// The recipes whose sessions orchestrate a PR stack (the form's `PR_STACK_RECIPES`).
pub const PR_STACK_RECIPES: &[&str] = &["pr-stack", "orchestrate-pr-stack", "plan-pr-stack"];

/// What the calling agent narrows the answer by. Every field is optional.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StartOptionsQuery {
    /// The host to describe; empty is this host.
    pub daemon_instance_id: String,
    /// The session type the models are for when no `agent` is named (its harness is the agent).
    pub session_type: String,
    /// The agent whose models to list.
    pub agent: String,
    /// List this provider account's models instead of the agent's vendor's.
    pub provider_account_id: String,
}

/// One choice with an id and the label the form shows for it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabelledChoice {
    pub id: String,
    pub label: String,
}

/// The models one agent (or provider account) runs, and the one the form preselects.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelChoices {
    pub default: String,
    pub choices: Vec<LabelledChoice>,
}

/// One session as the stack pickers filter it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateSession {
    pub session_id: String,
    pub project_id: String,
    pub daemon_instance_id: String,
    pub branch: String,
    pub recipe: String,
    pub orchestrator_session_id: String,
}

/// `DescribeSessionStartOptions`'s answer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartOptions {
    pub daemon_instance_id: String,
    pub agents: Vec<LabelledChoice>,
    pub models: ModelChoices,
    pub provider_routes: Vec<LabelledChoice>,
    pub ssh_config_hosts: Vec<String>,
    /// Qualified `name@daemon_instance_id`, as `specialized_agents` takes them.
    pub subagents: Vec<String>,
    pub branches: Vec<String>,
    pub stack_parents: Vec<CandidateSession>,
    pub pr_stack_base_candidates: Vec<CandidateSession>,
}

/// The sessions the form offers as a PR-stack parent: a PR-stack recipe, and not itself a child of
/// another orchestrator.
pub fn pr_stack_orchestrators(sessions: &[CandidateSession]) -> Vec<CandidateSession> {
    // TODO(start-options): implement
    let _ = sessions;
    Vec::new()
}

/// The sessions whose branch may seed a new PR-stack orchestrator's stack on `daemon_instance_id`
/// in `project_id`: owning a branch, in that project, on that host, not already stacked. Nothing
/// when `project_id` is empty.
pub fn stack_base_session_candidates(
    sessions: &[CandidateSession],
    project_id: &str,
    daemon_instance_id: &str,
) -> Vec<CandidateSession> {
    // TODO(start-options): implement
    let _ = (sessions, project_id, daemon_instance_id);
    Vec::new()
}

/// The tool's definition.
pub fn start_options_tool_def() -> Option<ToolDef> {
    // TODO(start-options): implement
    None
}
