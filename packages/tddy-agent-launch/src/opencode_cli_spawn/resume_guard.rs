//! What keeps a resume to one agent per session, and what it says in the daemon log.
//!
//! A resume claims its session's main launch before it does anything else and holds the claim
//! until it returns: a second resume in the meantime — over a Coder target a resume takes minutes,
//! long enough for an operator to click Resume again — or a resume of a session whose agent is
//! up would launch another agent on the same worktree, and its registration would orphan the first.
//! It also logs when it begins and when its agent is up, so a long target resume is not silent.

use tddy_cli_sessions::cli_session_manager::{
    CliSessionManager, MainLaunchClaim, MainLaunchRefusal,
};
use tddy_rpc::Status;

/// Claim `session_id`'s main launch for a resume, refused when another resume of it is in flight
/// or its agent is still running.
pub(super) async fn claim_the_main_launch(
    cli_manager: &CliSessionManager,
    session_id: &str,
) -> Result<MainLaunchClaim, Status> {
    cli_manager
        .claim_main_launch(session_id)
        .await
        .map_err(|refusal| {
            Status::failed_precondition(match refusal {
                MainLaunchRefusal::Launching => format!(
                    "session {session_id} is already being resumed: wait for that resume to finish"
                ),
                MainLaunchRefusal::Running => {
                    format!("session {session_id} is already running: stop it before resuming it")
                }
            })
        })
}

/// Log that `session_id`'s resume has begun, on `host` when it runs on a target.
pub(super) fn log_resuming(kind: &str, session_id: &str, host: Option<&str>) {
    log::info!(
        "resuming {kind} session {session_id} (target host: {})",
        host.unwrap_or("none")
    );
}

/// Log that `session_id`'s resumed agent is up as `pid`, on `host` when it runs on a target.
pub(super) fn log_resumed(kind: &str, session_id: &str, host: Option<&str>, pid: u32) {
    log::info!(
        "resumed {kind} session {session_id} (target host: {}, pid {pid})",
        host.unwrap_or("none")
    );
}
