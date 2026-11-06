//! A session's provider route, resolved once at its start or resume.
//!
//! The account is read from the session owner's vault exactly as the GitHub identity is
//! ([`SessionAccountAccess`]), and with the same refusals: a locked vault, no vault, or an account
//! it does not hold stop the start before anything is spawned. **No fallback**: a session asked to
//! run on an account never runs on the harness's vendor instead.
//!
//! Unlike the GitHub token, a provider key does reach the agent's environment — a harness CLI reads
//! its key from there and has no per-call socket to ask instead. It goes to the agent child only:
//! [`pty_route_env`] for a PTY, the jail's secret files for a sandbox, the route file for a tool
//! session.

use std::path::Path;

use tddy_accounts::AccountsError;
use tddy_coding_agent_specs::{routed_agent, CodingAgentRegistry};
use tddy_core::provider_route::{
    agent_routable, choose_route, resolve_route, HarnessRoute, RouteChoice, RouteEndpoints,
    RouteRefusal, RouteSecret, RouteSources, RouteTarget, RoutedAgent,
};
use tddy_core::{
    read_session_provider_account, write_session_provider_account, SessionProviderAccount,
};
use tddy_rpc::Status;
use tddy_service::proto::session::StartSessionRequest;

use crate::session_acting_identity::{SessionAccountAccess, SessionIdentityRefusal};
use crate::spec_launch::HostCodingAgents;

/// The session-level choice a start request carries. The other levels are filled by their owners.
#[must_use]
pub fn session_route_sources(req: &StartSessionRequest) -> RouteSources {
    // The proto makes the two exclusive; if a caller sends both, the named account is the more
    // specific choice and wins — never the vendor the operator did not ask for.
    let session = if !req.provider_account_id.is_empty() {
        Some(RouteChoice::Account(req.provider_account_id.clone()))
    } else if req.provider_built_in {
        Some(RouteChoice::BuiltIn)
    } else {
        None
    };
    RouteSources {
        session,
        ..RouteSources::default()
    }
}

/// [`session_route_sources`] plus the owner's default for `harness` — what a start that may run on
/// a provider account is decided from. A locked vault refuses rather than starting on the vendor
/// the person's default may have been chosen to avoid.
///
/// The default is read only when it can matter: a start that already names an account or
/// "built in" never consults the vault for it, so an explicit choice is never refused for a vault
/// it does not need.
pub fn session_route_sources_with_default(
    req: &StartSessionRequest,
    harness: &str,
    access: &SessionAccountAccess,
) -> Result<RouteSources, Status> {
    let sources = session_route_sources(req);
    if sources.session.is_some() || sources.agent_definition.is_some() {
        return Ok(sources);
    }
    let user_default = access
        .default_provider_account(harness)
        .map_err(status_of_refusal)?;
    Ok(RouteSources {
        user_default,
        ..sources
    })
}

/// The route `agent` runs `model` on, after precedence — `None` for the agent's vendor.
///
/// Reads the chosen account from the session owner's vault; refuses when it cannot be read, is not
/// a provider account, no model was chosen, or the agent cannot run on it.
pub fn resolve_session_route(
    access: &SessionAccountAccess,
    agent: &RoutedAgent,
    sources: &RouteSources,
    model: &str,
) -> Result<Option<HarnessRoute>, Status> {
    choose_route(sources)
        .map(|account_id| route_on_account(access, agent, &account_id, model))
        .transpose()
}

/// The route `agent` runs `model` on the account `account_id` names in the owner's vault.
pub(crate) fn route_on_account(
    access: &SessionAccountAccess,
    agent: &RoutedAgent,
    account_id: &str,
    model: &str,
) -> Result<HarnessRoute, Status> {
    // Checked first: an agent that cannot be pointed at any provider is refused by name, whatever
    // account or model the start also names.
    agent_routable(agent).map_err(route_refusal_status)?;
    // Every model alias the harness could fall through to is pinned to this one; an empty pin
    // would leave the harness choosing a vendor model name the account does not serve.
    if model.trim().is_empty() {
        return Err(Status::invalid_argument(format!(
            "a session on provider account {account_id:?} needs a model: choose one the account serves"
        )));
    }
    let account = access
        .provider_account(account_id)
        .map_err(status_of_refusal)?;
    let target = RouteTarget {
        preset: account.preset.id.to_string(),
        endpoints: RouteEndpoints {
            anthropic_messages: account.endpoints.anthropic_messages,
            openai_responses: account.endpoints.openai_responses,
            openai_chat: account.endpoints.openai_chat,
        },
        api_key: RouteSecret::new(account.api_key.expose()),
        model: model.to_string(),
    };
    resolve_route(agent, &target).map_err(route_refusal_status)
}

/// A route refusal as the status a start answers with: an agent whose route is not built yet is
/// unimplemented, and every other refusal is an invalid argument naming what would have to change.
pub(crate) fn route_refusal_status(refusal: RouteRefusal) -> Status {
    match refusal {
        RouteRefusal::NotYetRoutable { .. } => Status::unimplemented(refusal.to_string()),
        other => Status::invalid_argument(other.to_string()),
    }
}

/// A vault refusal as the status a start answers with: the same codes whichever account kind was
/// asked for.
pub(crate) fn status_of_refusal(refusal: SessionIdentityRefusal) -> Status {
    let message = refusal.to_string();
    match refusal {
        SessionIdentityRefusal::NoVaultsConfigured => Status::failed_precondition(message),
        SessionIdentityRefusal::Vault(AccountsError::NoSuchSession) => {
            Status::unauthenticated(message)
        }
        SessionIdentityRefusal::Vault(AccountsError::Locked | AccountsError::Uninitialized) => {
            Status::failed_precondition(message)
        }
        SessionIdentityRefusal::Vault(AccountsError::NotFound { .. }) => Status::not_found(message),
        SessionIdentityRefusal::Vault(AccountsError::Unavailable(_)) => Status::internal(message),
        SessionIdentityRefusal::Identity(_) => Status::failed_precondition(message),
        SessionIdentityRefusal::AmbiguousLabel(_) => Status::invalid_argument(message),
    }
}

/// The environment a PTY-hosted agent is launched with for `route`: its plain and secret pairs.
/// Files and arguments are not environment and are applied by the caller.
#[must_use]
pub fn pty_route_env(route: &HarnessRoute) -> Vec<(String, String)> {
    route
        .env
        .iter()
        .cloned()
        .chain(
            route
                .secret_env
                .iter()
                .map(|(name, secret)| (name.clone(), secret.expose().to_string())),
        )
        .collect()
}

/// What a resumed session runs on: the account and model its start recorded beside `.session.yaml`
/// (`provider-account.yaml`), as the session-level choice; the vendor when nothing was recorded.
/// The key is not in the record — [`resolve_session_route`] reads it from the vault again.
pub fn route_sources_on_resume(
    session_dir: &Path,
) -> Result<(RouteSources, Option<String>), Status> {
    let recorded = read_session_provider_account(session_dir)
        .map_err(|e| Status::internal(format!("provider-account record unreadable: {e}")))?;
    Ok(match recorded {
        Some(SessionProviderAccount { account_id, model }) => (
            RouteSources {
                session: Some(RouteChoice::Account(account_id)),
                ..RouteSources::default()
            },
            Some(model),
        ),
        None => (RouteSources::default(), None),
    })
}

/// What a start that chose an account is launched with, and what it records for the resume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartRoute {
    /// The pairs the PTY is launched with — see [`pty_route_env`].
    pub env: Vec<(String, String)>,
    /// The account and model the session's directory records; never the key.
    pub account: SessionProviderAccount,
}

impl StartRoute {
    /// Record the account beside the session's `.session.yaml`, so a resume re-applies it.
    pub fn record_in(&self, session_dir: &Path) -> Result<(), Status> {
        write_session_provider_account(session_dir, &self.account)
            .map_err(|e| Status::internal(format!("failed to record the provider account: {e}")))
    }
}

/// The route a start of `agent` on `model` is launched with, resolved before anything is created;
/// `None` when no account was chosen, so the agent runs on its vendor.
pub fn start_route(
    access: &SessionAccountAccess,
    agent: RouteAgent<'_>,
    sources: &RouteSources,
    model: &str,
) -> Result<Option<StartRoute>, Status> {
    let Some(account_id) = choose_route(sources) else {
        return Ok(None);
    };
    let route = route_on_account(access, &agent.resolve()?, &account_id, model)?;
    Ok(Some(StartRoute {
        env: pty_route_env(&route),
        account: SessionProviderAccount {
            account_id,
            model: model.to_string(),
        },
    }))
}

/// The route a resumed session runs on: the account its start recorded, re-resolved against the
/// owner's vault as it stands now. `None` for a session on its vendor.
pub fn resume_route(
    access: &SessionAccountAccess,
    agent: RouteAgent<'_>,
    session_dir: &Path,
) -> Result<Option<HarnessRoute>, Status> {
    let (sources, model) = route_sources_on_resume(session_dir)?;
    let Some(model) = model else {
        return Ok(None);
    };
    resolve_session_route(access, &agent.resolve()?, &sources, &model)
}

/// The environment a resumed PTY session is relaunched with: the route its start recorded,
/// re-resolved against the owner's vault as it stands now. Empty for a session on its vendor.
pub fn resume_route_env(
    access: &SessionAccountAccess,
    agent: RouteAgent<'_>,
    session_dir: &Path,
) -> Result<Vec<(String, String)>, Status> {
    Ok(resume_route(access, agent, session_dir)?
        .map(|route| pty_route_env(&route))
        .unwrap_or_default())
}

/// The route a sandboxed start of `agent` on `model` runs on; `None` when no account was chosen.
///
/// The route is resolved before `session_dir` is created, so a refusal leaves nothing on disk. The
/// account and model it runs on are then recorded in `session_dir` (never the key), so a resume
/// re-applies them. The jail receives the route through its secret files and environment.
pub fn sandboxed_start_route(
    access: &SessionAccountAccess,
    agent: RouteAgent<'_>,
    sources: &RouteSources,
    model: &str,
    session_dir: &Path,
) -> Result<Option<HarnessRoute>, Status> {
    let Some(account_id) = choose_route(sources) else {
        return Ok(None);
    };
    let route = route_on_account(access, &agent.resolve()?, &account_id, model)?;
    std::fs::create_dir_all(session_dir)
        .map_err(|e| Status::internal(format!("failed to create session dir: {e}")))?;
    write_session_provider_account(
        session_dir,
        &SessionProviderAccount {
            account_id,
            model: model.to_string(),
        },
    )
    .map_err(|e| Status::internal(format!("failed to record the provider account: {e}")))?;
    Ok(Some(route))
}

/// What a route body knows of the agent `id` names in `registry`: its spec, built into a
/// [`RoutedAgent`]. `id` is an agent id (`claude`) or the CLI session type that runs one
/// (`claude-cli`) — both name the same spec. An id the registry does not hold is refused
/// ("unknown agent"), never resolved to another agent.
pub fn routed_agent_in(registry: &CodingAgentRegistry, id: &str) -> Result<RoutedAgent, Status> {
    let spec = registry.get(id).or_else(|| registry.for_session_type(id));
    spec.map(routed_agent).ok_or_else(|| {
        Status::invalid_argument(format!(
            "the {id:?} agent is unknown to this host: only an installed coding agent can run on \
             a provider account"
        ))
    })
}

/// [`routed_agent_in`] over the coding agents a host has installed. A host with none refuses:
/// there is no spec to build a route from, and no other source is consulted.
pub fn routed_agent_on_host(
    agents: Option<&HostCodingAgents>,
    id: &str,
) -> Result<RoutedAgent, Status> {
    let agents = agents.ok_or_else(|| {
        Status::failed_precondition(format!(
            "this host has no coding agents installed: the route of {id} is built from its spec"
        ))
    })?;
    routed_agent_in(agents.agents(), id)
}

/// The agent id Claude Code's sessions are routed as: the spec the `claude-cli` session type runs.
pub const CLAUDE_ROUTE_AGENT: &str = "claude";

/// The agent a route is resolved for, built only when a route is actually asked for: a session that
/// chose no account never needs its agent's spec, so a host without coding agents still starts it.
#[derive(Clone, Copy)]
pub enum RouteAgent<'a> {
    /// An agent already built from its spec.
    Built(&'a RoutedAgent),
    /// The agent `id` of a host's installed coding agents; refused when the host has none or does
    /// not know the id.
    OnHost {
        agents: Option<&'a HostCodingAgents>,
        id: &'a str,
    },
}

impl<'a> From<&'a RoutedAgent> for RouteAgent<'a> {
    fn from(agent: &'a RoutedAgent) -> Self {
        RouteAgent::Built(agent)
    }
}

impl<'a> RouteAgent<'a> {
    /// Claude Code among a host's coding agents: the spec the `claude-cli` session type runs.
    pub fn claude_on(agents: Option<&'a HostCodingAgents>) -> Self {
        RouteAgent::OnHost {
            agents,
            id: CLAUDE_ROUTE_AGENT,
        }
    }

    /// The agent the route is resolved for.
    pub fn resolve(&self) -> Result<RoutedAgent, Status> {
        match self {
            RouteAgent::Built(agent) => Ok((*agent).clone()),
            RouteAgent::OnHost { agents, id } => routed_agent_on_host(*agents, id),
        }
    }
}
