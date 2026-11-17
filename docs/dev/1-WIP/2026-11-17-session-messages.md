# Changeset: Sessions send each other messages, and a monitor streams them

**Date**: 2026-11-17
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — six crates (daemon-kernel, session-activity, tool-engine, daemon-rpc, telegram-control, tddy-tools) plus a one-arm skip in the desktop notifier and a getter in session-lifecycle, a new notification source every surface must handle, an addressing rule read from session records, a role exception for the inbox watch, and ordering semantics (an inbox position beside the watch positions, a message delivered once across watches and reads)
**Stack**: `#activity-wake` 5/5 — branch `feature/activity-wake/session-messages`, base `feature/activity-wake/watch-monitor` (#934)

## Initial Discovery

[2026-11-17-session-messages-initial-discovery.md](./2026-11-17-session-messages-initial-discovery.md)

## Prerequisites

### ⚠ DURING — `answer_execute_tool` is over its length budget — [`complexity-exec-tool-ports-execute-tool.md`](../../../packages/tddy-daemon-rpc/docs/code-issues/complexity-exec-tool-ports-execute-tool.md)

`PostSessionMessage` and `ReadSessionMessages` are offered to every role and served through the
executor's existing tool dispatch; not one line is added to `answer_execute_tool`.

### ⚠ DURING — `run_call_tool` is over its complexity budget — [`complexity-cli-run-call-tool.md`](../../../packages/tddy-tools/docs/code-issues/complexity-cli-run-call-tool.md)

`tddy-tools send` is its own module (`send_cli.rs`); the `message` line extends `monitor_cli.rs`
(#934's). `run_call_tool` is not touched.

### ⚠ DURING — the Telegram notifier's complexity issues — [`complexity-telegram-notifier-on-server-message.md`](../../../packages/tddy-telegram-control/docs/code-issues/complexity-telegram-notifier-on-server-message.md)

A message never reaches Telegram: the skip is one arm in `telegram_session_subscriber.rs`'s source
match, and nothing is added to the notifier.

### ℹ ANSWERED — ACP turn input is not delivered — [`2026-11-13-acp-turn-input-is-not-delivered.md`](../todo/2026-11-13-acp-turn-input-is-not-delivered.md)

A message needs no `turn_input`, so it is the first way to tell an ACP or Cursor session anything.
The entry stays open.

## Affected Packages

- **tddy-daemon-kernel**: [daemon-kernel.md](../../../packages/tddy-daemon-kernel/docs/daemon-kernel.md) — `SessionNotificationSource::Message` and its `SessionMessage` part
- **tddy-session-activity**: [session-notifications.md](../../../packages/tddy-session-activity/docs/session-notifications.md) — the `message` record in the log; the inbox position (kept with the watches); the inbox watch and the watch kind `message`; the web stream skips the source
- **tddy-tool-engine** — new `session_message_tools`: `PostSessionMessage`, `ReadSessionMessages`, their port and refusals, offered to every role; `CreateSessionWatch` accepts the kind `message` through #930's kind vocabulary
- **tddy-daemon-rpc** — the port, the addressing rule and the inbox watch for a session that is not an orchestrator (`exec_tool/session_messages.rs`)
- **tddy-telegram-control** — `telegram_session_subscriber.rs` skips the source
- **tddy-desktop** — `native_notifications.rs` skips the source
- **tddy-tools** / **tddy-tools-cli** — `send_cli.rs`, `Subcommand::Send`; the `message` line in `monitor_cli.rs`; the two tools registered for the MCP server (`exec_tool_admission.rs`)
- **tddy-session-lifecycle** — `DaemonSessionHost::session_notification_bus()`, the getter the daemon publishes a posted message through (added at pass 2, 6d; kept at 6e); `message: None` in its notification literals

## Related Feature Documentation

- [PRD-2026-11-17-session-messages.md](../../ft/daemon/1-WIP/PRD-2026-11-17-session-messages.md)
- [Project concept](../../ft/daemon/project-concept.md), [Session notifications](../../ft/daemon/session-notifications.md)

## Summary

Any session can send a message to a session it may address: its orchestrator, the project's main
session, the sender of a message it received, or (for an orchestrator) any session it can see. A
message is an **activity record** — a `message` record in the project notification log, stored there
once. It is never typed into a terminal and never shown in the drawer, on the web stream or in
Telegram.

The recipient receives it by reading activity, through one of two references into the log:

- its **inbox watch** — `CreateSessionWatch` with the kind `message` alone, which any session may
  hold — delivered like any watch: on its tool answers, or streamed by
  `tddy-tools monitor --kinds message` (#934's `WaitSessionWatchEvents`);
- its **inbox position**, through which `ReadSessionMessages` reads (for an agent with no monitor).

An orchestrator hears what its sessions send through an ordinary watch that includes the kind
`message`. A message that reaches a session more than one way is delivered once.

## Background

See the PRD. `SendSessionMessage` is the only agent-to-agent verb, and it is orchestrator-only,
pastes a turn and needs `turn_input`. A delegated session therefore has no way to answer the
orchestrator that started it except attention notifications meant for a human.

## Scope

- [ ] The `message` source: kernel type, log storage, skipped by the web stream, Telegram and desktop
- [ ] `PostSessionMessage` with the addressing rule and its refusals
- [ ] The inbox position; `ReadSessionMessages`
- [ ] The inbox watch: any session may watch its own messages; the `message` line in `tddy-tools monitor`
- [ ] The watch kind `message`, delivered once across watches and reads
- [ ] `tddy-tools send`
- [ ] Docs: `project-concept.md`, `session-notifications.md` (the source), package docs

## Responsibility

This PR owns **messages between sessions, read as activity**:

- `SessionNotificationSource::Message` and `SessionMessage { to_session_id, message_id, reply_to,
  text }` in `tddy-daemon-kernel`, and their storage in the project log;
- `tddy_session_activity::session_messages` — the inbox position over the log
  (`SessionWatchStore::read_messages`, `inbox_position`, `received_message`, kept in
  `session-watches.json`), the inbox watch (`WatchFilter::inbox_of`, `is_inbox_of`), `InboxMessage`,
  `InboxRead`, `refuse_a_bad_message_text`, `MAX_MESSAGE_BYTES` (8 KiB), `MAX_MESSAGES_PER_READ`;
- the matcher's `message` kind, the inbox watch's match, and the once-only rule across watches and
  reads;
- `tddy_tool_engine::session_message_tools` — `POST_SESSION_MESSAGE`, `READ_SESSION_MESSAGES`, their
  definitions, `SessionMessagePort`, `Addressable` and `MessageRefusal` (the refusal texts);
- the addressing rule and the port in `tddy-daemon-rpc`, the bus getter it publishes through
  (`DaemonSessionHost::session_notification_bus`, `tddy-session-lifecycle`), and the inbox watch for
  a session that is not an orchestrator (`run_inbox_watch_tool`);
- the skips in the web stream, Telegram and desktop;
- `tddy-tools send`, and the `message` line of `tddy-tools monitor` (`MonitorLine::Message`,
  `message_line_of`).

## Boundaries

- **No turn.** A message never pastes into a terminal; `SendSessionMessage` is unchanged.
- **No second store, no second wait.** A message is stored once, in the log; the inbox is a position
  into it, and the only wait is #934's `WaitSessionWatchEvents` over a watch. There is no inbox file,
  inbox stream, inbox acknowledgement or inbox long poll (user, 2026-11-17).
- **No sibling addressing** — a session may not message another session its orchestrator started
  (PRD open question 1, decided).
- **No cross-host message**, no host argument.
- **No web surface**, no proto change: the source never reaches the stream.
- **No change to any #930/#934 published signature.** The inbox watch is asked for through
  `CreateSessionWatch`'s existing arguments (the kind `message` alone, no session selected); the
  monitor streams it with #934's loop, lease and acknowledgement, unchanged. This node adds the
  `message` line, the seventh kind, the inbox watch's match and its role exception.
- **Only the inbox watch is open to every session.** Every other watch stays an orchestrator's
  (project-home role).
- **No change to the wake's decisions** — #931's. A `message` watch reaches the waker through the
  store like every other kind.
- **No tool-call activity** (`agent-activity.jsonl` rows) — a later node (see Successor PRs).

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/activity-wake/session-ended-notice` (#929) | the precedent of a new `SessionNotificationSource` with a structured part (`SessionEnded` / `SessionEndNotice`), carried through the log | `Message` / `SessionMessage` follow the same shape | change `SessionEnded` or its forwarder |
| `feature/activity-wake/session-watches` (#930) | `SessionWatchStore`, the matcher, `WatchKind`, `CreateSessionWatch` and its kind validation, the `watchEvents` attachment | adds `WatchKind::Message`; the inbox watch is a `WatchFilter` naming its owner with the kind `message`, created through `CreateSessionWatch`'s existing args; the attachment carries a session's inbox watch like any watch | change a watch's shape, `CreateSessionWatch`'s arguments, the six kinds, the other refusals or the attachment |
| `feature/activity-wake/idle-wake` (#931) | the waker, `deliver_with`, `wake_text` | a `message` event is woken like any kind | change the idle rule, the timings or the wording of other kinds |
| `feature/activity-wake/watch-monitor` (#934) | `tddy-tools monitor`, `MonitorLine`, `WaitSessionWatchEvents`, acknowledgement, the lease and takeover, the per-call deadline | `monitor --kinds message` streams the inbox watch through them; an event of kind `message` is printed as `MonitorLine::Message`, beside #934's line types | change the loop, the lease, acknowledgement, the arguments, or the existing line types |

**Tests that need a parent green** (recorded at pass 2, re-recorded at 6e): the three CLI tests in
`send_and_message_monitor_cli_acceptance.rs` run the real `tddy-tools monitor`, whose loop is #934's
(`TODO(watch-monitor)` in `run_monitor`): the integration is the point, so the real loop is kept and
they go green only after #934 does. `an_idle_orchestrator_is_woken_by_a_watched_message` needs #931's
waker and the `watchEvents` tests #930's attachment (both green). No other test reaches a
`TODO(watch-monitor)`: the message-line mapping is tested on its own
(`monitor_message_line.rs`).

Adding a variant to `SessionNotificationSource` puts a new arm into every exhaustive `match` on it
(about ten sites, listed in the discovery). That is the variant's mechanical fallout, not a change
to those modules' behaviour for the existing sources.

## Draft PR contract

The first push of this PR's code (commit 2) publishes:

- `SessionNotificationSource::Message`, `SessionMessage`, their log fields; every exhaustive match
  given its arm (the web, Telegram and desktop skips real, since they are trivial; the rest
  `// TODO(session-messages): implement` where behaviour is owed);
- `session_messages`: `SessionWatchStore::read_messages`, `inbox_position`, `received_message`
  (refusing / empty with TODO texts), `WatchFilter::inbox_of` and `is_inbox_of` (real: data),
  `InboxMessage` (`of` a TODO), `InboxRead`, `refuse_a_bad_message_text` (a TODO),
  `MAX_MESSAGE_BYTES`, `MAX_MESSAGES_PER_READ`;
- `WatchKind::Message` named in the vocabulary (`WatchKind::ALL` and `WATCH_KINDS` now seven, the
  default when no kind is named), matching nothing yet; the inbox watch's match a TODO in the matcher;
- `POST_SESSION_MESSAGE`, `READ_SESSION_MESSAGES`, their definitions in every role's catalogue (and
  allowed for the project-home role, and registered for the MCP server), `PostMessageRequest`,
  `ReadMessagesRequest`, `Addressable`, `MessageRefusal`, `SessionMessagePort`,
  `execute_session_message_tool` (argument reading a TODO); the daemon's implementation
  (`exec_tool/session_messages.rs`, routed from `run_locally`) refusing with a TODO text;
- `is_inbox_watch_call` / `run_inbox_watch_tool`: a watch-tool call from a session that is not an
  orchestrator is routed there (ahead of the project-home role gate), denied with `PermissionDenied`
  and a TODO text — the code the role gate answers, so the parents' role tests still pass;
- `DaemonSessionHost::session_notification_bus()`;
- `tddy-tools send` (`SendArgs`, `run_send`, the reason on stderr with the TODO text) and
  `MonitorLine::Message`, `message_line_of` (a TODO);
- every test below, **failing** on the missing behaviour, not on compilation — except the surface
  tests that pass in red by contract: the two skips (`the_web_stream_never_carries_a_message`,
  `a_message_is_never_sent_to_telegram`), `the_message_tools_are_offered_to_every_role` and
  `an_event_of_another_kind_is_no_message_line`.

## Green wave

**Wave:** 4 of 4
**Greenable independently:** no — the `monitor --kinds message` tests run #934's loop,
acknowledgement and lease, and the `message` watch tests need #930's deliveries and #931's wake to
behave. See Dependencies for the per-test sequencing.
**Concurrent with:** none
**Blocks:** none

Real dependency edges, as opposed to the branch line:

    session-ended-notice → session-watches → idle-wake → watch-monitor → session-messages
    (surface)              (behaviour)       (behaviour)    (behaviour)

## Technical Changes

### State A (Current — on `feature/activity-wake/watch-monitor`, as planned)

- `SessionNotificationSource` has six variants; the log stores attention records with `source`,
  `seq`, scope fields, and per-source optional parts (`ended`, `activity_status`, …).
- Watches (#930) match six kinds over those records; the wake (#931) and the monitor (#934) deliver
  them. Every watch tool is an orchestrator's (the project-home role).
- `SendSessionMessage` (`project_home_turn_tools.rs:35`) is the only way one session's agent reaches
  another's: orchestrator-only, a pasted turn, refused without `turn_input`.
- A session's record names the orchestrator that started it (`orchestrator_session_id`,
  `tddy-agent-launch/src/conversation_spawn.rs:88`).

### State B (Target)

- **Record.** `PostSessionMessage` publishes a `SessionNotification` of source `Message`, kind
  `AttentionRequired`, with `session_id` = sender and the `SessionMessage` part. Bus subscribers see
  it: the log stores it (once — the only copy), the waker cues on it, the long polls wake on it. The
  web stream, Telegram and the desktop notifier skip the source; `ReadProjectNotifications` and
  `WaitProjectNotifications` leave it out.
- **Addressing.** For a caller in project P, OS user U, on this host, the recipient must be in P, run
  as U, and not be the caller. It must also be one of:
  - any visible session, when the caller is an orchestrator;
  - the caller's `orchestrator_session_id`;
  - P's main session;
  - the sender of the message `reply_to` names, which the caller must have received.

  Each refusal names who the caller may address.
- **The inbox watch.** `CreateSessionWatch { kinds: ["message"], ttl_seconds }` with no session
  selected is the caller's inbox watch, kept as `WatchFilter::inbox_of(caller)`: it matches the
  `message` records addressed to the caller, whoever sent them, and is delivered like any watch — on
  its answers (`watchEvents`), by the wake, or streamed by `tddy-tools monitor --kinds message`
  through `WaitSessionWatchEvents` (#934's lease, acknowledgement and takeover, unchanged). Any
  session may hold it: a watch-tool call from a session that is not an orchestrator is served by
  `run_inbox_watch_tool`, which creates only the inbox watch and serves the other watch tools over
  the caller's own watches; anything else is denied with `PermissionDenied`, exactly as the
  project-home role gate denies it, so #930's and #934's role tests
  (`a_session_that_is_not_an_orchestrator_is_refused_the_watch_tools`, `…_the_stream_tool`) stand; for such a caller the turn-input
  refusal does not apply, since the watch need not paste.
- **The inbox position.** `ReadSessionMessages { after_message_id?, limit ≤ 50 }` answers the
  `message` records addressed to the caller after its inbox position — a `seq` kept per session in
  `session-watches.json` — and advances past them. It holds no message: it is a reference into the
  log.
- **Delivered once.** A message addressed to a session is handed to it once across its watches and
  its reads, under the project's lock: every watch delivery (attachment, wake, stream) skips a
  message to its owner that the inbox position has passed, and a read skips one a watch of the
  reader has already passed. Across watches, #930's union by `seq` already hands a record once.
- **Watch kind.** `WatchKind::of` maps `source = message` to `Message`. A `message` watch of an
  orchestrator selects by sender, through #930's session selection.
- **The monitor's line.** A streamed event of kind `message` is printed as
  `{"type":"message","seq","watch_ids","message_id","from_session_id","from","to_session_id","reply_to"?,"text"}`,
  read from the event's log record.

### Delta (What's Changing)

#### tddy-daemon-kernel

- `SessionNotificationSource::Message`; `SessionMessage`; the part on `SessionNotification`.

#### tddy-session-activity

- `project_notification_log/storage.rs`: the message fields.
- New `session_messages.rs` (the inbox position, the inbox watch's filter).
- `session_watches` matcher: the `message` kind; the inbox watch's match; the once-only rule.
- `streams.rs`: the web stream skips the source.

#### tddy-tool-engine

- New `session_message_tools.rs`; every role's catalogue lists the two tools; `read_only.rs` allows
  them for the project-home role; `WATCH_KINDS` gains `message`.

#### tddy-daemon-rpc

- New `exec_tool/session_messages.rs` (port, addressing, the inbox watch for a non-orchestrator);
  routed from `run_locally` ahead of the local runner and the project-home role gate;
  `ProjectHomeHost` carries the bus.

#### tddy-session-lifecycle

- `DaemonSessionHost::session_notification_bus()`.

#### tddy-telegram-control, tddy-desktop

- One skip arm each.

#### tddy-tools / tddy-tools-cli

- New `send_cli.rs`, `Subcommand::Send`; `monitor_cli.rs`: `MonitorLine::Message`,
  `message_line_of`.

#### every exhaustive match on `SessionNotificationSource`

- The `Message` arm.

## Implementation Milestones

- [ ] Source, storage, skips; storage tests green
- [ ] Post with addressing; acceptance green
- [ ] The inbox position and `ReadSessionMessages`; acceptance green
- [ ] The inbox watch (match, role exception) and the `message` line; acceptance and CLI green (after #934)
- [ ] Watch kind, once-only across watches and reads
- [ ] `send`; docs

## Testing Plan

### Testing Strategy

The addressing rule, the inbox watch's role exception and the once-only delivery are where this can
go wrong quietly, so the rule and the exception are acceptance-tested case by case through
`tddy-daemon-rpc`'s project-home harness, using real session records with `orchestrator_session_id`
set. The once-only delivery and the inbox position are unit-tested on the store against a real log.
The CLI is tested end to end, as #934's monitor tests are; the message line on its own.

### Acceptance Tests

**tddy-daemon-rpc** — `packages/tddy-daemon-rpc/tests/session_messages_acceptance.rs`

- `a_started_session_messages_its_orchestrator_and_the_orchestrator_reads_it`
- `a_reply_carries_reply_to_and_is_refused_for_a_message_never_received`
- `addressing_outside_the_rule_is_refused_naming_who_may_be_addressed` (another project, another OS
  user, oneself, a sibling)
- `a_message_is_not_in_read_project_notifications`
- `read_session_messages_returns_each_message_once`
- `an_orchestrators_message_watch_delivers_what_its_sessions_send_once_even_when_also_in_its_inbox`
- `an_idle_orchestrator_is_woken_by_a_watched_message`
- `a_started_session_may_watch_its_own_messages_and_no_other_watch` (added at 6e)
- `a_sessions_own_message_watch_carries_the_messages_addressed_to_it` (added at 6e)
- `a_session_without_turn_input_reads_its_messages`

**tddy-tools-cli** — `packages/tddy-tools-cli/tests/send_and_message_monitor_cli_acceptance.rs`
(needs #934 green)

- `send_prints_the_message_id_and_the_recipient_receives_it`
- `monitor_kinds_message_streams_a_sessions_own_messages_one_line_each`
- `a_message_text_with_newlines_stays_on_one_line`

### Unit Tests

**tddy-session-activity** — `packages/tddy-session-activity/tests/session_messages_unit.rs`

- `a_message_round_trips_through_the_log`
- `the_inbox_position_advances_past_what_is_read`
- `an_inbox_watch_matches_only_the_messages_addressed_to_its_owner`
- `a_message_read_through_the_inbox_is_not_delivered_by_a_watch`
- `a_message_a_watch_delivered_is_not_read_again`
- `the_message_kind_matches_only_message_records`
- `a_message_over_eight_kib_is_refused`

**tddy-session-activity** — `tests/session_notification_event_unit.rs` (beside the stream tests)

- `the_web_stream_never_carries_a_message` (passes in red: the skip is real)

**tddy-telegram-control** — `tests/telegram_notification_subscriber_unit.rs`

- `a_message_is_never_sent_to_telegram` (passes in red: the skip is real)

**tddy-tool-engine** — `packages/tddy-tool-engine/tests/session_message_tools.rs`

- `the_message_tools_are_offered_to_every_role` (passes in red: the catalogue is surface)
- `a_read_of_more_than_fifty_messages_is_refused`
- `a_read_hands_the_port_where_to_read_from`

**tddy-tools** — `packages/tddy-tools/tests/monitor_message_line.rs` (added at 6e)

- `a_message_event_becomes_one_message_line`
- `an_event_of_another_kind_is_no_message_line` (passes in red: nothing is a message line yet)

### Testing Principles Applied

Real session records and log, a real CLI binary, one behaviour per test, and the clock injected.

## Technical Debt & Production Readiness

- To write at implementation: a Future-enhancement TODO for sibling addressing (PRD open question 1,
  decided: not in this node).

## Decisions & Trade-offs

- **An idle session is woken by its own message watch** (user, 2026-11-17). Creating the watch is the session asking to be told, so #931's waker pastes a watched message into an idle session that holds one, exactly as for an orchestrator's watched prompt. No store change excludes self-watches from the wake.

- **A message is an activity record; the inbox references it** (user, 2026-11-17: "messages should
  be as activities; waiting for activities could use a filter to only wait for messages; the inbox
  could reference activity"). A message is stored once, in the project log. A session's inbox is a
  position over the records addressed to it, and its live inbox is a watch filtered to the kind
  `message` — so there is one wait, #934's, filtered, and no inbox store, stream or acknowledgement
  of its own. Replaces pass 2's `SessionInbox`, `session-inbox.json`, `WaitSessionMessages` and
  `monitor --inbox`.
- **The inbox watch is asked for with `CreateSessionWatch`'s existing arguments** — the kind
  `message` alone, no session selected — so no #930/#934 signature changes. It is the one watch a
  session that is not an orchestrator may hold; every other watch stays an orchestrator's.
- **`message` is in the default watch kinds** (user, 2026-11-17: kept).
- **Published as `AttentionRequired`, skipped by source** at the web stream, Telegram and the desktop
  (user, 2026-11-17: kept), so the log and the waker take it unchanged.
- **The bus getter stays** (user, 2026-11-17): the daemon publishes a post through
  `DaemonSessionHost::session_notification_bus()`.
- **PRD open questions 1–3 decided as proposed** (user, 2026-11-17): no sibling addressing; 8 KiB per
  message, sharing the log's 1 000-record window; a separate node from #934.
- **A log record, not a new store.** The cost is that messages share the log's 1 000-record window
  with attention records (PRD open question 2).
- **Skipped at the surfaces, not kept off the bus.** The bus is what wakes the waker and the long
  polls, so a message must be published there; the web stream, Telegram and desktop each skip it.
- **Complexity 5** — a boundary warning. A new source, a rule, two tools and the inbox watch stay
  together because a message nobody can read, or a reader with nothing to read, delivers nothing.

## Refactoring Needed

### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results

### /validate-changes
### /validate-tests
### /validate-prod-ready
### /analyze-clean-code

## Successor PRs

- **Tool-call activity as watchable activity** (a later node; user, 2026-11-17): the
  `agent-activity.jsonl` rows a session's tools write are activity too, and a watch could filter on
  them as it filters on `message`. Out of this node.

## TODO

- [x] Record initial discovery (`2026-11-17-session-messages-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (6e 2026-11-17: redesign — messages as activity, one filtered wait — then the redesigned tests approved)
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p tddy-daemon-kernel -p tddy-session-activity -p tddy-tool-engine -p tddy-daemon-rpc -p tddy-telegram-control -p tddy-tools -p tddy-tools-cli -p tddy-session-lifecycle`) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p <pkg> -- -D warnings` per touched package, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
