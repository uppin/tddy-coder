# Changeset: Project-home agent answers a permission prompt

**Created:** 2026-10-25
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 5 — six packages, the daemon's first hold-until-answered RPC, a hook mode that prints a decision (Claude's contract), authorization across two sessions, bounded timeouts on three layers
**Stack:** `#home-inbox 6/8` — branch `feature/home-inbox/permission-answer`, base `feature/home-inbox/terminal-keys`
**PRD:** [PRD-2026-10-25-permission-answer.md](../../ft/daemon/1-WIP/PRD-2026-10-25-permission-answer.md)

## Initial Discovery

[2026-10-25-home-inbox-permission-answer-initial-discovery.md](./2026-10-25-home-inbox-permission-answer-initial-discovery.md)

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-09-10-the-permission-engines-ndjson-unix-socket-protocol-survives.md](../todo/2026-09-10-the-permission-engines-ndjson-unix-socket-protocol-survives.md) | — Unrelated: that is the headless `approval_prompt` relay; this node answers interactive claude-cli hooks over `ActivityService` and does not touch it |
| [2026-10-23-project-home-running-heuristic.md](../todo/2026-10-23-project-home-running-heuristic.md) | ⚠ During — "the orchestrator is active" must use the **served terminal** of the home session on this daemon, not pid liveness, so a dead home with a reused pid never makes a child wait |
| [2026-10-23-project-home-auto-stop-untracked-processes.md](../todo/2026-10-23-project-home-auto-stop-untracked-processes.md) | — Unrelated |
| `packages/tddy-tools/docs/code-issues/oversized-file-server.md` | ⚠ During — the await mode lives in `session_hook` modules, not `server.rs` |

## Affected Packages

- [ ] [`tddy-service`](../../../packages/tddy-service/README.md): `AwaitQuestionAnswer`
- [ ] [`tddy-session-activity`](../../../packages/tddy-session-activity/README.md): pending-answer registry, the long poll, `answer_permission`
- [ ] [`tddy-tools`](../../../packages/tddy-tools/README.md): `session-hook --await-answer`
- [ ] [`tddy-agent-backend`](../../../packages/tddy-agent-backend/README.md): the `PermissionRequest` hook command + `timeout`
- [ ] [`tddy-discovery`](../../../packages/tddy-discovery/README.md): `AnsweredBy::Orchestrator`
- [ ] [`tddy-daemon-kernel`](../../../packages/tddy-daemon-kernel/README.md): `answer_wait_secs` config
- [ ] [`tddy-tool-engine`](../../../packages/tddy-tool-engine/README.md), [`tddy-daemon-rpc`](../../../packages/tddy-daemon-rpc/README.md): `AnswerSessionPermission`

## Responsibility

- **Proto.** `ActivityService.AwaitQuestionAnswer(AwaitQuestionAnswerRequest { session_id, hook_token, os_user, request_id })`
  → `{ outcome: ANSWERED | NO_ORCHESTRATOR | TIMED_OUT, option_id, cancelled, message, answered_by }`.
- **One answering vocabulary, `#coagents`'** (`tddy_discovery::subagent::permission`):
  - A claude-cli child's pending permission (`3/8`) is offered as a `PermissionRequestView`:
    - `request_id`;
    - `tool_call { title: tool_name, kind, raw_input: tool_input }`;
    - `options` `allow_once` and `reject_once`, plus `allow_always` when Claude sends permission suggestions.
  - The answer is a `PermissionAnswer::{Selected { option_id }, Cancelled}`.
  - `AnsweredBy` gains `Orchestrator(String)`, the answering session id. That is this node's only
    change in `tddy-discovery`.
- **`tddy_session_activity::pending_answers::PendingAnswers`** (in memory, one waiter per `request_id`):
  - It implements `PermissionAnswering`: the first answer wins, and a later one is refused naming who
    answered first.
  - It also exposes `wait(request_id, deadline)`.
- **`tddy_session_activity::orchestrator_presence::OrchestratorPresence`** (port):
  - `is_active(orchestrator_session_id) -> bool` is true when this daemon serves that session's terminal.
  - An orchestrator on another host (`orchestrator_daemon_instance_id` set and not this host) is not
    active here.
- **`tddy_session_activity::answers::answer_permission(caller, target, answer: PermissionAnswer, message)`**
  refuses unless all of these hold:
  - the target's changeset `orchestrator_session_id` names the caller (a project-home session, or a
    spawn-capable one, per `#start-parity 9/9`);
  - a pending question is open under that `request_id`;
  - its kind is `permission`.

  It then records who answered, when and what in the session's activity and clears the record.
- **`tddy-tools session-hook --await-answer`.** It reports through `3/8`'s fields, then calls
  `AwaitQuestionAnswer`.
  - On `ANSWERED` with an `allow_*` option it prints
    `{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}`.
  - On a `reject_*` option it prints the same JSON with `"behavior":"deny"` and the message.
  - On `cancelled`, `TIMED_OUT` or `NO_ORCHESTRATOR` it prints nothing.
  - It always exits 0.
- **Hooks.** The `PermissionRequest` entry runs with `--await-answer` and
  `timeout = answer_wait_secs + 10`.
- **Config.** `DaemonConfig.claude_cli.answer_wait_secs` (default 120).
- **Tool.** `AnswerSessionPermission(session_id, daemon_instance_id?, request_id, option_id | cancelled, message?)`,
  in a new module `project_home_answer_tools.rs`.
  - Its fields mirror `AnswerAgentPermissionRequest`.
  - It is a target tool under `#start-parity 1/9`'s forward, so it runs on the child's host.
  - Port: `ProjectHomeAnswerPort::answer_permission`.

## Boundaries

- No question answers (`8/8`) — a question is refused naming `AnswerSessionQuestion`.
- No answers from Telegram or the web; no auto-answer policies.
- Sessions without an active orchestrator are unchanged (the await returns `NO_ORCHESTRATOR` at once).

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `#home-inbox 3/8` elicit-content | the structured `PendingQuestion` with `request_id`, `tool_name` and `tool_input_json`; the `PermissionRequest` hook entry; `ReportSessionStatusResponse.request_id` | awaits on the returned `request_id`; builds the `PermissionRequestView` from the record; extends the hook entry's command | change the capture, the keep and clear rules, or the record's fields |
| `#home-inbox 2/8` terminal-read | the target-tool pattern for project-home tools | same targeting and forward | — |
| `#coagents 3/9` permission-yield + `8/9` web-approvals (#722, #727, merged) | `PermissionRequestView`, `PermissionAnswer`, `PermissionAnswering`, `AnsweredBy`, and `AnswerAgentPermissionRequest`'s field names | implements the trait; adds `AnsweredBy::Orchestrator`; mirrors the field names | change `ParkedPermissions`, `ApprovalRoutes`, `AnswerAgentPermission`, or the ACP flow |
| `#start-parity 2/9` start-tool (#751) + `9/9` spawn-conversation (#758) | `orchestrator_session_id` and `orchestrator_daemon_instance_id` on every start; spawn-capable sessions as orchestrators | authorises by `orchestrator_session_id`; reads `orchestrator_daemon_instance_id` for presence | change how starts record the orchestrator |
| `#start-parity 1/9` remote-targets (#750, merged) | the generic forward | `AnswerSessionPermission` is a target tool | change the forward |

**Sequencing fact (replan 2026-10-26).** The `#start-parity` line predates `#coagents` on `master`,
so this base lacks `tddy-discovery/src/subagent/permission.rs`. This node's pass 2 needs that line
rebased onto `master` first. That rebase is `#start-parity`'s work, not this node's.

## Draft PR contract

The first push publishes the proto RPC (regenerated), `PendingAnswers`, `OrchestratorPresence`,
`answer_permission`, the hook flag, the config field, the tool (bodies `// TODO(permission-answer): implement`),
and the failing tests.

## Green wave

**Wave:** 3 of 4
**Greenable independently:** no — the hook test reports through `3/8`'s `ReportSessionStatus` and
answers the record it stores (behaviour).
**Concurrent with:** `#home-inbox 7/8` home-wait
**Blocks:** `#home-inbox 8/8` question-answer (reuses `PendingAnswers`, the await RPC and the hook mode)

    n1 → n3, n4, n7      n2 → n5      n3 → n6, n7      n6 → n8      (cross-stack, replan 2026-10-26: #start-parity #750 [merged] → n2, n5, n6, n7, n8 · #755 → n3 · #751, #758 → n6 · #coagents [merged] → n6)

## State A (after `3/8`)

A permission prompt is recorded and announced; the hook returns at once with no decision; Claude
shows its own prompt. The daemon has no RPC that holds a caller for a human answer.

## State B

An orchestrated child's hook reports, then waits ≤ 120 s; `AnswerSessionPermission` from its own
home wakes it with allow/deny; Claude proceeds. No orchestrator, a dead orchestrator, or the deadline
→ no output → Claude's own prompt.

## Delta

### New
- Proto RPC; `pending_answers.rs`, `orchestrator_presence.rs`, `answers.rs` (session-activity); `project_home_answer_tools.rs` (tool-engine); the port impl (daemon-rpc)
### Modified
- `session_hook.rs` (await mode), `build_claude_hooks_settings` (command flag + `timeout`), `DaemonConfig`
- Catalog pins; `project_home_prompt.md`
- `docs/ft/daemon/{claude-cli-session.md,project-concept.md}` at wrap

## Testing Strategy

### Acceptance tests

**`tddy-session-activity`**: `packages/tddy-session-activity/tests/permission_answer_acceptance.rs`
- [ ] `an_orchestrated_childs_await_returns_allow_when_its_home_answers_allow`
- [ ] `deny_with_a_message_returns_deny_with_that_message`
- [ ] `with_no_answer_before_the_deadline_the_await_times_out`
- [ ] `a_child_without_an_active_orchestrator_returns_no_orchestrator_at_once`
- [ ] `another_home_an_unknown_id_and_an_already_answered_question_are_refused_and_the_waiter_keeps_waiting`

**`tddy-tools`**: `packages/tddy-tools/tests/session_hook_await_answer_acceptance.rs`
- [ ] `an_answered_permission_prints_claudes_allow_decision_json_and_exits_zero`
- [ ] `a_timeout_or_daemon_error_prints_nothing_and_exits_zero`

**`tddy-daemon-rpc`**: `packages/tddy-daemon-rpc/tests/project_home_answer_permission_acceptance.rs`
- [ ] `answer_session_permission_from_the_childs_home_wakes_the_waiting_hook`
- [ ] `a_caller_that_is_not_the_childs_orchestrator_is_refused_as_permission_denied`
- [ ] `a_spawn_capable_orchestrator_answers_its_own_child`
- [ ] `a_second_answer_is_refused_naming_who_answered_first`
- [ ] `a_question_question_is_refused_naming_answer_session_question`

### Test level decisions
| Aspect | Level | Rationale |
|---|---|---|
| Wait/answer semantics | Integration over the activity service with a fake presence port | Timing is controllable |
| Hook output | Process-level | Claude parses stdout |

## Decisions & Trade-offs

- The hook always runs `--await-answer`; the daemon decides whether to wait, because a child is
  tagged with its orchestrator only after its hooks are written.
- In-memory registry: a daemon restart drops waiters, and their hooks fall back to Claude's prompt.
- **One answering vocabulary (replan 2026-10-26).** `#coagents` landed `PermissionRequestView`,
  `PermissionAnswer`, `PermissionAnswering` and `AnsweredBy` for ACP and headless subagents. This node
  uses them rather than its own decision enum. An orchestrator reads a claude-cli child's request
  and an ACP subagent's request in one shape, and both refuse a second answer the same way.
  `ParkedPermissions` itself is not reused: it parks an ACP turn in process, and this node holds
  a hook process across an RPC.
- **Same host only, for now.** The await waits only while the orchestrator runs on the child's
  host. An orchestrator on another host (`orchestrator_daemon_instance_id` set, `#start-parity 2/9`)
  gets `NO_ORCHESTRATOR`, so Claude's own prompt appears. Asking a peer daemon whether its session is
  live is a new peer RPC, recorded as a TODO rather than built here.

## TODO

- [x] Record initial discovery (`2026-10-25-home-inbox-permission-answer-initial-discovery.md`)
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
