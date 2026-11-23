//! `DescribeSessionStartOptions`: the per-host choices behind `StartSession`'s arguments, answered
//! from the same RPCs the web's New-session form fills its pickers from (`#start-parity` 5/9).
//!
//! The closed lists — session types, recipes, permission modes, hosts — are enums in `StartSession`'s
//! own schema. What depends on a host and on other choices (the agents a host offers, the models of
//! one agent or provider account, the branches of the project's checkout there) is asked for here,
//! when the agent needs it, rather than frozen into a schema that would go stale.
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

/// One host a daemon reaches through a remote shell, as `StartSession` can be addressed at it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DerivedHostChoice {
    /// What `StartSession`'s `target` names: `<parent>/ssh/<alias>` or
    /// `<parent>/coder/<account>/<owner>/<workspace>`.
    pub id: String,
    pub label: String,
    /// `ssh` or `coder`.
    pub kind: String,
    /// The daemon the session actually starts on; the agent runs on the derived host.
    pub parent_daemon_instance_id: String,
    /// Whether a session can start on it now.
    pub usable: bool,
    /// Why not, in the daemon's words, when `usable` is false.
    pub unusable_reason: String,
}

/// The models one agent (or provider account) runs, and the one the form preselects.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelChoices {
    pub default: String,
    pub choices: Vec<LabelledChoice>,
}

/// The effort levels one coding agent takes (its spec's `coding_agent.effort`), under the session
/// type that starts it (`#orchestrators` 3/11).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentEffortLevels {
    pub session_type: String,
    /// The level the spec preselects; empty leaves the agent's own default.
    pub default: String,
    pub levels: Vec<String>,
}

/// `DescribeSessionStartOptions`'s answer.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartOptions {
    /// The host that answered: this daemon's id.
    pub daemon_instance_id: String,
    /// The session types this host declares (its tool files' `extensions.session`), each with its
    /// label. `workspace` — a worktree with no agent — is always startable besides, and declared by
    /// nothing.
    pub session_types: Vec<LabelledChoice>,
    pub agents: Vec<LabelledChoice>,
    pub models: ModelChoices,
    /// Why `models` is empty, when the query could not name a list of models. Empty when it could.
    pub models_unavailable: String,
    /// The effort levels of every coding agent on this host that declares any, in the registry's
    /// order; an agent missing from it takes no effort.
    pub effort_levels: Vec<AgentEffortLevels>,
    pub provider_routes: Vec<ProviderRouteChoice>,
    /// Why `provider_routes` is empty, when no accounts could be asked: none is attached on this
    /// host, the harness is unnamed or has no provider accounts, or the caller's vault refused.
    pub provider_routes_unavailable: String,
    pub ssh_config_hosts: Vec<String>,
    /// Why `ssh_config_hosts` is empty, when the host could not list its SSH config.
    pub ssh_config_hosts_unavailable: String,
    /// The hosts this host reaches through a remote shell — its SSH config aliases and the
    /// caller's Coder workspaces — each startable by naming its `id` as `StartSession`'s `target`
    /// (`#remote-shell` 11/11). A wildcard `Host` entry (`coder config-ssh`'s `Host coder.*`) is
    /// not an alias and is not here; the workspaces it would reach are.
    pub derived_hosts: Vec<DerivedHostChoice>,
    /// Why `derived_hosts` is empty, when the host could not list them. Empty when it could.
    pub derived_hosts_unavailable: String,
    /// Qualified `name@daemon_instance_id`, as `specialized_agents` takes them.
    pub subagents: Vec<String>,
    pub branches: Vec<String>,
}

/// The coding agent among `agents` that `session_type` starts, when it is one's.
fn agent_of_session_type<'a>(
    agents: &'a [crate::start_session_args::AgentChoice],
    session_type: &str,
) -> Option<&'a crate::start_session_args::AgentChoice> {
    agents
        .iter()
        .find(|choice| !choice.session_type.is_empty() && choice.session_type == session_type)
}

/// The harness whose provider accounts a start for `query` may run on, as the form chooses it: a
/// coding agent's session type (one of `agents`) is its own harness, and any other session type
/// runs the agent it names — except the no-agent session type, which runs none. An agent whose
/// injected choice says no route can point it elsewhere runs only on its own vendor, so none. The
/// reason when there is none.
pub fn provider_harness(
    query: &StartOptionsQuery,
    agents: &[crate::start_session_args::AgentChoice],
) -> Result<String, String> {
    let session_type = query.session_type.trim();
    let agent = query.agent.trim();
    if let Some(choice) = agent_of_session_type(agents, session_type) {
        if !choice.routable {
            return Err(format!(
                "{session_type} runs only on its own vendor, so no provider account can run it"
            ));
        }
        return Ok(session_type.to_string());
    }
    if tddy_workflow::is_no_agent_session_type(session_type) {
        return Err(format!(
            "a {session_type} session runs no agent: provider routes are per harness"
        ));
    }
    if !agent.is_empty() {
        return Ok(agent.to_string());
    }
    if session_type.is_empty() {
        return Err("name a session_type or an agent: provider routes are per harness".to_string());
    }
    Err(format!(
        "a {session_type} session names its agent: provider routes are per harness"
    ))
}

/// The agent whose models a query lists: the agent named, else the session type when it is a
/// coding agent's (one of `agents`). The reason when neither names one.
pub fn models_agent(
    query: &StartOptionsQuery,
    agents: &[crate::start_session_args::AgentChoice],
) -> Result<String, String> {
    let agent = query.agent.trim();
    if !agent.is_empty() {
        return Ok(agent.to_string());
    }
    match query.session_type.trim() {
        "" => Err("name an agent or a session_type: models are per agent".to_string()),
        session_type if agent_of_session_type(agents, session_type).is_some() => {
            Ok(session_type.to_string())
        }
        session_type => Err(format!(
            "a {session_type} session names no agent of its own: name the agent whose models to list"
        )),
    }
}

/// The tool's definition. Every argument is optional, and each is described.
pub fn start_options_tool_def() -> ToolDef {
    tool_def(
        DESCRIBE_SESSION_START_OPTIONS,
        "Describe the choices a StartSession call can make on a host: its agents, the models of an agent or of a provider account, the provider accounts that can run a session type's harness, its SSH config hosts, its subagents, and the branches of this project's checkout there. Narrow it with session_type, agent or provider_account_id. An empty daemon_instance_id is this host. daemon_instance_id in the answer names the host that answered; a list that could not be answered is empty and its *_unavailable field says why. An SSH or Coder host's `usable` means it is listed as startable, not probed: a reachable-looking host can still fail the start.",
        json!({
            "type": "object",
            "properties": {
                "daemon_instance_id": {
                    "type": "string",
                    "description": "The host to describe, as ListProjectSessions reports it in its daemon_instance_id; empty for this host."
                },
                "session_type": {
                    "type": "string",
                    "description": "The session type the start is for. Its harness decides the provider accounts offered, and a coding agent's session type's models are its agent's. Any other session type names its agent instead."
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
