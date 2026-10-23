# 2026-10-23 — Project-home idle reaper (#project-home 6/7)

New `project_home_idle` module: `IdleObservation::from_metadata` (busy = `activity_status == "Running"`, idle since `updated_at`), the pure `should_stop` policy, and `LaunchSessions::reap_idle_project_home_sessions(now)`, which stops idle active `project-home` sessions through `CliSessionManager::stop_session` and waits up to 5 s for the pid to exit. Feature: [project-home auto-stop](../../../../docs/ft/daemon/project-home-auto-stop.md).
