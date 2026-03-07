---
description: Execute feature development from A to Z using TDD
---
## Feature Development

This command executes complete feature development using failing acceptance tests as guides.

**For TDD methodology, see `@tdd` rule.**

## Prerequisites

Development plan should exist from `/plan-ft-dev`:
- Changeset document: `docs/dev/1-WIP/YYYY-MM-DD-changeset.md`
- Feature/PRD document from `/plan-ft`

## Workflow

### 1. Review Development Context

Read the changeset document:
- Affected packages
- Implementation milestones
- Testing plan
- Acceptance tests list

### 2. Verify Branch Alignment

Ensure working on correct branch (not master/main)

### 3. Create Failing Acceptance Tests

For each acceptance test in changeset:
1. Write test that captures desired behavior
2. **CRITICAL** The test should not be empty. It should be implemented as if it was already testing real implementation.
3. Test is expected to fail due to missing functionality (GOOD!)
4. Verify test fails for the right reason
5. If tests are passing, they should be removed as they are not verifying anything new.
6. Follow `@testing-practices` standards

**CRITICAL**: After creating tests, present to user:
- List of all test titles created
- Clickable links to test case locations (line in a file)
- Brief summary of what each test validates

**Example output**:
```markdown
## Acceptance Tests Created

Created 14 acceptance tests in `packages/my-pkg/tests/self_install.rs`:

**First-time Installation**:
- [should create installation directory on first run](packages/my-pkg/tests/self_install.rs#L31)
- [should copy target/release/ to installation](packages/my-pkg/tests/self_install.rs#L40)
- [should copy target/release/ to installation](packages/my-pkg/tests/self_install.rs#L50)

**Update Existing Installation**:
- [should skip installation if already at current version](packages/my-pkg/tests/self_install.rs#L109)
- [should update installation if newer version available](packages/my-pkg/tests/self_install.rs#L120)

All tests currently FAILING (as expected in Red phase).
```

### 4. Implement Using TDD Red-Green Cycle

For each milestone:
1. **Red**: Write failing test (`@red`)
2. **Green**: Make test pass with minimal code (`@green`)
3. **Refactor**: Improve code quality (`@refactor`)
4. Check off milestone in changeset when complete

### 5. Update Changeset Progress

As work progresses:
- [ ] Check off completed milestones
- [ ] Check off passing acceptance tests
- [ ] Document technical decisions
- [ ] Track technical debt discovered
- [ ] Update status from 🚧 to ✅ when complete

### 6. Run All Tests

```bash
cargo test
```

Ensure all tests pass before considering feature complete.

## Implementation Strategy

**Follow TDD cycle:**
```
1. Write failing test (Red)
2. Make test pass (Green)
3. Improve code (Refactor)
4. Repeat for next test
```

**Track progress in changeset:**
- Milestone completion
- Acceptance test status
- Technical debt items
- Design decisions

**Maintain test quality:**
- No conditional logic in tests
- No try/catch workarounds
- No fallback assertions
- 100% deterministic behavior

## When Complete

Feature is complete when:
- ✅ All milestones checked off
- ✅ All acceptance tests passing
- ✅ All regular tests passing
- ✅ No skipped tests
- ✅ Changeset status = Complete

## Output

Update changeset status:
```markdown
**Status**: ✅ Complete
```

Print this line:
```
Feature development complete!

Changeset: docs/dev/1-WIP/YYYY-MM-DD-changeset.md
Status: ✅ Complete

Next steps:
1. Run `/validate-changes` command to assess code quality
2. Run `/validate-tests` command to check test quality
3. Use `/validate-prod-ready` command for production readiness
4. Use `/wrap-context-docs` command to update dev docs
5. Use /pr to create pull request
```

## Best Practices

✅ **Do:**
- Follow TDD red-green-refactor cycle
- Update changeset as work progresses
- Write deterministic tests
- Check off milestones when complete
- Document decisions in changeset

❌ **Don't:**
- Don't skip tests to make progress faster
- Don't add fallbacks to make tests pass
- Don't leave milestones unchecked
- Don't forget to update changeset
- Don't consider complete with failing tests

## Related

**Rules**: `@tdd`, `@testing-practices`
**Related**: Subagent `refactor`, Commands `/validate-changes`, `/validate-tests`, `/validate-prod-ready`, `/wrap-context-docs`
**Commands**: `/plan-ft-dev` (previous), `/test-acceptance`, `/red`, `/green`, `/pr`
