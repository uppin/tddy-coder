---
description: Run and validate acceptance criteria tests
---
## Test Acceptance Criteria

This command validates acceptance criteria defined in development documentation by running acceptance tests.

## Prerequisites

Development documentation with acceptance tests:
- Changeset: `docs/dev/1-WIP/YYYY-MM-DD-changeset.md`
- Feature/PRD docs with acceptance criteria

## Workflow

### 1. Identify Acceptance Tests

Read changeset "Acceptance Tests" section:
```markdown
### package-name (crate)
- [ ] **E2E/Integration/Unit**: Test description (test-file.rs)
```

### 2. Run Acceptance Tests

For each crate with acceptance tests:
```bash
# From repo root (workspace)
cargo test -p package-name

# Or run all workspace tests
cargo test
```

### 3. Analyze Results

For each acceptance test:
- ✅ **Passing**: Check off in changeset
- ❌ **Failing**: Identify root cause
- ⏭️ **Skipped**: Identify why skipped

### 4. Update Changeset Status

Mark passing tests:
```markdown
- [x] **E2E**: Test description (test-file.rs) ✅
```

Track failing tests:
```markdown
- [ ] **Integration**: Test description (test-file.rs) ❌
  - Failure reason: [description]
  - Next action: [what needs fixing]
```

### 5. Verify Acceptance Criteria

Cross-reference with feature/PRD document:
- Are all acceptance criteria covered by tests?
- Are all tests passing?
- Any gaps in coverage?

## Output Format

```markdown
## Acceptance Test Results

**Overall Status**: ✅ All Passing | ⚠️ Some Failing | ❌ Multiple Failures

### package-1 (crate)
- [x] **E2E**: Complete workflow test ✅
- [x] **Integration**: Service integration test ✅
- [ ] **Unit**: Edge case handling ❌
  - Reason: Edge case not properly handled
  - Action: Fix edge case logic in src/handler.rs

### package-2 (crate)
- [x] **Integration**: Database operations ✅

**Summary**:
- Total tests: 4
- Passing: 3
- Failing: 1
- Coverage: 75% acceptance criteria met

**Next actions**:
1. Fix edge case handling in package-1
2. Re-run failing test
3. Verify all criteria met
```

## Update Documentation

**If changeset exists**: Update "Validation Results" section:
```markdown
### Test Acceptance (@test-acceptance)
**Last Run**: YYYY-MM-DD
**Status**: ✅ Passed | ⚠️ Partial | ❌ Failed
**Results**: X/Y tests passing
```

**If feature/dev doc exists**: Update acceptance criteria status

## When All Tests Pass

Print:
```
✅ All acceptance criteria validated!

Changeset: docs/dev/1-WIP/YYYY-MM-DD-changeset.md
Test results: X/X passing

Feature is ready for:
- `/validate-changes` command - Code quality assessment
- `/validate-tests` command - Test quality validation
- `/validate-prod-ready` command - Production readiness check
```

## Best Practices

✅ **Do:**
- Run all acceptance tests, not just some
- Update changeset with actual results
- Investigate failing tests thoroughly
- Cross-reference with acceptance criteria

❌ **Don't:**
- Don't skip failing tests without fixing
- Don't mark tests as passing if they're flaky
- Don't ignore test warnings
- Don't proceed to PR with failing tests

## Related

**Rules**: `@testing-practices`, `@dev-doc`, `@feature-doc`, `@amendment-doc`, `@prd-doc`
**Related Commands**: `/validate-tests`, `/validate-prod-ready`
**Commands**: `/ft-dev`
