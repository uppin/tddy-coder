# An idle orchestrator is woken by what it watches — PRD

**Date**: 2026-11-16
**PRD Type**: Enhancement
**Stack**: `#activity-wake` 3/3 — branch `feature/activity-wake/idle-wake`, base `feature/activity-wake/session-watches`

## Affected Features

- **Primary Feature**: [Project concept](../project-concept.md) — a watching orchestrator that is idle
  is sent a turn carrying what its watches matched.
- **Related Feature**: [Project-home auto-stop](../project-home-auto-stop.md) — a project-home main
  session that holds a live watch is not auto-stopped.
- **Related Feature**: [Session notifications](../session-notifications.md) — the wake reads the
  orchestrator's own reported status; nothing new is published.

## Summary

The predecessor (`feature/activity-wake/session-watches`) delivers matched events on the
orchestrator's next tool answer, which only helps an orchestrator that is mid-turn. This node covers
the idle one: when an orchestrator with live watches is **idle** and matched events are waiting, the
daemon **sends it one turn** — the same terminal paste `SendSessionMessage` makes — listing the
events, which are thereby delivered (and so never ride on a later answer as well). Bursts are
coalesced into one turn after a short settle, wakes are spaced by a minimum gap, and a main session
holding a live watch is not auto-stopped while it waits.

## Background

An orchestrator that finished its turn sits at its prompt. Nothing it watches can reach it until a
human types: a child's permission prompt waits for an operator who delegated exactly that to the
orchestrator. The project-home main session is also auto-stopped after ten idle minutes — which, for
an orchestrator waiting to be woken, would end its watches with it. The developer chose that **a live
watch holds auto-stop off** (2026-11-16).

## Proposed Changes

### What's Changing

- **Idle, for a wake.** An orchestrator is idle when its main terminal is live and its reported
  status is `Done`, `WaitingForInput` with no pending question of its own, or `Started` / `Ended` (the
  latter is what `/clear` writes). It is busy while `Running` or `ExecutingTool`. An orchestrator
  waiting on its own permission prompt or question is **not** woken — a paste would answer that
  prompt — and its events wait.
- **The wake.** When matched events are waiting for an idle orchestrator, the daemon pastes and
  submits one message into its main terminal:

  ```
  [tddy watch] 2 watched events (oldest first):
  - Session fix-login — permission: wants to run Bash: cargo test (request_id 3f2a…)
  - Session docs-pass — turn_finished: ✅ finished this turn
  Answer with AnswerSessionPermission / AnswerSessionQuestion, or read ReadProjectNotifications for detail.
  ```

  with `lost` and ended watches appended when there are any. The events in it count as delivered;
  they are not attached to a later tool answer. A paste that fails delivers nothing — the events stay
  waiting and the next chance retries.
- **Mid-turn.** A busy orchestrator is not woken; its next tool answer carries the events as before.
  When its turn ends with events still waiting (none of its tool calls took them), it is woken then.
- **Debounce.** A wake is made **2 s** after the first event that triggers it (so a burst is one
  turn), and never sooner than **30 s** after the orchestrator's previous wake; events arriving inside
  the gap ride on the next wake, or on a tool answer if the orchestrator is woken by something else
  first.
- **No self-wake.** The orchestrator's own events are never watched (predecessor), so it is never
  woken by itself; and a wake is a turn its own agent makes, which raises no event it watches.
- **Auto-stop.** The idle reaper does not stop a project-home session that holds a live watch. When
  its last watch ends (expiry, cancel), the ordinary idle clock applies again.

### What's Staying the Same

- Watches, their tools, filters, endings and the `watchEvents` answer key — the predecessor's.
- A stopped orchestrator is not started by a wake: its agent ending already ended its watches.
- `SendSessionMessage` keeps its behaviour and refusals.
- Orchestrator-role sessions were never auto-stopped and still are not.
- Only claude-cli can be woken (the only agent with `turn_input: terminal_paste`); creation already
  refuses the others.

## Impact Analysis

### Technical Impact

- `tddy-daemon-rpc`: a waker — a bus subscriber that only records "look at project P / owner O" and
  returns, and a task that settles, decides idle, takes the delivery and pastes it through the same
  path `SendSessionMessage` uses.
- `tddy-session-activity`: a delivery that is committed only once its paste succeeds (an addition to
  the watch store).
- `tddy-agent-launch`: the idle reaper's observation gains "holds a live watch".
- `tddy-daemon`: starts the waker and hands the reaper the store.
- Cost: one status read per triggered owner per settle; no polling of idle orchestrators.

### User Impact

- An operator who tells the main session "watch the sessions you start and answer their permission
  prompts" sees it do so while it would otherwise sit idle.
- A watching main session stays up past its idle timeout; its watch's TTL (≤ 24 h) bounds that.

## Implementation Plan

1. The committed-on-success delivery in the store.
2. The idle decision, unit-tested over session records.
3. The waker: trigger, settle, gap, paste; acceptance-tested with a recording terminal.
4. The reaper's hold, unit- and acceptance-tested.

## Acceptance Criteria

- [ ] An idle orchestrator watching a session is sent one turn naming that session's permission
      prompt and its `request_id` ([Project concept](../project-concept.md))
- [ ] A busy orchestrator is not woken; when its turn ends with the event still waiting, it is
- [ ] An orchestrator waiting on its own prompt is not woken
- [ ] Three events within the settle make one turn; an event inside the gap waits for the gap
- [ ] An event delivered by a wake is not attached to a later tool answer; a failed paste delivers
      nothing
- [ ] A project-home main holding a live watch is not auto-stopped; once its watch ends, it is
      ([Project-home auto-stop](../project-home-auto-stop.md))
- [ ] The touched packages' tests pass (`./test -p <pkg>` per package)

## References

### Affected Features (Complete List)

- [Project concept](../project-concept.md) — the wake turn
- [Project-home auto-stop](../project-home-auto-stop.md) — the live-watch hold
- [Session notifications](../session-notifications.md) — referenced

### Related Documentation

- Predecessors: `feature/activity-wake/session-ended-notice` (#929), `feature/activity-wake/session-watches` (#930)
- `docs/dev/todo/2026-10-23-project-home-running-heuristic.md` — liveness from the served terminal, not the pid
