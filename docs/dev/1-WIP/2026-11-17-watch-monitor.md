# Changeset: An orchestrator streams its watches to a native monitor

**Date**: 2026-11-17
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — five crates (session-activity, tool-engine, daemon-rpc, session-tool-client, tddy-tools), ordering semantics (acknowledged positions beside #930's exactly-once delivery and #931's commit-on-paste, an in-memory lease racing the waker), a long-poll tool over every session transport, and one design question still open (does a `Monitor` notification start a turn for an idle agent)
**Stack**: `#activity-wake` 4/5 — branch `feature/activity-wake/watch-monitor`, base `feature/activity-wake/idle-wake` (#931)

## Initial Discovery

[2026-11-17-watch-monitor-initial-discovery.md](./2026-11-17-watch-monitor-initial-discovery.md)

## Prerequisites

### ⚠ DURING — `answer_execute_tool` is over its length budget — [`complexity-exec-tool-ports-execute-tool.md`](../../../packages/tddy-daemon-rpc/docs/code-issues/complexity-exec-tool-ports-execute-tool.md)

`WaitSessionWatchEvents` is served through the project-home executor's existing tool dispatch, like
`WaitProjectNotifications`; not one line is added to `answer_execute_tool`. The issue stays open and
must not grow.

### ⚠ DURING — `run_call_tool` is over its complexity budget — [`complexity-cli-run-call-tool.md`](../../../packages/tddy-tools/docs/code-issues/complexity-cli-run-call-tool.md)

`tddy-tools monitor` is a module of its own (`monitor_cli.rs`, beside `notify_cli.rs`) that calls the
session-tool dispatch directly; `run_call_tool` is not touched.

### ⚠ DURING — no LiveKit RPC call has a client-side deadline — [`2026-08-14-no-livekit-rpc-call-has-a-client-side-deadline.md`](../todo/2026-08-14-no-livekit-rpc-call-has-a-client-side-deadline.md)

A split session's monitor long-polls over LiveKit, where a peer that stops answering hangs the call.
The monitor bounds every call itself (`timeout_ms` plus a margin) and, on a lapse, prints an `error`
line and exits non-zero, so `Monitor` reports it instead of sitting silent. The entry stays open: this
bounds one caller, not the transport.

### ℹ ANSWERED — `tddy-tools/src/server.rs` is oversized — [`oversized-file-server.md`](../../../packages/tddy-tools/docs/code-issues/oversized-file-server.md)

Nothing lands in `server.rs`.

## Affected Packages

- **tddy-session-activity**: [session-notifications.md](../../../packages/tddy-session-activity/docs/session-notifications.md) — `session_watches`: acknowledged stream positions, the stream lease, and the skip in the existing deliveries
- **tddy-tool-engine** — new `project_home_watch_stream_tools`: `WaitSessionWatchEvents`, its port and refusals; the project-home catalogue lists it
- **tddy-daemon-rpc** — the port on `DaemonProjectHome` (`exec_tool/project_home_watch_stream.rs`), the bus-woken long poll
- **tddy-session-tool-client** — `WaitSessionWatchEvents` in the remote blocking-argument clamp
- **tddy-tools** / **tddy-tools-cli** — `monitor_cli.rs` and the `Monitor` subcommand

## Related Feature Documentation

- [PRD-2026-11-17-session-watch-monitor.md](../../ft/daemon/1-WIP/PRD-2026-11-17-session-watch-monitor.md)
- [Project concept](../../ft/daemon/project-concept.md), [Session notifications](../../ft/daemon/session-notifications.md)

## Summary

`tddy-tools monitor` streams an orchestrator's watch events as one JSON object per stdout line, for
Claude Code's `Monitor` tool to turn into notifications. It creates a watch, or attaches to one with
`--watch`, and loops on a new long-poll tool, `WaitSessionWatchEvents`. A streamed watch's position
advances only when the next call acknowledges what was printed. While a watch is streamed, and for
15 s after each call, its events are neither attached to tool answers nor pasted by the wake; when
the lease lapses, they return to both.

## Background

See the PRD, including why a WebSocket destination was rejected. The watch store already delivers
two ways (`take_delivery`, `deliver_with`). A stream is a third consumer of the same positions, so it
must hold the watch exclusively while it runs, and must not lose a batch the process received but
did not print.

## Scope

- [ ] Acknowledged stream positions and the lease in the watch store; the existing deliveries skip a streamed watch
- [ ] `WaitSessionWatchEvents`: the tool, its refusals, the bus-woken long poll
- [ ] The remote clamp for its `timeout_ms`
- [ ] `tddy-tools monitor`: create or attach, the loop, the line protocol, exit codes, the per-call deadline
- [ ] Docs: `project-concept.md` (the monitor), package docs
- [ ] Manual: under Claude Code's `Monitor`, an idle orchestrator is told of a watched permission prompt (PRD open question 1)

## Responsibility

This PR owns **streaming a watch to a process**:

- in `tddy_session_activity::session_watches`: `StreamBatch`, `StreamRefusal`,
  `SessionWatchStore::take_stream_batch` (hand the events after the acknowledged position, advancing
  on `ack_seq`, holding the lease) and `SessionWatchStore::is_streamed`, with
  `STREAM_LEASE_GRACE` (15 s);
- the skip of streamed watches inside the store's existing deliveries;
- `tddy_tool_engine::project_home_watch_stream_tools` — `WAIT_SESSION_WATCH_EVENTS`, its definition,
  `ProjectHomeWatchStreamPort`, its refusal texts;
- the port's implementation in `tddy-daemon-rpc` and its bus wake-up;
- the clamp entry in `tddy-session-tool-client`;
- `tddy-tools monitor` — `MonitorArgs`, `MonitorLine`, the loop and its exit codes.

## Boundaries

- **No WebSocket**, client or server, and no daemon listener change (see the PRD).
- **No change to a watch**: its filter, kinds, TTL, limits, endings and refusals are #930's.
  `CreateSessionWatch`'s definition and text are not edited.
- **No change to the wake's decisions** — idle, settle, gap, wording — which are #931's. The waker
  is not edited: it sees a streamed watch as having nothing to deliver, because the store says so.
- **No new notification, no proto change.** The stream is a reader of the project log.
- **No `message` line, no inbox.** Those are `feature/activity-wake/session-messages`'s.
- **No tool-call activity** (the `agent-activity.jsonl` rows): out of scope (PRD open question 2).
- **Orchestrators only**, one host — as #930's watches.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/activity-wake/session-watches` (#930) | `SessionWatchStore`, `SessionWatch`, `WatchOwner`, `WatchKind`, `WatchDelivery`, `take_delivery`, the `watchEvents` attachment, `CreateSessionWatch` and its port, the project-home role guard | `take_stream_batch` reads the same watches and positions; `tddy-tools monitor` creates a watch through `CreateSessionWatch`; the attachment stops seeing a streamed watch's events because `take_delivery` skips it | change a watch's shape, the matcher's kinds, `CreateSessionWatch`, or the attachment function |
| `feature/activity-wake/idle-wake` (#931) | `deliver_with` (commit on success), `WatchWaker`, `orchestrator_is_idle`, `wake_text` | `deliver_with` skips a streamed watch the same way; a monitor line's `summary` uses `wake_text`'s per-event wording | change the waker, its timings, the idle rule, or the wake text |

Changing *which* watches `take_delivery` and `deliver_with` consider (skipping streamed ones) is this
node's behaviour, and it is added inside the store. Their signatures, and their behaviour for every
watch nobody streams, are untouched.

## Draft PR contract

The first push of this PR's code (commit 2) publishes:

- `StreamBatch`, `StreamRefusal`, `STREAM_LEASE_GRACE`, `SessionWatchStore::take_stream_batch` and
  `is_streamed`, returning `// TODO(watch-monitor): implement` refusals / `false`;
- `WAIT_SESSION_WATCH_EVENTS`, its definition in the project-home catalogue, and
  `ProjectHomeWatchStreamPort`, the daemon's implementation refusing with a TODO text;
- the clamp entry;
- `tddy-tools monitor` parsing its arguments and printing an `error` line with the TODO text;
- every test below, **failing** on the missing behaviour, not on compilation.

## Green wave

**Wave:** 3 of 4
**Greenable independently:** no — its skip and lease tests need #930's deliveries and #931's
`deliver_with` and waker to behave (an unstreamed watch must still be answered and woken), and its
CLI tests create watches through #930's `CreateSessionWatch`.
**Concurrent with:** none
**Blocks:** `feature/activity-wake/session-messages` — `monitor --inbox` reuses this node's loop,
line protocol, acknowledgement and per-call deadline.

Real dependency edges, as opposed to the branch line:

    session-ended-notice → session-watches → idle-wake → watch-monitor → session-messages
    (surface)              (behaviour)       (behaviour)    (behaviour)

## Technical Changes

### State A (Current — on `feature/activity-wake/idle-wake`)

- `SessionWatchStore` keeps per-project `session-watches.json`; one async lock per project
  serialises every operation. `take_delivery` advances and persists; `deliver_with` advances only when
  its closure succeeds; `has_live_watches` answers the auto-stop hold.
- `watch_events.rs` attaches `take_delivery`'s result to each unary `ExecuteTool` answer as
  `watchEvents`; `watch_wake.rs` pastes `deliver_with`'s result into an idle orchestrator.
- `WaitProjectNotifications` is the long-poll model: `MAX_WAIT_MS = 20_000`, held until the bus
  publishes for the project or the timeout passes, clamped for remote calls in
  `clamp_remote_blocking_args`.
- `tddy-tools` subcommands live in `tddy-tools-cli/src/main.rs`; session-scoped ones resolve the
  session with `session_source::resolve_current_session` and dispatch with `dispatch_session_tool`.
- Nothing streams a watch, and nothing outside a tool answer or a paste ever receives one.

### State B (Target)

- **Store.** `take_stream_batch(owner, watch_ids, ack_seq, log, now) -> Result<StreamBatch,
  StreamRefusal>`:
  - under the project lock, first advances each named watch's position to `ack_seq` (refused when
    `ack_seq` is past what was handed, or names a watch not held by this stream);
  - renews the lease;
  - returns the matched records after the new position (union by `seq`, oldest first, at most 50),
    the `lost` count and the ended watches, **without** advancing past them.

  A watch is streamed while a lease is held (`now < lease_until`); the lease is in memory only and
  renewed by each call. `take_delivery` and `deliver_with` skip a streamed watch entirely.
- **Tool.** `WaitSessionWatchEvents { watch_ids, ack_seq?, timeout_ms ≤ 20 000 }` →
  `{ events, lost, ended, next_ack_seq }`. It answers at once when `take_stream_batch` has something,
  and otherwise waits on the bus for a record of the owner's project, or the timeout.
  - Refusals: not an orchestrator; a watch that is not the caller's or has ended; a watch streamed by
    another call (lease held, different stream); `timeout_ms` above the cap.
  - The lease's holder is the stream id the first call returns and each later call presents.
- **CLI.** `tddy-tools monitor`:
  - With `--watch`, it attaches; otherwise it creates a watch via `CreateSessionWatch` and prints the
    `watch` line.
  - It then loops `WaitSessionWatchEvents` with the last printed `seq` as `ack_seq`. Each event is
    printed as one compact JSON line and flushed; `lost` and `ended` are printed as they come.
  - It exits 0 when every streamed watch has ended, and non-zero after an `error` line on any
    refusal, transport error or per-call deadline lapse (`timeout_ms` + 10 s).

### Delta (What's Changing)

#### tddy-session-activity

- `session_watches.rs`: `StreamBatch`, `StreamRefusal`, `STREAM_LEASE_GRACE`, `take_stream_batch`,
  `is_streamed`; the skip in the shared selection that `take_delivery` and `deliver_with` both use.
- New `session_watches/stream.rs` for the lease map and the acknowledgement.

#### tddy-tool-engine

- New `project_home_watch_stream_tools.rs`; `project_home_catalogue.rs` lists it for the project-home
  role.

#### tddy-daemon-rpc

- New `exec_tool/project_home_watch_stream.rs` (the port and its bus wait); `DaemonProjectHome`
  routes the tool to it.

#### tddy-session-tool-client

- `remote_block_arg_keys`: `WaitSessionWatchEvents` → `timeout_ms`.

#### tddy-tools / tddy-tools-cli

- New `tddy-tools/src/monitor_cli.rs`; `Subcommand::Monitor` in `tddy-tools-cli/src/main.rs`.

## Implementation Milestones

- [ ] Store: acknowledgement, lease, skip; unit tests green
- [ ] Tool and port, bus-woken; acceptance green
- [ ] Clamp
- [ ] CLI against the harness daemon; CLI acceptance green
- [ ] Docs; the manual `Monitor` check

## Testing Plan

### Testing Strategy

What can go wrong quietly is concentrated in the store: an event delivered twice across a stream, an
answer and a wake; a batch lost at a kill; a lease that never lapses. So that lives in unit tests
against a real `ProjectNotificationLog` in a temp dir, fed through the real bus, with the clock
injected. The tool and the skip are acceptance-tested through `tddy-daemon-rpc`'s project-home harness
(the `World` of `project_home_session_watch_acceptance.rs`, the recording terminal of #931's wake
tests). The CLI is tested end to end against that harness's daemon socket, reading stdout line by
line as `Monitor` would.

### Acceptance Tests

**tddy-daemon-rpc** — `packages/tddy-daemon-rpc/tests/project_home_watch_stream_acceptance.rs`

- `a_streamed_watch_hands_its_events_to_the_stream_and_not_to_tool_answers`
- `a_streamed_watch_does_not_wake_its_idle_orchestrator`
- `events_after_the_last_acknowledgement_return_to_answers_and_the_wake_once_the_lease_lapses`
- `an_unacknowledged_batch_is_handed_again`
- `a_second_stream_on_a_streamed_watch_is_refused_naming_the_watch`
- `streaming_a_watch_that_is_not_the_callers_or_has_ended_is_refused`
- `a_session_that_is_not_an_orchestrator_is_refused_the_stream_tool`
- `the_wait_answers_empty_at_its_timeout`

**tddy-tools-cli** — `packages/tddy-tools-cli/tests/monitor_cli_acceptance.rs`

- `monitor_prints_a_watch_line_then_one_event_line_per_matched_record`
- `every_stdout_line_is_one_json_object`
- `monitor_exits_zero_after_an_ended_line_for_each_watch`
- `monitor_prints_an_error_line_and_exits_non_zero_on_a_refusal`
- `monitor_with_watch_continues_from_the_acknowledged_position`

### Unit Tests

**tddy-session-activity** — `packages/tddy-session-activity/tests/session_watch_stream_unit.rs`

- `a_stream_batch_does_not_advance_until_acknowledged`
- `acknowledging_past_what_was_handed_is_refused`
- `take_delivery_skips_a_streamed_watch`
- `deliver_with_skips_a_streamed_watch`
- `the_lease_lapses_after_its_grace_with_no_call`
- `one_stream_holds_a_watch_at_a_time`

**tddy-tool-engine** — `packages/tddy-tool-engine/tests/project_home_watch_stream_tools.rs`

- `the_stream_tool_is_offered_to_the_project_home_role_only`
- `the_wait_is_capped_at_twenty_seconds`
- `the_stream_tool_hands_the_port_its_watches_and_acknowledgement`

**tddy-session-tool-client** — `packages/tddy-session-tool-client/tests/` (beside the existing clamp tests)

- `the_stream_wait_is_clamped_for_a_remote_call`

### Testing Principles Applied

Real log, real store, real harness, real CLI binary; injected clock for lease and expiry; the
exactly-once property raced with real tasks; stdout read line by line, never as a whole blob.

## Technical Debt & Production Readiness

- The lease is in memory: a daemon restart ends every stream's hold. That is the intended fallback,
  not debt.
- PRD open question 1 (does a `Monitor` notification start a turn for an idle agent) must be checked
  on a real Claude Code session before green. If the answer is no, the lease must not suppress the
  wake, and this changeset's State B changes.

## Decisions & Trade-offs

- **Command, not WebSocket** — see the PRD § Considered.
- **Acknowledge, don't take.** A stream advances on the next call's `ack_seq`, so a process killed
  mid-batch loses nothing; the cost is a possible re-print of one batch, recognisable by `seq`.
- **A lease, not a flag.** The stream holds a watch only while it keeps calling, so a dead monitor
  hands its watch back to answers and the wake on its own, with no cleanup call.
- **Complexity 5** is a boundary warning. The store change, the tool and the CLI stay in one node
  because none of them delivers anything alone: a stream tool with no CLI reaches no agent, and a CLI
  with no tool has nothing to read.

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

- `feature/activity-wake/session-messages` (`#activity-wake` 5/5)

## TODO

- [x] Record initial discovery (`2026-11-17-watch-monitor-initial-discovery.md`)
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
- [ ] Run the touched packages' tests (`./test -p tddy-session-activity -p tddy-tool-engine -p tddy-daemon-rpc -p tddy-session-tool-client -p tddy-tools-cli`) — verify 100% pass
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
