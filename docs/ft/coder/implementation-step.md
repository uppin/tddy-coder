# Implementation Step — Feature Document

**Product Area**: Coder
**Status**: Complete
**Updated**: 2026-03-10

## Summary

The Implementation Step is the TDD Red-Green phase of the tddy-coder workflow. The **red** goal reads PRD.md and acceptance-tests.md from the plan directory, creates skeleton production code and failing lower-level tests, and persists its session to `changeset.yaml`. The **green** goal resumes the red session via `changeset.yaml`, reads progress.md (required) plus PRD.md and acceptance-tests.md (optional for context), implements production-grade code to make all failing tests pass, updates progress.md and acceptance-tests.md with results, verifies completion by running both unit and acceptance tests, and executes the demo plan when `demo-plan.md` exists.

## Background

tddy-coder follows a strict TDD workflow: plan → acceptance-tests → red → green. The red and green goals form the implementation phase. Red creates skeletons and failing tests; green implements production code to make them pass. Both use the same agent session (red starts it, green resumes it) for context continuity. Backend is selected via `--agent` (claude or cursor).

## Requirements

### CLI Interface

1. Accepts `--goal red` to create skeleton code and failing lower-level tests from PRD and acceptance-tests.md
2. Accepts `--goal green` to implement production code that makes failing tests pass
3. `--plan-dir <path>` is required when `--goal red` or `--goal green`
4. `--model`, `--agent`, `--agent-output`, `--conversation-output`, `--allowed-tools`, `--debug` work with both goals (Updated: 2026-03-07)

### Red Workflow

1. Read PRD.md and acceptance-tests.md from the plan directory specified by `--plan-dir`
2. Read `changeset.yaml` for model and state; start a fresh Claude session (does not resume planning session)
3. Use `--permission-mode acceptEdits` with same allowlist as acceptance-tests
4. System prompt instructs Claude to: plan implementation structure; create skeleton code that compiles; write failing lower-level tests; run `cargo test` to verify tests fail; emit structured response with tests and skeletons
5. Parse Claude's output to extract summary, tests, and skeletons
6. Write `red-output.md` and `progress.md` to the plan directory
7. Update `changeset.yaml` with new session entry for green to resume
8. On successful exit, output summary, test list, and skeleton list

### Green Workflow

1. Read `progress.md` from the plan directory specified by `--plan-dir` (required)
2. Read `PRD.md` and `acceptance-tests.md` from the plan directory if present (optional, for richer LLM context)
3. Read the session ID and model from `changeset.yaml` in the plan directory (persisted by the red goal)
4. Resume the Claude session using `--resume <session-id>` for context continuity with red
5. Use `--permission-mode acceptEdits` with same allowlist as red
6. System prompt instructs Claude to:
   - Read progress.md for the list of failing tests and skeleton implementations
   - Implement production-grade code to make all failing tests pass
   - Use detailed logging (`log::debug!`, `log::info!`, etc.) to reveal flows and system state during development; logs will be cleaned in later phases
   - After implementing, run the project's test command to verify tests pass
   - Run acceptance tests to verify end-to-end behavior
   - Emit structured response with implementation summary and test results
7. Parse Claude's output to extract summary, test results (pass/fail per test), and implementation details
8. Update `progress.md` in the plan directory: mark passing tests as `[x]`, mark implemented skeletons as `[x]`, mark still-failing tests with `[!]` and reason
9. Update `acceptance-tests.md` in the plan directory: update test statuses from "failing" to "passing" for tests that now pass
10. **Completion determination**:
    - If ALL unit tests AND ALL acceptance tests pass → state transitions to `GreenComplete`
    - If any test fails → state transitions to `Failed` with details of which tests failed
11. **Demo execution**: When `demo-plan.md` exists, green goal executes the demo plan and writes `demo-results.md`
12. On successful exit, output a human-readable summary (tests passed/failed count, implementation summary)

### Output Artifacts

#### red-output.md

- Written by the red goal to the plan directory
- **How to run tests**: Same as acceptance-tests.md
- **Prerequisite actions**: Same as acceptance-tests.md
- **How to run a single or selected tests**: Same as acceptance-tests.md
- Lists failing tests and skeletons (trait, struct, method, function, module)

#### changeset.yaml (session persistence)

- Updated by the red goal with a new session entry (id, agent, tag, system_prompt_file)
- Green goal reads the session ID from `changeset.yaml` to resume the same session for context continuity

#### progress.md

- Written by the red goal to the plan directory
- Unfilled milestones and checkboxes for failed tests and skeletons
- Green goal updates this document: marks passing tests as `[x]`, implemented skeletons as `[x]`, still-failing tests with `[!]` and reason

### State Machine

1. **Red goal**: Transitions `Init`/`Planned`/`AcceptanceTestsReady` → `RedTesting` → `RedTestsReady` (or `Failed`)
2. **Green goal**: Transitions `RedTestsReady` → `GreenImplementing` → `GreenComplete` (or `Failed`)

### Exit Output

- **red**: Summary of created skeletons and failing tests (requires `--plan-dir`)
- **green**: Summary of implementation results — tests passed/failed counts, implementation summary (requires `--plan-dir`)

## Acceptance Criteria

### Red Goal

- [x] `tddy-coder --goal red --plan-dir <path>` creates skeleton code and failing lower-level tests
- [x] Red goal reads PRD.md and acceptance-tests.md from plan directory
- [x] Red goal starts fresh session (no resume)
- [x] Red goal persists session to `changeset.yaml` in the plan directory
- [x] Red goal uses AcceptEdits permission mode and correct allowlist
- [x] Output prints summary, test list, and skeleton list
- [x] State machine transitions: Init/Planned/AcceptanceTestsReady → RedTesting → RedTestsReady
- [x] Error handling: missing plan-dir, missing PRD.md, missing acceptance-tests.md
- [x] `--model` and `--agent-output` flags work with the red goal

### Green Goal

- [x] `tddy-coder --goal green --plan-dir <path>` implements production code to make failing tests pass
- [x] Green goal reads `progress.md` from plan directory (required); reads `PRD.md` and `acceptance-tests.md` if present (optional context)
- [x] Green goal resumes the red session via `changeset.yaml` using `--resume`
- [x] System prompt instructs Claude to implement production-grade code guided by progress.md
- [x] System prompt instructs Claude to add detailed logging (feedback channels) that reveals flows and system state
- [x] Before completion, Claude runs both unit tests and acceptance tests to collect status
- [x] If all unit tests AND acceptance tests pass → `GreenComplete` state; output reports success
- [x] If any test fails → `Failed` state; output reports which tests failed
- [x] Green goal updates `progress.md`: marks passing tests `[x]`, implemented skeletons `[x]`, failing tests `[!]` with reason
- [x] Green goal updates `acceptance-tests.md`: changes test statuses from "failing" to "passing" where applicable
- [x] State machine transitions: `RedTestsReady` → `GreenImplementing` → `GreenComplete` (or `Failed`)
- [x] Error handling: missing plan-dir, missing progress.md, missing changeset.yaml
- [x] `--model` and `--agent-output` flags work with the green goal
- [x] Output prints implementation summary with test pass/fail counts
- [x] Structured response format consistent with other goals (`<structured-response>` JSON block)
