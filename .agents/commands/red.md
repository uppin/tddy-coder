---
description: TDD Red Phase - write fully implemented failing tests that define the expected behavior before implementation
---

# TDD Red Phase: Write Failing Tests

You are executing the RED phase of TDD. Your job is to write failing tests that define the expected behavior before any implementation exists.

**Fluent-tests is the mandatory test style for this repo.** Before writing any tests, read
`.agents/skills/fluent-tests/references/generic-guidelines.md` and the framework-specific
reference matching the test type you're writing. Every test must comply with the fluent-tests
standard: Given/When/Then structure, named page-object helpers (no raw selectors in test bodies),
one behavior per test, meaningful fixture values, and in-memory backends instead of `cy.intercept`
for Cypress component tests.

**For the complete TDD workflow, see `.cursor/rules/tdd.mdc`.**
**For testing standards, see `.cursor/rules/testing-practices.mdc`.**

## Rules

1. **Write fully implemented tests** -- every test must have real assertions, real setup, and real expected values. No skeleton tests, no `todo!()`, no `unimplemented!()`, no empty test bodies. The ideal test needs no changes when going green.
2. **Define the public API through tests** -- the tests should express how the module/function/struct will be used. Import paths, method signatures, parameter types, return values, and error handling in tests become the contract.
3. **Tests must fail for the right reasons** -- a test should fail because the production code doesn't exist yet or doesn't implement the behavior, NOT because of syntax errors, missing imports that you could add, or broken test infrastructure. The code must still compile and the imports must work.
4. **No conditional logic in tests** -- no `if/else`, no match arms that skip assertions. Tests must be linear: setup, act, assert.
5. **No try/catch workarounds** -- do not wrap assertions in error-swallowing blocks. If a test panics, that is the signal.
6. **One behavior per test** -- each `#[test]` function should verify exactly one aspect of the behavior.
7. **Follow fluent-tests style** -- see mandatory reading above. Violations are treated as test bugs.
8. **Don't write implementation code yet** -- no production code, no minimal stubs to make tests pass.

## Process

1. Read `.agents/skills/fluent-tests/references/generic-guidelines.md` and the relevant framework reference.
2. Read the current task or milestone requirements (from the changeset, TODO, or user description).
3. Identify the behaviors that need to be tested. Cover all four categories -- don't stop at the happy path:
   - Main functionality
   - Edge cases (empty input, `None`/empty values, boundaries)
   - Error scenarios
   - API boundaries
4. Write the test file(s) with all tests fully implemented in fluent-tests style.
5. Run the tests to confirm every new test fails:
   ```bash
   # From repo root (workspace)
   cargo test -p package-name

   # Filter by test name
   cargo test -p package-name test_name_substring

   # Or from the package directory
   cd packages/package-name && cargo test
   ```
6. Examine each failure -- verify it fails because the production code is missing or incomplete, not because the test itself is broken.

## Keep It Clean

**CRITICAL**: Never put "red phase" or "green phase" in code comments, test descriptions, or production code. Keep TDD phase information in chat context only.

## Output Format

Present results as follows:

### Test Coverage

| Test | File | Expected Failure Reason |
|------|------|------------------------|
| `test_name_here` | `path/to/test.rs` | Description of why it fails |

Group the listing by main functionality, edge cases, error scenarios, and API boundaries so coverage gaps are visible.

### API Definition

List the public API surface implied by these tests:
- Structs, traits, functions, methods with their signatures
- Import paths

### Readiness Check

- [ ] All tests are fully implemented (no skeletons)
- [ ] Code compiles and imports work
- [ ] All tests fail when run
- [ ] Failures are due to missing production code, not test bugs
- [ ] No conditional logic or try/catch in tests
- [ ] Each test covers exactly one behavior

If any readiness check fails, fix the tests before presenting results.

## Update Documentation

**If a changeset exists**, add test-structure follow-ups to its "Refactoring Needed" section (no production code exists yet, so keep these about the tests):

```markdown
### From /red (TDD Red Phase)
- [ ] Test helper function needed for repeated setup
- [ ] Test data builder would improve readability
- [ ] Fixture factory needed for complex dependencies
```

### Next Step

Suggest the user run `/green` to implement the production code that makes these tests pass.
