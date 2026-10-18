//! A tool session's provider route, as the route file its `tddy-coder` child is handed.
//!
//! Resolved like a CLI session's ([`crate::session_provider_route`]), for the session's agent; an
//! agent that cannot be routed (`cursor`, `stub`, a specialized definition) refuses the start with
//! the reason.

use tddy_core::provider_route::RouteSources;
use tddy_rpc::Status;

use crate::session_acting_identity::SessionAccountAccess;

/// The route-file JSON for `agent` running `model` after precedence; `None` for the vendor.
pub fn tool_session_route_json(
    access: &SessionAccountAccess,
    agent: &str,
    sources: &RouteSources,
    model: &str,
) -> Result<Option<String>, Status> {
    let _ = (access, agent, sources, model);
    // TODO(tool-route): implement
    todo!()
}
