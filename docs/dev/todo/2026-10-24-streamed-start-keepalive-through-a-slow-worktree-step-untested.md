# 2026-10-24 — No test drives a streamed start's keep-alives through a slow worktree cut

**Category:** Deferred verification
**Source:** `#start-progress` 4/8 (split-stream), PR #737 — planned acceptance test
`a_streamed_start_sends_keep_alives_through_a_slow_worktree_step` (`packages/tddy-agent-launch/tests/start_keepalive.rs`), not written

A streamed start wraps its whole life in `with_activity_keepalive(.., STARTING_ACTIVITY, ..)`, so the
relay's 30 s idle timeout never fires on a live start. What is pinned:

- the keep-alive loop's timing, on a paused clock (`tests/start_keepalive.rs`, `with_keepalive` /
  `with_activity_keepalive`);
- keep-alives relayed across the split hop (`tddy-session-split/tests/split_room_hosting_port.rs`) and a
  forward that outlives the old 330 s budget while the peer keeps it alive
  (`tddy-session-lifecycle/tests/remote_managed_worktree_acceptance.rs`).

What is not: that `start_session_core` actually emits keep-alives *while* its worktree cut is slow.

**Why deferred:** the worktree cut is the free function `workspace_session::start_workspace_session`
running real `git` against a real repository. There is no injected port to slow it, and adding a
test-only delay branch is forbidden (no test-only production branches). The honest seam is a
worktree-cut port on `LaunchSessions` (production implementation: today's function), which is a
refactor of the start path beyond this node's Responsibility.

**What would close it:** introduce that port, script a cut that takes longer than the idle window on a
paused clock, and assert a `StartKeepAlive` every `START_KEEPALIVE_INTERVAL` until the worktree phase ends.
