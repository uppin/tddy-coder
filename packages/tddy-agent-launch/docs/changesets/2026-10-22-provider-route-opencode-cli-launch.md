# 2026-10-22 — opencode-cli start and resume

**Type:** Feature

`#provider-route` 3/9 ([#664](https://github.com/uppin/tddy-coder/pull/664)). `opencode_cli_spawn.rs` (start, `LaunchSessions::resume_opencode_cli`), `cli_route_launch.rs`
(a route applied to any CLI harness: `0600` files, `OPENCODE_CONFIG`, env and args;
`start_cli_route` / `resume_cli_route`), `cli_session_worktree.rs` (`provision_cli_worktree`,
`semantic_index_env`, shared with cursor-cli — `cursor_cli_spawn.rs` 562 → 418 production lines, its
backlog entry deleted), `LaunchSessions::start_opencode_cli` in `cli_branch_starts.rs` (sandbox:
`FAILED_PRECONDITION`, `TODO(provider-route-jail)`; managed codebase: `INVALID_ARGUMENT`).
`session_type_is_facilitated_here` and the empty-coordinates answer of `ConnectSession` read the
registry. See [module layout](../module-layout.md).
