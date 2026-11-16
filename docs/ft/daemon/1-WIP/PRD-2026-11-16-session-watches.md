# An orchestrator watches the sessions it can see — PRD

**Date**: 2026-11-16
**PRD Type**: Enhancement
**Stack**: `#activity-wake` 2/3 — branch `feature/activity-wake/session-watches`, base `feature/activity-wake/session-ended-notice`

## Affected Features

- **Primary Feature**: [Project concept](../project-concept.md) — the project-home tools gain
  `CreateSessionWatch`, `ListSessionWatches`, `RenewSessionWatch`, `CancelSessionWatch`, and every
  tool answer an orchestrator receives can carry the events its watches matched.
- **Related Feature**: [Session notifications](../session-notifications.md) — watches read the
  project notification log; nothing new is published.
- **Related Feature**: [Project-home auto-stop](../project-home-auto-stop.md) — unchanged here; the
  successor node makes a live watch hold auto-stop off.

## Summary

An orchestrator (the project's main session, or a session with `session_role: orchestrator`) creates
a **watch**: a filter over the sessions it can already see and the kinds of event it cares about,
with a required time to live. From then on, **every tddy-tools answer it receives carries the events
its watches matched since the last answer**, each event delivered once. A watch ends when its TTL
runs out, when it is cancelled, when its orchestrator's agent ends, or when every session it names
has ended. Waking an idle orchestrator is the successor node's (`feature/activity-wake/idle-wake`).

## Background

An orchestrator learns what its sessions are doing only by asking — `WaitProjectNotifications` in a
loop, which holds its turn for up to 20 s at a time and costs a tool call per look. While it is busy
with other work, a session's permission prompt waits unseen until the orchestrator thinks to look.
A watch turns that into delivery: the event rides on whatever tool call the orchestrator makes next.

## Proposed Changes

### What's Changing

- **Four tools**, offered only to orchestrator sessions (the project-home tool role):
  - `CreateSessionWatch { session_ids?, started_by_me?, kinds?, ttl_seconds }` → the watch
    (`watch_id`, its filter, `expires_at_unix_ms`). At least one of `session_ids` / `started_by_me`.
    `kinds` defaults to all.
  - `ListSessionWatches {}` → the caller's live watches, each with its filter, expiry and how many
    matched events are waiting.
  - `RenewSessionWatch { watch_id, ttl_seconds }` → the new expiry.
  - `CancelSessionWatch { watch_id }` → the watch is gone; nothing more is delivered for it.
- **Kinds** — six, over the log's records:

  | Kind | Matches a record of source… |
  |---|---|
  | `question` | `pending_question` whose question is not a permission; `presenter` |
  | `permission` | `pending_question` whose question is a permission |
  | `turn_finished` | `activity_status` with status `Done` — the session finished its turn |
  | `waiting_for_input` | `activity_status` with status `WaitingForInput` and no question — it waits for its next prompt |
  | `agent_notification` | `agent` (the session's own `tddy-tools notify`) |
  | `ended` | `session_ended` (`#activity-wake` 1/3) |

  An `activity_status` record that carries a question is the same prompt its `pending_question`
  record already announced, so it matches nothing — an event is delivered once, never twice for one
  prompt.

  To tell `turn_finished` from `waiting_for_input`, an activity-status notification now carries the
  **status it was raised for** (`SessionNotification.activity_status`), and the project log keeps it
  (`"activity_status": "Done"`). Records written before this change carry none; they match neither
  of the two kinds (they still match every other kind they would).
- **Which sessions.** A record matches when its session is one of `session_ids`, or when
  `started_by_me` is set and the session's orchestrator is the caller. Visibility is exactly
  `ReadProjectNotifications`'s: the caller's project and OS user, on this host, never the caller's own
  session.
- **Delivery.** Every tool answer the orchestrator receives on this host — any tool, including the
  watch tools themselves — gains a `watchEvents` object when something is waiting:
  `{ "events": [<the inbox's record JSON>…], "lost": N, "ended": [{"watch_id", "reason"}…] }`.
  Events are oldest first, at most 50 per answer (the rest ride on the next one), each delivered
  **once** across all the caller's watches and across concurrent calls. `lost` counts matched
  events the log evicted before they could be delivered (it keeps 1000 per project). `ended` names
  the watches that ended since the last answer and why (`expired`, `orchestrator_ended`,
  `sessions_ended`). An error answer carries nothing, so nothing is consumed by it.
- **Lifetime.** `ttl_seconds` is required, 1 … 86 400 (24 h); renewing restarts it with the same
  bound. A watch ends at expiry; when cancelled; when the orchestrator's own agent ends (its
  `session_ended` record); when every session in `session_ids` has ended and `started_by_me` is
  off. A watch survives a daemon restart and the orchestrator's `/clear` (its session id stays).
- **Refusals**, each saying what to do:
  - a caller that is not an orchestrator;
  - an orchestrator whose agent declares no `turn_input` — naming the agent, worded as
    `SendSessionMessage` words it — since a watch promises a wake that agent cannot take;
  - `ttl_seconds` missing, zero or above 86 400;
  - neither `session_ids` nor `started_by_me`; an unknown kind;
  - a `session_ids` entry the caller cannot see (another project, another OS user, not on this host),
    or the caller's own session;
  - more than 16 live watches for one orchestrator;
  - renewing or cancelling a watch that is not the caller's, or has ended.

### What's Staying the Same

- `ReadProjectNotifications` / `WaitProjectNotifications` and their cursors: a watch has its own
  position in the log, and reading the inbox does not consume watch events, nor the reverse.
- No session other than an orchestrator sees, or is affected by, watches.
- Nothing is published; Telegram and the drawer are untouched.
- No wake: an orchestrator that makes no tool call receives nothing until it does
  (`#activity-wake` 3/3 adds the wake).
- The web shows no watches (a Future-enhancement TODO).

## Impact Analysis

### Technical Impact

- `tddy-daemon-kernel` / `tddy-session-activity`: `SessionNotification.activity_status`, set by
  `notification_for_activity_status`, kept in the log's JSON. Not on the stream proto — nothing on
  the web reads it. Every `SessionNotification` literal gains `activity_status: None` (about 25
  files, again).
- `tddy-session-activity`: a per-project watch store beside the notification log
  (`<data dir>/projects/<project_id>/session-watches.json`), matching records against filters,
  advancing each watch's position, ending watches.
- `tddy-tool-engine`: the four tool definitions and their port.
- `tddy-daemon-rpc`: the port's implementation, and the attachment of `watchEvents` to tool answers —
  as a function of its own around `execute_tool`, which is already over its length budget.
- `tddy-daemon`: opens the store and hands it to the RPC handler.
- Cost: one read of the project log's tail per tool answer of an orchestrator that has live
  watches; none for any other session.

### User Impact

- An orchestrator asked to "answer the permission prompts of the sessions you start" creates one
  watch and learns of each prompt on its next tool call, with the prompt's `request_id` and content.
- Not breaking: an answer gains a key only for a caller that created a watch.

## Implementation Plan

1. The watch store and the matcher, unit-tested over log records.
2. The four tools and their refusals, through the `tddy-daemon-rpc` project-home harness.
3. The attachment to tool answers, consumed once; restart survival.
4. Watch endings: expiry, cancel, the orchestrator's end, every named session's end.

## Acceptance Criteria

- [ ] An orchestrator creates a watch on a session it started; that session's permission prompt
      arrives in `watchEvents` of the orchestrator's next tool answer, with its `request_id`
      ([Project concept](../project-concept.md))
- [ ] Each matched event is delivered once — not again on the next answer, not twice to two
      concurrent calls, not twice for two watches matching it
- [ ] A finished turn matches `turn_finished` and not `waiting_for_input`, and the reverse
- [ ] A kind not in the filter, and a session not in it, deliver nothing; the caller's own
      session's events are never delivered
- [ ] `ListSessionWatches` shows live watches with their waiting count; `RenewSessionWatch` moves
      the expiry; `CancelSessionWatch` stops delivery
- [ ] A watch ends — and is reported once in `ended` — at expiry, when every named session ends, and
      when its orchestrator's agent ends
- [ ] A watch and its position survive a daemon restart
- [ ] Every refusal listed above is given, in words that say what to do
- [ ] A non-orchestrator session is not offered the tools and is refused if it calls them
- [ ] The touched packages' tests pass (`./test -p <pkg>` per package)

## References

### Affected Features (Complete List)

- [Project concept](../project-concept.md) — the watch tools and the `watchEvents` answer key
- [Session notifications](../session-notifications.md) — watches as a reader of the project log
- [Project-home auto-stop](../project-home-auto-stop.md) — referenced; changed by the successor

### Related Documentation

- Predecessor: `feature/activity-wake/session-ended-notice` (`#activity-wake` 1/3, #929)
- Successor: `feature/activity-wake/idle-wake` (`#activity-wake` 3/3)
- `docs/dev/todo/2026-10-29-spawner-read-scope-is-same-host-only.md` — the same one-host limit
