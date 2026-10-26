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
//! A list that could not be answered is empty, and its `*_unavailable` field says why; a list that
//! was answered has an empty reason. The reason is never a silent stand-in for the list.
//!
//! Feature doc: docs/ft/daemon/project-concept.md § Project-home tools.

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::catalog::ToolDef;
use crate::project_home_tools::tool_def;

pub const DESCRIBE_SESSION_START_OPTIONS: &str = "DescribeSessionStartOptions";

/// The recipes whose sessions orchestrate a PR stack (the form's `PR_STACK_RECIPES`).
pub const PR_STACK_RECIPES: &[&str] = &["pr-stack", "orchestrate-pr-stack", "plan-pr-stack"];

/// What the calling agent narrows the answer by. Every field is optional.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StartOptionsQuery {
    /// The host to describe; empty is this host.
    pub daemon_instance_id: String,
    /// The session type the start is for: its harness runs the provider accounts offered, and, when
    /// no `agent` is named, its own harness is the agent whose models are listed.
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

/// One provider account as the accounts that can run a harness are offered: whether it can, and why
/// not (the accounts service's own verdict).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderRouteChoice {
    pub account_id: String,
    pub label: String,
    pub preset: String,
    pub supported: bool,
    pub reason: String,
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
    /// The host that answered: this daemon's id.
    pub daemon_instance_id: String,
    pub agents: Vec<LabelledChoice>,
    pub models: ModelChoices,
    /// Why `models` is empty, when the query could not name a list of models. Empty when it could.
    pub models_unavailable: String,
    pub provider_routes: Vec<ProviderRouteChoice>,
    /// Why `provider_routes` is empty, when no accounts could be asked: none is attached on this
    /// host, the harness is unnamed or has no provider accounts, or the caller's vault refused.
    pub provider_routes_unavailable: String,
    pub ssh_config_hosts: Vec<String>,
    /// Why `ssh_config_hosts` is empty, when the host could not list its SSH config.
    pub ssh_config_hosts_unavailable: String,
    /// Qualified `name@daemon_instance_id`, as `specialized_agents` takes them.
    pub subagents: Vec<String>,
    pub branches: Vec<String>,
    pub stack_parents: Vec<CandidateSession>,
    pub pr_stack_base_candidates: Vec<CandidateSession>,
}

/// The sessions the form offers as a PR-stack parent: a PR-stack recipe, and not itself a child of
/// another orchestrator.
pub fn pr_stack_orchestrators(sessions: &[CandidateSession]) -> Vec<CandidateSession> {
    sessions
        .iter()
        .filter(|s| {
            PR_STACK_RECIPES.contains(&s.recipe.as_str()) && s.orchestrator_session_id.is_empty()
        })
        .cloned()
        .collect()
}

/// The sessions whose branch may seed a new PR-stack orchestrator's stack on `daemon_instance_id`
/// in `project_id`: owning a branch, in that project, on that host, not already stacked. Nothing
/// when `project_id` is empty.
pub fn stack_base_session_candidates(
    sessions: &[CandidateSession],
    project_id: &str,
    daemon_instance_id: &str,
) -> Vec<CandidateSession> {
    if project_id.is_empty() {
        return Vec::new();
    }
    sessions
        .iter()
        .filter(|s| {
            !s.branch.is_empty()
                && s.project_id == project_id
                && s.daemon_instance_id == daemon_instance_id
                && s.orchestrator_session_id.is_empty()
        })
        .cloned()
        .collect()
}

/// The harness whose provider accounts a start for `query` may run on, as the form chooses it: a CLI
/// session type is its own harness, a tool session runs the agent it names, and Warp runs only on its
/// own vendor, so none. The reason when there is none.
pub fn provider_harness(query: &StartOptionsQuery) -> Result<String, String> {
    let session_type = query.session_type.trim();
    let agent = query.agent.trim();
    match session_type {
        "warp-cli" => Err(
            "warp-cli runs only on its own vendor, so no provider account can run it".to_string(),
        ),
        "tool" if agent.is_empty() => {
            Err("a tool session names its agent: provider routes are per harness".to_string())
        }
        "tool" => Ok(agent.to_string()),
        "" if agent.is_empty() => {
            Err("name a session_type or an agent: provider routes are per harness".to_string())
        }
        "" => Ok(agent.to_string()),
        other => Ok(other.to_string()),
    }
}

/// The agent whose models a query lists: the agent named, else the session type when it is a CLI
/// harness. The reason when neither names one.
pub fn models_agent(query: &StartOptionsQuery) -> Result<String, String> {
    let agent = query.agent.trim();
    if !agent.is_empty() {
        return Ok(agent.to_string());
    }
    match query.session_type.trim() {
        "" => Err("name an agent or a session_type: models are per agent".to_string()),
        session_type @ ("tool" | "workspace") => Err(format!(
            "a {session_type} session names no agent of its own: name the agent whose models to list"
        )),
        session_type => Ok(session_type.to_string()),
    }
}

/// The tool's definition. Every argument is optional, and each is described.
pub fn start_options_tool_def() -> ToolDef {
    tool_def(
        DESCRIBE_SESSION_START_OPTIONS,
        "Describe the choices a StartSession call can make on a host: its agents, the models of an agent or of a provider account, the provider accounts that can run a session type's harness, its SSH config hosts, its subagents, the branches of this project's checkout there, and the sessions that can parent or seed a PR stack. Narrow it with session_type, agent or provider_account_id. An empty daemon_instance_id is this host. daemon_instance_id in the answer names the host that answered; a list that could not be answered is empty and its *_unavailable field says why.",
        json!({
            "type": "object",
            "properties": {
                "daemon_instance_id": {
                    "type": "string",
                    "description": "The host to describe, as ListProjectSessions reports it in its daemon_instance_id; empty for this host."
                },
                "session_type": {
                    "type": "string",
                    "description": "The session type the start is for. Its harness decides the provider accounts offered, and a CLI session type's models are its agent's. A tool session names its agent instead."
                },
                "agent": {
                    "type": "string",
                    "description": "The agent whose models to list, as the catalogue names it: a tool session's agent, or a CLI session type."
                },
                "provider_account_id": {
                    "type": "string",
                    "description": "A connected provider account whose models to list, instead of the agent's vendor's."
                }
            },
            "additionalProperties": false
        }),
    )
}
