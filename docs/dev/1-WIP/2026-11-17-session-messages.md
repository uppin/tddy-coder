# Changeset: Sessions send each other messages, and a monitor streams them

**Date**: 2026-11-17
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — six crates (daemon-kernel, session-activity, tool-engine, daemon-rpc, telegram-control, tddy-tools) plus a one-arm skip in the desktop notifier, a new notification source every surface must handle, an addressing rule read from session records, and ordering semantics (an inbox position beside the watch positions, a message delivered once when both reach it)
**Stack**: `#activity-wake` 5/5 — branch `feature/activity-wake/session-messages`, base `feature/activity-wake/watch-monitor` (#934)

## Initial Discovery

[2026-11-17-session-messages-initial-discovery.md](./2026-11-17-session-messages-initial-discovery.md)

## Prerequisites

### ⚠ DURING — `answer_execute_tool` is over its length budget — [`complexity-exec-tool-ports-execute-tool.md`](../../../packages/tddy-daemon-rpc/docs/code-issues/complexity-exec-tool-ports-execute-tool.md)

`PostSessionMessage` and `ReadSessionMessages` are offered to every role and served through the
executor's existing tool dispatch; not one line is added to `answer_execute_tool`.

### ⚠ DURING — `run_call_tool` is over its complexity budget — [`complexity-cli-run-call-tool.md`](../../../packages/tddy-tools/docs/code-issues/complexity-cli-run-call-tool.md)

`tddy-tools send` is its own module (`send_cli.rs`); `monitor --inbox` extends `monitor_cli.rs`
(#934's). `run_call_tool` is not touched.

### ⚠ DURING — the Telegram notifier's complexity issues — [`complexity-telegram-notifier-on-server-message.md`](../../../packages/tddy-telegram-control/docs/code-issues/complexity-telegram-notifier-on-server-message.md)

A message never reaches Telegram: the skip is one arm in `telegram_session_subscriber.rs`'s source
match, and nothing is added to the notifier.

### ℹ ANSWERED — ACP turn input is not delivered — [`2026-11-13-acp-turn-input-is-not-delivered.md`](../todo/2026-11-13-acp-turn-input-is-not-delivered.md)

A message needs no `turn_input`, so it is the first way to tell an ACP or Cursor session anything.
The entry stays open.

## Affected Packages

- **tddy-daemon-kernel**: [daemon-kernel.md](../../../packages/tddy-daemon-kernel/docs/daemon-kernel.md) — `SessionNotificationSource::Message` and its `SessionMessage` part
- **tddy-session-activity**: [session-notifications.md](../../../packages/tddy-session-activity/docs/session-notifications.md) — the `message` record in the log; the inbox position; the watch kind `message`; the web stream skips the source
- **tddy-tool-engine** — new `session_message_tools`: `PostSessionMessage`, `ReadSessionMessages`, their port and refusals, offered to every role; `CreateSessionWatch` accepts the kind `message` through #930's kind vocabulary
- **tddy-daemon-rpc** — the port and the addressing rule (`exec_tool/session_messages.rs`)
- **tddy-telegram-control** — `telegram_session_subscriber.rs` skips the source
- **tddy-desktop** — `native_notifications.rs` skips the source
- **tddy-tools** / **tddy-tools-cli** — `send_cli.rs`, `Subcommand::Send`; `monitor --inbox` and the `message` line in `monitor_cli.rs`

## Related Feature Documentation

- [PRD-2026-11-17-session-messages.md](../../ft/daemon/1-WIP/PRD-2026-11-17-session-messages.md)
- [Project concept](../../ft/daemon/project-concept.md), [Session notifications](../../ft/daemon/session-notifications.md)

## Summary

Any session can send a message to a session it may address: its orchestrator, the project's main
session, the sender of a message it received, or (for an orchestrator) any session it can see. A
message is a `message` record in the project notification log. It is never typed into a terminal and
never shown in the drawer, on the web stream or in Telegram.

The recipient receives it through its **inbox**: `tddy-tools monitor --inbox` streams it as a
`message` line, or `ReadSessionMessages` reads it. An orchestrator can also **watch** what its
sessions send, through a seventh watch kind, `message`. A message that reaches it both ways is
delivered once.

## Background

See the PRD. `SendSessionMessage` is the only agent-to-agent verb, and it is orchestrator-only,
pastes a turn and needs `turn_input`. A delegated session therefore has no way to answer the
orchestrator that started it except attention notifications meant for a human.

## Scope

- [ ] The `message` source: kernel type, log storage, skipped by the web stream, Telegram and desktop
- [ ] `PostSessionMessage` with the addressing rule and its refusals
- [ ] The inbox position; `ReadSessionMessages`
- [ ] `tddy-tools monitor --inbox` and the `message` line
- [ ] The watch kind `message`, delivered once alongside the inbox
- [ ] `tddy-tools send`
- [ ] Docs: `project-concept.md`, `session-notifications.md` (the source), package docs

## Responsibility

This PR owns **messages between sessions**:

- `SessionNotificationSource::Message` and `SessionMessage { to_session_id, message_id, reply_to,
  text }` in `tddy-daemon-kernel`, and their storage in the project log;
- `tddy_session_activity::session_messages` — `SessionInbox`, the per-session acknowledged position
  (persisted beside `session-watches.json`), `MAX_MESSAGE_BYTES` (8 KiB);
- the matcher's `message` kind;
- `tddy_tool_engine::session_message_tools` — `POST_SESSION_MESSAGE`, `READ_SESSION_MESSAGES`, their
  definitions, `SessionMessagePort`, refusal texts;
- the addressing rule and the port in `tddy-daemon-rpc`;
- the skips in the web stream, Telegram and desktop;
- `tddy-tools send`, and `monitor --inbox` with its `message` line.

## Boundaries

- **No turn.** A message never pastes into a terminal; `SendSessionMessage` is unchanged.
- **No sibling addressing** — a session may not message another session its orchestrator started
  (PRD open question 1).
- **No cross-host message**, no host argument.
- **No web surface**, no proto change: the source never reaches the stream.
- **No change to the monitor's loop, lease, acknowledgement or deadline** — #934's. This node adds a
  line type and an inbox source to them.
- **No change to watches' six kinds, endings or limits** — #930's. This node adds the seventh kind.
- **No change to the wake's decisions** — #931's. A `message` watch reaches the waker through the
  store like every other kind.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/activity-wake/session-ended-notice` (#929) | the precedent of a new `SessionNotificationSource` with a structured part (`SessionEnded` / `SessionEndNotice`), carried through the log | `Message` / `SessionMessage` follow the same shape | change `SessionEnded` or its forwarder |
| `feature/activity-wake/session-watches` (#930) | `SessionWatchStore`, the matcher, `WatchKind`, `CreateSessionWatch`'s kind validation | adds `WatchKind::Message` to the matcher; `CreateSessionWatch` accepts it through the kind vocabulary | change the six kinds, a watch's shape, the tool's other refusals or the `watchEvents` attachment |
| `feature/activity-wake/idle-wake` (#931) | the waker, `deliver_with`, `wake_text` | a `message` event is woken like any kind; `wake_text` gains its wording for the kind | change the idle rule, the timings or the wording of other kinds |
| `feature/activity-wake/watch-monitor` (#934) | `tddy-tools monitor`, `MonitorLine`, `WaitSessionWatchEvents`, acknowledgement, the lease, the per-call deadline | `--inbox` runs the same loop over a `WaitSessionMessages`-shaped call on the inbox; `MonitorLine::Message` is added beside #934's line types | change the loop, the lease, acknowledgement, or the existing line types |

Adding a variant to `SessionNotificationSource` puts a new arm into every exhaustive `match` on it
(about ten sites, listed in the discovery). That is the variant's mechanical fallout, not a change
to those modules' behaviour for the existing sources.

## Draft PR contract

The first push of this PR's code (commit 2) publishes:

- `SessionNotificationSource::Message`, `SessionMessage`, their log fields; every exhaustive match
  given its arm (the web, Telegram and desktop skips real, since they are trivial; the rest
  `// TODO(session-messages): implement` where behaviour is owed);
- `session_messages` with `SessionInbox` and `MAX_MESSAGE_BYTES`, methods refusing with TODO texts;
- `WatchKind::Message` named in the vocabulary, matching nothing yet;
- `POST_SESSION_MESSAGE`, `READ_SESSION_MESSAGES`, their definitions in every role's catalogue,
  `SessionMessagePort`, the daemon's implementation refusing with a TODO text;
- `tddy-tools send` and `monitor --inbox` parsing their arguments and printing an `error` line with
  the TODO text;
- every test below, **failing** on the missing behaviour, not on compilation.

## Green wave

**Wave:** 4 of 4
**Greenable independently:** no — `monitor --inbox` tests run #934's loop, acknowledgement and lease,
and the `message` watch tests need #930's deliveries and #931's wake to behave.
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
  them.
- `SendSessionMessage` (`project_home_turn_tools.rs:35`) is the only way one session's agent reaches
  another's: orchestrator-only, a pasted turn, refused without `turn_input`.
- A session's record names the orchestrator that started it (`orchestrator_session_id`,
  `tddy-agent-launch/src/conversation_spawn.rs:88`).

### State B (Target)

- **Record.** `PostSessionMessage` publishes a `SessionNotification` of source `Message`, with
  `session_id` = sender and the `SessionMessage` part. Bus subscribers see it: the waker cues on it,
  and the long polls wake on it. The log stores it; the web stream, Telegram and the desktop notifier
  skip the source.
- **Addressing.** For a caller in project P, OS user U, on this host, the recipient must be in P, run
  as U, and not be the caller. It must also be one of:
  - any visible session, when the caller is an orchestrator;
  - the caller's `orchestrator_session_id`;
  - P's main session;
  - the sender of the message `reply_to` names, which the caller must have received.

  Each refusal names who the caller may address.
- **Inbox.** `SessionInbox` keeps one acknowledged `seq` per recipient, per project, persisted beside
  `session-watches.json`.
  - `ReadSessionMessages` answers `message` records addressed to the caller after that `seq` (at most
    50) and advances past them.
  - `monitor --inbox` reads the same records through a long-poll call and advances on its next call's
    acknowledgement, as #934's stream does.
  - The orchestrator's own `message` records addressed to it are excluded from its watches' delivery
    when its inbox has already acknowledged them, and the reverse, so each is delivered once.
- **Watch kind.** `WatchKind::of` maps `source = message` to `Message`. A `message` watch selects by
  sender, through #930's session selection.

### Delta (What's Changing)

#### tddy-daemon-kernel

- `SessionNotificationSource::Message`; `SessionMessage`; the part on `SessionNotification`.

#### tddy-session-activity

- `project_notification_log/storage.rs`: the message fields.
- New `session_messages.rs` (`SessionInbox`).
- `session_watches` matcher: the `message` kind; the once-only rule against the inbox.
- `streams.rs`: the web stream skips the source.

#### tddy-tool-engine

- New `session_message_tools.rs`; every role's catalogue lists the two tools.

#### tddy-daemon-rpc

- New `exec_tool/session_messages.rs` (port, addressing, long poll for `--inbox`).

#### tddy-telegram-control, tddy-desktop

- One skip arm each.

#### tddy-tools / tddy-tools-cli

- New `send_cli.rs`, `Subcommand::Send`; `monitor_cli.rs`: `--inbox`, `MonitorLine::Message`.

#### every exhaustive match on `SessionNotificationSource`

- The `Message` arm.

## Implementation Milestones

- [ ] Source, storage, skips; storage tests green
- [ ] Post with addressing; acceptance green
- [ ] Inbox, read, `--inbox`; acceptance and CLI green
- [ ] Watch kind, once-only with the inbox
- [ ] `send`; docs

## Testing Plan

### Testing Strategy

The addressing rule and the once-only delivery are where this can go wrong quietly, so the rule is
acceptance-tested case by case through `tddy-daemon-rpc`'s project-home harness, using real session
records with `orchestrator_session_id` set. The once-only delivery is unit-tested on the store
against a real log. The CLI is tested end to end, as #934's monitor tests are.

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
- `a_session_without_turn_input_reads_its_messages`

**tddy-tools-cli** — `packages/tddy-tools-cli/tests/send_and_inbox_cli_acceptance.rs`

- `send_prints_the_message_id_and_the_recipient_receives_it`
- `monitor_inbox_prints_one_message_line_per_message`
- `a_message_text_with_newlines_stays_on_one_line`
- `monitor_inbox_reprints_an_unacknowledged_message_after_a_kill`

### Unit Tests

**tddy-session-activity** — `packages/tddy-session-activity/tests/session_messages_unit.rs`

- `a_message_round_trips_through_the_log`
- `the_inbox_advances_only_when_acknowledged`
- `a_message_acknowledged_by_the_inbox_is_not_delivered_by_a_watch`
- `the_message_kind_matches_only_message_records`
- `a_message_over_eight_kib_is_refused`

**tddy-session-activity** — `tests/` beside the stream tests

- `the_web_stream_never_carries_a_message`

**tddy-telegram-control** — beside the subscriber tests

- `a_message_is_never_sent_to_telegram`

### Testing Principles Applied

Real session records and log, a real CLI binary, one behaviour per test, and the clock injected.

## Technical Debt & Production Readiness

- To write at implementation: a Future-enhancement TODO for sibling addressing if the developer
  defers it (PRD open question 1).

## Decisions & Trade-offs

- **A log record, not a new store.** Messages ride the project log so watches, the wake and the
  monitor reach them without a second delivery path. The cost is that they share the log's 1 000-record
  window with attention records (PRD open question 2).
- **Skipped at the surfaces, not kept off the bus.** The bus is what wakes the waker and the long
  polls, so a message must be published there; the web stream, Telegram and desktop each skip it.
- **A separate node from #934**, at the developer's direction: the monitor is reviewable alone, and
  merging the two would push one node past complexity 5.
- **Complexity 5** — a boundary warning. A new source, a rule and two tools stay together because a
  message nobody can read, or a reader with nothing to read, delivers nothing.

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

- None planned.

## TODO

- [x] Record initial discovery (`2026-11-17-session-messages-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p tddy-daemon-kernel -p tddy-session-activity -p tddy-tool-engine -p tddy-daemon-rpc -p tddy-telegram-control -p tddy-tools-cli`) — verify 100% pass
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
