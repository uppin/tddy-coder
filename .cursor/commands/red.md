---
description: TDD Red Phase - Write failing tests before implementation
---
## Red Phase - Write Failing Tests First

This command guides creation of comprehensive failing tests as the first step in TDD.

**For complete TDD workflow, see `@tdd` rule.**
**For testing standards, see `@testing-practices` rule.**

## Prerequisites

- This step is about writing tests FIRST, before any implementation exists
- Tests will initially fail (no implementation exists yet)
- Tests clearly define expected behavior and API design

## Workflow

### 1. Write Comprehensive Failing Tests

Capture all desired behavior and edge cases:
- Main functionality tests
- Edge case tests
- Error scenario tests
- API boundary tests

### 2. Implement Tests (Not Skeletons)

**CRITICAL**: Tests should be fully implemented, not placeholders:
- Complete test setup
- Full test execution
- Proper assertions
- Clear test names

Ideal test needs no changes when going green.

### 3. Define Public API Through Tests

Tests show how feature should be called:
- Function signatures
- Parameter types
- Return values
- Error handling

### 4. Verify Tests Fail Correctly

```bash
# From repo root (workspace)
cargo test -p package-name

# Or from package directory
cd packages/package-name && cargo test
```

Tests should fail for right reasons:
- ✅ Missing implementation
- ❌ NOT test bugs

### 5. Present Failing Test Results

Show User that tests are failing correctly and ready for green phase.

## Goals

1. ✅ Write comprehensive failing tests
2. ✅ Tests fully implemented (no empty/skeleton tests)
3. ✅ Define public API through test usage
4. ✅ Establish clear test structure
5. ✅ Verify tests fail for right reasons
6. ✅ Code compiles (imports work)

## What NOT to Do

- ❌ Don't write implementation code yet
- ❌ Don't make tests pass with minimal stubs
- ❌ Don't skip edge cases or error scenarios
- ❌ Don't write vague or broad tests
- ❌ **Don't write tests with conditional logic**
- ❌ **Don't add try/catch blocks or fallbacks**
- ❌ **Don't write placeholder tests**

**Follow `@testing-practices`** - Write reliable, deterministic tests.

## Keep It Clean

**CRITICAL**: Never put "red phase" or "green phase" in:
- Code comments
- Test descriptions
- Production code

Keep TDD phases in chat context only.

## Output Format

```markdown
## 🔴 Red Phase Complete - Failing Tests Ready

### Tests Created
- `test-file.rs` - X tests

### Test Coverage
**Main Functionality:**
- [x] Test: should do main thing
- [x] Test: should handle input X

**Edge Cases:**
- [x] Test: should handle empty input
- [x] Test: should handle None/empty values

**Error Scenarios:**
- [x] Test: should throw on invalid input
- [x] Test: should fail gracefully on error

### API Definition (via tests)
```rust
// How feature will be called (defined by tests)
pub fn my_feature(input: InputType) -> OutputType
```

### Test Results
```bash
cargo test -p package-name
# Or filter by test name: cargo test -p package-name test_name_substring
```

**Output:**
```
❌ All tests failing (expected)
- Missing implementation for myFeature
- No error handling implemented
```

### Readiness Check
- ✅ All tests fully implemented
- ✅ Code compiles
- ✅ Imports work
- ✅ Tests fail for right reasons (missing implementation)
- ✅ No test bugs detected

**Ready for Green Phase**: ✅

Next: Use `@green` to write minimal implementation making tests pass
```

## Update Documentation

**If changeset exists**: Add to "Refactoring Needed":
```markdown
### From @red (TDD Red Phase)
- [ ] Test helper function needed for repeated setup
- [ ] Test data builder would improve readability
- [ ] Mock factory needed for complex dependencies
```

Focus on test structure issues, not implementation (no production code exists yet).

## Success Criteria

Red phase complete when:
- ✅ All tests fully implemented (not skeletons)
- ✅ Tests comprehensively cover functionality
- ✅ API design clear from test usage
- ✅ Code compiles
- ✅ Tests fail for right reasons
- ✅ No test bugs
- ✅ Ready for green phase

## Best Practices

✅ **Do:**
- Write comprehensive tests covering all scenarios
- Fully implement tests (not placeholders)
- Define clear API through tests
- Follow `@testing-practices` standards
- Verify tests fail correctly

❌ **Don't:**
- Don't write implementation yet
- Don't create skeleton tests
- Don't add conditional test logic
- Don't skip edge cases
- Don't put TDD phase info in code

## Related

**Rules**: `@tdd`, `@testing-practices`
**Subagents**: `refactor` (after green)
**Commands**: `/green` (next phase)
