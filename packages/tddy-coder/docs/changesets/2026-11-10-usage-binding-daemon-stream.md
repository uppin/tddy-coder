# 2026-11-10 — tddy-coder: the session usage watcher removed

`run_daemon` no longer spawns the per-session usage watcher (`SESSION_USAGE_POLL_INTERVAL`,
`resolve_agent_home_dir` gone with it), and `tddy-usage` is no longer a dependency: the daemon streams
every session's usage (`ActivityService.StreamSessionUsage`). `#usage-binding 2/5`, PR
[#891](https://github.com/uppin/tddy-coder/pull/891).
