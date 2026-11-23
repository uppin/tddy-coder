# 2026-11-22 — A session's inspector shows its PR and its worktree's sync state

**Type:** Feature · **Stack:** `#workqueue` 4/14 (PR #1005)

- **Proto** (`tddy-service`): `SessionEntry.pr_number` (42) and `pr_state` (43). On
  `WorktreeService`: `GetWorktreeSyncState`, `PullWorktree` and `PushWorktree`
  (`SessionWorktreeRequest` → `WorktreeSyncState`), and `StreamSessionPullRequest`
  (→ stream `SessionPullRequestView`). The four are registered in `unbundle_service_split.rs`'s
  `WORKTREE_METHODS`.
- **`tddy-worktree-service`**: `worktree_sync` (sync state by ancestry both ways, ff-only pull, plain
  push) and `pull_request_view` (the view read from n2's inbox rows, and a watch that re-reads on the
  events that concern it). The handlers live in the child module `service/inspector.rs`.
- **`tddy-session-lifecycle`**: `DaemonSessionHost` implements `SessionPullRequests` over the trunk
  watch's inbox and `HostTrunkSource`. **`tddy-agent-launch`**: session listing fills the PR fields
  from the inbox cache. **`tddy-daemon`**: `attach_pull_requests` wiring.
- **Web**: `SessionPullRequestTab`, `CheckRow`, `CommentThreads` and `CommitList`, plus
  `WorktreeSyncSection` / `useWorktreeSyncState` (`tddy-workspace-web`). The `pr` tab name
  (`tddy-client-web`). `defaultInspectorTab` (PR, else Worktree) wired into the drawer
  (`tddy-session-web`).

Deferred: a target session's sync state and PR view —
[`docs/dev/todo/2026-11-22-target-session-inspector-has-no-sync-state-or-pr.md`](../todo/2026-11-22-target-session-inspector-has-no-sync-state-or-pr.md).
`tddy-worktree-service/src/service.rs` measured 831 → 897 production lines; the record is
`packages/tddy-worktree-service/docs/code-issues/oversized-file-service.md`.

Feature docs: [session-pr-inspector.md](../../ft/web/session-pr-inspector.md),
[session-worktree-inspector.md](../../ft/web/session-worktree-inspector.md).
