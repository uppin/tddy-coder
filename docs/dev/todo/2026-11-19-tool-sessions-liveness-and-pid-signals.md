# 2026-11-19 — `tool` sessions list inactive and are still signalled by their recorded pid

**Category:** Bug risk (deferred to a planned stack node)
**Source:** [2026-11-19-session-liveness-from-agent-tabs.md](../1-WIP/2026-11-19-session-liveness-from-agent-tabs.md) (`#session-liveness` 1/4)

- Session liveness now comes from the live agent tabs the daemon holds. A `tool` (tddy-coder)
  session has none: its child is held by `tddy-supervisor`, the forked spawn worker or a detached
  `child.wait()` thread (`packages/tddy-spawn/src/spawner.rs`), and the daemon is handed only
  `SpawnResult.pid`. So a **running tool session lists inactive**.
- Delete, SignalSession and TerminateSession still reach a tool session through the pid recorded in
  its `.session.yaml` — the only handle the daemon has — so the recycled-pid hazard the change fixed
  for every other kind remains for tool sessions: a delete or signal can reach an unrelated process.
- **Why deferred:** making a tool session a tab in the same registry needs an exit signal from each
  of the three spawn backends (the supervisor's session state, a new exit message from the spawn
  worker, a channel from the direct spawner). Developer decision 2026-11-19: its own stack node.
- **What closes it:** `#session-liveness` 3/4 — register the tool child as a process tab of its
  session, live until its backend reports the exit; delete and signal go through it; remove the
  `session_type == "tool"` carve-out in delete / SignalSession. Test: a running tool session lists
  active, and deleting one whose recorded pid names an unrelated process leaves that process running.
