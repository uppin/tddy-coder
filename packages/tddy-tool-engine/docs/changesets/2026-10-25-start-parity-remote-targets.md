# 2026-10-25 — Target host argument and cross-host listing types

**Type:** Feature

`#start-parity` 1/9 ([#750](https://github.com/uppin/tddy-coder/pull/750)). `project_home_tools`:
`ListedSession.daemon_instance_id`, `ListProjectSessionsResult { sessions, unreachable_hosts }`,
`UnreachableHost`, `TARGET_DAEMON_INSTANCE_ID`, `target_daemon_instance_id`,
`is_project_home_target_tool`, and `ProjectHomePort::list_peer_sessions`, merged into the listing.
Every target tool's schema (read, diff, the seven git tools) offers the optional host.
`project_home_tools.rs` is at 500 production lines — recorded in
`docs/code-issues/oversized-file-project-home-tools.md`, split deferred until the stack lands.

Tests: `tests/project_home_target_host.rs`.
