# 2026-10-03 — Sync and host-bridge binding suites

**Type:** Feature

PR [#576](https://github.com/uppin/tddy-coder/pull/576). Cross-package entry:
[2026-10-03-agent-worktree-caller-sync.md](../../../../docs/dev/changesets/2026-10-03-agent-worktree-caller-sync.md).

Tests only. `tests/conversation_worktree_sync_acceptance.rs` (5): a sync merges the session
worktree's edit, merges nothing when it is unchanged or there is no worktree, names conflicts and
moves nothing, and a diff to the tip right after a sync includes the caller's changes.
`conversation_worktree_host_bridge_acceptance::a_relayed_call_naming_another_session_is_refused`
(`PermissionDenied`, nothing pulled); the bridge suites build their handler with
`sandbox_rpc_handler(session_id, session_dir)`.
