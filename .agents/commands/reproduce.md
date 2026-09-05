---
description: Reproduce a reported bug with a failing test that becomes the acceptance criterion for the fix
---

# Reproduce Bug with Failing Test

Write a failing test that demonstrates a reported bug. The test becomes the acceptance criterion for the fix.

**Fluent-tests is the mandatory test style for this repo.** Before writing the test,
read `.agents/skills/fluent-tests/references/generic-guidelines.md` and the
framework-specific reference for the test type. The reproduction test must comply:
Given/When/Then structure, an intent-revealing name, one behavior per test, named
page-object/driver helpers (no raw selectors or wire-format handling in the test
body), and meaningful fixture values.

**For testing best practices and anti-patterns, see `.cursor/rules/testing-practices.mdc`.**

## Process

### 1. Gather Bug Information

Ask the user for any missing details -- **never guess**:
- **Description**: What is the incorrect behavior?
- **Expected behavior**: What should happen instead?
- **Steps to reproduce**: What sequence of actions triggers the bug?
- **Affected component**: Which package/module is involved?
- **Conditions**: Does it happen always, or only under specific circumstances?

### 2. Locate Test Suite

**Prefer an existing test suite** that already covers the affected code area. Match by scope:
- Bug affects a specific component/unit → that component's unit test suite
- Bug affects module integration → the integration test suite
- Bug affects a user workflow → the E2E test suite

Only create a new test file when no existing file covers the affected component/module, the area is completely untested, or the existing structure doesn't logically fit the scenario. When creating one, follow the project's test conventions (see CLAUDE.md and the `fluent-tests` skill at `.agents/skills/fluent-tests/`), including the framework's naming convention for unit vs. integration vs. E2E suites, and structure it so further tests can be added later.

### 3. Write the Failing Test

Write a test that:
- **Reproduces the exact bug scenario** -- follows the steps to reproduce as closely as possible.
- **Isolates the bug** -- focused on the specific issue, not unrelated functionality, and not so broad that the cause is ambiguous.
- **Is fully implemented** -- real setup, real inputs, real assertions. No placeholders, no `todo!()`, no empty bodies.
- **Uses realistic data** -- inputs that match the real-world scenario that triggers the bug.
- **Asserts the expected (correct) behavior** -- the test fails because the code currently has the bug. When the bug is fixed, the test will pass.
- **Fails consistently** -- reliably, every run; never a test that might pass or fail randomly.
- **Has a descriptive name** -- e.g., `test_parser_handles_empty_input_without_panic`.
- **Includes a doc comment** describing the bug being reproduced. Describe the buggy behavior itself -- no meta-commentary like "reproducing bug" or "this will fail until fixed".
- **Keeps proper setup/teardown** -- don't skip it; skipped setup can hide the bug.

### 4. Verify

Run the test scoped to the relevant package and confirm it fails:

```bash
cargo test -p package-name -- test_name
# Or: cd packages/package-name && cargo test
```

Verify the failure message clearly indicates the bug (not a test setup issue), and that the code compiles and imports resolve.

## What NOT to Do

- Don't fix the bug yet -- only write the test that demonstrates it.
- Don't make the test pass by adding workarounds.
- Don't write tests with conditional logic or multiple code paths.
- Don't add try/catch blocks or fallback assertions.

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
- How the test demonstrates the reported issue

### Next Step

Suggest running `/green` to implement the fix that makes this test pass.

## Worked Example

```
User: "The parser crashes when the input contains emoji"

Assistant (clarifying):
- Which package/module is affected?
- Which characters exactly trigger the crash (emoji, other unicode)?
- Does it happen on initial parse or on a later pass?

User: "tddy-core, the output parser, emoji in the payload, crashes on the first parse"

Assistant (actions):
1. Searches for the existing test suite: finds the output parser's tests
2. Adds test: "should parse a payload containing emoji without panicking"
3. Runs: cargo test -p tddy-core -- emoji
4. Test fails with the expected panic
5. Confirms: "Bug reproduced. Ready to run /green to fix."
```
