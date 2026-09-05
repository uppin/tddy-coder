---
description: Plan and drive complete feature development from PRD through TDD to production readiness
---

# Plan TDD One-Shot

Complete feature development from planning through production readiness. This is the most comprehensive development command.

**Fluent-tests is the mandatory test style for this repo.** Every test written during this
workflow (acceptance, unit, integration, behavior-preservation) must comply with the
`fluent-tests` skill at `.agents/skills/fluent-tests/`. Before writing tests, read
`.agents/skills/fluent-tests/references/generic-guidelines.md` and the framework-specific
reference for the test type. Required: Given/When/Then, intent-revealing names, one
behavior per test, named page-object/driver helpers (no raw selectors in test bodies),
meaningful fixtures, and in-memory backends instead of `cy.intercept` for Cypress
component tests. `/validate-tests` enforces this standard in Phase 3.

The core workflow implemented here lives in the `.agents/skills/plan-tdd-dev/` skill; the
red-green methodology in `.agents/skills/tdd/`.

**Prerequisites**: the user has described the feature or change they want, plus context about the affected code/packages if modifying existing behavior.

## Step 1: Gather Requirements

**MANDATORY** — gather requirements explicitly with the `AskQuestion` tool. Do not skip this step or assume requirements. Ask about:

- What is the user trying to achieve?
- Is this new functionality or a modification of existing functionality?
- Which parts of the codebase will be affected?
- Any specific requirements or constraints?
- Design preferences and user experience expectations

Present questions in organized multi-select widgets where appropriate and wait for responses. Then create the PRD document following the `/plan-ft` process.

## Step 2: Collaborative Planning

Use the EnterPlanMode tool to switch to plan mode for collaborative planning with the user. Discuss:
- Architecture decisions
- Implementation approach
- Testing strategy
- Risk areas and trade-offs

Plan mode is used so multiple implementation options can be explored, trade-offs discussed before any code is written, and the changeset and milestones designed together. Stay in plan mode until the user approves the plan.

## Step 3: Create Changeset

Create a development changeset following the `/plan-ft-dev` process. **CRITICAL**: include the entire plan-mode discussion/document as the first section of the changeset — this preserves the collaborative planning context, documents why approaches were chosen over others, and captures the user preferences and constraints discussed.

## Step 4: Generate TODO List

Generate approximately 25 detailed TODOs organized into three phases. Every step is a discrete, actionable item — never condense the list into 5-6 high-level phases. User review checkpoints and validation steps are always separate TODOs marked MANDATORY.

### Phase 1: Planning (2 TODOs)

- [ ] **TODO-01**: Create PRD document (`/plan-ft`)
- [ ] **TODO-02**: Create changeset with development plan (`/plan-ft-dev`)

### Phase 2: Development / TDD Cycle (9 TODOs)

For each milestone in the changeset:
- [ ] **TODO-03**: Write acceptance tests via `/ft-dev` (tests should FAIL initially)
- [ ] **TODO-04**: **MANDATORY** — run the newly created acceptance tests and confirm every one fails
- [ ] **TODO-05**: Implement minimum code to pass first test (`/green`)
- [ ] **TODO-06**: Continue TDD cycle for milestone 1 (`/red` -> `/green` -> `/update-context-docs`)
- [ ] **TODO-07**: **CHECKPOINT: Ask the user to review acceptance tests**
- [ ] **TODO-08**: TDD cycle for milestone 2
- [ ] **TODO-09**: TDD cycle for milestone 3
- [ ] **TODO-10**: **CHECKPOINT: Ask the user to review initial implementation**
- [ ] **TODO-11**: Integration and refinement — run the full suite (`cargo test`), 100% pass

Rules that apply throughout Phase 2:

- Verify you are on a working branch, not `master`/`main`, before writing tests.
- Acceptance tests must be **fully implemented**, as if testing a real implementation — never empty bodies or placeholders. Follow `.cursor/rules/testing-practices.mdc`.
- Tests must fail for the **right reason** (missing behavior, failing assertion) — not for setup, compile, or lint errors.
- If a new acceptance test **passes**, remove or change it — it verifies nothing new.
- When presenting tests, list every test title with a clickable link to its location (`path/to/file.rs#LX`), a one-line summary of what it validates, and confirmation that all are currently FAILING.
- Green phase: **never compromise implementation quality to force a test to pass.** If the tests do not pass with quality code, document the mismatch instead.

### Phase 3: Production Readiness (14 MANDATORY Validation Steps)

Every single one of these steps is MANDATORY. Do not skip any.

- [ ] **TODO-12**: `validate-changes` - Review all changed files for correctness
- [ ] **TODO-13**: Use the Agent tool (`refactor` subagent) to fix issues found in TODO-12
- [ ] **TODO-14**: `validate-tests` - Run full test suite, verify all tests pass
- [ ] **TODO-15**: Use the Agent tool to fix any failing tests from TODO-14
- [ ] **TODO-16**: `validate-tests` - Re-run tests after fixes
- [ ] **TODO-17**: `validate-prod-ready` - Check for TODO/FIXME annotations, debug code, hardcoded values
- [ ] **TODO-18**: Use the Agent tool to address issues found in TODO-17
- [ ] **TODO-19**: `analyze-clean-code` - Check code style, naming, structure
- [ ] **TODO-20**: Use the Agent tool to apply clean code improvements from TODO-19
- [ ] **TODO-21**: Run `cargo clippy -- -D warnings` and fix all warnings
- [ ] **TODO-22**: Run `cargo fmt` to ensure consistent formatting
- [ ] **TODO-23**: Run full test suite one final time
- [ ] **TODO-24**: Update and wrap documentation (`/update-context-docs`, then `/wrap-context-docs` if all acceptance criteria are met)
- [ ] **TODO-25**: **CHECKPOINT: Ask the user to review completed work**

#### Validation steps cannot be skipped

**NEVER** skip a validation step on assumptions such as "tests pass so validation will find nothing", "linting passed so code quality is good", "type-check passed so no issues exist", or "the implementation looks clean".

- Tests verify **behavior**; validation assesses **quality and safety**.
- Linting checks **syntax**; validation checks **production readiness**.
- Type-checking verifies **types**; validation finds **architectural issues**.
- Visual inspection misses subtle bugs and patterns that tools catch.

Each tool has a distinct purpose:

1. `/validate-changes` — production threats, testing-infrastructure risks, security vulnerabilities, unsafe code patterns.
2. `/validate-tests` — test anti-patterns (conditional logic, fallbacks, try/catch), non-deterministic behavior, test-specific branches in production code.
3. `/validate-prod-ready` — mock code in production, TODO/FIXME markers, unused code and imports, fallback logic added without consent.
4. `/analyze-clean-code` — function length and complexity, nesting depth, parameter count, magic numbers, duplication.

After each validation, record the results in the changeset's "Validation Results" section and the issues found in "Refactoring Needed". After each refactor step, verify the tests still pass and update the changeset with the fixes applied.

## Mandatory Checkpoints

There are 3 mandatory user review checkpoints. At each checkpoint:

1. Present a summary of work completed
2. Ask the user for feedback — use the `AskQuestion` tool with structured options, never a plain-text question
3. Do not proceed until the user approves; **never assume approval**

**Checkpoint 1** (TODO-07): Review acceptance tests before continuing implementation.
Present: number of tests, categories, clickable links, what each validates, and confirmation all are FAILING.
Options: "Approve - tests cover all requirements, proceed with implementation" / "Request changes - tests need modifications or additions" / "Review manually - I want to review the test files first".
Why it matters: prevents implementing against the wrong tests, lets the user add scenarios, catches missing edge cases before implementation effort is spent.

**Checkpoint 2** (TODO-10): Review initial implementation before production readiness.
Present: acceptance tests passing, milestones completed, test coverage overview, key implementation decisions; demo or walk through the functionality if applicable.
Options: "Approve - implementation looks good, proceed with validation" / "Request changes - implementation needs modifications" / "Test manually first - I want to test the feature before continuing" / "Review validation results - I need to examine the findings".
Why it matters: the user may want to test manually, their feedback may change validation priorities, and starting validation on the wrong implementation wastes the effort.

**Checkpoint 3** (TODO-25): Final review of all completed work.
Present: all acceptance criteria met, all tests passing, all validation steps completed, documentation updated and wrapped, code production-ready.
Options: "Create Pull Request - ready to submit for review" / "Manual testing - I want to test end-to-end first" / "Continue development - I have additional changes" / "Review changes - I want to review before proceeding" / "Done - no further action needed".
**Never create a PR automatically** — the user chooses when to submit.

## Plan Template

```markdown
# Development Plan: {Feature Name}

**PRD:** {path}
**Changeset:** {path}

## Overview
[What we're building and why]

## Affected Packages
- `package-name` - [what changes]

## Implementation Strategy
[High-level approach and key technical decisions]

## Phase 1: Planning
- [ ] TODO-01: Create PRD
- [ ] TODO-02: Create changeset

## Phase 2: Development (TDD)
- [ ] TODO-03 through TODO-11
- [ ] CHECKPOINT after TODO-07
- [ ] CHECKPOINT after TODO-10

## Phase 3: Production Readiness
- [ ] TODO-12 through TODO-25 (ALL MANDATORY)
- [ ] CHECKPOINT after TODO-25

## Technical Decisions
[Key architectural decisions made during planning]

## Risks and Mitigations
[Potential risks and how to address them]

## Acceptance Criteria
[What "done" looks like]
```

The plan body provides explanation and context; the TODO list carries the granular,
individually trackable steps. Both must be present.

## Best Practices

**During PRD creation**: use `AskQuestion` for requirements, keep the PRD concise, focus on *what* not *how*, list all affected feature docs, and keep acceptance criteria actionable.

**During technical planning**: clarify ambiguities, explore multiple approaches, weigh trade-offs explicitly, document the rationale, and produce a changeset with milestones, testing plan and technical decisions.

**During execution**: follow the plan in order, check off completed items, pause at every checkpoint, adapt the plan when new information emerges, track deviations and their reasons, and never compromise code quality to pass tests.

**Documentation**: keep docs current as implementation progresses, track progress with checkboxes, capture technical choices and rationale, and link feature docs to dev docs to PRs.

## Related Commands

**Skills**: `.agents/skills/plan-tdd-dev/` (core workflow), `.agents/skills/tdd/` (red-green methodology), `.agents/skills/fluent-tests/` (mandatory test style)

**Planning**: `/plan-ft` (PRD only), `/plan-ft-dev` (changeset only)

**Development**: `/ft-dev` (acceptance tests), `/red` (delegates to `test-writer`), `/green` (delegates to `tdd-implementer`), `/update-context-docs`

**Production readiness**: `/validate-changes`, `/validate-tests`, `/validate-prod-ready`, `/analyze-clean-code`, `/wrap-context-docs`, `/pr`

**Subagents**: `test-writer`, `tdd-implementer`, `refactor`

**Rules**: `.cursor/rules/prd-doc.mdc`, `.cursor/rules/feature-doc.mdc`, `.cursor/rules/dev-doc.mdc`, `.cursor/rules/changeset-doc.mdc`, `.cursor/rules/testing-practices.mdc`, `.cursor/rules/tdd.mdc`, `.cursor/rules/coding-practices.mdc`

See CLAUDE.md for project structure, build commands, and testing guidelines.
