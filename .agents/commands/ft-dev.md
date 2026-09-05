---
description: Execute feature development from A to Z using TDD, driven by failing acceptance tests
---

# Feature Development: TDD from A to Z

You are developing a feature end-to-end using TDD. A development plan (changeset) should already exist. If it does not, ask the user to provide one or describe the feature.

**Fluent-tests is the mandatory test style for this repo.** Every test written in this
flow (acceptance, unit, integration) must comply with the `fluent-tests` skill at
`.agents/skills/fluent-tests/`. Before writing tests, read
`.agents/skills/fluent-tests/references/generic-guidelines.md` and the framework-specific
reference for the test type. Required: Given/When/Then, intent-revealing names, one
behavior per test, named page-object/driver helpers (no raw selectors in test bodies),
meaningful fixtures, and in-memory backends instead of `cy.intercept` for Cypress
component tests.

For TDD methodology see `.cursor/rules/tdd.mdc`; for test quality standards see
`.cursor/rules/testing-practices.mdc`.

## Prerequisites

- A changeset or development plan describing the feature milestones — typically `docs/dev/1-WIP/CS-YYYY-MM-DD-feature-name.md` from `/plan-ft-dev`, plus the PRD from `/plan-ft`
- A working branch aligned with the feature (if on main/master, create a feature branch first)

## Process

### 1. Review the Plan

- Read the changeset / plan documents (check `docs/dev/1-WIP/` for active changesets): affected packages, implementation milestones, testing plan, acceptance test list.
- Verify the current branch is appropriate for this feature (not `master`/`main`).
- List the milestones and their status.

### 2. Create Failing Acceptance Tests

For the current milestone, write acceptance-level tests that define "done":

- Tests must be **fully implemented** -- real assertions, real setup, real expected values. No empty test bodies, no `todo!()`, no placeholders.
- Tests must follow the **fluent-tests** mandatory style (Given/When/Then, named page-object/driver helpers, one behavior per test, meaningful fixtures).
- Tests should cover the milestone's key behaviors end-to-end.
- Run `cargo test` to confirm they fail, and that they fail for the **right reason** (missing behavior, failing assertion) — not a setup or compile error.
- If a new test **passes**, remove or change it — it is not verifying anything new.

Present the test list, grouped by behavior, with a clickable link to each test's exact line:

| Test | File | Status |
|------|------|--------|
| `test_name` | `packages/.../tests/file.rs#L31` | FAILING (expected) |

Example:

```markdown
## Acceptance Tests Created

Created 14 acceptance tests in `packages/my-pkg/tests/self_install.rs`:

**First-time Installation**:
- [should create installation directory on first run](packages/my-pkg/tests/self_install.rs#L31)
- [should copy target/release/ to installation](packages/my-pkg/tests/self_install.rs#L40)

**Update Existing Installation**:
- [should skip installation if already at current version](packages/my-pkg/tests/self_install.rs#L109)

All tests currently FAILING (as expected in the Red phase).
```

### 3. TDD Red-Green Cycle

For each milestone, iterate:

1. **Red**: Write or review failing unit/integration tests for the next piece of behavior. Follow the same rules as `/red` -- fully implemented tests, no skeletons, no conditional logic.
2. **Green**: Write minimal production code to make tests pass. Follow the same rules as `/green` -- real implementation, no fakes, no test-specific branches.
3. **Refactor**: Improve code quality with the tests still green (delegate to the `refactor` subagent when the cleanup is substantial).
4. **Verify**: Run `cargo test` (package-scoped) after each green step.

Use the Agent tool to delegate implementation work when appropriate, providing clear context about the failing tests and expected behavior.

Maintain test quality throughout: no conditional logic in tests, no try/catch workarounds, no fallback assertions, 100% deterministic behavior.

### 4. Update Progress

After completing a milestone, update the changeset:
- [ ] Check off completed milestones
- [ ] Check off passing acceptance tests
- [ ] Document technical decisions made
- [ ] Track technical debt discovered

Then run the full test suite (`cargo test`) and `cargo clippy -- -D warnings`.

### 5. Repeat

Move to the next milestone and repeat from step 3.

## When Complete

The feature is complete when all milestones are checked off, all acceptance tests pass, all other tests pass, no tests are skipped, and the changeset status is `Complete`.

## Output Format

### Completion Status

| Milestone | Status | Tests |
|-----------|--------|-------|
| Milestone 1 | DONE / IN PROGRESS / TODO | X passing |

### Test Results

```
<full cargo test output>
```

### Next Steps

- What remains to be done
- Any blockers or decisions needed from the user
- When all milestones are complete, suggest the production-readiness sequence:
  1. `/validate-changes` — assess code quality
  2. `/validate-tests` — check test quality
  3. `/validate-prod-ready` — production readiness
  4. `/wrap-context-docs` — update dev docs
  5. `/pr` — create the pull request

## Best Practices

**Do:** follow the red-green-refactor cycle; update the changeset as work progresses; write deterministic tests; check off milestones when complete; document decisions in the changeset.

**Don't:** skip tests to make progress faster; add fallbacks to make tests pass; leave milestones unchecked; forget to update the changeset; consider the feature complete with failing tests.

## Related

**Rules:** `.cursor/rules/tdd.mdc`, `.cursor/rules/testing-practices.mdc`
**Commands:** `/plan-ft-dev` (previous), `/test-acceptance`, `/red`, `/green`, `/validate-changes`, `/validate-tests`, `/validate-prod-ready`, `/wrap-context-docs`, `/pr`
**Subagents:** `tdd-implementer`, `refactor`
