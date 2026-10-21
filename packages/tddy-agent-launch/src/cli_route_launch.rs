//! Applying a provider route to a CLI harness launch: its files written owner-only in the session
//! directory, the harness's configuration variable pointed at them, and its environment and
//! arguments collected for the PTY.

use std::io;
use std::path::Path;

use tddy_core::atomic_file::write_atomic_with_mode;
use tddy_core::cli_harness::CliHarness;
use tddy_core::provider_route::HarnessRoute;

use crate::session_provider_route::pty_route_env;

/// A route file names the endpoint and holds the account's configuration, so only the owner may read
/// it. Applied when the file is created, so its bytes are never on disk at a wider mode.
const ROUTE_FILE_MODE: u32 = 0o600;

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
) -> io::Result<CliRouteLaunch> {
    let Some(route) = route else {
        return Ok(CliRouteLaunch::default());
    };
    for file in &route.files {
        write_atomic_with_mode(
            &session_dir.join(&file.name),
            &file.contents,
            ROUTE_FILE_MODE,
        )?;
    }
    let mut env = pty_route_env(route);
    if let (Some(config_env), Some(config)) = (harness.route_config_env, route.files.first()) {
        let path = session_dir.join(&config.name);
        let path_text = path.to_str().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{} is not valid UTF-8", path.display()),
            )
        })?;
        env.push((config_env.to_string(), path_text.to_string()));
    }
    Ok(CliRouteLaunch {
        env,
        args: route.args.clone(),
    })
}
