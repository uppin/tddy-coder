# 2026-10-25 — a forwarded start's cancel can report "left in place" on the calling host

**Category:** Open question
**Source:** `#start-parity` 4/9 (`feature/start-parity/async-start`, PR #753)

- A start forwarded to another host also runs a `start_session` task on the calling host. When that
  caller-side task is cancelled after the forwarded start has produced its session,
  `undo_a_cancelled_start` (`packages/tddy-agent-launch/src/start_task.rs`) reads the cut branch
  from a session directory that lives on the **other** host. The read fails, and the caller-side
  stream ends `INTERNAL` with "the branch it cut could not be read, so its session … was left in
  place" — although the owning host undoes the start (session and branch) itself.
- The orchestrator's `CancelSessionStart` cancels the task on the host running it, so it does not
  hit this; the web UI's Cancel on the caller-side task can.
- **Why deferred:** the right shape is a caller-side undo that recognises a forwarded start and
  leaves the undo to its owner, which touches the forward path in the start core this PR's
  boundaries exclude.
- **Would close it:** the caller-side undo skips (or forwards) the branch read for a forwarded start,
  and a two-host acceptance test cancels the caller-side task and expects `CANCELLED`.
