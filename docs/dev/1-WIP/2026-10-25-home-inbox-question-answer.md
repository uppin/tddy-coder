# Changeset: Project-home agent answers a session's question

**Created:** 2026-10-25
**Status:** 🚧 In Progress — pass 2 contract published; red phase done
**Type:** Feature
**Complexity**: 3 — six packages (four own surface), reuses `6/8`'s wait machinery; the risk is the Claude-version-dependent decision format, settled in pass 2
**Stack:** `#home-inbox 8/8` — branch `feature/home-inbox/question-answer`, base `feature/home-inbox/home-wait`
**PRD:** [PRD-2026-10-25-question-answer.md](../../ft/daemon/1-WIP/PRD-2026-10-25-question-answer.md)

## Initial Discovery

[2026-10-25-home-inbox-question-answer-initial-discovery.md](./2026-10-25-home-inbox-question-answer-initial-discovery.md)

State A below is distilled from that file. Do not duplicate grep traces or file dumps here.

## Prerequisites

| Record | Verdict |
|---|---|
| `packages/tddy-tools/docs/code-issues/oversized-file-server.md` | ⚠ During — question encoding in the `session_hook` modules |

## Affected Packages

- [ ] [`tddy-session-activity`](../../../packages/tddy-session-activity/README.md): `answer_question` + validation; the await result carries answers
- [ ] [`tddy-service`](../../../packages/tddy-service/README.md): `AwaitQuestionAnswerResponse.answers`
- [ ] [`tddy-tools`](../../../packages/tddy-tools/README.md): the question decision encoding in `--await-answer`
- [ ] [`tddy-agent-backend`](../../../packages/tddy-agent-backend/README.md): the `PreToolUse(AskUserQuestion)` entry gains `--await-answer` + `timeout`
- [ ] [`tddy-tool-engine`](../../../packages/tddy-tool-engine/README.md), [`tddy-daemon-rpc`](../../../packages/tddy-daemon-rpc/README.md): `AnswerSessionQuestion`

## Responsibility

The surface as published in pass 2 (bodies `// TODO(question-answer): implement`):

- `tddy_session_activity::answers` (`src/answers/question.rs`, re-exported):
  `answer_question(&AnswerContext, &AnsweringCaller, target, request_id, Vec<QuestionAnswer>) -> Result<AnsweredQuestion, AnswerRefusal>`,
  `validate_question_answers(&PendingQuestion, &[QuestionAnswer]) -> Result<(), String>`,
  `question_tool_call_id(request_id) -> String` (`question:<id>`),
  `answered_question_output(answered_by, &[QuestionAnswer]) -> Value`,
  `AnsweredQuestion`, `ANSWER_SESSION_PERMISSION`, and `QuestionAnswer` (the proto message, re-exported).
  `AnswerRefusal` gains `NotAQuestion`, `InvalidAnswers { request_id, reason }`, `NoRequestId { session_id }`.
- `tddy_session_activity::pending_answers` (`src/pending_answers/question.rs`): `PendingAnswers::{open_question, open_question_request, wait_for_question, answer_question_with, question_answered_by}`, `OpenQuestion`, `QuestionWaitOutcome`, `DeliveredQuestionAnswers`; the waiters live in a private `QuestionSlots` beside `6/8`'s permission waiters.
- Proto (`activity.proto`): `message QuestionAnswer { uint32 question_index = 1; repeated string selected = 2; optional string other = 3; }` and `AwaitQuestionAnswerResponse.answers = 6` (4 and 5 are `message` and `answered_by`).
- `tddy_tools::session_hook::question_decision::encode_question_decision(&serde_json::Value /* the hook's tool_input */, &[QuestionAnswer]) -> Option<String>` — the `updatedInput` mechanism, verified on Claude Code 2.1.312 (Decisions). `None` prints nothing.
- Tool `AnswerSessionQuestion(session_id, daemon_instance_id?, request_id, answers)` in `project_home_answer_tools.rs`: `ANSWER_SESSION_QUESTION`, its def (returned by `project_home_answer_tool_defs()`, accepted by `is_project_home_answer_tool`), `AnswerQuestionArgs`, `QuestionAnswerArg`, and port method `ProjectHomeAnswerPort::answer_question` (default refuses).
- **Green work, no surface in the contract:** `PreToolUse(AskUserQuestion)` runs `--await-answer` with `timeout = answer_wait_secs + 10` (`claude_hooks.rs`); the question branch of the hook's await mode and of `await_question_answer_impl`; wiring `AnswerSessionQuestion` into dispatch, the catalog pins and `project_home_prompt.md`.

## Boundaries

- No change to permissions (`6/8`) or to capture (`3/8`).
- No free-text prompts outside `AskUserQuestion`; no Telegram/web answers.
- No answers to a `tool` session's clarification. `#start-parity 6/9` records those in the same file
  (`select`/`multi_select`/`text`), but they are answered through the presenter, which this node
  does not drive. A question whose record has no `request_id` is refused, naming why.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `#home-inbox 6/8` permission-answer | `PendingAnswers`, `AwaitQuestionAnswer`, `OrchestratorPresence`, the `--await-answer` hook mode, `project_home_answer_tools.rs` + `ProjectHomeAnswerPort`, `answer_wait_secs` | adds a question branch to the await mode and the registry's `Answer`; adds a tool and a port method beside `AnswerSessionPermission` | change `AnswerSessionPermission`, the registry's wait semantics, presence, or the config |
| `#start-parity 6/9` session-follow (#755) | `PendingQuestion` and `ReadSessionActivity` | the home agent reads the question it answers there; answers are validated against the record | change either |
| `#home-inbox 3/8` elicit-content | question `PendingQuestion` (questions, options, `multi_select`) and its hook entry | validates answers against it; extends the entry's command | change capture |

**Sequencing (recorded in pass 2).** `6/8`'s green owns wiring `AnswerSessionPermission` into the
daemon's dispatch and the catalog; this node's green wires only `AnswerSessionQuestion`, and runs
**after** `6/8`'s green and `3/8`'s. Until then these acceptance tests are blocked: the await
handler's body (`6/8`), the hook's await mode (`6/8`), the `PreToolUse(AskUserQuestion)` status
mapping that makes the hook report at all (`3/8`).

**Additive edits to parent files (accepted):** `pending_answers.rs` (a `mod` and one private field),
`answers.rs` (a `mod`, a `pub use`, three `AnswerRefusal` variants), `project_home_answer_tools.rs`
(a second def, the name in `is_project_home_answer_tool`, a port method), and `..Default::default()`
in the five `AwaitQuestionAnswerResponse` literals of `6/8`'s tests the new proto field broke.

## Draft PR contract

The first push publishes the surface listed under Responsibility (bodies
`// TODO(question-answer): implement`, never a panic) and the failing tests below. A stub that must
not fail open does not: `validate_question_answers` returns `Err`, `answer_question` a refusal,
`encode_question_decision` `None`, the registry methods refuse or time out.

## Green wave

**Wave:** 4 of 4
**Greenable independently:** no — its tests wait through `6/8`'s registry and await RPC (behaviour).
**Concurrent with:** nothing
**Blocks:** nothing

    n1 → n3, n4, n7      n2 → n5      n3 → n6, n7      n6 → n8      (cross-stack, replan 2026-10-26: #start-parity #750 [merged] → n2, n5, n6, n7, n8 · #755 → n3 · #751, #758 → n6 · #coagents [merged] → n6)

## State A (after `6/8`)

Permission requests of orchestrated children can be answered; questions are
recorded but only Claude's own dialog answers them.

## State B

An orchestrated child's `AskUserQuestion` waits ≤ 120 s; `AnswerSessionQuestion` from its home,
validated against the recorded options, wakes it; the hook prints the decision that hands Claude
those answers.

## Delta

### New
- `answer_question`, `validate_question_answers`; `question_decision.rs`; `AnswerSessionQuestion`
### Modified
- `AwaitQuestionAnswerResponse`, the await-mode hook, the hook entry, catalog pins, `project_home_prompt.md`
- `docs/ft/daemon/{claude-cli-session.md,project-concept.md}` at wrap

## Testing Strategy

### Acceptance tests

Approved by the orchestrator (developer AFK). Each fails now; "blocked" = fails before reaching this node's own code.

**`tddy-session-activity`**: `packages/tddy-session-activity/tests/question_answer_acceptance.rs`
- [x] `an_orchestrated_childs_question_await_returns_the_home_agents_answers` — also pins the child's `question:<id>` record. Fails: `the hook never opened its wait for req-which-language`. **Blocked on `6/8`** (`await_question_answer_impl`).
- [x] `an_unknown_question_index_an_unoffered_label_or_two_selections_on_a_single_choice_question_are_refused_and_the_waiter_keeps_waiting` — same failure, **blocked on `6/8`**.
- [x] `answer_session_question_on_a_permission_question_is_refused_naming_answer_session_permission` — fails: `expected a NotAQuestion refusal, got Err(UnknownRequest { … not implemented })`. Not blocked.

**`tddy-tools-cli`** (not `tddy-tools`, as `6/8`'s): `packages/tddy-tools-cli/tests/session_hook_question_answer_acceptance.rs`
- [x] `an_answered_question_prints_the_decision_that_hands_claude_those_answers` (pins the chosen format)
- [x] `with_no_answer_before_the_deadline_the_question_hook_prints_nothing`
  - Both fail: `the hook never asked AwaitQuestionAnswer; it called ["ReportAgentActivity"]`. **Blocked on `3/8`** (status mapping) **and `6/8`** (hook await mode).

**`tddy-daemon-rpc`**: `packages/tddy-daemon-rpc/tests/project_home_answer_question_acceptance.rs`
- [x] `answer_session_question_from_the_childs_home_wakes_the_waiting_hook` — fails: `the hook of <child> never opened its wait`. **Blocked on `6/8`** (await body, dispatch and port wiring).

### Unit / integration tests (6f)

- `packages/tddy-session-activity/tests/question_answer_unit.rs` — 26 cases. Red (22): every `validate_question_answers` case (6 accepted, 8 refused with their reason), the registry cycle (open/answer/wake, answer before wait, first wins, timeout removes the waiter, question vs permission waiters) and the `question:<id>` record (id, output shape). Green already (4): the three `AnswerRefusal` Displays and a wait on an unopened request.
- `packages/tddy-tools/tests/question_decision.rs` — 9 cases. Red (6): single, multi, free text alone, labels + free text, an unanswered question left out of the map, the call's other input kept. Green already (3, the stub's `None`): no answers, an unknown index, an input without `questions`.
- `packages/tddy-tool-engine/tests/project_home_answer_question_tool.rs` — 4 cases: the name, the two defs in order, the question def's schema, the args parsing. All green: this surface is real. Dispatch through a port is covered by the acceptance tests; a `ProjectHomePort` fake is too wide for a unit test.

### Test level decisions
| Aspect | Level | Rationale |
|---|---|---|
| Validation | Integration over the activity service | Refusal + continued wait is the behaviour |
| Decision format | Process-level, pinned string | Claude parses it |

## Decisions & Trade-offs

- A separate tool (`AnswerSessionQuestion`) rather than widening `6/8`'s, so no node changes a
  symbol another owns (2026-10-25, stated to the developer).
- **Mechanism: `updatedInput` (a), decided in pass 2 (2026-10-28).** Verified interactively against
  Claude Code **2.1.312**: a `PreToolUse(AskUserQuestion)` hook printing
  `{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow","updatedInput":{"questions":[…unchanged…],"answers":{"<question text>":"<answer>"}}}}`
  makes Claude skip its dialog, run the tool as answered (`User answered Claude's questions: · … → Blue`)
  and continue; no denied call appears in the transcript. `claude -p` does not offer `AskUserQuestion`,
  so headless runs cannot check it. An answer string is the labels joined with `", "` (the build's own
  schema: "multi-select answers are comma-separated"), then `other`; `other` alone is the text.
  (b) deny-with-answers was not needed. The format is pinned in `question_decision.rs` and by
  `an_answered_question_prints_the_decision_that_hands_claude_those_answers`.
- **Pass-2 deviations from the Responsibility section** (each also stated in the contract commit):
  `AwaitQuestionAnswerResponse.answers = 6` (4 and 5 are `message` and `answered_by`); the
  `QuestionAnswer` type is the proto message, re-exported where the changeset named a Rust struct;
  `encode_question_decision(&Value, &[QuestionAnswer]) -> Option<String>` takes the hook's own
  `tool_input` (the hook has stdin, not a `PendingQuestion`) and prints nothing on `None`; the
  process-level test lives in `tddy-tools-cli` (as `6/8`'s does).

- **Answer validation (pass 2).** Refused, naming why: an index the call has no question at; a question
  answered twice; a label not offered, exactly spelled; more than one answer to a single-choice question
  (a label and `other` count one each); an answer that selects nothing. `other` is free text, always
  accepted. A question the answers leave out is not refused.
- **Recording (decided with the orchestrator, 2026-10-28; mirrors `6/8`).** The child's activity gets
  `question:<request_id>` = `{"answeredBy", "answers": [{"questionIndex", "selected", "other"?}]}`, beside
  `permission:<id>` = `{"answeredBy","optionId"}`. The home's side is the `AnswerSessionQuestion` tool call its
  log already records; nothing extra is written there. Pinned by the first acceptance test and the unit tests.
- **Registry (pass 2).** The question waiters are separate methods and state of `PendingAnswers`: `PermissionAnswer`
  and `DeliveredAnswer` have no room for per-question answers, and a new field on `DeliveredAnswer` would break
  `6/8`'s constructors when it greens.

## TODO

- [x] Record initial discovery (`2026-10-25-home-inbox-question-answer-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (approved by the orchestrator, developer AFK)
- [x] TDD Red — write failing unit/integration tests
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
