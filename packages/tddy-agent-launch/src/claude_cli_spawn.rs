use std::path::{Path, PathBuf};

use tddy_cli_sessions::cli_session_manager::CliSessionManager;
use tddy_core::output::SESSIONS_SUBDIR;
use tddy_core::provider_route::RouteSources;
use tddy_rpc::Status;

use crate::session_acting_identity::SessionAccountAccess;
use crate::session_provider_route::{start_route, RouteAgent, StartRoute};

/// Record `session_id`'s running `main` terminal in its session directory under `tddy_data_dir`, so
/// its history outlives the process: rewritten while it runs, and once more at its exit.
pub(crate) async fn record_main_terminal(
    manager: &CliSessionManager,
    tddy_data_dir: &Path,
    session_id: &str,
) {
    let path = tddy_cli_sessions::cli_session_manager::main_capture_recording_path(
        tddy_data_dir,
        session_id,
    );
    manager.record_main_capture(session_id, path).await;
}

/// The session's provider route and its directory under `sessions_base/sessions/<id>/`, with the
/// route's account recorded there. The route is resolved **before** the directory is created: a
/// locked vault or an unusable account refuses the start with nothing on disk, and a chosen account
/// never falls back to the vendor.
// TODO(cli-dispatch-collapse): no production caller since the bespoke claude start went — only
// `session_provider_route_tests` reach it. Unplanned deletion: see
// docs/dev/todo/2026-11-05-callerless-bespoke-launch-helpers.md.
#[allow(dead_code)]
pub(crate) fn create_routed_session_dir(
    sessions_base: &Path,
    session_id: &str,
    account_access: &SessionAccountAccess,
    route_sources: &RouteSources,
    model: &str,
    agent: RouteAgent<'_>,
) -> Result<(PathBuf, Option<StartRoute>), Status> {
    let provider_route = start_route(account_access, agent, route_sources, model)?;
    let session_dir = sessions_base.join(SESSIONS_SUBDIR).join(session_id);
    std::fs::create_dir_all(&session_dir)
        .map_err(|e| Status::internal(format!("failed to create session dir: {}", e)))?;
    if let Some(route) = &provider_route {
        route.record_in(&session_dir)?;
    }
    Ok((session_dir, provider_route))
}
