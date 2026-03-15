# Plan TDD Migration

Plan and execute code migrations without changing features. Migrations restructure, rename, or reorganize code while preserving all existing behavior.

## Key Difference from Feature Development

- **No new feature docs** - migrations don't add features
- **Tests pass BEFORE implementation** - behavior preservation tests are written against the current implementation and must pass immediately
- **Migration TDD pattern**: tests pass -> refactor code -> tests still pass

## Step 1: Gather Migration Context

Ask the user:
- **What** is being migrated? (module restructuring, API rename, dependency change, etc.)
- **Why** is this migration needed? (tech debt, performance, maintainability, etc.)
- **Which packages** are affected?

## Step 2: Establish Test Baseline

Run tests for all affected packages to establish a passing baseline:

```bash
./test -p {package-name}
```

Record the test results. All tests must pass before proceeding. If tests fail, stop and address failures first.

## Step 3: Collaborative Planning

Use the EnterPlanMode tool to switch to plan mode for collaborative planning with the user. Discuss:
- Migration strategy (big bang vs incremental)
- Risk areas
- Rollback approach
- Behavior that must be preserved

## Step 4: Create Migration Changeset

Create a changeset following the `/plan-ft-dev` process, with these migration-specific additions:

### Behavior Preservation Strategy

Document explicitly which behaviors must be preserved:
- Public API contracts
- Error handling behavior
- Performance characteristics
- Side effects and ordering

### Behavior Preservation Tests

Write tests against the **current** implementation. These tests should **PASS immediately**:

```
1. Write test that captures current behavior
2. Run test -> PASSES (confirms test is correct)
3. Perform migration refactoring
4. Run test -> should still PASS (confirms behavior preserved)
```

If a behavior preservation test fails after writing it, the test is wrong, not the code.

## Step 5: Execute Migration (TDD Pattern)

For each migration milestone:

1. **Write behavior preservation tests** that pass against current code
2. **Refactor the code** (rename, move, restructure)
3. **Run tests** - all must still pass
4. **Repeat** for next milestone

## Step 6: Production Readiness (MANDATORY)

Same mandatory validation steps as `/plan-tdd-one-shot` Phase 3:

- [ ] `validate-changes` - Review all changed files for correctness
- [ ] Use the Agent tool to refactor issues found
- [ ] `validate-tests` - Run full test suite, verify all tests pass
- [ ] Use the Agent tool to fix any failing tests
- [ ] `validate-tests` - Re-run tests after fixes
- [ ] `validate-prod-ready` - Check for TODO/FIXME annotations, debug code, hardcoded values
- [ ] Use the Agent tool to address issues found
- [ ] `analyze-clean-code` - Check code style, naming, structure
- [ ] Use the Agent tool to apply clean code improvements
- [ ] Run `cargo clippy -- -D warnings` and fix all warnings
- [ ] Run `cargo fmt` to ensure consistent formatting
- [ ] Run full test suite one final time
- [ ] Update documentation (see `/update-context-docs`)
- [ ] **CHECKPOINT: Ask the user to review completed migration**

## Migration Changeset Template

```markdown
# Migration Changeset: {Migration Name}

**Created:** YYYY-MM-DD
**Status:** Created
**Reason:** {Why this migration is needed}

## Affected Packages
- [ ] `package-name` - Brief description of changes

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

## Rollback Plan
How to revert if needed.
```

See CLAUDE.md for project structure, build commands, and testing guidelines.
