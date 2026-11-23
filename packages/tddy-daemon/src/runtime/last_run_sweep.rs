use crate::runtime::*;

/// Remove the headless conversation files this daemon's previous run left behind. A failure is
/// logged, not fatal: the files are a previous run's, and nothing this run serves depends on them.
fn sweep_headless_files(
    config: &tddy_daemon_kernel::config::DaemonConfig,
    tddy_data_dir: &std::path::Path,
) {
    match tddy_session_lifecycle::coding_agent_launcher::sweep_headless_files_left_by_this_daemon(
        config,
        tddy_data_dir,
    ) {
        Ok(swept) if swept.is_empty() => {}
        Ok(swept) => log::info!(
            "removed the headless files of {} conversation(s) a previous run left: {}",
            swept.len(),
            swept.join(", ")
        ),
        Err(e) => log::warn!(
            "could not sweep the headless conversation files under {}: {e}",
            tddy_data_dir.display()
        ),
    }
}

/// What earlier runs left that this one must not serve: headless conversation files (their MCP
/// configs carry agent credentials), and the project stack records a work queue replaced
/// (`#workqueue` 5/14) — deleted, not migrated; idempotent, self-logging.
pub(super) fn sweep_what_the_last_run_left(config: &DaemonConfig, tddy_data_dir: &std::path::Path) {
    sweep_headless_files(config, tddy_data_dir);
    match tddy_core::session_work_queue::drop_old_project_stack_records(
        &tddy_data_dir.join("projects"),
    ) {
        Ok(0) => {}
        Ok(dropped) => log::info!("dropped the old stack records of {dropped} project(s)"),
        Err(e) => log::error!("could not drop the old project stack records: {e}"),
    }
}
