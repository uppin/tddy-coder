use std::path::Path;

use tddy_cli_sessions::cli_session_manager::CliSessionManager;

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
