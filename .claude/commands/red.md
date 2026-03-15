# TDD Red Phase: Write Failing Tests

You are executing the RED phase of TDD. Your job is to write failing tests that define the expected behavior before any implementation exists.

## Rules

1. **Write fully implemented tests** -- every test must have real assertions, real setup, and real expected values. No skeleton tests, no `todo!()`, no `unimplemented!()`, no empty test bodies.
2. **Define the public API through tests** -- the tests should express how the module/function/struct will be used. Import paths, method signatures, and type names in tests become the contract.
3. **Tests must fail for the right reasons** -- a test should fail because the production code doesn't exist yet or doesn't implement the behavior, NOT because of syntax errors, missing imports that you could add, or broken test infrastructure.
4. **No conditional logic in tests** -- no `if/else`, no match arms that skip assertions. Tests must be linear: setup, act, assert.
5. **No try/catch workarounds** -- do not wrap assertions in error-swallowing blocks. If a test panics, that is the signal.
6. **One behavior per test** -- each `#[test]` function should verify exactly one aspect of the behavior.

## Process

1. Read the current task or milestone requirements (from the changeset, TODO, or user description).
2. Identify the behaviors that need to be tested.
3. Write the test file(s) with all tests fully implemented.
4. Run `cargo test` (scoped to the relevant package with `-p <package>`) to confirm every new test fails.
5. Examine each failure -- verify it fails because the production code is missing or incomplete, not because the test itself is broken.

## Output Format

Present results as follows:

### Test Coverage

| Test | File | Expected Failure Reason |
|------|------|------------------------|
| `test_name_here` | `path/to/test.rs` | Description of why it fails |

### API Definition

List the public API surface implied by these tests:
- Structs, traits, functions, methods with their signatures
- Import paths

### Readiness Check

- [ ] All tests are fully implemented (no skeletons)
- [ ] All tests fail when run
- [ ] Failures are due to missing production code, not test bugs
- [ ] No conditional logic or try/catch in tests
- [ ] Each test covers exactly one behavior

If any readiness check fails, fix the tests before presenting results.

### Next Step

Suggest the user run `/green` to implement the production code that makes these tests pass.
