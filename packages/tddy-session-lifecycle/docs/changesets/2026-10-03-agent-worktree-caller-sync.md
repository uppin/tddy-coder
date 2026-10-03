# 2026-10-03 — The `Sync` op, and the host bridge bound to its jail's session

**Type:** Feature

PR [#576](https://github.com/uppin/tddy-coder/pull/576). Cross-package entry:
[2026-10-03-agent-worktree-caller-sync.md](../../../../docs/dev/changesets/2026-10-03-agent-worktree-caller-sync.md).

- `run_conversation_worktree_op` serves `Sync`: `{"sync": {…}}`, `{"sync": null}` (no worktree,
  nothing new, or a merge that changed no file), `{"conflicts": [≤ SYNC_NOTICE_PATHS], "moreConflicts": n}`
  (`conflicts_answer`); `info` logs on merge and conflict. One helper per op.
- Host bridge bound per jail: `BoundJailSession { session_id, session_dir }` on each jail's
  `DaemonRpcHandler`, built by `sandbox_rpc_handler(session_id, session_dir)` in
  `svc_start_sandboxed_cursor_cli_session.rs`, `jail_launch_steps.rs` and `relaunch_jail_steps.rs`;
  `install_sandbox_rpc_bridge` keeps a `Weak<DaemonSessionHost>`. `conversation_worktree_from_jail`
  answers `PermissionDenied` for another session and resolves the worktree through
  `workspace_session::resolve_worktree_root_in_session_dir` (shared with
  `resolve_worktree_root_for_session`).

Resolves the backlog entry *A jail can name another session's conversation worktree over the host
bridge* (`2026-10-01-a-jail-can-name-another-sessions-conversation-worktree-over-the-host-bridge`).
`complexity-daemon-rpc-handler-handle-rpc`: 182 → 185 lines, nesting 8 — row recorded.
