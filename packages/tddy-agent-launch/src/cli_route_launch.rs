//! Applying a provider route to a CLI harness launch: its files written owner-only in the session
//! directory, the harness's configuration variable pointed at them, and its environment and
//! arguments collected for the PTY.

use std::io;
use std::path::Path;

use tddy_core::atomic_file::write_atomic_with_mode;
use tddy_core::cli_harness::CliHarness;
use tddy_core::provider_route::{choose_route, HarnessRoute, RouteSources};
use tddy_core::{write_session_provider_account, SessionProviderAccount};
use tddy_rpc::Status;

use crate::session_acting_identity::SessionAccountAccess;
use crate::session_provider_route::{
    pty_route_env, resolve_session_route, route_on_account, route_sources_on_resume,
};

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

/// The model a launch of `harness` names on its own command line. A config-file route (OpenCode's)
/// names the model itself, on the provider it generates, so a `routed` launch leaves it blank; an
/// environment-only route (Muse Code's) hands over the key and nothing else, so the session keeps
/// its own model. An unrouted launch always keeps it.
pub(crate) fn launch_model_for<'a>(harness: &CliHarness, routed: bool, model: &'a str) -> &'a str {
    if routed && harness.route_config_env.is_some() {
        ""
    } else {
        model
    }
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

/// The launch a start of `harness` on `model` is given on the account `sources` choose, resolved
/// before anything is spawned. Writes the route's files into `session_dir` and records the account
/// and model beside them (never the key), so a resume re-applies it. `None` — and nothing written —
/// when no account was chosen, so the harness runs on the person's own setup.
pub fn start_cli_route(
    access: &SessionAccountAccess,
    harness: &CliHarness,
    sources: &RouteSources,
    model: &str,
    session_dir: &Path,
) -> Result<Option<CliRouteLaunch>, Status> {
    let Some(account_id) = choose_route(sources) else {
        return Ok(None);
    };
    let route = route_on_account(access, harness.route_harness, &account_id, model)?;
    let launch = write_cli_route(harness, &route, session_dir)?;
    write_session_provider_account(
        session_dir,
        &SessionProviderAccount {
            account_id,
            model: model.to_string(),
        },
    )
    .map_err(|e| Status::internal(format!("failed to record the provider account: {e}")))?;
    Ok(Some(launch))
}

/// The launch a resumed session of `harness` is given: the route its start recorded, re-resolved
/// against the owner's vault as it stands now, its files written again. Nothing for a session on
/// the person's own setup.
pub fn resume_cli_route(
    access: &SessionAccountAccess,
    harness: &CliHarness,
    session_dir: &Path,
) -> Result<CliRouteLaunch, Status> {
    let (sources, model) = route_sources_on_resume(session_dir)?;
    let Some(model) = model else {
        return Ok(CliRouteLaunch::default());
    };
    match resolve_session_route(access, harness.route_harness, &sources, &model)? {
        Some(route) => write_cli_route(harness, &route, session_dir),
        None => Ok(CliRouteLaunch::default()),
    }
}

fn write_cli_route(
    harness: &CliHarness,
    route: &HarnessRoute,
    session_dir: &Path,
) -> Result<CliRouteLaunch, Status> {
    cli_route_launch(harness, Some(route), session_dir)
        .map_err(|e| Status::internal(format!("failed to write the provider route: {e}")))
}
