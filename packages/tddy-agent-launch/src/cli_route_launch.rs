//! Applying a provider route to a CLI harness launch: its files written owner-only in the session
//! directory, the harness's configuration variable pointed at them, and its environment and
//! arguments collected for the PTY.

use std::path::Path;

use tddy_core::cli_harness::CliHarness;
use tddy_core::provider_route::HarnessRoute;

/// What a routed CLI launch adds to the harness's own argv and environment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CliRouteLaunch {
    /// Environment for the agent child — the route's plain and secret pairs, and the harness's
    /// configuration variable naming the written file.
    pub env: Vec<(String, String)>,
    /// Arguments appended to the harness's argv.
    pub args: Vec<String>,
}

/// Write `route`'s files into `session_dir` (mode 0600) and collect what the launch needs. An
/// unrouted launch (`None`) adds nothing and writes nothing.
pub fn cli_route_launch(
    harness: &CliHarness,
    route: Option<&HarnessRoute>,
    session_dir: &Path,
) -> std::io::Result<CliRouteLaunch> {
    let _ = (harness, route, session_dir);
    // TODO(opencode-cli): implement
    todo!()
}
