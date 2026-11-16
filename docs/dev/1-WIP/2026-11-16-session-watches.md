# Changeset: An orchestrator watches the sessions it can see

**Date**: 2026-11-16
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 5 — several crates (daemon-kernel, session-activity, tool-engine, daemon-rpc, daemon), concurrency semantics (exactly-once delivery across concurrent calls and several watches), a persisted per-project store that must survive restart, and a decoration of every tool answer
**Stack**: `#activity-wake` 2/3 — branch `feature/activity-wake/session-watches`, base `feature/activity-wake/session-ended-notice` (#929)

## Initial Discovery

[2026-11-16-session-watches-initial-discovery.md](./2026-11-16-session-watches-initial-discovery.md)

## Prerequisites

### ⚠ DURING — `ExecToolRpcHandler::execute_tool` is over its length budget — [`complexity-exec-tool-ports-execute-tool.md`](../../../packages/tddy-daemon-rpc/docs/code-issues/complexity-exec-tool-ports-execute-tool.md)

69 lines against 60. The attachment of `watchEvents` is a function of its own that `execute_tool`'s
caller-facing result passes through; **not one line of matching or attaching is written inline** in
`execute_tool`. The issue stays open (this node does not shrink it), and must not grow.

### ⚠ DURING — `build_session_host` is over its length budget — [`complexity-mount-host-build-session-host.md`](../../../packages/tddy-daemon/docs/code-issues/complexity-mount-host-build-session-host.md)

The store is opened beside the notification log in `open_shared`'s neighbourhood and handed on
through `Shared`; nothing is added to `build_session_host`.

### ℹ ANSWERED — one host per read — [`2026-10-29-spawner-read-scope-is-same-host-only.md`](../todo/2026-10-29-spawner-read-scope-is-same-host-only.md)

A watch sees its orchestrator's own host only, like the inbox tools. A `session_ids` entry on
another host is refused as not visible. The entry stays open.

### ℹ ANSWERED — ACP turn input is not delivered — [`2026-11-13-acp-turn-input-is-not-delivered.md`](../todo/2026-11-13-acp-turn-input-is-not-delivered.md)

An orchestrator whose agent declares `turn_input: acp_prompt` is refused a watch with the words
`SendSessionMessage` already uses for it; the entry stays open and, when it closes, the refusal goes
with it.

### ℹ ANSWERED — the project-home `Running` heuristic — [`2026-10-23-project-home-running-heuristic.md`](../todo/2026-10-23-project-home-running-heuristic.md)

Not consulted here — nothing in this node decides busy or idle. The successor decides it.

## Affected Packages

- **tddy-daemon-kernel**: [daemon-kernel.md](../../../packages/tddy-daemon-kernel/docs/daemon-kernel.md) — `SessionNotification.activity_status`
- **tddy-session-activity**: [session-notifications.md](../../../packages/tddy-session-activity/docs/session-notifications.md) — the status on activity-status notifications and in the log's JSON; the new `session_watches` module
- **tddy-tool-engine** — `project_home_watch_tools`: four tool definitions, their port, refusal texts; the catalogue offers them to the project-home role
- **tddy-daemon-rpc** — the port on `DaemonProjectHome`; the `watchEvents` attachment around `execute_tool`
- **tddy-daemon** — opens the store and hands it to the exec-tool handler
- every crate with a `SessionNotification` literal — `activity_status: None`

## Related Feature Documentation

- [PRD-2026-11-16-session-watches.md](../../ft/daemon/1-WIP/PRD-2026-11-16-session-watches.md)
- [Project concept](../../ft/daemon/project-concept.md), [Session notifications](../../ft/daemon/session-notifications.md)

## Summary

Orchestrators get `CreateSessionWatch` / `ListSessionWatches` / `RenewSessionWatch` /
`CancelSessionWatch`. A watch is a filter (sessions by id and/or "started by me", six kinds) with its
own position in the project notification log and a required TTL ≤ 24 h. Every tool answer an
orchestrator receives on its host carries, under `watchEvents`, the records its watches matched since
the last answer — each once — plus `lost` and the watches that `ended`. Watches persist per project
and end on expiry, cancel, the orchestrator's own end, or every named session's end.

## Background

See the PRD. The log already holds every event an orchestrator would watch for, scoped exactly as an
orchestrator may see it; what is missing is a per-orchestrator position in it, a filter, and a way
for the matched records to reach the orchestrator without it asking.

## Scope

- [ ] `activity_status` on activity-status notifications and in the log
- [ ] The watch store: create, list, renew, cancel, take-delivery, live check; persisted
- [ ] The matcher: six kinds, session selection, no double delivery for one prompt
- [ ] Endings: expiry, cancel, orchestrator ended, every named session ended
- [ ] The four tools, offered to the project-home role only, with every refusal
- [ ] `watchEvents` on every answer to a watching orchestrator, consumed once
- [ ] Docs: `project-concept.md` (the tools), package docs; a Future-enhancement TODO for showing watches on the web

## Responsibility

This PR owns **watches and their delivery on tool answers**:

- `SessionNotification.activity_status` and its place in the log's JSON;
- `tddy_session_activity::session_watches` — `SessionWatchStore`, `SessionWatch`, `WatchOwner`,
  `WatchFilter`, `WatchKind`, `WatchEnd`, `WatchDelivery`, and the limits `MAX_WATCH_TTL_SECS`
  (86 400), `MAX_LIVE_WATCHES` (16), `MAX_EVENTS_PER_ANSWER` (50);
- `tddy_tool_engine::project_home_watch_tools` — the tool names, definitions, `ProjectHomeWatchPort`,
  refusal texts;
- `tddy-daemon-rpc`'s port implementation and `watchEvents` attachment;
- the daemon's opening of the store.

## Boundaries

- **No wake.** An orchestrator that makes no tool call receives nothing; turns are
  `feature/activity-wake/idle-wake`'s. This node decides nothing about busy or idle and never types
  into a terminal.
- **No auto-stop change.** That a live watch holds auto-stop off is the successor's.
- **Nothing published, no new notification.** A watch is a reader of the log.
- **No change to the inbox tools** — their arguments, cursors and answers are as they are, and the
  two positions (inbox cursor, watch position) never move each other.
- **One host.** No cross-host watch, no host argument.
- **Unary `ExecuteTool` only.** `StreamExecuteTool` frames (the split-session path) carry no
  `watchEvents`; what was waiting stays waiting for the next unary answer.
- **No web.** No RPC, no UI for watches (TODO).
- **Not on the stream proto.** `activity_status` stays daemon-side; `activity.proto` is untouched.
- **The session-ended notice is the predecessor's** — this node matches its records, never raises
  one (see Dependencies).

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `feature/activity-wake/session-ended-notice` (#929) | `SessionNotificationSource::SessionEnded`, `SessionNotification.ended`, the log's `"source": "session_ended"` records | the `ended` kind matches those records; a watch ends when its owner's, or every named session's, `session_ended` record is in the log | raise a session-ended notification, change its text or `ended` part, or touch the session manager's agent-end announcement |

## Draft PR contract

The first push of this PR's code (pass 2, commit 2) publishes:

- `SessionNotification.activity_status: Option<String>` with `None` on every literal, and the log's
  DTO field — `notification_for_activity_status` not yet setting it;
- `session_watches` with every type and limit above, and the store's methods returning
  `// TODO(session-watches): implement` refusals / empty deliveries;
- the four tool definitions in the project-home catalogue and `ProjectHomeWatchPort`, the daemon's
  implementation refusing with a TODO text;
- the attachment function, wired, attaching nothing yet;
- every acceptance and unit test below, **failing** on the missing behaviour, not on compilation.

## Green wave

**Wave:** 1 of 2
**Greenable independently:** yes — its tests write their own `session_ended` records into the log
(through the bus, as the inbox acceptance tests publish their notifications); none needs the
predecessor's agent-end *behaviour*, only its types.
**Concurrent with:** #929 (`session-ended-notice`)
**Blocks:** `feature/activity-wake/idle-wake` — its wake turn delivers this node's
`take_delivery` and its auto-stop hold asks this node's `has_live_watches`; both must behave.

Real dependency edges, as opposed to the branch line:

    session-ended-notice → session-watches (surface)      session-watches → idle-wake (behaviour)

## Technical Changes

### State A (Current)

- The project log (`tddy-session-activity/src/project_notification_log.rs`) keeps attention records
  per project, `seq` as cursor, 1 000 per project, `lost` on eviction, scope
  `ProjectNotificationScope { project_id, os_user, exclude_session_id, started_by }`.
- `notification_for_activity_status` (`session_notifications.rs:236-281`) raises `Done` and
  `WaitingForInput` as `AttentionRequired` / `ActivityStatus` with different text and no structured
  status — the two are indistinguishable in a record except by prose.
- Project-home tools are defined in `tddy-tool-engine` with a port per family
  (`project_home_inbox_tools.rs` is the model), implemented on `DaemonProjectHome` in
  `tddy-daemon-rpc/src/exec_tool/` (`project_home_inbox.rs`, `scope_of`), offered to
  `SessionToolRole::ProjectHome` — the project-home main and `session_role: orchestrator` sessions
  (`conversation_spawn.rs:21-40`).
- `ExecToolRpcHandler::execute_tool` (`ports.rs:34-102`) forwards to a peer, authorizes, serves a
  hosted clone, serves a caller held elsewhere, or runs locally; every branch returns a
  `ExecuteToolResponse { result_json, is_error, … }` that the tool client hands the agent as-is.
  A daemon-added key in `result_json` has a precedent: a conversation's mutating call gains
  `worktreeChange`.
- `turn_input_of` (`project_home/turn.rs:100-122`) words the no-`turn_input` refusal; `acp_prompt` is
  refused by `terminal_submit_key`.
- No watch, subscription or per-orchestrator position exists anywhere.

### State B (Target)

- **Status.** `notification_for_activity_status` sets `activity_status: Some(<wire status>)`; the log
  stores `"activity_status"`; replay reads it back (absent on older lines).
- **Store.** `SessionWatchStore::open(data_dir)` keeps `<data dir>/projects/<project_id>/session-watches.json`
  (whole-file atomic rewrite on every change; read once per project). A watch is
  `{ watch_id, owner: WatchOwner { project_id, session_id, os_user }, filter, created_at, expires_at,
  position: seq, lost }`. One async lock per project serialises every operation, which is what makes
  delivery exactly-once across concurrent calls.
- **Matcher.** `WatchKind::of(record) -> Option<WatchKind>`:
  `pending_question` + permission → `permission`; `pending_question` otherwise, `presenter` → `question`;
  `activity_status` + `Done` → `turn_finished`; `activity_status` + `WaitingForInput` + no question →
  `waiting_for_input`; `agent` → `agent_notification`; `session_ended` → `ended`; anything else (an
  `activity_status` with a question, one with no status) → none. A record is selected when its
  session is in `session_ids`, or `started_by_me` and its `orchestrator_session_id` is the owner —
  always within the owner's scope (own project, own OS user, never the owner's session).
- **Delivery.** `take_delivery(owner, log, now) -> WatchDelivery { events, lost, ended }`: first ends
  what has ended (see below), then reads each live watch's records after its position, unions the
  matches by `seq` (one event for two watches), takes the oldest 50, advances each watch's position
  past what it delivered, adds evictions to `lost`, persists, returns. Nothing waiting → an empty
  delivery and no write.
- **Endings**, checked at every operation on the owner's watches: `now ≥ expires_at` → `expired`; the
  owner's own `session_ended` record after the watch's creation → `orchestrator_ended` (read
  unscoped, since the scope excludes the owner's session); every `session_ids` entry has a
  `session_ended` record after creation and `started_by_me` is off → `sessions_ended`. An ended watch
  is removed and reported once in the next delivery's `ended`.
- **Tools** (`tddy-tool-engine`), with the refusals the PRD lists. `CreateSessionWatch` checks, in
  order: role, turn input (`turn_input_of`'s wording; `acp_prompt` worded as `SendSessionMessage`
  words it), ttl, selection, kinds, each session id's visibility (its `.session.yaml` under the
  caller's sessions base, in the caller's project, not the caller), the live cap. The new watch's
  position is the log's current head: it sees only what happens after it.
- **Attachment** (`tddy-daemon-rpc`): `answer_with_watch_events(&req, response)` runs after every
  `execute_tool` branch except the caller-held-elsewhere one (that caller's own host attaches). When
  the response is not an error, `result_json` is a JSON object, and the caller has live watches,
  it takes a delivery and inserts `watchEvents` when the delivery is non-empty. Otherwise it consumes
  nothing.
- **Daemon**: the store is opened in `open_shared` beside the log, carried on `Shared`, and installed
  on the exec-tool handler and `DaemonProjectHome`.

### Delta (What's Changing)

#### tddy-daemon-kernel

- `SessionNotification.activity_status: Option<String>`.

#### tddy-session-activity

- `session_notifications.rs`: set `activity_status`.
- `project_notification_log/storage.rs`: the `activity_status` field, optional both ways.
- `project_notification_log.rs`: an unscoped `records_after(project, seq)` reader for the store
  (`pub(crate)`), and the current head `seq`.
- New `session_watches.rs` (+ `session_watches/storage.rs`, `session_watches/matcher.rs`).

#### tddy-tool-engine

- New `project_home_watch_tools.rs`; `project_home_catalogue.rs` lists them for the project-home role;
  the executor routes them to the port.

#### tddy-daemon-rpc

- New `exec_tool/project_home_watches.rs` (the port) and `exec_tool/watch_events.rs` (the
  attachment); `ports.rs` passes the result through it; `DaemonProjectHome` gains the store.

#### tddy-daemon

- `runtime/mount_host.rs` / `Shared`: open and carry the store.

#### every crate with a `SessionNotification` literal

- `activity_status: None`.

## Implementation Milestones

- [ ] Status carried and stored; storage tests green
- [ ] Store + matcher + endings; unit tests green
- [ ] Tools and refusals through the harness
- [ ] Attachment consumed once; acceptance green
- [ ] Daemon wiring; restart survival
- [ ] Docs and the web TODO

## Testing Plan

### Testing Strategy

The logic that can be wrong quietly — matching, exactly-once, endings, persistence — lives in the
store, so it is unit-tested there against a real `ProjectNotificationLog` in a temp dir, fed through
the real bus. The tools, refusals and the attachment are acceptance-tested through
`tddy-daemon-rpc`'s project-home harness (`tests/support`, the `World` of
`project_home_wait_acceptance.rs`, the question fixtures of `project_home_turn_control_acceptance.rs`),
calling `ExecuteTool` as an orchestrator's tddy-tools does and reading the answer JSON.

### Acceptance Tests

**tddy-daemon-rpc** — `packages/tddy-daemon-rpc/tests/project_home_session_watch_acceptance.rs`

- `a_permission_prompt_of_a_watched_session_rides_on_the_orchestrators_next_answer` — watch
  `started_by_me`, kinds `[permission]`; the child's permission prompt is published; the
  orchestrator's next `ListProjectSessions` answer carries `watchEvents.events[0]` with its
  `request_id`.
- `a_delivered_event_is_not_delivered_again` — the answer after that carries no `watchEvents`.
- `an_unwatched_kind_or_session_is_not_delivered` — a `turn_finished` under a `[permission]` watch;
  another session's prompt under a `session_ids` watch.
- `a_finished_turn_and_a_wait_for_input_are_told_apart` — `[turn_finished]` gets `Done`, not
  `WaitingForInput`, and `[waiting_for_input]` the reverse.
- `the_orchestrators_own_events_are_never_delivered`.
- `a_watch_is_listed_renewed_and_cancelled` — list shows it with its waiting count; renew moves
  `expires_at_unix_ms`; cancel stops delivery.
- `a_watch_whose_sessions_all_ended_is_reported_ended_once` — both named sessions' `session_ended`
  records → `watchEvents.ended == [{watch_id, "sessions_ended"}]`, then gone from the list.
- `creating_a_watch_is_refused_with_what_to_do` — one case per refusal: not an orchestrator; no
  `turn_input` (naming the agent); ttl missing / 0 / 86 401; no selection; unknown kind; a session of
  another project; the caller's own session; the 17th watch.
- `an_ordinary_session_is_not_offered_the_watch_tools` — `ListExecTools` for an ordinary session.
- `an_error_answer_consumes_nothing` — a refused call carries nothing; the next good answer carries
  the event.

### Unit Tests

**tddy-session-activity** — `packages/tddy-session-activity/tests/session_watches_unit.rs`

- `each_kind_matches_only_its_records` (six kinds, and the question-carrying status matching none)
- `two_watches_matching_one_record_deliver_it_once`
- `concurrent_deliveries_never_hand_out_the_same_event_twice`
- `a_delivery_holds_at_most_fifty_and_the_rest_wait`
- `events_the_log_evicted_before_delivery_are_counted_lost`
- `an_expired_watch_ends_as_expired`
- `a_watch_ends_when_its_orchestrators_agent_ends`
- `watches_and_their_positions_survive_reopening_the_store`
- `a_new_watch_sees_only_what_happens_after_it`

**tddy-session-activity** — `tests/project_notification_log_storage.rs`

- `a_status_record_round_trips_its_activity_status`, `an_older_line_without_a_status_still_reads`

**tddy-tool-engine** — `src/project_home_watch_tools.rs` `#[cfg(test)]`

- `the_watch_tools_are_offered_to_the_project_home_role_only`, `create_requires_ttl_seconds_in_its_schema`

### Testing Principles Applied

Real log, real files, real harness; the clock is injected (`now`) so expiry needs no sleep; the
exactly-once test races real tasks against one store, not a mock; one behaviour per test.

## Technical Debt & Production Readiness

- To write at implementation: `docs/dev/todo/2026-11-16-session-watches-are-not-shown-on-the-web.md`
  (Future enhancement — the developer chose not to build a web view in this stack).

## Decisions & Trade-offs

- **Six kinds, with a structured status** (developer's choice, 2026-11-16) over a merged `idle`:
  costs `activity_status: None` across ~25 files of literals, and older log lines match neither
  status kind.
- **A position in the log, not a queue of copies.** The log already keeps, scopes and evicts; a watch
  is a cursor plus a filter. The per-watch cap is the log's window, and `lost` is its eviction count.
- **Attach on the result object** (`watchEvents`), following `worktreeChange`, rather than a new proto
  field — every transport (unary, LiveKit, relay) passes `result_json` through untouched, so no
  client changes. A non-object result carries nothing and consumes nothing.
- **Refuse at creation for an agent with no turn input**, although this node's delivery needs none:
  a watch promises the successor's wake, and a promise the agent cannot take is refused up front
  (the brief's requirement).
- **Complexity 5** is a boundary warning: the store, the tools and the attachment could be split.
  They are kept together because no one of them is reviewable as a vertical slice alone — a store
  without tools delivers nothing, tools without the attachment deliver nothing.

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

- `feature/activity-wake/idle-wake` (`#activity-wake` 3/3)

## TODO

- [x] Record initial discovery (`2026-11-16-session-watches-initial-discovery.md`)
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
- [ ] Run the touched packages' tests (`./test -p tddy-daemon-kernel -p tddy-session-activity -p tddy-tool-engine -p tddy-daemon-rpc -p tddy-daemon`) — verify 100% pass
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
