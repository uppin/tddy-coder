---
description: Validates test quality by detecting anti-patterns, checking description-body alignment, and ensuring deterministic behavior. Use proactively when reviewing test changes.
---

## Validate Test Quality

This command validates test quality to ensure tests are reliable, maintainable, and provide production confidence by detecting anti-patterns and checking for deterministic behavior.

## Context Documents

**Expect in context**:
- Changeset document (`docs/dev/1-WIP/YYYY-MM-DD-*.md`) - tracks implementation progress
- PRD document (`docs/ft/*/1-WIP/YYYY-MM-DD-*.md`) - tracks requirement changes

**Use these documents to**:
- Understand expected test coverage from "Acceptance Tests" section
- Validate tests align with planned testing strategy
- Update "Validation Results" section in changeset

## When Invoked

1. **Check for context documents**:
   - Read changeset if provided
   - Review "Testing Plan" and "Acceptance Tests" sections

2. **Identify test files in changes**:
   ```bash
   git diff --name-only HEAD~1 | grep -E '\.(test|spec)\.(ts|tsx|js|jsx)$'
   ```

2. **Read each test file** and analyze for issues.

## Anti-Patterns to Detect

### Always-Passing Tests
- Tests with no assertions
- Tests that catch and swallow errors
- Tests with `expect(true).toBe(true)`

### Skipped/Focused Tests
- `.skip` markers that shouldn't be committed
- `.only` markers that limit test runs
- Commented-out test cases

### Flaky Tests
- Timing-dependent assertions (`setTimeout`, fixed delays)
- Order-dependent tests
- Tests relying on external state

### Description-Body Misalignment
- Test name doesn't match what it actually tests
- Misleading `describe` blocks
- Generic names like "should work"

### Hardcoded Values
- Values that should be dynamic
- Dates/times that will break
- Environment-specific paths

### Missing Edge Cases
- Only happy path tested
- No error handling tests
- No boundary condition tests

## Output Format

```markdown
## Test Validation Report

### Tests Analyzed
- test1.rs (X test cases)
- test2.rs (Y test cases)

### Quality Summary
| Metric | Count |
|--------|-------|
| Total test cases | X |
| Anti-patterns found | Y |
| Skipped tests | Z |

### Issues Found

#### 🔴 Critical
1. **[file:line]** Always-passing test
   ```rust
   // problematic code
   ```
   - Fix: Add meaningful assertions

#### ⚠️ Warnings
1. **[file:line]** Description-body mismatch
   - Test says: "should validate input"
   - Test does: Only checks output format

#### ℹ️ Suggestions
1. **[file:line]** Consider adding edge case for null input

### Run Tests
```bash
cargo test
```
```

## Update Changeset

If changeset document exists, update the **Validation Results** section:

```markdown
### Test Validation (@validate-tests)

**Last Run**: YYYY-MM-DD
**Status**: ✅ Passed | ⚠️ Warnings | ❌ Issues Found

**Summary**:
- Tests analyzed: X
- Anti-patterns found: Y
- Description-body mismatches: Z

**Issues Found**:
- [List critical issues with file:line]

**Added to Refactoring Needed**:
- [List items added to refactoring section]
```

Also add issues to **Refactoring Needed** section under `### From @validate-tests`.

## Reference

- **Command**: `/validate-tests`
- **Rules**: `@testing-practices`, `@changeset-doc`
