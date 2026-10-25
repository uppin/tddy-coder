# 2026-10-25 — a cancelled start leaves the branch it pushed on origin

**Category:** Open question
**Source:** `#start-parity` 4/9 (`feature/start-parity/async-start`, PR #753)

- A cancelled streamed start now force-deletes the **local** branch it cut
  (`packages/tddy-agent-launch/src/start_cut_branch.rs`, called from `undo_a_cancelled_start` in
  `packages/tddy-agent-launch/src/start_task.rs`).
- A start with `create_remote_branch` has also pushed that branch to origin by the time the cancel
  lands. Nothing deletes it there, so the remote keeps a branch no session owns.
- **Why deferred:** deleting a remote branch is a push with the user's identity and is not
  reversible; whether a cancel should do it (always, never, or only when the start pushed it and
  nothing else moved it) is a developer decision that was not taken in this PR.
- **Would close it:** a decision, and if "delete", a `git push origin --delete <branch>` step in the
  cancel undo guarded by the ref still pointing at the commit the start pushed, pinned by an
  acceptance test in `packages/tddy-session-lifecycle/tests/start_cancel_acceptance.rs`.
