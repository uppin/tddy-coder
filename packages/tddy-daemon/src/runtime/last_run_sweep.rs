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
/// configs carry agent credentials), and PR stacks still in a `changeset.yaml`, moved into their
/// projects' store — idempotent, self-logging; a session it cannot move lists no stack plan.
pub(super) fn sweep_what_the_last_run_left(config: &DaemonConfig, tddy_data_dir: &std::path::Path) {
    sweep_headless_files(config, tddy_data_dir);
    tddy_pr_stack::stack_migration::migrate_session_stacks(
        tddy_data_dir,
        &tddy_data_dir.join("projects"),
    );
}
