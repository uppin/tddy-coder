# Changeset: A session whose agent ends says so

**Date**: 2026-11-16
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — more than one crate (cli-sessions, daemon-kernel, session-activity, session-lifecycle, service, daemon, session-web), a proto contract change, a spawned-process seam (real PTY child, tmux detach watcher)
**Stack**: `#activity-wake` 1/3 — branch `feature/activity-wake/session-ended-notice`, base `master` — PR [#929](https://github.com/uppin/tddy-coder/pull/929)

## Initial Discovery

[2026-11-16-session-ended-notice-initial-discovery.md](./2026-11-16-session-ended-notice-initial-discovery.md)

## Prerequisites

### ⚠ DURING — `CliSessionManager::spawn_tool_on` is over its length budget — [`complexity-pty-spawn-spawn-tool-on.md`](../../../packages/tddy-cli-sessions/docs/code-issues/complexity-pty-spawn-spawn-tool-on.md)

The announcement is raised from `spawn_terminal_cleanup` and the detach watcher, **not** from
`spawn_tool_on`. Nothing is added to `spawn_tool_on`; this node must not grow it.

### ⚠ DURING — `build_session_host` is over its length budget — [`complexity-mount-host-build-session-host.md`](../../../packages/tddy-daemon/docs/code-issues/complexity-mount-host-build-session-host.md)

The forwarder is started by a function of its own, called where the bus is assembled
(`mount_host.rs`, `open_shared`'s caller), never inline in `build_session_host` or `open_shared`.

### ℹ ANSWERED — split agents' exit cause is unknown — [`2026-11-15-split-agent-exit-cause-unknown.md`](../todo/2026-11-15-split-agent-exit-cause-unknown.md)

That entry is about *why* a split agent exits (the log line #905 added); it stays open. This node
publishes the **fact** of an exit for every main the session manager runs, a split agent's included
on the host that runs it — it does not diagnose the cause.

### ℹ ANSWERED — `tddy-session-activity` has no tests of its own — [`2026-09-12-tddy-session-activity-has-no-tests-of-its-own.md`](../todo/2026-09-12-tddy-session-activity-has-no-tests-of-its-own.md)

Stale in part (the crate has `tests/` today). This node adds storage tests there; it does not close
the entry.

## Affected Packages

- **tddy-cli-sessions**: the session manager announces agent ends
  - [module-layout.md](../../../packages/tddy-cli-sessions/docs/module-layout.md) — `agent_end.rs`, the two announcing sites
- **tddy-daemon-kernel**: [daemon-kernel.md](../../../packages/tddy-daemon-kernel/docs/daemon-kernel.md) — `SessionNotificationSource::SessionEnded`, `SessionEndNotice`, `SessionNotification.ended`
- **tddy-session-activity**: [session-notifications.md](../../../packages/tddy-session-activity/docs/session-notifications.md) — the log's JSON: `source: "session_ended"`, `ended`
- **tddy-session-lifecycle**: [README.md](../../../packages/tddy-session-lifecycle/README.md) — the agent-end forwarder
- **tddy-service** — `activity.proto`: one source value, `SessionEndNotice`
- **tddy-daemon** — starts the forwarder
- **tddy-telegram-control**: exhaustive `match` on the source only — declines it
- **tddy-session-web**: [README.md](../../../packages/tddy-session-web/README.md) — the drawer's dot ignores the new source
- **tddy-client-web**, **tddy-rust-typescript-tests**: regenerated `activity_pb.ts`
- **tddy-desktop** (tests only) — a `SessionNotification` literal in `src-tauri/tests/support`

## Related Feature Documentation

- [PRD-2026-11-16-session-ended-notice.md](../../ft/daemon/1-WIP/PRD-2026-11-16-session-ended-notice.md)
- [Session notifications](../../ft/daemon/session-notifications.md), [Project concept](../../ft/daemon/project-concept.md)

## Summary

When the agent of a session's main terminal ends, the daemon publishes one `ATTENTION_REQUIRED`
notification with source `SESSION_ENDED`, text `⏹ Session {label} ended: {outcome}` and a structured
`ended { outcome, exit_code }`. It reaches the project notification log and so the project-home inbox
tools. Neither `/clear` nor a tab raises it, Telegram does not send it, and the drawer does not light for it.

## Background

An orchestrator cannot tell a session that is quiet from one whose agent is gone: `Ended` raises
nothing, and is also written by `/clear`; a main's exit is only logged. `#activity-wake` needs the
fact as an event — as the watchable kind `ended`, and to end a watch whose sessions have all ended —
and every inbox reader gains from it now.

## Scope

- [ ] The session manager announces every agent end, from both sites, exactly once each
- [ ] The new source and `SessionEndNotice` through the bus model, the proto, the conversions and the log's JSON
- [ ] The forwarder: an announced end → a labelled, owned bus notification
- [ ] The daemon starts the forwarder after the bus is assembled
- [ ] Telegram declines it; the drawer's dot ignores it
- [ ] Feature and package docs updated

## Responsibility

This PR owns **the fact that a session's agent ended, as a session notification**:

- `tddy_cli_sessions::cli_session_manager::AgentEnd` and `CliSessionManager::agent_ends()`;
- `SessionNotificationSource::SessionEnded`, `SessionEndNotice`, `SessionNotification.ended`;
- `SESSION_NOTIFICATION_SOURCE_SESSION_ENDED`, `message SessionEndNotice`, `SessionNotificationEvent.ended`;
- the log's `session_ended` source value and `ended` object;
- `tddy_session_lifecycle::session_notifications::{notification_for_agent_end, forward_agent_ends}`;
- the daemon's start of the forwarder;
- the drawer's rule that this source lights no dot.

## Boundaries

- **No watch, no filter, no wake.** Watches are `feature/activity-wake/session-watches`; the wake is
  `feature/activity-wake/idle-wake`. Nothing here knows what an orchestrator is.
- **No change to the inbox tools' arguments or scope.** `ReadProjectNotifications` gains a record
  kind by the log storing it, not by any new parameter.
- **`SessionEnd` / `Ended` keeps raising nothing** — `notification_for_activity_status` is untouched.
- **No Telegram message.** Its subscriber's source match gains the arm, declining it.
- **No exit-cause diagnosis** (the split-agent TODO stays open), no change to the `main exited` log
  line, the exit hooks or their order.
- **No web rendering of the end.** The drawer ignores it; showing "ended" anywhere is not this PR's.
- **Tabs** (non-main terminals) never announce.

## Dependencies

None — this is the root node, cut from `master` @ `769c16e86`.

## Draft PR contract

The first push of this PR's code (pass 2, commit 2) publishes:

- `AgentEnd { session_id, os_user: Option<String>, outcome: String, exit_code: Option<i32> }` and
  `CliSessionManager::agent_ends() -> tokio::sync::broadcast::Receiver<AgentEnd>`, announcing nothing
  yet (`// TODO(session-ended-notice): implement`);
- the kernel types and the `ended: None` on every existing `SessionNotification` literal;
- the proto value and message, generated code, and the wire conversion's source arm (`ended` not yet
  carried);
- `notification_for_agent_end` and `forward_agent_ends` with `todo!`-free bodies that publish
  nothing yet;
- the storage DTO's `session_ended` value and `ended` object;
- the web's `dotForNotification` returning today's answer for every event;
- every acceptance and unit test below, **failing** on the missing behaviour, not on compilation.

## Green wave

**Wave:** 1 of 2
**Greenable independently:** yes — every test spawns its own stub child or publishes its own
notification; nothing needs another node.
**Concurrent with:** `feature/activity-wake/session-watches` (it consumes this node's *surface* —
the `SessionEnded` source — not its behaviour; its tests publish their own ended notifications)
**Blocks:** none in wave terms; `feature/activity-wake/idle-wake` is wave 2 because of
`session-watches`, not this node.

Real dependency edges, as opposed to the branch line:

    session-ended-notice → session-watches (surface)      session-watches → idle-wake (behaviour)

## Technical Changes

### State A (Current)

- `spawn_terminal_cleanup` (`pty_spawn.rs:243-298`) waits for the task to end, logs `main exited`
  (main only), runs the `on_exit` hook, classifies a main's end (`classify_ended_main` →
  `MainExit::{ClientOnly, AgentEnded}`), deregisters the terminal and calls
  `DetachWatch::main_ended`. `AgentEnded` drops the managed workflow and exposure; `ClientOnly` with a
  tmux session starts a watcher that runs the same cleanup when `tmux_session_alive` turns false
  (`tmux_reattach.rs:62-104`). **Neither site tells anyone outside the manager.**
- `outcome_of(&TaskStatus)` (`main_exit.rs`) words an end: `exit code N`, `exit code unknown`,
  `stopped by the daemon`, a failure's message.
- `SessionNotificationSource` has five values (kernel `session_notification.rs:22-35`, proto
  `activity.proto:473-486`); `SessionNotification` has `agent` and `question` optional parts and no
  `Default`.
- The bus (`SessionNotificationBus`) is assembled in `tddy-daemon/src/runtime/mount_host.rs:87-108`
  with the project log as a subscriber; the `CliSessionManager` is created earlier, in
  `runtime/integrations.rs:43`, and held as `registries.shared_cli_manager`.
- The label rule is `tddy_session_lifecycle::session_notifications::resolve_session_label`.
- The project log stores `AttentionRequired` only; its DTO maps the source in
  `project_notification_log/storage.rs:350-360`.
- The drawer (`useSessionNotifications.ts:51-56`) lights yellow for every `ATTENTION_REQUIRED`.

### State B (Target)

- Both agent-end sites send one `AgentEnd` on the manager's broadcast channel: the immediate
  `AgentEnded` with `outcome_of(status)` and the exit code, the watcher's with `exit code unknown`
  and no code. `ClientOnly` sends nothing.
- The daemon subscribes once the bus exists and runs `forward_agent_ends`: per end, resolve the
  sessions base from the OS user (an end with no OS user is attributed to the daemon's own user, the
  user such a spawn ran as; one whose base cannot be resolved is logged at `warn` and dropped — a
  notification naming no owner reaches nobody), resolve the label, publish
  `notification_for_agent_end(end, label)`.
- The log stores it (`source: "session_ended"`, `ended: {outcome, exit_code}`), so the inbox tools
  return it.
- Telegram's source match declines it; the drawer's `dotForNotification` returns none for it.
- A lagging forwarder (`RecvError::Lagged`) logs how many ends it missed at `warn` and continues —
  ends are rare, and the channel is sized well past any burst a host produces (`AGENT_END_CAPACITY`).

### Delta (What's Changing)

#### tddy-cli-sessions

- New `cli_session_manager/agent_end.rs`: `AgentEnd`, `AGENT_END_CAPACITY`, the sender held by the
  manager and cloned into `DetachWatch`.
- `spawn_terminal_cleanup`: after classification, `AgentEnded` sends.
- `DetachWatch::start`'s `Ok(false)` arm sends.

#### tddy-daemon-kernel

- `SessionNotificationSource::SessionEnded`; `SessionEndNotice { outcome, exit_code }`;
  `SessionNotification.ended: Option<SessionEndNotice>`.

#### tddy-service

- `activity.proto`: `SESSION_NOTIFICATION_SOURCE_SESSION_ENDED = 6;`
  `message SessionEndNotice { string outcome = 1; optional int32 exit_code = 2; }`;
  `SessionEndNotice ended = 13;` on `SessionNotificationEvent`.

#### tddy-session-activity

- `storage.rs`: the source value, the `ended` object (omitted when `None`), read back on replay.
- `streams.rs`: the new source arm and `ended` on the wire.

#### tddy-session-lifecycle

- `session_notifications.rs`: `notification_for_agent_end`, `forward_agent_ends`.

#### tddy-daemon

- `runtime/mount_host.rs`: a `start_agent_end_forwarder` function, called after the bus is built.

#### tddy-telegram-control

- `telegram_session_subscriber.rs:240`: `SessionEnded => false`.

#### tddy-session-web

- `lib/sessionNotificationDot.ts` (new): `dotForNotification(event) -> 'attention' | 'activity' | null`;
  `useSessionNotifications.ts` uses it.

#### every crate with a `SessionNotification` literal

- `ended: None` (about 25 files, mostly tests).

## Implementation Milestones

- [ ] `AgentEnd` announced from both sites; cli-sessions tests green
- [ ] Kernel, proto, conversions, storage carry the new source and `ended`
- [ ] Forwarder publishes a labelled, owned notification; lifecycle acceptance green
- [ ] Daemon starts the forwarder
- [ ] Telegram declines; drawer ignores
- [ ] Docs: `docs/ft/daemon/session-notifications.md` *What is notified* row; package docs

## Testing Plan

### Testing Strategy

The process seam is the risky part — whether an end is announced, once, from the right site — so it
is tested with a **real PTY child** (a stub script), as `tests/exit_hook.rs` does, and the tmux
branch with the injected liveness probe the manager already uses (`with_detach_watch_interval`,
`classify_main_exit`). The notification's shape and its route into the log are tested in-process
through the real bus and the real `ProjectNotificationLog`; the inbox answer through the
`tddy-daemon-rpc` harness that already serves `ReadProjectNotifications`.

### Acceptance Tests

**tddy-cli-sessions** — `packages/tddy-cli-sessions/tests/agent_end.rs`

- `a_main_whose_agent_exits_announces_its_end_once_with_the_exit_code` — stub exits 3 → one `AgentEnd`
  with `exit_code: Some(3)`, `outcome: "exit code 3"`; nothing more within a held window.
- `a_main_stopped_by_the_daemon_announces_its_end_as_stopped` — `kill_all` on a running stub →
  `outcome: "stopped by the daemon"`.
- `a_tab_ending_announces_nothing` — a non-main terminal exits → no `AgentEnd`.

**tddy-cli-sessions** — `packages/tddy-cli-sessions/tests/tmux_agent_end.rs`

- `a_tmux_client_ending_while_its_agent_lives_announces_nothing` — client ends, tmux session alive
  → nothing.
- `a_detached_agent_whose_tmux_session_goes_announces_its_end_as_unknown` — the watcher sees the
  session gone → one `AgentEnd`, `exit_code: None`, `outcome: "exit code unknown"`. (Gated on tmux
  like `tmux_exit_hook.rs`.)

**tddy-session-lifecycle** — `packages/tddy-session-lifecycle/tests/session_ended_notification_acceptance.rs`

- `an_ended_agent_lands_in_its_projects_notification_log` — a real manager runs a stub main for a
  session recorded with a `project_id`; the forwarder runs on a bus with the real log → one record,
  source `SessionEnded`, text `⏹ Session {label} ended: exit code 0`, `ended.exit_code == Some(0)`.
- `an_end_whose_owner_has_no_sessions_base_is_not_published` — the daemon user resolves to no
  sessions base → nothing on the bus.

**tddy-daemon-rpc** — `packages/tddy-daemon-rpc/tests/project_home_session_ended_acceptance.rs`

- `the_inbox_returns_a_visible_sessions_end_with_its_outcome` — a `SessionEnded` notification of a
  child session → `ReadProjectNotifications` returns it with `"source": "session_ended"` and
  `"ended": {"outcome": "exit code 1", "exit_code": 1}`.
- `the_inbox_does_not_return_the_end_of_a_session_the_caller_cannot_see` — another OS user's
  session's end → not returned.

### Unit Tests

- `tddy-session-lifecycle` `session_notifications.rs`: `an_agent_end_becomes_an_attention_notification_naming_its_outcome`.
- `tddy-session-activity` `tests/project_notification_log_storage.rs`:
  `an_ended_record_round_trips_through_the_log_file`, `a_record_without_an_ended_part_writes_no_ended_key`.
- `tddy-session-activity` `tests/session_notification_event_unit.rs` (the wire conversion is
  `streams::session_notification_event`, not `tddy-service`): `a_session_ended_notification_converts_with_its_end`,
  `an_end_with_no_exit_code_has_none_on_the_wire`.
- `tddy-telegram-control` `tests/telegram_notification_subscriber_unit.rs`: `telegram_declines_a_session_end`.
- `tddy-session-web` `src/lib/sessionNotificationDot.test.ts`: `a session end lights no dot`,
  `attention and activity light as before`, `a kind this build does not know lights nothing`.

### Testing Principles Applied

Real process, real bus, real log; one test per behaviour; no sleeps as synchronisation (bounded
`eventually`), and a *nothing happens* assertion is a held window over a channel, never a sleep and
a check.

## Technical Debt & Production Readiness

_(populated during development)_

## Decisions & Trade-offs

- **Structured `ended` part, not text only** (developer's choice, 2026-11-16): an orchestrator
  branches on the exit code; costs `ended: None` across ~25 files of literals.
- **Broadcast channel from the manager, not a callback trait.** The manager is built before the bus;
  a receiver taken later lets the daemon wire it without reordering construction. Lag is logged, not
  hidden.
- **`ATTENTION_REQUIRED`, not a new kind.** The log stores attention only; an ended session is
  something an orchestrator must act on. The drawer opts out by source instead.
- **The watcher's end has no exit code.** Nothing saw the agent's process exit; `exit code unknown`
  says so rather than inventing a 0.

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

- `feature/activity-wake/session-watches` (`#activity-wake` 2/3)
- `feature/activity-wake/idle-wake` (`#activity-wake` 3/3)

## TODO

- [x] Record initial discovery (`2026-11-16-session-ended-notice-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p tddy-cli-sessions -p tddy-daemon-kernel -p tddy-session-activity -p tddy-session-lifecycle -p tddy-service -p tddy-daemon -p tddy-daemon-rpc -p tddy-telegram-control`, and the session-web unit test) — verify 100% pass
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
