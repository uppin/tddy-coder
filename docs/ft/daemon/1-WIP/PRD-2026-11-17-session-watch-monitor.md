# An orchestrator streams its watches to a native monitor — PRD

**Date**: 2026-11-17
**PRD Type**: Enhancement
**Stack**: `#activity-wake` 4/5 — branch `feature/activity-wake/watch-monitor`, base
`feature/activity-wake/idle-wake` (#931)

## Affected Features

- **Primary Feature**: [Project concept](../project-concept.md) — an orchestrator can stream what its
  watches match as one line per event, from `tddy-tools monitor`, for its agent's own monitor tool
  (Claude Code's `Monitor`) to turn into notifications.
- **Related Feature**: session watches (`#activity-wake` 2/5,
  `feature/activity-wake/session-watches` (#930)) — a streamed watch is still a watch: same filter, kinds,
  TTL, endings and limits.
- **Related Feature**: idle wake (`#activity-wake` 3/5, `feature/activity-wake/idle-wake` (#931)) — a
  streamed watch is not also pasted into the orchestrator's terminal or put on its tool answers.

## Summary

The watches of `#activity-wake` reach an orchestrator in two ways. A busy orchestrator gets them on
its next tool answer (`watchEvents`). An idle one gets them as a pasted terminal turn (the wake).
Both work for any agent with `turn_input: terminal_paste`, but both are tddy pushing text *at* the
agent.

Claude Code has a native way to take events: its `Monitor` tool runs a command and turns each line
the command writes on stdout into a notification in the conversation. The agent keeps working while
they arrive. This node adds **`tddy-tools monitor`**, a command that streams an orchestrator's watch
events as **one JSON object per line**. An orchestrator runs it under `Monitor` and receives each
watched event the moment it happens, mid-turn or idle, with no paste and no busy/idle guess. While a
monitor streams a watch, that watch's events are delivered to the monitor only: not on tool answers,
and not by a wake. When the monitor stops, they go back to those two.

## Background

### Why a stream, and why now

- **The wake guesses.** The wake must decide that an orchestrator is idle from its reported status
  (`orchestrator_is_idle`, 2 s settle, 30 s gap). A paste into a terminal whose state was misread
  answers the wrong prompt, so the wake holds back whenever it is unsure. A monitor notification has
  no such risk: the agent's own harness queues it into the conversation.
- **Tool answers only help a busy agent**, and only when its next call comes.
- **The agent already has the primitive.** `Monitor({ command, description, timeout_ms })` streams
  stdout lines as events. A command that prints watch events is all it needs.

### Considered: a WebSocket destination for watches — not in this stack

The idea weighed: `CreateSessionWatch` takes a **destination URL the agent chooses**, and the daemon
opens a **WebSocket client** to it and sends each matched event as a text frame. It was rejected as
the way to reach the agent, and deferred as a general feature:

1. **It cannot reach the agent's own monitor.** Claude Code's `Monitor({ ws: { url } })` is also a
   WebSocket *client*: it connects out to a URL and reads frames. A daemon-side client and a
   `Monitor` client cannot connect to each other. Making them meet needs a WebSocket *server* in
   between, a relay that someone has to run and secure. Neither Claude Code nor the agent's shell
   provides one, so every orchestrator would need that extra hop just to get what one stdout line
   gives it.
2. **An agent-chosen URL turns the daemon into the agent's network client.**
   - The daemon runs outside every jail and can reach the host's private network. An agent that
     names the URL can make it connect anywhere (server-side request forgery).
   - It would also send session content to wherever the agent asks. That gets around a jailed
     agent's egress allow-list: the jail stops the agent from reaching a host, but the daemon would
     reach it on the agent's behalf.
3. **It is a delivery system of its own:** reconnection and backoff, buffering while the
   destination is down, at-least-once acknowledgement over a protocol tddy does not own,
   credentials for the destination, and a per-watch outbound connection the daemon must supervise
   and bound. None of that exists, and none of it is needed for the agent case, which point 1 says
   this route cannot serve anyway.

**What would make it sensible** is a destination that is **already a WebSocket server and not an
agent**: a dashboard, a relay, another orchestration system. Even then the agent should not supply
the URL. The shape would be **operator-declared sinks**: `daemon.yaml` names each destination
(`watch_sinks: { ops-relay: { url, auth } }`), and a watch selects one **by name**
(`sink: ops-relay`). The operator owns where data may go, so the allow-list survives. That is a
separate node, with a separate PRD, once such a consumer exists.

**For the agent, a command-mode monitor is the answer** (this node): `tddy-tools monitor` writes
each event to stdout, and `Monitor({ command })` turns each line into a notification. It works over
whatever transport `tddy-tools` already detects (`detect_session_tool_transport`): the daemon
socket on the host, tool IPC in a jail, LiveKit for a split session. So it reaches the agent
wherever the watch tools already work, with no new listener, credential or outbound connection.

## Proposed Changes

### What's Changing

- **`tddy-tools monitor`**, a new subcommand, run from the orchestrator's own shell (the session is
  found as every session-scoped `tddy-tools` command finds it: `.tddy/session.json`, then the
  transport it names).

  ```bash
  # Create a watch and stream it:
  tddy-tools monitor --started-by-me --kinds permission,question,turn_finished,ended --ttl 3600
  # Stream a watch that already exists (re-arming after Monitor's 30-minute expiry):
  tddy-tools monitor --watch <watch_id>
  ```

  - `--watch <id>` (repeatable) streams existing watches of the caller.
  - Otherwise `--session <id>` (repeatable), `--started-by-me`, `--kinds <k,…>` and `--ttl <secs>`
    create one watch, exactly as `CreateSessionWatch` does, with the same refusals.
  - `--full` adds the log record to each event line. By default an event line is compact, because
    every line becomes a message in the agent's conversation.
- **The line protocol** — stdout carries **only** these lines. Each is one compact JSON object,
  written and flushed as it happens, with no embedded newline. Logs go to stderr.

  ```jsonl
  {"type":"watch","watch_id":"w-1a2b","filter":{"started_by_me":true,"kinds":["permission","question","turn_finished","ended"]},"expires_at_unix_ms":1763400000000}
  {"type":"event","seq":4182,"watch_ids":["w-1a2b"],"kind":"permission","session_id":"01a1…","session":"fix-login","summary":"wants to run Bash: cargo test","request_id":"3f2a…"}
  {"type":"event","seq":4190,"watch_ids":["w-1a2b"],"kind":"turn_finished","session_id":"01a2…","session":"docs-pass","summary":"finished this turn"}
  {"type":"lost","count":3}
  {"type":"ended","watch_id":"w-1a2b","reason":"sessions_ended"}
  ```

  - `summary` is worded as the wake words each event (`wake_text`), so a monitor line and a wake turn
    read alike.
  - `request_id` is present on `permission` and `question` events, so the agent can answer with
    `AnswerSessionPermission` / `AnswerSessionQuestion`.
- **Exit codes** — `Monitor` treats exit as the end of the watch, and silence must never pass for
  success:
  - **0** once every streamed watch has ended, after printing an `ended` line for each;
  - **non-zero** on any failure (refusal, transport loss, the session not found), after printing
    `{"type":"error","message":"…"}` on **stdout**, so a filter-free `Monitor` still sees it.
- **`WaitSessionWatchEvents`**, a new project-home tool that the monitor calls in a loop:
  `{ watch_ids, ack_seq?, timeout_ms ≤ 20 000 }` → `{ events, lost, ended, next_ack_seq }`. It holds
  until a streamed watch matches something or the timeout passes, like `WaitProjectNotifications`
  (same 20 s cap, same remote clamp in `clamp_remote_blocking_args`).
- **Acknowledged, not taken.** A streamed watch's position advances only when the next call presents
  `ack_seq`, i.e. once the monitor has printed the batch.
  - A monitor killed between receiving a batch and printing it loses nothing: the batch is handed
    again. The only duplicate possible is a batch printed but not yet acknowledged when the process
    died, and `seq` lets the agent recognise it.
  - Answers and wakes keep their exactly-once delivery: they never touch a streamed watch's events
    (next point).
- **The stream lease.** A watch counts as *streamed* while a `WaitSessionWatchEvents` call for it is
  in flight, and for **15 s** after one returns, which covers the monitor's turnaround between calls.
  - While a watch is streamed, its events are **not** attached to the orchestrator's tool answers and
    **do not wake it**.
  - When the lease lapses, everything after the last acknowledged `seq` goes back to answers and the
    wake, as if no monitor had run. That happens when `Monitor` expires and is not re-armed, the
    command is stopped, or the daemon restarts (the lease is in memory).
- **One stream per watch, and the newest wins.** A new monitor that the watch's own orchestrator
  starts on its own live watch **takes the stream over at once**: it gets the lease and a new stream
  id, and resumes from the last acknowledged position, so a batch the old monitor never acknowledged
  is handed again. The old monitor's next call is refused, naming the watch and saying another
  monitor took it over; it prints an `error` line and exits non-zero. `--watch` on a watch that is
  not the caller's, or has ended, is refused as `RenewSessionWatch` refuses it. (Decided by the user,
  2026-11-17: "agent should be able to resume the monitor. If monitor is not resumed, its TTL would
  auto-unarm it.")
- **Resuming.** `Monitor` expires after at most 30 minutes. The agent re-runs `tddy-tools monitor
  --watch <id>` with the `watch_id` from the first line, at once — it does not wait for the old
  lease. Events that arrive between expiry and resume wait in the log at the watch's position, or are
  woken/answered once the lease has lapsed. A watch nobody resumes ends at its TTL, as any watch
  does.
- **Telling the agent.** `WaitSessionWatchEvents`'s description and
  [Project concept](../project-concept.md) say that a Claude Code orchestrator may stream a watch with
  `Monitor` and `tddy-tools monitor` rather than wait to be woken. They give the command,
  `timeout_ms: 1800000`, and the re-arm. `CreateSessionWatch` is #930's tool, and its text is left as
  it is.

### What's Staying the Same

- **Watches** — the filter, kinds, TTL, limits (16 live, 24 h), endings and every refusal of
  `#activity-wake` 2/5. `tddy-tools monitor` creates a watch through the same port.
- **A watch that is not streamed** is delivered exactly as before: on tool answers, and by the wake
  when idle.
- **Visibility and role** — only orchestrators (the project-home tool role) may watch or stream, on
  their own host, never their own session.
- **No new notification, no new proto field** — a monitor is a reader of the project log, as a watch
  is. `activity.proto` stays untouched.
- **No WebSocket**, no daemon listener change, no web UI.

## Impact Analysis

### Technical Impact

- **`tddy-session-activity`** (`session_watches`): positions that advance on acknowledgement for a
  streamed watch, beside `take_delivery` / `deliver_with`; the in-memory stream lease (attach,
  renew on each call, lapse after 15 s); `take_delivery` and `deliver_with` skip streamed watches.
- **`tddy-tool-engine`**: `WaitSessionWatchEvents`'s definition and refusals; the project-home
  catalogue lists it; its description is where an agent learns of the monitor.
- **`tddy-daemon-rpc`**: the tool's port — the long poll woken by the notification bus, as
  `WaitProjectNotifications`'s is — and the waker skipping streamed watches (a check in
  `watch_wake.rs`, not a new path).
- **`tddy-session-tool-client`**: `WaitSessionWatchEvents` added to the remote blocking-argument
  clamp.
- **`tddy-tools`** (`tddy-tools-cli`): the `monitor` subcommand — create or attach, the call loop,
  the line protocol, exit codes.
- **Cost**: one blocking tool call per 20 s per streamed watch, with no polling of the log (the bus
  wakes the wait); no cost for unstreamed watches.

### User Impact

- An operator who tells the main session "watch the sessions you start and answer their permission
  prompts" sees each prompt reach it within about a second, whether it is mid-turn or idle, as a
  `Monitor` notification rather than a pasted turn.
- Nothing changes for an orchestrator that never runs the monitor.

## Implementation Plan

1. Acknowledged positions and the stream lease in the watch store; `take_delivery` / `deliver_with`
   skip streamed watches (unit tests over a real log).
2. `WaitSessionWatchEvents` and its refusals, through the `tddy-daemon-rpc` project-home harness.
3. The waker skips a streamed watch; the lease's lapse hands events back (acceptance tests with the
   recording terminal of `#activity-wake` 3/5).
4. `tddy-tools monitor`: create/attach, loop, lines, exit codes (CLI tests against the harness's
   daemon socket).
5. Tool descriptions and docs.

## Acceptance Criteria

- [ ] `tddy-tools monitor --started-by-me --kinds permission --ttl 600`, run as an orchestrator,
      prints a `watch` line, then one `event` line per matched permission prompt of a session it
      started, with that prompt's `request_id`, each flushed as it happens
      ([Project concept](../project-concept.md))
- [ ] While the monitor runs, the same events are not on the orchestrator's tool answers
      (`watchEvents`) and do not wake it
- [ ] When the monitor stops, events after its last printed `seq` are delivered on the next tool
      answer or by a wake; none is lost, and none is delivered by both
- [ ] A monitor killed after receiving a batch but before acknowledging it gets that batch again on
      `--watch` re-attach
- [ ] `--watch <id>` re-attaches to an existing watch and continues from its acknowledged position
- [ ] The monitor exits 0 after printing `ended` lines once every streamed watch has ended, and exits
      non-zero after printing an `error` line on a refusal or a lost transport
- [ ] A second monitor on a streamed watch takes it over and the first is refused from its next
      call; a non-orchestrator is refused; `--watch` on another orchestrator's watch is refused
- [ ] It works over each transport `tddy-tools` detects: daemon socket (host) and tool IPC (jail)
- [ ] Manual: under Claude Code's `Monitor`, an idle orchestrator receives a watched permission
      prompt as a notification and answers it (see open question 1)
- [ ] The touched packages' tests pass (`./test -p <pkg>` per package)

## Open questions

1. **✅ Answered (2026-11-17): yes — a `Monitor` notification starts a turn for an *idle* Claude
   Code agent.** Observed on the orchestrator's own Claude Code session that planned this stack:
   on 2026-11-17 its `Monitor` events re-invoked the idle session repeatedly, each one starting a
   turn. So the lease may suppress the wake as designed: a streamed watch's events reach an idle
   orchestrator through the monitor alone. The design is no longer conditional; the manual
   acceptance criterion above stays as a confirmation on a real session (an idle orchestrator told
   of a watched *permission prompt* through `tddy-tools monitor` specifically), not as a gate on
   the design.
2. **Tool-call activity.** "Session activities" could also mean a watched session's tool calls (the
   rows `ReadSessionActivity` reads from `agent-activity.jsonl`). Those are not in the project log
   that watches read, so they would need a seventh kind with its own per-session cursor. **Proposed:
   out of scope here**; add a `tool_call` kind in a follow-up if wanted. Note that Cursor sessions
   report none today (`docs/dev/todo/2026-11-16-cursor-hooks-udoo.md`, fix 2).
3. **Non-orchestrator use.** Should an ordinary session be able to *watch* sessions it can see (e.g.
   a subagent watching its siblings)? **Proposed: no** — watches stay an orchestrator's tool. The one
   stream every session does get is its **inbox**, the messages addressed to it: `tddy-tools monitor
   --inbox`, added by the next node (session messages (`feature/activity-wake/session-messages`)), which
   also adds the `message` line type to this protocol.
4. **Lease length.** 15 s covers the loop's turnaround with margin. A shorter lease returns events
   to the wake sooner after a monitor dies; a longer one risks a wake racing a slow re-poll.

## References

### Affected Features (Complete List)

- [Project concept](../project-concept.md) — `tddy-tools monitor`, `WaitSessionWatchEvents`
- Session watches — `feature/activity-wake/session-watches` (#930)
- Idle wake — `feature/activity-wake/idle-wake` (#931)
- [Session notifications](../session-notifications.md) — referenced; nothing published

### Related Documentation

- Predecessors: `feature/activity-wake/session-ended-notice` (#929),
  `feature/activity-wake/session-watches` (#930), `feature/activity-wake/idle-wake` (#931)
- Wave: **3** — it needs #930's store and #931's waker to behave (the lease check sits in the waker,
  and the skip sits in `deliver_with`)
- Successor: session messages (`#activity-wake` 5/5) —
  `feature/activity-wake/session-messages`: `tddy-tools send` and the
  `message` line this monitor emits
- Claude Code `Monitor` tool — `command` mode, one notification per stdout line, at most 30 minutes
  per arm
