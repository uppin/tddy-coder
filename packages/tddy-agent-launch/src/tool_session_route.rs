//! A tool session's provider route, as the route file its `tddy-coder` child is handed.
//!
//! Resolved like a CLI session's ([`crate::session_provider_route`]), for the session's agent; an
//! agent that cannot be routed (`cursor`, `stub`, a specialized definition) refuses the start with
//! the reason.

use std::path::Path;

use tddy_core::provider_route::{choose_route, RouteSources};
use tddy_core::provider_route_file::route_to_json;
use tddy_rpc::Status;

use crate::session_acting_identity::SessionAccountAccess;
use crate::session_provider_route::{
    resolve_session_route, route_sources_on_resume, routed_agent_on_host,
};
use crate::spec_launch::HostCodingAgents;

/// The route-file JSON for `agent` running `model` after precedence; `None` for the vendor.
///
/// A session that chooses no account gets no route and never reads the vault. A session that does
/// choose one is refused, with the reason, when `agent` is no installed coding agent (a specialized
/// definition is its own endpoint, and `stub` runs no provider), when the host has no coding agents
/// at all, or when the agent's spec declares no route (Cursor). Nothing falls back to the vendor.
pub fn tool_session_route_json(
    access: &SessionAccountAccess,
    agents: Option<&HostCodingAgents>,
    agent: &str,
    sources: &RouteSources,
    model: &str,
) -> Result<Option<String>, Status> {
    if choose_route(sources).is_none() {
        return Ok(None);
    }
    if agent.is_empty() {
        return Err(Status::invalid_argument(
            "a tool session on a provider account must name its agent: the account is chosen for \
             the harness that runs it",
        ));
    }
    let routed = routed_agent_on_host(agents, agent)?;
    Ok(resolve_session_route(access, &routed, sources, model)?.map(|route| route_to_json(&route)))
}

/// The route a resumed tool session runs on again: the account and model its start recorded beside
/// `.session.yaml`, re-resolved against the owner's vault as it stands now. `None` when the start
/// chose the vendor.
pub fn resumed_tool_session_route_json(
    access: &SessionAccountAccess,
    agents: Option<&HostCodingAgents>,
    agent: &str,
    session_dir: &Path,
) -> Result<Option<String>, Status> {
    let (sources, model) = route_sources_on_resume(session_dir)?;
    let Some(model) = model else {
        return Ok(None);
    };
    tool_session_route_json(access, agents, agent, &sources, &model)
}
