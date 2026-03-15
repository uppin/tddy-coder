# Reproduce Bug with Failing Test

Write a failing test that demonstrates a reported bug. The test becomes the acceptance criterion for the fix.

## Process

### 1. Gather Bug Information

Ask the user for any missing details:
- **Description**: What is the incorrect behavior?
- **Expected behavior**: What should happen instead?
- **Steps to reproduce**: What sequence of actions triggers the bug?
- **Affected component**: Which package/module is involved?
- **Conditions**: Does it happen always, or only under specific circumstances?

### 2. Locate Test Suite

- Find the existing test file for the affected module.
- If no test file exists, create one following the project's test conventions (see CLAUDE.md for testing practices).

### 3. Write the Failing Test

Write a test that:
- **Reproduces the exact bug scenario** -- follows the steps to reproduce as closely as possible.
- **Is fully implemented** -- real setup, real inputs, real assertions. No placeholders, no `todo!()`, no empty bodies.
- **Asserts the expected (correct) behavior** -- the test fails because the code currently has the bug. When the bug is fixed, the test will pass.
- **Has a descriptive name** -- e.g., `test_parser_handles_empty_input_without_panic`.
- **Includes a doc comment** describing the bug being reproduced.

### 4. Verify

- Run `cargo test` (scoped to the relevant package) to confirm the new test fails.
- Verify the failure message clearly indicates the bug (not a test setup issue).

## Output

### Bug Reproduction

| Test | File | Failure Output |
|------|------|---------------|
| `test_name` | `path/to/test.rs` | Brief failure description |

### Failure Details

```
<relevant cargo test output showing the failure>
```

### Analysis

- Root cause hypothesis (if apparent from the reproduction)
- Affected code path

### Next Step

Suggest running `/green` to implement the fix that makes this test pass.
