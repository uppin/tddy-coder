# Changeset: Project-home agent answers a permission prompt

**Created:** 2026-10-25
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 5 — six packages, the daemon's first hold-until-answered RPC, a hook mode that prints a decision (Claude's contract), authorization across two sessions, bounded timeouts on three layers
**Stack:** `#home-inbox 6/8` (PR #771) — branch `feature/home-inbox/permission-answer`, base `feature/home-inbox/terminal-keys`
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

**Tests blocked on a parent (6e/6f, 2026-10-28).** The two `tddy-tools-cli` hook tests keep the
real report, because the report's `request_id` is what the hook waits on: they go green only once
`3/8` maps `PermissionRequest` to `WaitingForInput` and fills the tool fields. Every other test of
this node writes `3/8`'s record directly and depends on this node alone.

**Sequencing fact (replan 2026-10-26).** The `#start-parity` line predates `#coagents` on `master`,
so this base lacks `tddy-discovery/src/subagent/permission.rs`. This node's pass 2 needs that line
rebased onto `master` first. That rebase is `#start-parity`'s work, not this node's.

## Draft PR contract

The first push publishes the surface below with bodies `// TODO(permission-answer): implement`, and
the failing tests. Every stub refuses or answers `Unimplemented`; none panics.

- **Proto** (`activity.proto`, TS regenerated):
  - `rpc AwaitQuestionAnswer(AwaitQuestionAnswerRequest) returns (AwaitQuestionAnswerResponse)`.
  - `AwaitQuestionAnswerRequest { session_id = 1, hook_token = 2, os_user = 3, request_id = 4, permission_suggestions_json = 5 }`.
  - `enum AwaitQuestionOutcome { UNSPECIFIED = 0, ANSWERED = 1, NO_ORCHESTRATOR = 2, TIMED_OUT = 3 }`.
  - `AwaitQuestionAnswerResponse { outcome = 1, option_id = 2, cancelled = 3, message = 4, answered_by = 5 }`.
- **`tddy-discovery`**: `AnsweredBy::Orchestrator(String)` (the answering session id), with its
  `name()` and `Display` (`orchestrator session <id>`) arms.
- **`tddy-session-activity`**:
  - `pending_answers::PendingAnswers`: `open(session_id, PermissionRequestView)`,
    `open_request(id) -> Option<OpenRequest>`, `wait(id, Duration) -> WaitOutcome`,
    `answer_with_message(&PermissionAnswer, AnsweredBy, Option<String>)`, and `impl PermissionAnswering`.
    `WaitOutcome::{Answered(DeliveredAnswer { answer, by, message }), TimedOut}`.
  - `orchestrator_presence::{OrchestratorPresence { async fn is_active(&str) -> bool }, NoOrchestratorIsActive}`.
  - `answers`: `ALLOW_ONCE`, `ALLOW_ALWAYS`, `REJECT_ONCE`, `ANSWER_SESSION_QUESTION`;
    `AnsweringCaller { session_id, daemon_instance_id }`;
    `AnswerContext { sessions_base, this_daemon_instance_id, pending }`; `AnsweredPermission`;
    `AnswerRefusal::{NotTheOrchestrator, AlreadyAnswered, UnknownRequest, NotAPermission, UnknownOption, Io}`
    with its `Display`; `permission_request_view(&PendingQuestion, has_suggestions)`;
    `answer_permission(ctx, caller, target, PermissionAnswer, Option<String>) -> Result<AnsweredPermission, AnswerRefusal>`.
  - `AnswerWaitPorts { pending, presence, wait: Duration }` and `ActivityServiceImpl::with_question_answers`.
- **`tddy-session-lifecycle`** (wiring): `PeerRoutedActivity::await_question_answer` (not routed);
  `DaemonSessionHost::pending_answers()`; `ServedTerminalPresence` (stub); `activity_service()`
  applies `with_question_answers`.
- **`tddy-daemon-kernel`**: `ClaudeCliConfig.answer_wait_secs` (serde default 120),
  `DEFAULT_ANSWER_WAIT_SECS`, `DaemonConfig::claude_cli_answer_wait_secs()`.
- **`tddy-tools`**: `session-hook --await-answer` and `--await-timeout-secs` (default
  `DEFAULT_AWAIT_TIMEOUT_SECS` = 125); `permission_decision_json(&AwaitQuestionAnswerResponse, Option<&Value>) -> Option<String>`.
- **`tddy-tool-engine`**: `project_home_answer_tools`: `ANSWER_SESSION_PERMISSION`,
  `is_project_home_answer_tool`, `project_home_answer_tool_defs`, `AnswerPermissionArgs`,
  `ProjectHomeAnswerPort::answer_permission` (default refuses), `execute_project_home_answer_tool`.

### Deviations from the plan (found at 6d, approved 2026-10-28)

1. `PermissionAnswering::answer_as` carries no message, so `PendingAnswers::answer_with_message` is
   the real entry and `answer_as` delegates with `None`. `deferred_to_user` is always `false`;
   `pending_request()` is the earliest-opened open request across the daemon. An `answered` map
   outlives the waiter, so a second answer names the first answerer.
2. The registry and presence reach `ActivityServiceImpl` through the builder `with_question_answers`,
   not `ActivityPorts`, so existing `ActivityPorts` literals are unchanged.
3. `DaemonConfig.claude_cli` is `tddy-daemon-kernel/src/config/agent_cli.rs`'s `ClaudeCliConfig`; two
   `tddy-telegram-control` test literals gained `answer_wait_secs: 120`.
4. **Green's work:** the hook settings — `--await-answer` on the `PermissionRequest` entry and
   `timeout = answer_wait_secs + 10` — need `answer_wait_secs` on `HookCommandParams`, whose literals
   live in `tddy-agent-launch`, `tddy-session-split` and the cursor hooks. The entry itself is
   `3/8`'s and still a stub.
5. Extra hook flag `--await-timeout-secs`: the hook's own client bound, between the daemon's deadline
   and Claude's `timeout`, so the daemon answers `TIMED_OUT` before Claude kills the hook.
6. Extra request field `permission_suggestions_json = 5`: `3/8`'s record does not carry Claude's
   suggestions and is not this node's to change. They offer `allow_always`, and the hook echoes them
   back as `updatedPermissions`.
7. `AnswerRefusal::UnknownRequest` also covers "no hook is waiting any more" (a timed-out wait).
8. The hook's process test lives in `tddy-tools-cli` (it builds the binary), as `3/8`'s does.
9. **Green's work:** registering the tool — `is_project_home_tool`, `is_project_home_target_tool`,
   the `read_only.rs` pin, the catalog, `exec_tool_admission.rs`, `project_home_prompt.md` — and the
   daemon-rpc `ProjectHomeAnswerPort` impl, as `5/8` left its own.
10. Mechanical: `tddy-tools/tests/session_hook_pending_question.rs` gained the two new
    `SessionHookArgs` fields; `connection_service.rs` grew by 3 lines (the `pending_answers` field).

### Claude Code's `PermissionRequest` contract (https://code.claude.com/docs/en/hooks)

- Input: `tool_name`, `tool_input`, optional `permission_suggestions` (permission-update entries such
  as `{type: "addRules", rules: [{toolName, ruleContent}], behavior, destination}`).
- Output: `{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"|"deny", ...}}}`;
  `updatedInput` and `updatedPermissions` for allow only, `message` and `interrupt` for deny only.
- Exit 0 with no output is no decision, and the normal prompt follows; exit 2 is not honoured.
  Default command-hook timeout is 600 s.
- Mapping: `allow_once` → `allow`; `allow_always` → `allow` with `updatedPermissions` = the
  suggestions; `reject_*` → `deny` with `message`; cancelled, `TIMED_OUT`, `NO_ORCHESTRATOR` → nothing.

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

Written and failing (6e, approved by the orchestrator 2026-10-28; the developer AFK).

**`tddy-session-activity`**: `packages/tddy-session-activity/tests/permission_answer_acceptance.rs` — the
real handler, a fake presence port, an injected wait; the pending question is written directly.
- [x] `an_orchestrated_childs_await_returns_allow_when_its_home_answers_allow` — fails: the hook never opens its wait
- [x] `deny_with_a_message_returns_deny_with_that_message` — fails: the hook never opens its wait
- [x] `with_no_answer_before_the_deadline_the_await_times_out` — fails: `AwaitQuestionAnswer` is `Unimplemented`
- [x] `a_child_without_an_active_orchestrator_returns_no_orchestrator_at_once` (home not running / none / another host) — fails: `Unimplemented`
- [x] `another_home_an_unknown_id_and_an_already_answered_question_are_refused_and_the_waiter_keeps_waiting` — fails: the hook never opens its wait

**`tddy-tools-cli`**: `packages/tddy-tools-cli/tests/session_hook_await_answer_acceptance.rs` (moved from
`tddy-tools`, which builds no binary) — process level against a fake `/rpc` router.
**Blocked on `3/8`**: its `activity_status_from_hook_with_tool` stub maps `PermissionRequest` to no
status, so the hook sends no report and has no `request_id` to wait on.
- [x] `an_answered_permission_prints_claudes_allow_decision_json_and_exits_zero` — fails: stdout is empty
- [x] `a_timeout_or_daemon_error_prints_nothing_and_exits_zero` (timed out / no orchestrator / daemon error) — fails: `AwaitQuestionAnswer` never asked

**`tddy-daemon-rpc`**: `packages/tddy-daemon-rpc/tests/project_home_answer_permission_acceptance.rs` — the
real `activity_service()` and `ExecuteTool`; orchestrators run a stand-in terminal; the orchestrator
and pending question are written directly.
- [x] `answer_session_permission_from_the_childs_home_wakes_the_waiting_hook` — fails: the hook never opens its wait
- [x] `a_caller_that_is_not_the_childs_orchestrator_is_refused_as_permission_denied` — same
- [x] `a_spawn_capable_orchestrator_answers_its_own_child` — same
- [x] `a_second_answer_is_refused_naming_who_answered_first` — same
- [x] `a_question_question_is_refused_naming_answer_session_question` — fails: the tool is unregistered, so the read-only guard refuses it

### Unit tests (6f)

| Surface | File | Pins | Now |
|---|---|---|---|
| `permission_decision_json` | `packages/tddy-tools/tests/permission_decision_json.rs` | `allow_once` → plain allow; `allow_always` → allow + `updatedPermissions`; `reject_once` → deny + message; one line; timed out / no orchestrator / cancelled / unspecified → `None` | 4 fail, 4 pass (the `None` guards) |
| `permission_request_view` | `packages/tddy-session-activity/tests/permission_request_view.rs` | title, ACP kind (`execute`/`edit`/`read`/`other`), parsed input or its text, options in order with `allow_always` only with suggestions | 7 fail |
| `AnswerRefusal` `Display` | `packages/tddy-session-activity/tests/answer_refusal_messages.rs` | every refusal's exact text | 6 pass (implemented with the surface) |
| `PendingAnswers` | `packages/tddy-session-activity/tests/pending_answers_unit.rs` | open, answer wakes the wait, answer before the wait, second answer refused naming the first, unknown request refused, deadline closes the request | 6 fail, 1 pass (`deferred_to_user` is false) |
| `answer_wait_secs` | `packages/tddy-daemon-kernel/tests/answer_wait_secs_config.rs` | serde default 120; absent section → 120; configured value read | 1 fails, 2 pass (defaults) |

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
- **`ActivityServiceImpl::new` holds no hook** (accepted 2026-10-28): it defaults to
  `AnswerWaitPorts::default()`, whose `NoOrchestratorIsActive` answers `NO_ORCHESTRATOR` — the
  designed outcome when no orchestrator is present, reported explicitly rather than masking
  anything. The daemon wires `ServedTerminalPresence` through `with_question_answers`, and the
  daemon-rpc acceptance tests go through the real `activity_service()`, so they pin that wiring.
  Not a fallback; green does not revisit it.
- **Same host only, for now.** The await waits only while the orchestrator runs on the child's
  host. An orchestrator on another host (`orchestrator_daemon_instance_id` set, `#start-parity 2/9`)
  gets `NO_ORCHESTRATOR`, so Claude's own prompt appears. Asking a peer daemon whether its session is
  live is a new peer RPC, recorded as a TODO rather than built here.

### Decisions taken at green (2026-10-28, developer AFK)

- **`answer_permission` reads the offered options from the open waiter**, not from the pending
  question: `allow_always` is offered only when Claude sent suggestions, and only the waiter knows
  (`permission_suggestions_json` rides on the await request, deviation 6). A request with no waiter
  open is `UnknownRequest`, as deviation 7 says.
- **Check order** in `answer_permission`: orchestrator, already answered, pending question present,
  kind, waiter and option. "Already answered" precedes the pending-question check because an answer
  clears the question, and a second answerer must be told who answered first rather than "unknown".
- **The orchestrator's host** is the changeset's `orchestrator_daemon_instance_id`, or this host
  when absent; a caller on another host id is `NotTheOrchestrator`.
- **The activity record** of an answer is one `completed` row under `permission:<request_id>`
  (`input` = the request view, `result` = `answered_output`); its write failure is logged, not a
  refusal, because the hook is already woken. Clearing the pending question failing *is* an `Io`.
- **`PendingAnswers::wait`** removes the waiter on timeout (a late answer is `no hook is waiting`),
  and an answer that lands between the deadline and the lock is returned, not lost.
- **Hook flags and timeouts.** `HookCommandParams` gained `answer_wait_secs`; the `PermissionRequest`
  entry runs `--await-answer --await-timeout-secs <wait + 5>` with Claude's `timeout` = wait + 10
  (`AWAIT_CLIENT_MARGIN_SECS`, `CLAUDE_HOOK_TIMEOUT_MARGIN_SECS` in `tddy-agent-backend`). Cursor's
  literal passes the configured value, which cursor hooks never read.
- **The hook awaits only when its report recorded a question** (non-empty `request_id`); with none
  it prints nothing.
- **`AnswerSessionPermission` is also a spawn-capable session's tool**: `is_conversation_spawner_tool`
  names it (now five tools), since the acceptance test has a grill-me orchestrator answer its child.
- **Catalog order**: `AnswerSessionPermission` follows `SendSessionKeys`; the three pinned lists
  (`read_only.rs`, `exec_tool_admission.rs`, `project_home_session_tools_acceptance.rs`) say so.
- **`project_home_prompt.md`** does not mention `AnswerSessionQuestion`: that tool is `8/8`'s and is
  not registered here.

## TODO

- [x] Record initial discovery (`2026-10-25-home-inbox-permission-answer-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (reviewed by the orchestrator, developer AFK, 2026-10-28)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
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
