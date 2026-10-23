# 2026-10-23 — Project-home sessions stop once idle

A project-home session whose agent has been idle (no turn running, no new input) for
`project_home.idle_stop_secs` (default 600, `0` = off) is stopped by the daemon and resumes, same
conversation, on its next open. Other sessions are never auto-stopped. See
[project-home auto-stop](../project-home-auto-stop.md).
