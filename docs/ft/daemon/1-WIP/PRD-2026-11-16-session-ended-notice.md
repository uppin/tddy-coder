# A session whose agent ends says so — PRD

**Date**: 2026-11-16
**PRD Type**: Enhancement
**Stack**: `#activity-wake` 1/3 — branch `feature/activity-wake/session-ended-notice`, base `master`

## Affected Features

- **Primary Feature**: [Session notifications](../session-notifications.md) — a new notification,
  source `SESSION_ENDED`, raised when a session's agent process ends.
- **Related Feature**: [Project concept § waiting for notifications](../project-concept.md) — the
  project notification log, and so `ReadProjectNotifications` / `WaitProjectNotifications`, now
  carry the end of a project's sessions.
- **Related Feature**: [Telegram notifications](../telegram-notifications.md) — unchanged: the new
  source is not sent to a chat.

## Summary

When the agent process of a session's main terminal ends — it exited, crashed, or was stopped — the
daemon publishes one `ATTENTION_REQUIRED` notification with the new source `SESSION_ENDED`, whose text
names the session and how its agent ended (`exit code 0`, `exit code 3`, `stopped by the daemon`,
`exit code unknown`). Because it is an attention notification of a project's session, it lands in the
project notification log, so an orchestrator reading its inbox learns that a session it drives is gone.

## Background

An orchestrator drives other sessions. Today it can learn that one is waiting on a question or has
finished a turn, but not that its agent has **stopped existing**: `SessionEnd` maps to `Ended` and
raises nothing (`session_notifications.rs:267`), and a main PTY's exit is only logged
(`main exited: session=… outcome=…`). The orchestrator keeps waiting on a session that will never
speak again.

`Ended` in `.session.yaml` is not evidence of exit either: claude-cli's `/clear` writes it too
(conversation rotation), and the agent is alive. So the signal has to come from the process, not from
the hook.

This is the bottom node of `#activity-wake`, the stack that lets an orchestrator watch the sessions it
can see and be woken by what they do. Its successor (`feature/activity-wake/session-watches`) uses
this notification as the watchable kind `ended`, and to end a watch whose sessions have all ended.
It is useful on its own: every existing inbox reader gains it.

## Proposed Changes

### What's Changing

- **A session-ended notification.** When a session's **main** terminal ends and its **agent** has
  ended with it, one notification is published:
  - kind `ATTENTION_REQUIRED`, source `SESSION_ENDED` (new, in the bus model and in
    `activity.proto`'s `SessionNotificationSource`);
  - text `⏹ Session {label} ended: {outcome}` — outcome as the daemon log already words it
    (`exit code N`, `exit code unknown`, `stopped by the daemon`, a spawn failure's message);
  - a structured **`ended`** part beside the text: `outcome` (the same words) and `exit_code`
    (the agent's exit code when one was seen, absent otherwise) — in the bus model
    (`SessionNotification.ended: Option<SessionEndNotice>`), in `activity.proto`
    (`SessionEndNotice ended = 13` on `SessionNotificationEvent`) and in the project log's JSON
    (`"ended": {"outcome": "exit code 3", "exit_code": 3}`), so an orchestrator branches on the
    code rather than parsing prose. Every other source leaves it unset;
  - owner (`os_user`) and label resolved as every other notification's are.
- **When an agent counts as ended.** A PTY-run main's exit ends its agent. A tmux-run main's exit ends
  its agent only when the tmux session is gone; a tmux **client** that ends while its agent lives
  (detach, daemon restart) raises nothing, and the notification is raised later, when the detach
  watcher sees the tmux session go (outcome `exit code unknown`, since nothing saw the agent's exit).
- **Exactly once per agent end.** One main ending is one notification; a resumed session that ends
  again raises another.
- **The project notification log carries it** (it stores attention notifications), with
  `source: "session_ended"` in its JSON, so `ReadProjectNotifications` / `WaitProjectNotifications`
  return it to an orchestrator with visibility of the session.
- **The drawer is not lit by it.** The web's session drawer does not turn a session's dot yellow
  for a `SESSION_ENDED` event; an ended session already reads as stopped.

### What's Staying the Same

- `SessionEnd` → `Ended` still raises nothing, so `/clear` raises nothing.
- A tab (non-main terminal) ending raises nothing.
- Telegram sends nothing new: its subscriber takes the activity-status path, and this source is not
  on it.
- The log line `main exited: …` and the exit hooks run as they do now, before the notification.
- A split session's agent (`tddy-session-split`) is spawned by the same session manager on the host
  that runs it; nothing about split routing changes.

## Impact Analysis

### Technical Impact

- `tddy-cli-sessions`: the session manager announces each agent end (session id, OS user, outcome) on
  a channel the daemon subscribes to; both agent-end sites (the immediate `AgentEnded`, and the
  detach watcher's) announce.
- `tddy-daemon-kernel` / `tddy-session-activity`: the new source and the `SessionEndNotice` part;
  the log's JSON gains the `session_ended` source value and the `ended` object. Adding a field to
  `SessionNotification` touches every struct literal of it (about 25 files, most of them tests),
  each gaining `ended: None`.
- `tddy-session-lifecycle`: a forwarder that turns an announced agent end into a bus notification,
  with the label the drawer uses.
- `tddy-daemon`: starts the forwarder once the bus is assembled.
- `tddy-service` (`activity.proto`) and the generated web clients: one enum value and one message.
- `tddy-session-web`: the drawer ignores the new source for its dot.
- Performance: one notification per agent end; nothing on the hot path.

### User Impact

- An orchestrator's inbox shows `⏹ Session fix-login ended: exit code 1` when a session it can see
  ends.
- Operators see no new Telegram message and no new drawer colour.
- Not breaking: a client that does not know the enum value reads it as unknown and the event carries
  its text.

## Implementation Plan

1. The session manager announces agent ends (both sites), tested in `tddy-cli-sessions` with a real
   PTY child.
2. The new source through kernel, proto, conversions and the log's storage.
3. The forwarder in `tddy-session-lifecycle`; the daemon starts it.
4. Acceptance: a project-home caller's `ReadProjectNotifications` returns the end of a session it can
   see, in the `tddy-daemon-rpc` harness; a session that only `/clear`s raises nothing.
5. The drawer filter, with a component test.

## Acceptance Criteria

- [ ] A session whose main agent exits with code N raises one `SESSION_ENDED` attention notification
      whose text names the session and `exit code N` ([Session notifications](../session-notifications.md))
- [ ] The notification carries `ended { outcome, exit_code }` — on the bus, on the stream, and in the
      project log's JSON; `exit_code` is absent when no code was seen
- [ ] A session stopped by the daemon raises one, reading `stopped by the daemon`
- [ ] A tmux client ending while its agent lives raises nothing; the agent's end, seen by the detach
      watcher, raises one reading `exit code unknown`
- [ ] A tab's end, and a `SessionEnd` hook report (`/clear`), raise nothing
- [ ] `ReadProjectNotifications` returns the end, with `source: "session_ended"`, to a caller that
      can see the session, and not to one that cannot ([Project concept](../project-concept.md))
- [ ] Telegram sends nothing for it; the drawer dot does not turn yellow for it
- [ ] The touched packages' tests pass (`./test -p <pkg>` per package)

## References

### Affected Features (Complete List)

- [Session notifications](../session-notifications.md) — the new row in *What is notified*
- [Project concept](../project-concept.md) — the inbox now carries session ends
- [Telegram notifications](../telegram-notifications.md) — states it sends no session-end message

### Related Documentation

- Stack successor: `feature/activity-wake/session-watches` (`#activity-wake` 2/3)
- `docs/dev/todo/2026-11-15-split-agent-exit-cause-unknown.md` — the exit is logged since #905, not
  published; this PRD publishes it
