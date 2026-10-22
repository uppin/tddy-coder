//! A tool session's provider route, as the route file its `tddy-coder` child is handed.
//!
//! Resolved like a CLI session's ([`crate::session_provider_route`]), for the session's agent; an
//! agent that cannot be routed (`cursor`, `stub`, a specialized definition) refuses the start with
//! the reason.

use std::path::Path;

use tddy_core::provider_route::{choose_route, Harness, RouteSources};
use tddy_core::provider_route_file::route_to_json;
use tddy_rpc::Status;

use crate::session_acting_identity::SessionAccountAccess;
use crate::session_provider_route::{resolve_session_route, route_sources_on_resume};

/// The route-file JSON for `agent` running `model` after precedence; `None` for the vendor.
///
/// A session that chooses no account gets no route and never reads the vault. A session that does
/// choose one is refused, with the reason, when `agent` is not a harness (a specialized definition
/// is its own endpoint, and `stub` runs no provider) or when the harness cannot run on an account
/// at all (Cursor). Nothing falls back to the vendor.
pub fn tool_session_route_json(
    access: &SessionAccountAccess,
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
    let harness = Harness::from_agent_or_session_type(agent).ok_or_else(|| {
        Status::invalid_argument(format!(
            "the {agent:?} agent cannot run on a provider account: only a harness can, and a \
             specialized agent definition is its own endpoint"
        ))
    })?;
    Ok(resolve_session_route(access, harness, sources, model)?.map(|route| route_to_json(&route)))
}

/// The route a resumed tool session runs on again: the account and model its start recorded beside
/// `.session.yaml`, re-resolved against the owner's vault as it stands now. `None` when the start
/// chose the vendor.
pub fn resumed_tool_session_route_json(
    access: &SessionAccountAccess,
    agent: &str,
    session_dir: &Path,
) -> Result<Option<String>, Status> {
    let (sources, model) = route_sources_on_resume(session_dir)?;
    let Some(model) = model else {
        return Ok(None);
    };
    tool_session_route_json(access, agent, &sources, &model)
}
