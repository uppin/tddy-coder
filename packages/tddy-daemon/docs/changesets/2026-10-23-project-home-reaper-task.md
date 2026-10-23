# 2026-10-23 — Project-home reaper task (#project-home 6/7)

`RuntimeTasks::project_home_reaper` holds the session host (set in `mount_host.rs`); a task spawned in `runtime/tasks.rs` calls `DaemonSessionHost::reap_idle_project_home_sessions` every 30 s (`PROJECT_HOME_REAP_PERIOD`).
