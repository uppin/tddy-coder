//! Applying a provider route to a CLI harness launch: its files written owner-only in the session
//! directory, the harness's configuration variable pointed at them, and its environment and
//! arguments collected for the PTY.
//!
//! What a route needs of its agent is the spec's `coding_agent.route` section
//! (`tddy-coding-agent-specs`), and the route's body is chosen by that same spec's support
//! (`routed_agent`).

use std::io;
use std::path::Path;

use tddy_coding_agent_specs::{routed_agent, CodingAgentSpec};
use tddy_core::atomic_file::write_atomic_with_mode;
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

/// The variable the agent's `coding_agent.route.config_env` names its configuration file in, if any.
fn config_env_of(spec: &CodingAgentSpec) -> Option<&str> {
    spec.agent
        .route
        .as_ref()
        .and_then(|route| route.config_env.as_deref())
}

/// The model a launch of `spec` names on its own command line. A config-file route (OpenCode's)
/// names the model itself, on the provider it generates, so a `routed` launch leaves it blank; an
/// environment-only route (Muse Code's) hands over the key and nothing else, so the session keeps
/// its own model. An unrouted launch always keeps it.
pub(crate) fn launch_model_for<'a>(
    spec: &CodingAgentSpec,
    routed: bool,
    model: &'a str,
) -> &'a str {
    if routed && config_env_of(spec).is_some() {
        ""
    } else {
        model
    }
}

/// The model a launch of `spec` on a route whose arguments are `route_args` names on its own command line: blank when the route
/// names the model itself in its arguments (`--model` or `-m`) — an agent whose argv already
/// renders `--model {model}` would otherwise be given two — and [`launch_model_for`]'s rule
/// otherwise.
pub(crate) fn routed_launch_model<'a>(
    spec: &CodingAgentSpec,
    route_args: Option<&[String]>,
    model: &'a str,
) -> &'a str {
    let route_names_the_model =
        route_args.is_some_and(|args| args.iter().any(|arg| arg == "--model" || arg == "-m"));
    if route_names_the_model {
        return "";
    }
    launch_model_for(spec, route_args.is_some(), model)
}

/// Write `route`'s files into `session_dir` (mode 0600) and collect what the launch needs. An
/// unrouted launch (`None`) adds nothing and writes nothing.
pub fn cli_route_launch(
    spec: &CodingAgentSpec,
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
    if let (Some(config_env), Some(config)) = (config_env_of(spec), route.files.first()) {
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

/// The launch a start of the `spec` agent on `model` is given on the account
/// `sources` choose, resolved before anything is spawned. Writes the route's files into `session_dir`
/// and records the account and model beside them (never the key), so a resume re-applies it. `None`
/// — and nothing written — when no account was chosen, so the harness runs on the person's own setup.
pub fn start_cli_route(
    access: &SessionAccountAccess,
    spec: &CodingAgentSpec,
    sources: &RouteSources,
    model: &str,
    session_dir: &Path,
) -> Result<Option<CliRouteLaunch>, Status> {
    let Some(account_id) = choose_route(sources) else {
        return Ok(None);
    };
    let route = route_on_account(access, &routed_agent(spec), &account_id, model)?;
    let launch = write_cli_route(spec, &route, session_dir)?;
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

/// The launch a resumed session of the `spec` agent is given: the route its
/// start recorded, re-resolved against the owner's vault as it stands now, its files written again.
/// Nothing for a session on the person's own setup.
pub fn resume_cli_route(
    access: &SessionAccountAccess,
    spec: &CodingAgentSpec,
    session_dir: &Path,
) -> Result<CliRouteLaunch, Status> {
    let (sources, model) = route_sources_on_resume(session_dir)?;
    let Some(model) = model else {
        return Ok(CliRouteLaunch::default());
    };
    match resolve_session_route(access, &routed_agent(spec), &sources, &model)? {
        Some(route) => write_cli_route(spec, &route, session_dir),
        None => Ok(CliRouteLaunch::default()),
    }
}

fn write_cli_route(
    spec: &CodingAgentSpec,
    route: &HarnessRoute,
    session_dir: &Path,
) -> Result<CliRouteLaunch, Status> {
    cli_route_launch(spec, Some(route), session_dir)
        .map_err(|e| Status::internal(format!("failed to write the provider route: {e}")))
}
