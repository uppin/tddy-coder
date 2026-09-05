---
description: Plan and execute code migrations and refactorings that restructure code without changing features
---

# Plan TDD Migration

Plan and execute code migrations without changing features. Migrations restructure, rename, or reorganize code while preserving all existing behavior.

**Fluent-tests is the mandatory test style for this repo.** Every behavior-preservation
test written in this flow must comply with the `fluent-tests` skill at
`.agents/skills/fluent-tests/`. Before writing tests, read
`.agents/skills/fluent-tests/references/generic-guidelines.md` and the framework-specific
reference for the test type. Required: Given/When/Then, intent-revealing names, one
behavior per test, named page-object/driver helpers (no raw selectors in test bodies),
and meaningful fixture values.

## When to Use This Command

- Refactoring code without changing features
- Migrating to a new architecture pattern
- Updating dependencies or frameworks
- Improving code quality or performance
- Changing implementation without changing behavior

**Prerequisites**: the user has described the migration, context about the affected packages exists, and there is a clear understanding that **features must remain unchanged**.

## Key Difference from Feature Development

- **No new feature docs** - migrations don't add features
- **Tests pass BEFORE implementation** - behavior preservation tests are written against the current implementation and must pass immediately
- **Migration TDD pattern**: tests pass -> refactor code -> tests still pass
- **State A -> State B** describes a code/architecture transition, not a feature transition

What stays the same: plan-mode collaboration, changeset tracking, the `/green` implementation cycle, all mandatory validation steps, and the user review checkpoints.

## Step 1: Gather Migration Context

**MANDATORY** — use the `AskQuestion` tool to gather migration details; do not assume them. Ask:

- **What** code/architecture needs to change? (module restructuring, API rename, dependency change, etc.)
- **Why** is this migration needed? (tech debt, performance, maintainability, modernization)
- **Which packages/components** are affected?
- What technical constraints exist (backward compatibility, public API stability)?
- What is the target architecture/pattern?
- Are there breaking changes to internal APIs?

## Step 2: Establish Test Baseline

Run tests for the affected packages **only** — not the full suite, which is too broad and slow — to establish a baseline:

```bash
./test -p {package-name}
```

Record the results **per affected package**: package name, total counts (passing / failing / skipped), execution time, and any pre-existing warnings.

All tests must pass before proceeding. If tests fail, stop and address the failures first — unless they are known pre-existing failures this migration is not meant to fix, in which case document each one explicitly (test name, error, file and line, cause, tracking issue) so it can be verified unchanged after the migration.

Why this matters: it separates a clean baseline from new issues introduced by the migration, prevents "did the migration break this?" confusion, makes later validation accurate, and is the foundation of behavior-preservation verification.

Example baseline documentation:

```
Pre-existing test baseline (packages being migrated):
- tddy-core: 123 passing, 3 failing
- tddy-daemon: 89 passing, 0 failing

Pre-existing failures in tddy-core:
1. "should handle malformed input gracefully"
   - Error: assertion failed: index out of bounds
   - Location: packages/tddy-core/tests/parser.rs:67
   - Cause: known bug in legacy code
   - Tracking: Issue #234

Note: the migration should preserve these results (failing tests
should still fail for the same reasons afterwards).
```

## Step 3: Collaborative Planning

Use the EnterPlanMode tool to switch to plan mode for collaborative planning with the user. Discuss:
- Migration strategy (big bang vs incremental)
- Risk areas
- Rollback approach
- Behavior that must be preserved

Plan mode lets multiple refactoring approaches be explored and trade-offs weighed before any code changes, and the milestones designed together.

## Step 4: Create Migration Changeset

Create a changeset following the `/plan-ft-dev` process, in `docs/dev/1-WIP/`, with these migration-specific additions:

- **CRITICAL**: include the entire plan-mode discussion/document as the first section of the changeset, preserving why this approach was chosen over the alternatives.
- **Pre-existing Baseline**: the per-package test results from Step 2, including every pre-existing failure with its details. Present this to the user in chat as well, making clear these issues existed *before* the migration.
- **Rollback Strategy**: how to revert if issues arise.

No feature documentation is created — this is internal refactoring only.

### Behavior Preservation Strategy

Document explicitly which behaviors must be preserved:
- Public API contracts
- Error handling behavior
- Performance characteristics
- Side effects and ordering

### Behavior Preservation Tests

Write tests against the **current** implementation in fluent-tests style, targeting **external behavior, not implementation details**. These tests should **PASS immediately**:

```
1. Write test that captures current behavior (fluent-tests compliant)
2. Run test -> PASSES (confirms test is correct)
3. Perform migration refactoring
4. Run test -> should still PASS (confirms behavior preserved)
```

If a behavior preservation test fails after writing it, the test is wrong, not the code.

Verify you are on a working branch (not `master`/`main`) before writing them, and follow `.cursor/rules/testing-practices.mdc`.

**CHECKPOINT: Ask the user to review the behavior preservation tests** before any code changes. Present the number of tests, their titles, clickable links to their locations (`path/to/file.rs#LX`), what behavior each preserves, and confirmation that all are currently PASSING (locking in State A). This checkpoint prevents migrating without an adequate safety net and lets the user request additional scenarios.

## Step 5: Execute Migration (TDD Pattern)

For each migration milestone:

1. **Write behavior preservation tests** that pass against current code
2. **Refactor the code** (rename, move, restructure) via `/green` (`tdd-implementer` subagent), incrementally
3. **Run tests** - all must still pass; if a test fails, the implementation change broke behavior — fix the implementation, not the test
4. **Optionally** use `/red` to add tests for genuinely new implementation details or newly exposed internal APIs
5. **Update the changeset** (`/update-context-docs`) — check off milestones, record technical decisions and any technical debt discovered
6. **Repeat** for next milestone

Then run the full test suite (`./test`) and confirm 100% pass, with the behavior preservation tests among them.

**CHECKPOINT: Ask the user to review the completed migration** before validation begins. This checkpoint **cannot be skipped**. Present the migrated structure, the passing preservation tests, completed milestones and key technical decisions, and get explicit approval to proceed to Phase 3.

## Step 6: Production Readiness (MANDATORY)

Same mandatory validation steps as `/plan-tdd-one-shot` Phase 3. Never skip one because tests or lint pass — tests verify behavior, validation assesses quality and safety.

- [ ] `validate-changes` - Review all changed files for correctness
- [ ] Use the Agent tool (`refactor` subagent) to fix issues found
- [ ] `validate-tests` - Run full test suite, verify all tests pass
- [ ] Use the Agent tool to fix any failing tests
- [ ] `validate-tests` - Re-run tests after fixes
- [ ] `validate-prod-ready` - Check for TODO/FIXME annotations, debug code, hardcoded values
- [ ] Use the Agent tool to address issues found
- [ ] `analyze-clean-code` - Check code style, naming, structure
- [ ] Use the Agent tool to apply clean code improvements
- [ ] `validate-changes` - Final re-run, confirming refactoring introduced no new issues
- [ ] Run `cargo clippy -- -D warnings` and fix all warnings
- [ ] Run `cargo fmt` to ensure consistent formatting
- [ ] Run full test suite one final time
- [ ] Update documentation (see `/update-context-docs`), then wrap it:
  - Apply the changeset to affected package READMEs and dev docs so they reflect State B
  - Archive or delete the WIP changeset per `/wrap-context-docs`
  - Create one new entry file in the package `changesets/` — `YYYY-MM-DD-<slug>.md` (see [changelog-merge-hygiene.md](../../docs/dev/guides/changelog-merge-hygiene.md))
- [ ] **CHECKPOINT: Ask the user to review completed migration**
- [ ] Create the PR with `/pr` — summarize what was migrated, link to the archived changeset, **highlight that features remain unchanged**, and list the validation results

Record each validation's outcome in the changeset "Validation Results" section and the issues found under "Refactoring Needed".

## Migration Changeset Template

```markdown
# Migration Changeset: {Migration Name}

**Created:** YYYY-MM-DD
**Status:** Created
**Reason:** {Why this migration is needed}

## Affected Packages
- [ ] `package-name` - Brief description of changes

## Pre-existing Baseline
Per-package test results captured before any changes, including any
pre-existing failures with name, error, location, cause and tracking issue.

## Behavior Preservation

### Must Preserve
- Behavior 1
- Behavior 2

### Preservation Tests
- [ ] Test for behavior 1
- [ ] Test for behavior 2

## State A (Current)
Current structure...

## State B (Target)
Target structure...

## Migration Steps
1. Step 1
2. Step 2

## Technical Decisions
Key choices made and their rationale.

## Rollback Plan
How to revert if needed.
```

## Best Practices

- **Test first**: write behavior preservation tests before changing any code
- **Keep tests passing** throughout the migration; if they fail, fix the implementation
- **Migrate incrementally**: small, verifiable steps rather than one big bang where possible
- **Focus on external behavior**: test what, not how
- **Add integration tests** to verify end-to-end behavior is preserved
- **Manual testing**: ask the user to exercise critical paths after the migration
- **Quality first**: use the migration as an opportunity to improve code quality

## Related

**Skills**: `.agents/skills/plan-tdd-dev/`, `.agents/skills/tdd/`, `.agents/skills/fluent-tests/`
**Commands**: `/plan-ft-dev`, `/ft-dev`, `/red`, `/green`, `/update-context-docs`, `/validate-changes`, `/validate-tests`, `/validate-prod-ready`, `/analyze-clean-code`, `/wrap-context-docs`, `/pr`
**Subagents**: `tdd-implementer`, `refactor`
**Rules**: `.cursor/rules/dev-doc.mdc`, `.cursor/rules/changeset-doc.mdc`, `.cursor/rules/testing-practices.mdc`, `.cursor/rules/tdd.mdc`, `.cursor/rules/coding-practices.mdc`

See CLAUDE.md for project structure, build commands, and testing guidelines.
