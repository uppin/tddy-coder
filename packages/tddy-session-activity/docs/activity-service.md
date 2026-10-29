# `activity.ActivityService` — the ten methods and their ports

## Role

`tddy-session-activity` serves the ten RPCs of families M and N: what an agent is doing, what a
session's status is, and the transcript of both. The coordinate and the crate both arrived with
`#unbundle` node 7, which moved the families off `connection.ConnectionService` (50 methods → 33)
along with the notification bus.

| Method | Kind | Answers from |
|---|---|---|
| `ReportSessionStatus` | unary | a hook's per-session `hook_token`, not a session token; a `WaitingForInput` report carrying `tool_name` (a `PermissionRequest` or `PreToolUse(AskUserQuestion)`) records the structured question in `pending-question.json` under a new `request_id`, sets `pending_elicitation`, answers with the id and publishes one `PendingQuestion` notification; a `WaitingForInput` with no tool keeps a structured question still pending (the follow-up `Notification`, whose status notification carries the question) or else records its `message` (`kind: permission`); any other report — or a wait with no message and nothing pending — clears both. A malformed question is refused before anything is written (`pending_question_record`, `pending_question_notice`) |
| `ReportOperatorNotification` | unary | the same hook token, after the same session-type check (claude-cli or cursor-cli); a session that recorded no token is refused. Validates the title (1–200 characters after trimming) and body (≤ 2,000) first — `INVALID_ARGUMENT`, never truncation — then publishes one `AttentionRequired` / `Agent` notification carrying an `AgentNotification` (`service/operator_notifications.rs`). Not routed to a peer: `tddy-tools notify` runs on the host that holds its session |
| `AwaitQuestionAnswer` | unary, long-held | the same hook token, claude-cli only. `NO_ORCHESTRATOR` at once unless the session's changeset names an `orchestrator_session_id` on this host whose main terminal `AnswerWaitPorts::presence` reports active; refuses a `request_id` that is not the pending question, or not a permission; otherwise opens a waiter in `PendingAnswers` and holds the call for at most `AnswerWaitPorts::wait` → `ANSWERED` or `TIMED_OUT` (`service/question_answers.rs`). Not routed to a peer: the hook waits on the host that holds its session |
| `ReportAgentActivity` | unary | the same hook token; mints the `call_id` pairing a PreToolUse row to its PostToolUse row |
| `StreamSessionActivity` | server-stream | the persisted activity log, then the live hub |
| `StreamSessionNotifications` | server-stream | this host's notification bus, scoped to the caller's OS user |
| `StreamAgentActivityDelta` | server-stream | the per-session delta ring the session room measured |
| `StreamAcpReplay` | server-stream | the ACP transcript, then the live broadcast |
| `GetAcpToolCallDetail` | unary | one tool call's full bodies, which replay frames omit |
| `GetAcpReplayPage` | unary | a page of the transcript before a given position |

Proto: [`packages/tddy-service/proto/activity.proto`](../../tddy-service/proto/activity.proto). It
imports `types.proto` for `AgentActivityRecord` and `StreamMode`.

## Families M and N share one service

ACP transcript replay is arguably its own concern, but it is a *view* of agent activity: it shares
`AgentActivityRecord` and `StreamMode`, and three methods do not justify a fourth served coordinate
on the daemon's local socket.

## The delta tick numbering changed here, deliberately

Before this node, a session's first activity delta was numbered **0** — which is also the wire's
"no tick yet" sentinel, so a consumer could not tell *the first delta* from *no delta yet*. Both
LiveKit suites worked around it by warming the room before a client attached.

`StreamAgentActivityDelta` moved to a **new proto** in this node, and carrying a known ambiguity
into a fresh schema makes it permanent. So the rule is now explicit and lives in
[`tddy_service::session_activity`](../../tddy-service/src/session_activity.rs):

```rust
pub const NO_TICK: u64 = 0;
pub const FIRST_TICK: u64 = 1;
const _: () = assert!(FIRST_TICK != NO_TICK);
pub fn next_tick(last: Option<u64>) -> u64;   // None → FIRST_TICK
```

The invariant is a **compile-time** assertion rather than a test, because a constant nobody passes
has no test that would notice it drifting.

It is declared in `tddy-service` rather than here because the producers are this crate and
`tddy-daemon-livekit`'s session room, while the consumer is `tddy-session-sync` — a standalone
shipped binary. Reaching the rule through this crate would put its `livekit` dependency tree into that
binary for three symbols. `tddy-session-sync` migrated in the same PR.

This is a deliberate exception to the stack's "move only" rule, taken because a field-numbering
decision made once, with the single consumer migrated alongside, is cheaper now than at any later
point.

## The ports, and why each is one

`ActivityPorts` is a struct rather than six positional parameters: they are all wiring, and the two
`Arc<dyn _>` ports have no type-level distinction at a call site.

| Port | What only a daemon can answer |
|---|---|
| `os_users` | which OS user a session token belongs to — the `users[]` mapping is the daemon's config |
| `tddy_data_dir` | where this host keeps its data; an operator's choice, not a property of this subsystem |
| `activity` | node 1's `AgentActivityHub`, consumed unchanged — the sandbox subsystem publishes into the same hub, so a second one here would leave an in-jail tool call invisible to every stream |
| `notifications` | an `Option`, because a host with no subscribers and no stream clients raises no bus, and inventing one here would retain events nobody reads |
| `session_labels` | a session's display name, whose fallback reads `session_list_enrichment` — `ListSessions` is family C and stays in the daemon |
| `deltas` | the ring is filled by the session room's poll loop in `tddy-daemon-livekit`, and a patch is a measurement of a live checkout only that host ever took |

`SessionDeltaStores::delta_for_call` returns a `DeltaLookup` with four settled answers a client acts
on — and its `Err` is not a fifth absence, but "this host could not answer the question", which a
client retries.

## Holding a hook for an orchestrator's answer

`AwaitQuestionAnswer` is the one call this service holds open. What it needs reaches
`ActivityServiceImpl` through the builder `with_question_answers(AnswerWaitPorts { pending, presence,
wait })`, not `ActivityPorts`; `ActivityServiceImpl::new` alone wires `AnswerWaitPorts::default()`,
whose `NoOrchestratorIsActive` presence answers every hook `NO_ORCHESTRATOR`. The daemon
(`tddy-session-lifecycle`, `DaemonSessionHost::activity_service`) wires its one `PendingAnswers`, a
`ServedTerminalPresence` over its terminal store and `claude_cli.answer_wait_secs`.

- **`pending_answers::PendingAnswers`** — the waiters, one per `request_id`, in memory. It implements
  `#coagents`' `PermissionAnswering` (`answer_as` delegates to `answer_with_message` with no
  message; `deferred_to_user` is always false; `pending_request` is the earliest-opened open
  request). The first answer wins; answered ids are remembered for the last
  `ANSWERED_REQUESTS_KEPT` (1024) answers so a second is refused naming the first answerer. An answer
  given before its hook waits is that wait's outcome; a wait that times out closes its request, and
  an answer that lands between the deadline and the lock is returned, not lost. A wait replaced by a
  re-open of the same id never closes the new waiter.
- **`answers::answer_permission`** — what `AnswerSessionPermission` runs on the child's host: the
  orchestrator check (`require_orchestrator`), already answered, the pending question and its kind,
  the waiter and its offered option, then delivery, an activity row and clearing the question.
  `permission_request_view` turns `#home-inbox` 3/8's `PendingQuestion` into the
  `PermissionRequestView` an orchestrator reads; `AnswerRefusal` carries every refusal's text.
- **`orchestrator_presence::OrchestratorPresence`** — the port: whether this daemon serves a
  session's main terminal.

## What is deliberately not here

`daemon_instance_id` routing, for the reason node 6's `tddy-session-files` gives: forwarding a call
to the peer holding the transcript needs the common-room slot, the eligible-daemon roster and the
LiveKit forwarding clients. `tddy-daemon`'s `PeerRoutedActivity`
([`connection_service/svc_activity_ports.rs`](../../tddy-daemon/src/connection_service/svc_activity_ports.rs))
wraps this implementation and holds that fork — and with it the two streaming methods' "forwarding
is not supported yet" refusal, which is a statement about the transport's idle deadline rather than
about activity.

## The four relays

[`streams.rs`](../src/streams.rs) holds the live relays the four streaming methods hand back. Each
reads an `AgentActivityHub` or notification-bus receiver and writes frames into an mpsc channel
until the client hangs up, so the **relay**, not the handler, owns a subscription's whole life. None
of them touches a peer, a room or a config, which is why all four could move.

`relay_session_notifications` delivers only on a **positive** owner match: the bus is host-wide, so
that check is the only thing between one operator and another's sessions, and a notification whose
owner is empty reaches nobody.

## The second server

`tddy-coder`'s session participant serves families M and N too, for LiveKit-routed sessions, at the
same `activity.ActivityService` coordinate
([`packages/tddy-coder/src/session_participant/activity_service.rs`](../../tddy-coder/src/session_participant/activity_service.rs)).
It moved in lockstep in the same PR, for the reason
[`packages/tddy-coder/docs/changesets/2026-08-02-activities-tail-first-autoscroll.md`](../../tddy-coder/docs/changesets/2026-08-02-activities-tail-first-autoscroll.md)
records: when the two hosts drift, the same session opens tail-first over HTTP and head-first over
LiveKit.

Unlike node 6's terminal family — where the participant registers `tddy-terminal-rpc`'s own entry
constructor, so there is one implementation — the participant here keeps **its own** replay
handlers. Both read `tddy-service`'s shared `acp_replay` helpers, but the framing around them is
written twice. See
[`docs/dev/todo/2026-09-12-the-acp-replay-framing-is-written-twice.md`](../../../docs/dev/todo/2026-09-12-the-acp-replay-framing-is-written-twice.md).

## Module layout

| Module | Production lines | Holds |
|---|---:|---|
| `service.rs` | 202 | `ActivityServiceImpl`, the session-directory and hook-session resolution every method shares, `publish_for_session`, `activity_delta_frames` and the two stream adapters (`streamed`, `relayed`) |
| `service/ports.rs` | 146 | the ports the daemon injects: `ActivityPorts`, `SessionLabels`, `SessionDeltaStores` and `NoSessionDeltaStores`, `DeltaLookup`, `DeltaScope`, `MeasuredDelta` — re-exported from `service` |
| `service/rpc.rs` | 323 | the `activity.ActivityService` trait impl: the four streams, the two ACP replay reads, and the two hook reports as one-line delegations |
| `service/question_answers.rs` | 167 | `AnswerWaitPorts` and `await_question_answer_impl`, the long-held hook call |
| `answers.rs` | 387 | `answer_permission`, its refusals and `permission_request_view` |
| `pending_answers.rs` | 249 | `PendingAnswers`, the in-memory waiters |
| `orchestrator_presence.rs` | 32 | the `OrchestratorPresence` port and `NoOrchestratorIsActive` |
| `service/hook_reports.rs` | 215 | the two hook reports' bodies, `report_session_status_impl` and `report_agent_activity_impl`, as inherent methods: a trait impl cannot be split across files and `restructure`'s `extract_method` is refused inside an `#[async_trait]` impl, so the trait methods delegate here |

Production lines are counted to the first `#[cfg(test)]`; the budget is 500.

## Known gaps

- ⚠ **This crate has no tests at all** — no `tests/` directory, no `#[cfg(test)]` module, across
  1,573 production lines. Every assertion about the subsystem is made from `tddy-daemon`'s suites.
  See [`docs/dev/todo/2026-09-12-tddy-session-activity-has-no-tests-of-its-own.md`](../../../docs/dev/todo/2026-09-12-tddy-session-activity-has-no-tests-of-its-own.md).
- ⚠ **Nothing checks that the moved ACP replay output matches what the old coordinate produced.**
  For a move-only change that is the key check, and it is absent. Same entry as above.
- `report_agent_activity_impl` (`service/hook_reports.rs`) is 129 lines, over the 60-line function
  threshold, with no code-issue record yet: the `PreToolUse` and `PostToolUse` record builders are its
  seam.

## Related

- [session-notifications.md](./session-notifications.md) — the bus, its subscribers and their interest filters
- [docs/ft/daemon/session-notifications.md](../../../docs/ft/daemon/session-notifications.md) — the feature
- [docs/ft/web/agent-activity-pane.md](../../../docs/ft/web/agent-activity-pane.md) — the browser's reader
- [docs/ft/coder/acp-replay-lazy-tool-bodies.md](../../../docs/ft/coder/acp-replay-lazy-tool-bodies.md) — why replay frames omit bodies
- [docs/ft/daemon/session-worktree-sync.md](../../../docs/ft/daemon/session-worktree-sync.md) — what the delta ticks are for
- [connection-service.md](../../tddy-daemon/docs/connection-service.md) — the 33 methods that stayed
