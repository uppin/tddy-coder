# 2026-11-24 — A session's stop time is not recorded

**Category:** Future enhancement
**Source:** `fix/session-drawer-fold-and-sort` ([changeset](../../../packages/tddy-session-web/docs/changesets/2026-11-24-session-drawer-fold-and-sort.md))

The sessions drawer lists the sessions under **More** most recently touched first, by the later of
`SessionEntry.last_activity.at_unix_ms` and `SessionEntry.updated_at`
(`packages/tddy-session-web/src/utils/sessionSort.ts` `sessionLastTouchedMs`). The intent is
that the session which **stopped** most recently leads. That only holds when something bumps
`updated_at` at the moment a session stops, and today nothing reliably does.

## What the daemon does today

- `is_active` is not stored. Every `ListSessions` computes it from the in-memory `LiveSessions`
  snapshot (`packages/tddy-session-activity/src/session_reader.rs:96`,
  `live.contains(&metadata.session_id)`). A session goes inactive by leaving that set, and nothing
  writes `.session.yaml` when it does.
- `updated_at` is bumped only by `tddy_changeset::update_activity_status` (a hook-reported status
  such as claude-cli's SessionEnd `Ended`), `set_pending_elicitation` when the flag flips, and resume.
  So a claude-cli session whose SessionEnd hook fires gets roughly the right stop time. One that
  crashes, is killed with its tmux session, or runs an agent without hooks keeps the time of its last
  hook report.

## Where the stamp belongs

Bump `.session.yaml` `updated_at`, through a `tddy-changeset` helper next to
`update_activity_status`, when the session's main agent ends:

- `packages/tddy-cli-sessions/src/cli_session_manager/tmux_reattach.rs` `main_ended`, on
  `MainExit::AgentEnded`, which already drops the agent state and announces an `AgentEnd`; or
- a subscriber of that announcement: `agent_ends()`, consumed today by
  `tddy_session_lifecycle::session_notifications::forward_agent_ends`. A subscriber keeps disk
  writes out of the session manager, but a lagging broadcast receiver drops ends.

## Gap the stamp cannot close

A session that was live when the daemon stopped is never seen to end: after a restart it is simply
not in `LiveSessions`. Its stop time is unknown and it keeps its last `updated_at`. To close that
gap the daemon would have to stamp, at startup, every session that was live before the restart.

## Why deferred

The drawer fix is web-only. The operator chose to leave the daemon change for a follow-up rather
than widen the PR into `tddy-cli-sessions` and `tddy-changeset` (2026-11-24).
