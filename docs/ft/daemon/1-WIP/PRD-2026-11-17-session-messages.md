# Sessions send each other messages, and a monitor streams them — PRD

**Date**: 2026-11-17
**PRD Type**: Enhancement
**Stack**: `#activity-wake` 5/5 — branch `feature/activity-wake/session-messages`, base
`feature/activity-wake/watch-monitor` (4/5)

## Affected Features

- **Primary Feature**: [Project concept](../project-concept.md) — any session can send another
  session of its project a message, which the recipient receives without being interrupted.
- **Related Feature**: watch monitor (`#activity-wake` 4/5,
  `feature/activity-wake/watch-monitor`) — `tddy-tools monitor` emits messages as
  `message` lines: `--inbox` for the messages addressed to the caller, and a watch kind `message` for
  an orchestrator observing what its sessions send.
- **Related Feature**: session watches (`#activity-wake` 2/5, `feature/activity-wake/session-watches` (#930))
  — a seventh kind, `message`.
- **Related Feature**: [Session notifications](../session-notifications.md) — a message is a record in
  the project notification log, but it is not an attention notification: neither the drawer nor
  Telegram shows it.

## Summary

Today the only way one agent reaches another is `SendSessionMessage`. Only an orchestrator may call
it, it **pastes a new turn** into the recipient's terminal, and it works only for an agent that
declares `turn_input` (Cursor, for one, does not). This node adds a **mailbox**:

- **`tddy-tools send --to <session> "<text>"`** (and the tool `PostSessionMessage`) lets any session
  send a message to another session it may address. The message is recorded and delivered when the
  recipient asks for it; nothing is typed into the recipient's terminal.
- **`tddy-tools monitor --inbox`** streams the messages addressed to the caller as `message` lines,
  so a Claude Code agent under `Monitor` gets each one as a notification the moment it arrives.
- An orchestrator's **watch kind `message`** streams, answers or wakes it with the messages its
  watched sessions send, including the replies they send to it.

## Background

- An orchestrator coordinating several sessions has only one verb, "make it act now" (a pasted turn).
  It has no way to say "here is something for when you next look", and the sessions it started have
  no way to answer it at all, except by raising attention notifications meant for a human.
- The watch monitor (4/5) already gives every Claude Code agent a native, non-interrupting way to
  receive events. Messages are the obvious next event, and agent-to-agent conversation is what an
  orchestrator and its children are missing.

## Proposed Changes

### What's Changing

- **Sending.**
  - The tool **`PostSessionMessage { to_session_id, text, reply_to? }`** → `{ message_id,
    to_session_id, sent_at_unix_ms }`, offered to every session role. `text` must be non-empty and at
    most 8 KiB; `reply_to` names a message the caller received.
  - The CLI **`tddy-tools send --to <session-id> [--reply-to <message-id>] [<text> | -]`** calls it
    and prints the answer. `-` reads the text from stdin.
- **Who may address whom** — on one host, within one project and one OS user, never oneself:
  - an orchestrator → any session it can see (the scope of `ReadProjectNotifications`);
  - a session → its orchestrator (the session whose `orchestrator_session_id` started it), and the
    project-home main session;
  - a reply → the sender of the message it replies to.

  Anything else is refused, saying who the caller may address. Sibling-to-sibling messaging is open
  question 1.
- **The record.** A message is appended to the project notification log as source **`message`**:

  ```json
  {"seq": 5121, "source": "message", "session_id": "<sender>", "to_session_id": "<recipient>",
   "message_id": "m-…", "reply_to": "m-…", "text": "…", "at_unix_ms": …}
  ```

  - It is **not** an attention notification. The drawer, Telegram and `ReadProjectNotifications` do
    not show it, in the way the drawer ignores `session_ended` (`#activity-wake` 1/5).
  - It ages out of the log like every record (1 000 per project), and an evicted, undelivered
    message is counted in `lost`.
- **Receiving — the inbox.** A session's inbox is the `message` records addressed to it, with a
  per-session read position kept beside the watch store and moved only by acknowledgement, as a
  streamed watch's is.
  - **`tddy-tools monitor --inbox`**, for any session, prints one line per message and acknowledges
    after printing:

    ```jsonl
    {"type":"message","seq":5121,"message_id":"m-7c1d","from_session_id":"01a1…","from":"fix-login","reply_to":"m-6b02","text":"Tests pass; pushed 3f2a1c. Want me to open the PR?"}
    ```

    Each message's `text` is printed whole. Its newlines are escaped by JSON, so it stays one line.
  - **`ReadSessionMessages { after_message_id?, limit? }`**, offered to every session role, answers
    the unread messages for an agent that does not stream them, and acknowledges what it returned.
  - **`--inbox` can be combined with watches** (`tddy-tools monitor --inbox --watch <id>`), and one
    process then streams both.
- **Observing — the watch kind `message`.** `CreateSessionWatch` accepts a seventh kind, **`message`**.
  - A `message` watch matches messages **sent by** the sessions it selects, including messages they
    send to the orchestrator itself.
  - Those messages reach the orchestrator however the watch delivers: on tool answers, by the wake,
    or as `message` lines when the watch is streamed.
  - A message addressed to the orchestrator that it both watches and reads through its inbox is
    delivered once: the inbox acknowledgement and the watch position both move past it.
- **Not a turn.** A message never types into a terminal on its own. When a sender needs the recipient
  to act now, `SendSessionMessage` remains the way (orchestrators, `turn_input` agents). An
  orchestrator's watch on its children, with kind `message`, is what wakes it with their replies.

### What's Staying the Same

- `SendSessionMessage`: its role, its refusals, and its paste.
- Watches' six existing kinds, endings and limits; the monitor's line protocol, to which this adds
  the `message` line type only.
- The inbox tools (`ReadProjectNotifications` / `WaitProjectNotifications`): messages are not
  attention notifications and never appear there.
- One host: no message crosses to another daemon.

## Impact Analysis

### Technical Impact

- **`tddy-daemon-kernel` / `tddy-session-activity`**:
  - `SessionNotificationSource::Message`, carrying `to_session_id`, `message_id`, `reply_to` and
    `text` (as optional parts, the way `ended` is);
  - the log stores them, and the drawer and Telegram skip the source;
  - the inbox position per session, persisted beside `session-watches.json`;
  - the `message` kind in the matcher.
- **`tddy-tool-engine`**: `PostSessionMessage` and `ReadSessionMessages`, offered to every role, with
  their refusals; `CreateSessionWatch` accepts `message`.
- **`tddy-daemon-rpc`**: the ports; the addressing rule (who may address whom), read from the
  sessions' `.session.yaml` (`orchestrator_session_id`, project, OS user).
- **`tddy-tools`**: `send`; `monitor --inbox`.
- Every `SessionNotification` literal gains the new parts' `None`.

### User Impact

- An orchestrator can delegate, and each child replies with `tddy-tools send` when done. A
  Claude Code orchestrator under `Monitor` sees each reply as it arrives; any other orchestrator sees
  it on its next answer or is woken by it.
- Agents without `turn_input`, such as Cursor, can now be sent messages: they read them with
  `ReadSessionMessages`, or stream them if their harness has a monitor.

## Implementation Plan

1. The `message` record: source, storage, and the drawer and Telegram skipping it.
2. `PostSessionMessage` and the addressing rule, with its refusals.
3. The inbox position; `ReadSessionMessages`; `tddy-tools monitor --inbox`.
4. The watch kind `message`, delivered once alongside the inbox.
5. `tddy-tools send`; docs.

## Acceptance Criteria

- [ ] A session started by an orchestrator sends it a message with `tddy-tools send`; the
      orchestrator's `tddy-tools monitor --inbox` prints one `message` line with the text and the
      sender ([Project concept](../project-concept.md))
- [ ] A reply (`--reply-to`) carries `reply_to`, and is refused when the caller never received that
      message
- [ ] Addressing outside the rule is refused, naming who the caller may address: another project,
      another OS user, oneself, and a session that is neither its orchestrator nor the main session
- [ ] A message is not in `ReadProjectNotifications`, the drawer or Telegram
- [ ] `ReadSessionMessages` returns unread messages once; a message printed by `--inbox` is not
      returned again, and one received but not acknowledged before the monitor died is printed again
- [ ] An orchestrator's watch with kind `message` on its children delivers their messages on its next
      tool answer, by a wake when idle, or as `message` lines when streamed — each once, even when
      the message is also in its inbox
- [ ] A Cursor session (no `turn_input`) can be sent a message and reads it with `ReadSessionMessages`
- [ ] The touched packages' tests pass (`./test -p <pkg>` per package)

## Open questions

1. **Siblings.** May a session message a sibling, another session its orchestrator started?
   **Proposed: not in this node.** The orchestrator relays, which keeps it the one session that
   knows the conversation. Siblings can be added later as one more addressing rule.
2. **Message size and retention.** 8 KiB per message, and the log's 1 000-record window shared with
   attention records. A chatty pair of agents could evict a human-facing notification. If that shows
   up, messages get a log of their own.
3. **Separate node or part of 4/5?** Proposed **separate**. The monitor node (stream, lease, ack) is
   reviewable on its own, and messaging adds a new record source, an addressing rule and two tools.
   Merged, it would be a complexity 6+ node.

## References

### Affected Features (Complete List)

- [Project concept](../project-concept.md) — `PostSessionMessage`, `ReadSessionMessages`,
  `tddy-tools send`, `tddy-tools monitor --inbox`
- Watch monitor — `feature/activity-wake/watch-monitor`
- Session watches — `feature/activity-wake/session-watches` (#930)
- [Session notifications](../session-notifications.md) — the `message` source, not an attention
  notification

### Related Documentation

- Predecessor: `feature/activity-wake/watch-monitor` (#934, `#activity-wake` 4/5)
- `SendSessionMessage` (`packages/tddy-tool-engine/src/project_home_turn_tools.rs`) — the turn-pasting
  counterpart, unchanged
- `docs/dev/todo/2026-11-16-cursor-hooks-udoo.md` — Cursor declares no `turn_input`; messages are its
  first way to be told anything without a paste
