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

use tddy_core::provider_route::{Harness, HarnessRoute, RouteSources};
use tddy_rpc::Status;
use tddy_service::proto::session::StartSessionRequest;

use crate::session_acting_identity::SessionAccountAccess;

/// The session-level choice a start request carries. The other levels are filled by their owners.
#[must_use]
pub fn session_route_sources(req: &StartSessionRequest) -> RouteSources {
    let _ = req;
    // TODO(claude-route): implement
    todo!()
}

/// The route `harness` runs `model` on, after precedence — `None` for the harness's vendor.
///
/// Reads the chosen account from the session owner's vault; refuses when it cannot be read, is not
/// a provider account, or the harness cannot run on it.
pub fn resolve_session_route(
    access: &SessionAccountAccess,
    harness: Harness,
    sources: &RouteSources,
    model: &str,
) -> Result<Option<HarnessRoute>, Status> {
    let _ = (access, harness, sources, model);
    // TODO(claude-route): implement
    todo!()
}

/// The environment a PTY-hosted agent is launched with for `route`: its plain and secret pairs.
/// Files and arguments are not environment and are applied by the caller.
#[must_use]
pub fn pty_route_env(route: &HarnessRoute) -> Vec<(String, String)> {
    let _ = route;
    // TODO(claude-route): implement
    todo!()
}

/// What a resumed session runs on: the account and model its start recorded beside `.session.yaml`
/// (`provider-account.yaml`), as the session-level choice; the vendor when nothing was recorded.
/// The key is not in the record — [`resolve_session_route`] reads it from the vault again.
pub fn route_sources_on_resume(
    session_dir: &Path,
) -> Result<(RouteSources, Option<String>), Status> {
    let _ = session_dir;
    // TODO(claude-route): implement
    todo!()
}
