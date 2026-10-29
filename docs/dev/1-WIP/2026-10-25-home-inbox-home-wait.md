# Changeset: Project-home agent waits for its project's notifications

**Created:** 2026-10-25
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 4 — five packages, a durable cursor-addressed log with eviction accounting and restart replay, a blocking tool under the remote-call clamp
**Stack:** `#home-inbox 7/8` — branch `feature/home-inbox/home-wait`, base `feature/home-inbox/permission-answer`
**PRD:** [PRD-2026-10-25-home-wait-notifications.md](../../ft/daemon/1-WIP/PRD-2026-10-25-home-wait-notifications.md)

## Initial Discovery

[2026-10-25-home-inbox-home-wait-initial-discovery.md](./2026-10-25-home-inbox-home-wait-initial-discovery.md)

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-10-23-session-status-streams-do-not-reconnect.md](../todo/2026-10-23-session-status-streams-do-not-reconnect.md) | — Unrelated (web streams); the log is what makes a reconnect-free consumer possible server-side |
| [2026-10-23-project-home-auto-stop-untracked-processes.md](../todo/2026-10-23-project-home-auto-stop-untracked-processes.md) | ⚠ During — a home agent blocked in `WaitProjectNotifications` reports `Running` (it is inside a tool call), so auto-stop does not fire mid-wait; verify, do not change the reaper |
| `packages/tddy-tool-engine/docs/code-issues/oversized-file-lib.md` | ⚠ During — tools in their own module |

## Affected Packages

- [ ] [`tddy-session-activity`](../../../packages/tddy-session-activity/README.md): `ProjectNotificationLog` (subscriber + ring + JSONL + wait)
- [ ] [`tddy-daemon`](../../../packages/tddy-daemon/README.md): registers the log subscriber in `mount_host`
- [ ] [`tddy-tool-engine`](../../../packages/tddy-tool-engine/README.md): `WaitProjectNotifications`, `ReadProjectNotifications`
- [ ] [`tddy-daemon-rpc`](../../../packages/tddy-daemon-rpc/README.md): the port over the log
- [ ] [`tddy-session-tool-client`](../../../packages/tddy-session-tool-client/README.md): `remote_block_arg_keys` clamps `timeout_ms`
- [ ] [`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/README.md): `DaemonSessionHost::with_project_notification_log` / `project_notification_log()`, so the exec-tool handler reaches the log (pass 2 addition, D4)

`tddy-session-activity` gains a dependency on `tddy-tool-engine` (for `FollowPage`). The edge is acyclic: `tddy-tool-engine`'s normal tree contains no `tddy-session-activity` (D2).

## Responsibility

- `tddy_session_activity::project_notification_log::{ProjectNotificationLog, ProjectNotificationRecord { seq, session_id, label, kind, source, text, agent, question, at_unix_ms }, ProjectNotificationLog::{read(project_id, os_user, cursor, limit) -> FollowPage<ProjectNotificationRecord> + lost, wait(project_id, os_user, cursor, timeout) -> FollowPage<…> + timed_out}}`
  — paging is `#start-parity 6/9`'s: `cursor` is the decimal `seq` (empty = from now for a wait, from
  the oldest kept for a read); answers are `tddy_tool_engine::session_follow::FollowPage { items,
  next_cursor, has_more, clamped_to }`, with `FOLLOW_PAGE_DEFAULT`/`FOLLOW_PAGE_MAX` (50/200), plus
  `lost: N` when the cursor is older than the window
  — a `SessionNotificationSubscriber` taking `AttentionRequired`; resolves `project_id` from the session's metadata; 1,000 records per project; persisted as `<data dir>/projects/<project_id>/notifications.jsonl`, replayed on start; `tokio::sync::Notify` per project.
- Tools `WaitProjectNotifications(cursor?, timeout_ms ≤ 20000, daemon_instance_id?)` and `ReadProjectNotifications(cursor?, limit?, daemon_instance_id?)` (`project_home_inbox_tools.rs`), excluding the caller's own session.
- `remote_block_arg_keys`: `WaitProjectNotifications → timeout_ms`.

## Boundaries

- No activity events; no push into the agent's next turn.
- One host per call. `daemon_instance_id` forwards the call to that host's log through
  `#start-parity 1/9`. There is no merged cross-host view, and the home agent waits on each host it
  has children on.
- `StreamSessionNotifications` and the drawer unchanged.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `#home-inbox 1/8` agent-notify | `SessionNotification.agent` | stored and returned on records | change the model |
| `#home-inbox 3/8` elicit-content | `SessionNotification.question` with `id` | stored and returned, so the agent can call `6/8`'s and `8/8`'s answer tools | change capture or ids |

| `#start-parity 6/9` session-follow (#755) | `FollowPage`, `FOLLOW_PAGE_DEFAULT`/`FOLLOW_PAGE_MAX`, the cursor convention | reuses them (crate-private helpers widened to `pub(crate)`), so a home agent sees one paging convention | change `ReadSessionActivity`/`ReadSessionTranscript` |
| `#start-parity 1/9` remote-targets (#750, merged) | the generic forward | both tools are target tools | change the forward |

`#home-inbox 6/8` sits below but is not consumed.

## Draft PR contract

Published as commit 2 (pass 2). Bodies are `// TODO(home-wait): implement` and return refusals, an unchanged value, or `Ok(())`; none panics.

**`tddy-session-activity`: `project_notification_log.rs`**
- `PROJECT_NOTIFICATION_CAPACITY = 1000`, `MAX_NOTIFICATION_WAIT = 20 s`, `clamp_wait(Duration) -> Duration`, `type SessionsBaseFor`.
- `ProjectNotificationRecord { seq, session_id, label, kind, source, text, agent: Option<AgentNotification>, question: Option<QuestionNotice>, at_unix_ms }`.
- `ProjectNotificationScope { project_id, os_user, exclude_session_id }`.
- `NotificationRead { page: FollowPage<Record>, lost: u64 }`, `NotificationWait { page, timed_out: bool }`.
- `ProjectNotificationLog::{open(data_dir, sessions_base_for), read(scope, cursor, limit), async wait(scope, cursor, timeout)}`.
- `impl SessionNotificationSubscriber for ProjectNotificationLog`: `wants` is real (`AttentionRequired` only); `deliver` is a TODO.

**`tddy-tool-engine`: `project_home_inbox_tools.rs`**
- `WAIT_PROJECT_NOTIFICATIONS`, `READ_PROJECT_NOTIFICATIONS`, `MAX_WAIT_MS = 20_000`, `is_project_home_inbox_tool`, `project_home_inbox_tool_defs()` (real schemas: `timeout_ms` max 20000, `limit` max 200, `cursor`, `daemon_instance_id`, nothing required).
- `NotificationsPage { page: FollowPage<Value>, lost, timed_out }` (the port speaks JSON, D2).
- `trait ProjectHomeInboxPort: ProjectHomePort { read_notifications(home, cursor, limit), wait_notifications(home, cursor, timeout_ms) }`, both with refusing defaults.
- `execute_project_home_inbox_tool` (TODO).

**`tddy-daemon-rpc`**
- `exec_tool/project_home_inbox.rs`: `impl ProjectHomeInboxPort for DaemonProjectHome` (refusing stubs).
- `ProjectHomeHost.notification_log` and `DaemonProjectHome.notification_log`, from `host.project_notification_log()`.

**`tddy-session-lifecycle`**: `with_project_notification_log`, `project_notification_log()`.

**`tddy-session-tool-client`**: a `TODO(home-wait)` comment in `remote_block_arg_keys`; the `WaitProjectNotifications` arm is deliberately absent so its test fails.

## Green wave

**Wave:** 3 of 4
**Greenable independently:** no — its pending question test publishes through `3/8`'s capture path
(behaviour); the plain-notification tests need only `1/8`'s published types.
**Concurrent with:** `#home-inbox 6/8` permission-answer
**Blocks:** nothing

    n1 → n3, n4, n7      n2 → n5      n3 → n6, n7      n6 → n8      (cross-stack, replan 2026-10-26: #start-parity #750 [merged] → n2, n5, n6, n7, n8 · #755 → n3 · #751, #758 → n6 · #coagents [merged] → n6)

## State A

The bus is live-only; the home agent can only re-call `ListProjectSessions`.

## State B

Every attention notification of a project's sessions lands in that project's log; the home agent
waits on it with a cursor and gets events within milliseconds of publication, across restarts, with
eviction reported.

## Delta

### New
- `project_notification_log.rs` (session-activity); `project_home_inbox_tools.rs` (tool-engine); the port impl
### Modified
- `mount_host::open_shared` (+subscriber), `remote_block_arg_keys`, catalog pins, `project_home_prompt.md`
- `docs/ft/daemon/{project-concept.md,session-notifications.md}` at wrap

## Testing Strategy

### Acceptance tests

**`tddy-session-activity`**: `packages/tddy-session-activity/tests/project_notification_log_acceptance.rs`
- [x] `a_notification_published_after_the_wait_began_returns_the_wait_with_it_and_a_new_cursor`
- [x] `a_wait_with_nothing_new_times_out_and_a_timeout_above_20000_ms_is_capped`
- [x] `waiting_from_a_returned_cursor_never_repeats_or_skips_across_a_restart`
- [x] `another_projects_another_os_users_and_the_callers_own_records_never_appear`
- [x] `reading_from_an_evicted_cursor_reports_how_many_records_were_lost`
- [x] `an_question_record_carries_its_id_and_content`

**`tddy-daemon-rpc`**: `packages/tddy-daemon-rpc/tests/project_home_wait_acceptance.rs`
- [x] `a_project_home_agent_waits_and_receives_a_childs_agent_notification`
- [x] `a_non_project_home_caller_is_refused_as_permission_denied`

**`tddy-session-tool-client`**: unit
- [x] `wait_project_notifications_timeout_is_clamped_on_livekit`

### Red-phase results (pass 2)

All nine acceptance tests are written, compile, and fail because this node's bodies are missing; none is blocked on a parent's unimplemented body (they use only n1's and n3's published types and publish straight onto the bus).

| Test | Fails with |
|---|---|
| session-activity: all six | `ProjectNotificationLog::read: not implemented` / `…::wait: not implemented` |
| daemon-rpc `a_project_home_agent_waits…` | `PermissionDenied: tool WaitProjectNotifications is refused: a project-home session is read-only` (tool not registered, D6) |
| daemon-rpc `a_non_project_home_caller_is_refused…` | `unknown tool: WaitProjectNotifications` returned as a response, not `PermissionDenied` (D6) |
| tool-client clamp | left `Some(60000)`, right `Some(20000)` |

Unit tests (`tddy-session-activity/tests/project_notification_log_unit.rs`, 21 cases; `tddy-tool-engine/tests/project_home_inbox_tools.rs`, 13): 17 and 7 fail on the TODO bodies. The rest pass already and pin what is real: the `wants` filter, `clamp_wait` for values at or below the cap, the tool definitions' schema bounds (`timeout_ms` ≤ 20000, `limit` ≤ 200, `daemon_instance_id` present), name recognition, and the two refusals that the stub already gives.

### Test level decisions
| Aspect | Level | Rationale |
|---|---|---|
| Log semantics | Integration over the subscriber + temp data dir | Persistence and cursors are the contract |
| Tool | Integration over the exec-tool handler | Role gate + scoping |

## Decisions & Trade-offs

### Pass 2 deviations (accepted by the orchestrator, developer AFK)

- **D1 — scope struct.** `read`/`wait` take a `ProjectNotificationScope { project_id, os_user, exclude_session_id }` instead of `(project_id, os_user, …)`, because excluding the caller's own session after paging would break page sizes.
- **D2 — JSON port.** `tddy-tool-engine` cannot depend on `tddy-session-activity` (cycle through `tddy-daemon-sandbox`), so the port returns `NotificationsPage` over `FollowPage<serde_json::Value>`; the daemon serialises records into it. The record type stays in session-activity. New acyclic edge session-activity → tool-engine.
- **D3 — empty-page cursor.** The `next_cursor` of an empty page (including a timed-out wait from `""`) is the log's newest `seq` (`"0"` for a log that never held a record), a **deliberate divergence from #755's "cursor unchanged"**: with an unchanged empty cursor a wait from `""` skips whatever is published between two polls. `seq` starts at 1; `lost` counts records after the cursor evicted before reading. A wait that names no `timeout_ms` holds for the 20 s cap.
- **D4 — host plumbing.** `tddy-session-lifecycle` gets the builder and accessor (precedent: 5/8's `terminal_control()`); it is added to Affected Packages.
- **D9 — shared test support.** `tddy-daemon-rpc/tests/support/mod.rs` gains `a_host_logging_notifications` plus `daemon_over_customised`/`host_built_with`; `daemon_over` and `host_built_by` delegate unchanged.
- **D10 — test location.** The tool-client clamp test is an integration file in `tests/`.

### Left to green, explicitly

- **D5 — daemon wiring.** In `tddy-daemon/src/runtime/mount_host.rs`: construct the log, register it on the bus, call `DaemonSessionHost::with_project_notification_log`. The block already carries `TODO(desktop-notify)`.
- **D6 — registration.** The catalog entries; `is_project_home_target_tool` (the `daemon_instance_id` forward); `is_project_home_tool` and `is_project_home_allowed_tool` (`read_only.rs`); the daemon dispatch in `run_locally`; the `read_only.rs` pin, `project_home_session_tools_acceptance.rs`, `exec_tool_admission.rs`, `project_home_prompt.md` and its test; and `remote_block_arg_keys`' `WaitProjectNotifications => &["timeout_ms"]` arm.
- **D7 — #755's helpers.** Green makes `page_size` and `parse_cursor` in `tddy-tool-engine/src/session_follow.rs` `pub` and uses them in the log, rather than re-implementing them (the changeset's `pub(crate)` is not enough across crates).
- **D8 — `serde`.** Green adds `serde` (derive, the workspace dependency) to `tddy-session-activity` for the JSONL record DTOs; the public record carries no serde derives.
- The port impl in daemon-rpc needs `os_user` widened to `pub(super)` on `DaemonProjectHome`.

### Green decisions (taken by `/green`, developer AFK)

- **D11 — a wait reports `lost`.** `NotificationWait` gains `lost: u64`, and the wait's answer carries `lost` when it is non-zero, as a read's does. The Responsibility's "plus `lost: N` when the cursor is older than the window" and the log's "never silently trimmed" both cover a wait that starts from an evicted cursor; the contract's struct had no field for it. Additive: the acceptance tests read only `page` and `timed_out`.
- **D12 — `has_more` means "a record of the caller's scope follows `next_cursor`".** A newest-page read (empty cursor) therefore answers `has_more: false`: no older page is reachable with a forward cursor, so `true` would point the agent at nothing.
- **D13 — `lost` counts the project's records.** Sequence numbers are project-wide, so the count is over the project, whoever's records they were. Only a count leaves the log, never another session's record.
- **D14 — the file is bounded.** Lines are appended per record; once the file holds twice the window's lines it is rewritten to the window (temporary file, then rename). Sequence numbers survive compaction because they live in the lines.
- **D15 — an unreadable file fails `open`.** The daemon's start fails naming the file rather than starting an empty log, which could reuse sequence numbers. A malformed line is skipped with a warning, and the lines around it are kept.
- **D16 — a notification that names no readable session is an error.** `deliver` returns `Err` for a session whose sessions directory, record or project cannot be resolved. The bus logs it; the record is not written.
- **D17 — the trait defaults keep refusing.** The contract's `TODO(home-wait)` bodies in `ProjectHomeInboxPort`'s defaults were the daemon's job, not the defaults'; the defaults keep refusing (a host with no log) and the daemon's impl overrides both.
- **D18 — `tddy-daemon` names the log directly.** It gains a path dependency on `tddy-session-activity`, the one edge `mount_host` needs to construct `ProjectNotificationLog`. The log opens in `open_shared`, over the mount's sessions-base resolver and the data dir, before the bus is assembled, so the bus, the host and the RPC handler share one log.
- **D19 — dispatch and catalog order.** `DaemonProjectHome::execute` routes the inbox tools after the answer tools and before the terminal tools. The catalog places them after `AnswerSessionPermission`, before `TerminateSession`, and the `read_only.rs` pin is in that order.
- **D20 — no `TODO(desktop-notify)` in `mount_host`.** The changeset's D5 note names a block that carries one; grep finds none in `mount_host.rs` or elsewhere in `packages/`, so nothing was removed.
- **Added test.** `packages/tddy-session-activity/tests/project_notification_log_storage.rs` (2 tests) covers what the contract does not reach: the file staying bounded past the window with sequence numbers and `lost` intact across a restart, and a damaged line being skipped.

- **Pre-existing / flaky (not caused by this node).** Two `tddy-daemon-rpc` tests went red in a full-package run and were classified as flakes, re-run alone and left unfixed:
  - `coding_subagent_acceptance::cancelling_a_turn_paused_on_a_permission_request_answers_it_cancelled_and_keeps_the_conversation`: a read race. The test parses `record.jsonl` while it is being appended. It passes 3/3 alone.
  - `rpc_handlers_acceptance::a_catalog_handler_records_rpc_activity_on_the_hosts_own_idle_tracker`: the fixture's `IdleTimeoutTracker` has a 1 ms timeout, so a scheduler delay over 1 ms between the bump and the check flips it. It fails about 1 run in 3 alone. The path it exercises is `CatalogRpcHandler::list_tools`, which this node does not touch.
- The daemon-rpc full package needed `tddy-acp-stub` and four other helper bins built with `-p` first. Without them, 51 tests fail with "tddy-acp-stub not built".

- Cursor = sequence number, not time (dev-3.0's lesson: never guess a position from a window).
- 20 s cap keeps one call under every transport's deadline; the agent re-waits with the cursor.

## TODO

- [x] Record initial discovery (`2026-10-25-home-inbox-home-wait-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (approved by the orchestrator while the developer was AFK, 2026-10-28)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code (`/green`, 2026-10-29; decisions D11–D20 above)
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
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
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap

## Refactoring Needed
### From /red (TDD red phase)
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
| Validation | Last Run | Status |
|------------|----------|--------|
| /validate-changes | Not yet run | Pending |
| /validate-tests | Not yet run | Pending |
| /validate-prod-ready | Not yet run | Pending |
| /analyze-clean-code | Not yet run | Pending |
