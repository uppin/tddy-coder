# 2026-10-24 — a cancelled start waits out a worktree cut (or agent spawn) already running

**Category:** Future enhancement
**Source:** `#start-progress` 8/8 (`feature/start-progress/start-cancel`, PR #741)

- Cancelling a streamed start stops a running clone at once: the clone's `CloneHandle` kills git and
  the start ends `CANCELLED` (`packages/tddy-agent-launch/src/clone_stopper.rs`). Every step after the
  clone is refused for a cancelled token (`start_task::refuse_a_cancelled_start`).
- A cancel that lands **while** the worktree is being cut, or the agent spawned, does not interrupt
  that step. The start finishes it, then `StartTask` deletes the session it created through
  `delete_session_at_session_coordinate`. That stops the agent and removes the worktree and the
  session directory, and the stream ends `CANCELLED` (`packages/tddy-agent-launch/src/start_task.rs`,
  `undo_a_cancelled_start`; pinned by
  `cancelling_during_the_worktree_cut_leaves_no_session_directory_or_worktree` in
  `packages/tddy-session-lifecycle/tests/start_cancel_acceptance.rs`). So nothing is left behind,
  but the operator waits for the cut, and on a large repository that can take a while.
- **Why deferred:** the session id is minted inside each session type's branch of
  `start_session_on_request` (workspace, claude-cli, cursor-cli, each CLI harness, the tool session),
  and the worktree is cut by `tddy-git`'s blocking `git worktree add` several calls below that. Killing
  that git child needs the cancellation token threaded through every branch's worktree helper into
  `tddy-git`, and that is a refactor of the start path. The function is already a 391-line
  complexity record (`packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-session-core-start-session-core.md`),
  and its early-return guards cannot be extracted by the engine (E4).
- **What would close it:** once `start_session_on_request` is split per session type, pass the
  token into the worktree helper. Run `git worktree add` as a child the token can kill, the way
  `tddy-spawn`'s clone runner hands back a `CloneHandle`, and remove the partial worktree
  (`git worktree remove --force` plus `prune`). Test it with a `post-checkout` hook that sleeps
  much longer than the test allows, and assert the stream ends `CANCELLED` well before the hook
  would have finished.
