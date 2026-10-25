# Project-home agent waits for its project's notifications - PRD

**Date**: 2026-10-25
**PRD Type**: Enhancement
**Stack**: `#home-inbox 7/8` (branch `feature/home-inbox/home-wait`)

## Affected Features

- **Primary**: [Project concept § Project-home tools](../project-concept.md) - `WaitProjectNotifications` and `ReadProjectNotifications`
- **Related**: [Session notifications](../session-notifications.md) - a durable, per-project log of attention notifications, addressed by cursor
- **Related**: [Project-home auto-stop](../project-home-auto-stop.md) - a home agent waiting in a tool call is busy, not idle

## Summary

The project-home agent learns what happened in its project without polling:

```
WaitProjectNotifications(cursor?, timeout_ms?: ≤ 20000, daemon_instance_id?)
  → { items: [...], next_cursor, has_more, timed_out }
ReadProjectNotifications(cursor?, limit?: ≤ 200, daemon_instance_id?)
  → { items: [...], next_cursor, has_more, clamped_to?, lost? }
```

- **Events** are the project's attention notifications:
  - agent-written (`1/8`);
  - finished;
  - needs input;
  - pending questions (`3/8`), with their content and id, so `AnswerSessionPermission` (`6/8`) and `AnswerSessionQuestion` (`8/8`) can act
    on them.
- **The cursor** is opaque. Passing it back returns only what came after it.

## Background

The bus is live-only. That is right for the drawer, but it loses everything a home agent wasn't
connected for. dev-3.0's coordinator learns progress through `dev3 events --from <cursor>`, and
the lesson it encodes is to read events before composing any status, never poll on a timer, and never
treat a failed read as a quiet board.

## Proposed Changes

### What's Changing

- **A project notification log.** A bus subscriber appends every `ATTENTION_REQUIRED` notification
  of a project's sessions to that project's log on this daemon.
  - Each record carries a monotonic sequence number.
  - The log is bounded to the newest 1,000 records per project. Older records are evicted and
    counted.
- **`WaitProjectNotifications`** returns at once if records newer than `from` exist. Otherwise it
  holds until one arrives or `timeout_ms` passes.
  - `timeout_ms` is capped at 20,000, the remote-call clamp.
  - Without `from`, it waits for the next new record.
- **`ReadProjectNotifications`** returns up to `limit` (default 50) records after `from`, oldest
  first, plus the cursor to continue from.
  - Without `from`, it returns the last 50.
  - A cursor older than the log's window is reported (`lost: N`). The read is never
    silently trimmed.
- **Scope.** Records exclude the home session's own notifications and are scoped to the caller's
  operating-system user.
- **Tool guidance.** The tool descriptions say: wait with the last cursor; a timeout means nothing
  happened; never wait in a loop with nothing to decide.

### What's Staying the Same

- `StreamSessionNotifications` stays live-only.
- The drawer and Telegram.

## Impact Analysis

- **Technical.**
  - One bus subscriber.
  - A per-project ring, persisted as JSON lines under the project's data directory and replayed into
    memory when the daemon starts.
  - Two tools.
- **User.** The home agent reacts to a child finishing or asking within seconds, with no polling
  loop.

## Acceptance Criteria

- [ ] A notification published for a child after the wait began returns that wait with the event
  and a new cursor.
- [ ] A wait with no event returns `timed_out` after its timeout. A timeout above 20,000 ms is
  capped.
- [ ] Waiting from a returned cursor never repeats an event and never skips one, across a daemon
  restart.
- [ ] Records of another project, of another operating-system user, and of the home session itself
  never appear.
- [ ] `ReadProjectNotifications` from an evicted cursor reports how many records were lost.
- [ ] A pending question event carries its id and content.

## Out of scope

- Activity events.
- A merged view across hosts. Each call reads one host, which is this one unless `daemon_instance_id` names another.
- A push into the home agent's next turn (a follow-up).

## Changes from the 2026-10-25 plan

Replan of 2026-10-26: `master` moved, and this stack now sits on `#start-parity 9/9` (#758).

- **Paging.** Paging follows `ReadSessionActivity` (`#start-parity 6/9`): `cursor`, `limit` (default
  50, maximum 200, `clamped_to` when clamped), `items`, `next_cursor`, `has_more`. `older_than_window`
  is now `lost`.
- **Hosts.** Both tools take an optional `daemon_instance_id` and are forwarded to that host. This
  replaces "this host only".
