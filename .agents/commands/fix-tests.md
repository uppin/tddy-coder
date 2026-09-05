---
description: Systematically diagnose and fix failing tests one at a time, fixing root causes rather than weakening tests
---

# Systematic Test Fixing

Identify and fix failing tests methodically, one at a time.

**Fluent-tests is the mandatory test style for this repo.** Before modifying any test,
read `.agents/skills/fluent-tests/references/generic-guidelines.md` and the
framework-specific reference for the test type. Any test edit must keep the test
compliant with the fluent-tests standard (Given/When/Then, one behavior per test,
named helpers, meaningful fixtures). Never "fix" a test by weakening its structure
or assertions to bypass a fluent-tests violation — fix the underlying issue instead.

## Before Starting

Check for development documentation and active investigations, and show the relevant ones to the user to confirm alignment:
- Dev docs: `packages/{package}/docs/*.md`
- Changeset: `docs/dev/1-WIP/*.md`
- Investigation docs: `docs/investigations/*.md`

## Process

### 1. Discover Failures

- Run `cargo test` to get the full picture of failing tests.
- If the output is too large, use `./verify` and read `.verify-result.txt`.
- List all failing tests with their package and module.

### 2. Prioritize

Order failing tests by dependency -- fix foundational/unit tests before integration tests that depend on them.

### 3. Fix Each Test

For each failing test, in order. Do not move on to the next test until the current fix is verified.

**a. Isolate**: Run the single test with `cargo test -p <package> -- <test_name>` to get detailed output. Enable debug logging for full visibility while diagnosing (e.g. `DEBUG=*`, or a narrower `DEBUG=<namespace>:*`, for the JS/Cypress packages). Isolation removes noise from other tests and makes the root cause easier to see.

**b. Diagnose root cause**: Determine if the failure is:
- **Production code issue** -- the code has a bug and the test is correct.
- **Test issue** -- the test has incorrect expectations, outdated assertions, or broken setup.
- **Infrastructure issue** -- missing test fixtures, environment problems, etc.

Also ask:
- **Is the test still relevant?** Does it cover a real requirement, or is it duplicated elsewhere and better removed?
- **Are additional tests needed?** Does the failure expose a coverage gap or missing edge case?

**c. Fix** (priority order: production code bugs → test infrastructure → test code → coverage gaps):
- For production code issues: use the Agent tool to delegate the fix (`bug-fixer` subagent where available), providing the failing test and root cause analysis.
- For test issues: update the test to match current correct behavior. Never weaken assertions just to make tests pass -- if the expected behavior has genuinely changed, update the test; if not, fix the production code.
- For infrastructure issues: fix the test setup/fixtures.

**d. Validate alignment**: Ensure the fix follows testing practices (see CLAUDE.md and `.cursor/rules/testing-practices.mdc`), production code quality standards (`.cursor/rules/coding-practices.mdc`), and the fluent-tests standard:
- No conditional logic in tests (no `if/else`, no match arms that skip assertions)
- No try/catch workarounds and no fallback assertions
- No test-specific branches in production code (`cfg!(test)`)
- Tests are linear: setup, act, assert
- Given/When/Then structure, one behavior per test, named page-object/driver helpers (no raw selectors in test bodies), meaningful fixture values

**e. Verify**: Run `cargo test -p <package> -- <test_name>` to confirm the fix. Confirm the fix addresses the root cause, not the symptom, that the test passes consistently, and that any temporary debug logging added while diagnosing has been removed.

### 4. Full Suite Verification

After fixing all individual tests:
- Run `cargo test` for the full suite.
- Run `cargo clippy -- -D warnings`.
- Confirm: no newly skipped tests (without a stated reason), no flaky tests, and no leftover debug code in production or test files.

## Common Failure Patterns

| Pattern | Symptom | Fix |
|---------|---------|-----|
| Async timing | Passes sometimes, fails other times | Proper async/await or an eventually-style retry helper, not sleeps |
| Test order dependency | Fails with others, passes alone | Isolate shared state; add proper cleanup |
| Mock/stub issue | Test expects mocked data, gets real data | Verify mock setup, or remove unnecessary mocking |
| Environment dependency | Passes locally, fails in CI | Remove environment-specific assumptions |
| Event handler conflicts | Multiple listeners interfering | Namespace/deregister handlers properly |

## Output Format

### Test-by-Test Diagnostics

| # | Test | Package | Root Cause | Fix Applied | Status |
|---|------|---------|------------|-------------|--------|
| 1 | `test_name` | `tddy-core` | Brief cause | Brief fix | PASS/FAIL |

### Final Results

```
<full cargo test output>
```

- Total: X tests
- Passing: X
- Still failing: X (with explanation for each)

### Notes

- Any tests that could not be fixed and need user input
- Any production code changes that were made (for user review)
- Key insights from debug output worth recording

## Update Documentation

**If a changeset exists**, update its "Validation Results" section:

```markdown
### Test Fixes (/fix-tests)
**Last Run**: YYYY-MM-DD
**Status**: ✅ All Passing

**Summary**:
- Tests diagnosed: X (one at a time, isolated)
- Root causes identified: Y
- All fixes verified: ✅
- Key insights: [findings]
```
