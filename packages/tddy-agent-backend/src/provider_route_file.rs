//! The file a tool session's route travels in, from the daemon to `tddy-coder`.
//!
//! A tool session is spawned with a closed environment allowlist — commit identity only, "never a
//! credential" — so its route cannot travel as environment. It travels the way an agent definition's
//! credential already does: a JSON file in the session directory, owner-only, named on the command
//! line (`--provider-route-path`). `tddy-coder` refuses a route file anyone else can read.

use std::path::Path;

use crate::provider_route::HarnessRoute;

/// The route file's name inside a session directory.
pub const PROVIDER_ROUTE_FILENAME: &str = "provider-route.json";

/// `route` as the route file holds it — secrets included, which is why the file is owner-only.
#[must_use]
pub fn route_to_json(route: &HarnessRoute) -> String {
    let _ = route;
    // TODO(tool-route): implement
    todo!()
}

/// The route a route file holds.
pub fn route_from_json(json: &str) -> Result<HarnessRoute, String> {
    let _ = json;
    // TODO(tool-route): implement
    todo!()
}

/// Read the route file at `path`, refusing one whose mode lets anyone but its owner read it.
pub fn load_route_file(path: &Path) -> Result<HarnessRoute, String> {
    let _ = path;
    // TODO(tool-route): implement
    todo!()
}
