# Changeset: Project-home agent answers a session's question

**Created:** 2026-10-25
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 3 — four packages, reuses `6/8`'s wait machinery; the risk is the Claude-version-dependent decision format, settled in pass 2
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

- `tddy_session_activity::answers::{answer_question(caller_home, target, request_id, answers: Vec<QuestionAnswer { question_index, selected: Vec<String>, other: Option<String> }>), validate_question_answers(&PendingQuestion, &[QuestionAnswer]) -> Result<(), String>}`.
- Proto: `AwaitQuestionAnswerResponse.answers = 4` (`repeated QuestionAnswer`).
- `tddy_tools::session_hook::question_decision::{encode_question_decision(&PendingQuestion, &[QuestionAnswer]) -> String}` — the mechanism chosen in pass 2 (`updatedInput` vs deny-with-answers) after verifying the installed Claude Code; the build is recorded in Decisions.
- Hook: `PreToolUse(AskUserQuestion)` runs `--await-answer` with `timeout = answer_wait_secs + 10`.
- Tool `AnswerSessionQuestion(session_id, daemon_instance_id?, request_id, answers)` in `project_home_answer_tools.rs`; port `ProjectHomeAnswerPort::answer_question`.

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

## Draft PR contract

The first push publishes `answer_question`/`validate_question_answers`, the proto field, the
encoder signature, the tool (bodies `// TODO(question-answer): implement`), and the failing tests.

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

**`tddy-session-activity`**: `packages/tddy-session-activity/tests/question_answer_acceptance.rs`
- [ ] `an_orchestrated_childs_question_await_returns_the_home_agents_answers`
- [ ] `an_unknown_question_index_an_unoffered_label_or_two_selections_on_a_single_choice_question_are_refused_and_the_waiter_keeps_waiting`
- [ ] `answer_session_question_on_a_permission_question_is_refused_naming_answer_session_permission`

**`tddy-tools`**: `packages/tddy-tools/tests/session_hook_question_answer_acceptance.rs`
- [ ] `an_answered_question_prints_the_decision_that_hands_claude_those_answers` (pins the chosen format)
- [ ] `with_no_answer_before_the_deadline_the_question_hook_prints_nothing`

**`tddy-daemon-rpc`**: `packages/tddy-daemon-rpc/tests/project_home_answer_question_acceptance.rs`
- [ ] `answer_session_question_from_the_childs_home_wakes_the_waiting_hook`

### Test level decisions
| Aspect | Level | Rationale |
|---|---|---|
| Validation | Integration over the activity service | Refusal + continued wait is the behaviour |
| Decision format | Process-level, pinned string | Claude parses it |

## Decisions & Trade-offs

- A separate tool (`AnswerSessionQuestion`) rather than widening `6/8`'s, so no node changes a
  symbol another owns (2026-10-25, stated to the developer).
- Mechanism (`updatedInput` vs deny-with-answers): **open until pass 2**, verified against the
  installed Claude Code and recorded here with its version.

## TODO

- [x] Record initial discovery (`2026-10-25-home-inbox-question-answer-initial-discovery.md`)
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
