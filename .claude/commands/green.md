---
description: TDD Green Phase - Delegate implementation to tdd-implementer subagent. Goal is quality implementation, not forcing tests to pass.
---
## Green Phase - Delegate Implementation

This command delegates to the tdd-implementer subagent to implement production-quality code. The goal is correct, maintainable implementation - not making tests pass at any cost.

**CRITICAL**: Code quality > Test passage. Never compromise code to force tests to pass.

**For complete TDD workflow, see `@tdd` rule.**
**For testing standards, see `@testing-practices` rule.**

## Prerequisites

1. **Failing tests must exist** - Use `/red` first to write comprehensive failing tests
2. **Tests failing for right reasons** - Missing implementation, not test bugs
3. **Don't start** if failing test suite not ready

## Workflow

### 1. Review Failing Tests

Understand what needs to be implemented:
- Which tests are failing?
- What functionality do they require?
- What API do tests expect?

Run tests to see current state:
```bash
cargo test -p package-name
# Or: cd packages/package-name && cargo test
```

### 2. Delegate to tdd-implementer Subagent

Delegate implementation directly:

**Invoke**: `tdd-implementer` subagent

**Provide to subagent**:
- The failing tests location and names
- Any context about the feature being implemented
- Expected API from tests (if known)

**The tdd-implementer will**:
- Analyze failing tests to understand requirements
- Implement production-quality code
- Focus on correct, quality implementation
- Verify each step with test runs
- Use minimal but proper implementation approach
- Keep tests unchanged (unless absolutely necessary)
- Report completion (tests may or may not pass - quality is priority)

**IMPORTANT**: If implementation is correct but tests still fail:
- That's acceptable - document why tests fail
- DO NOT compromise code quality to force passage
- May indicate test issues or incomplete understanding
- Better to have quality code with failing tests than bad code with passing tests

## Goals

1. ✅ Review and understand failing tests
2. ✅ Delegate to tdd-implementer subagent
3. ✅ Minimal production-quality implementation
4. ✅ Focus on functionality over form
5. ✅ Real code (not fake/hardcoded)
6. ✅ Quality implementation (tests passing is goal, not requirement)

## What NOT to Do

- ❌ Don't refactor existing code (unless necessary)
- ❌ Don't add extra features not covered by tests
- ❌ Don't focus on optimization or performance
- ❌ Don't make significant test structure changes
- ❌ **Don't add workarounds to make tests pass**
- ❌ **Don't use environment detection to force passage**
- ❌ **Don't ignore errors indicating real problems**
- ❌ **Don't hardcode values just to pass tests**

**CRITICAL**: If tests fail, fix implementation, not the test.

## Implementation Quality Principles

**CRITICAL**: Never compromise these for test passage:

### ✅ Acceptable - Quality Production Code

```rust
// ✅ Real production implementation
export function processData(input: Data): Result {
  if (!input || !input.value) {
    throw new Error('Invalid input');
  }

  return {
    processed: transformValue(input.value),
    timestamp: Date.now()
  };
}
```

**If tests fail with this quality code:**
- Document why they fail
- Investigate test vs. implementation mismatch
- DO NOT add workarounds

### ❌ NOT Acceptable - Compromised Code

```rust
// ❌ Hardcoded for tests
export function processData(input: Data): Result {
  if (input.value === 'test-value') {
    return { processed: 'expected-output', timestamp: 123 };
  }
  return { processed: '', timestamp: 0 };
}

// ❌ Test-specific branch
if (process.env.NODE_ENV === 'test') {
  return mockData;
}

// ❌ Workaround to force passage
try {
  return realImplementation();
} catch {
  return fakeResult; // Just to make test pass
}
```

**These are NEVER acceptable** - even if tests fail without them.

## Keep It Clean

**CRITICAL**: Never put "red phase" or "green phase" in:
- Code comments
- Test descriptions
- Production code

Remove any such comments if you see them.

## Output Format

```markdown
## 🟢 Green Phase Complete - Implementation Delegated

### Delegation to tdd-implementer ✅

**Delegated to**: `tdd-implementer` subagent

**Context provided to subagent:**
- Failing tests location and names
- Feature context and requirements
- Expected API from tests

**Result**: Quality implementation complete
- ✅ Production-quality code implemented
- 📊 Test status: [X passing, Y failing]
- 📝 If tests fail: [Reason and next steps]

### Implementation Created (by tdd-implementer)
- `src/feature.rs` - Main implementation
- `src/helpers.rs` - Supporting functions

### Implementation Summary
**Main functionality:**
- Implemented core feature logic (following plan)
- Added input validation (as planned)
- Implemented error handling (plan strategy)

**Code quality notes:**
- Minimal implementation (plan-guided)
- Production-quality code (not fake)
- Followed planned architecture
- TODO markers for future improvements

### Test Results
```bash
cargo test -p package-name
```

**Output:**
```
[✅ All tests passing | ⚠️ Some tests still failing]
- X tests passed
- Y tests failed
```

**If tests fail:**
- Analyze why: Implementation issue? Test issue? Misunderstanding?
- Document failure reasons
- DO NOT compromise code to force passage
- Consider if tests need adjustment or more work needed

### Test Modifications (by tdd-implementer)
[None | Minimal changes:]
- Adjusted import path in test setup
- [Explain why change was necessary]

### Code Quality ✅
- ✅ Real production code (not fake/hardcoded)
- ✅ Functional implementation
- ✅ No test-specific branches
- ✅ No workarounds
- ✅ Followed plan architecture
- ✅ Code quality NOT compromised for tests
- ⏭️ Refactoring may be needed (normal for green phase)

### FIXME/TODO Markers Added
- [ ] TODO: Add proper types (file:line)
- [ ] FIXME: Extract magic value to constant (file:line)
- [ ] If tests still failing: [Reason and investigation needed]

**Ready for Next Step**: ✅
- If all tests passing: Use `refactor` subagent to improve code
- If some tests failing: Investigate root cause, don't force passage
```

## Update Documentation

**If changeset/dev doc exists**: Update with implementation status.

## Success Criteria

Complete workflow successful when:

- ✅ Reviewed and understood failing tests
- ✅ Delegated to tdd-implementer subagent
- ✅ Quality implementation complete (production-quality, not fake)
- ✅ Minimal implementation (no over-engineering)
- ✅ Tests remain largely unchanged
- ✅ No workarounds added to tests
- ✅ Code quality NOT compromised
- 📊 Tests passing is goal, not hard requirement
- ✅ Ready for next step (refactor or investigation)

## Best Practices

✅ **Do:**
- Review failing tests before delegating
- Delegate to tdd-implementer subagent
- Provide test locations and names
- Provide any relevant context
- Wait for subagent completion
- Accept that tests may not all pass if code is correct
- Prioritize code quality over test passage
- Verify implementation is quality code

❌ **Don't:**
- Don't implement yourself (delegate it)
- Don't skip the delegation step
- Don't micromanage the subagent
- Don't demand tests pass at any cost
- Don't compromise code quality for test passage
- Don't add workarounds to force passage

## Related

**Rules**: `@tdd`, `@testing-practices`
**Subagents**: `tdd-implementer` (for implementation), `refactor` (next phase)
**Commands**: `/red` (previous phase), `/green-attack` (exploratory alternative)

## Workflow Summary

```
/green command invoked
       ↓
1. Review failing tests
       ↓
2. Delegate to tdd-implementer subagent
       ↓
3. tdd-implementer analyzes tests and implements
       ↓
Quality implementation complete ✅
(Tests may or may not pass - quality is priority)
```

**Key Benefits**:
- Simple, direct delegation to implementation specialist
- Prioritizes code quality over forcing tests to pass
- Honest about test status - no workarounds to hide failures
- tdd-implementer determines approach from tests
