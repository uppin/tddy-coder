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

The first push publishes the log's types and method signatures, the two tools, the clamp entry
(bodies `// TODO(home-wait): implement`), and the failing tests.

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
- [ ] `a_notification_published_after_the_wait_began_returns_the_wait_with_it_and_a_new_cursor`
- [ ] `a_wait_with_nothing_new_times_out_and_a_timeout_above_20000_ms_is_capped`
- [ ] `waiting_from_a_returned_cursor_never_repeats_or_skips_across_a_restart`
- [ ] `another_projects_another_os_users_and_the_callers_own_records_never_appear`
- [ ] `reading_from_an_evicted_cursor_reports_how_many_records_were_lost`
- [ ] `an_question_record_carries_its_id_and_content`

**`tddy-daemon-rpc`**: `packages/tddy-daemon-rpc/tests/project_home_wait_acceptance.rs`
- [ ] `a_project_home_agent_waits_and_receives_a_childs_agent_notification`
- [ ] `a_non_project_home_caller_is_refused_as_permission_denied`

**`tddy-session-tool-client`**: unit
- [ ] `wait_project_notifications_timeout_is_clamped_on_livekit`

### Test level decisions
| Aspect | Level | Rationale |
|---|---|---|
| Log semantics | Integration over the subscriber + temp data dir | Persistence and cursors are the contract |
| Tool | Integration over the exec-tool handler | Role gate + scoping |

## Decisions & Trade-offs

- Cursor = sequence number, not time (dev-3.0's lesson: never guess a position from a window).
- 20 s cap keeps one call under every transport's deadline; the agent re-waits with the cursor.

## TODO

- [x] Record initial discovery (`2026-10-25-home-inbox-home-wait-initial-discovery.md`)
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
