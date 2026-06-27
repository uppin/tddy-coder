# Systematic Test Fixing

Identify and fix failing tests methodically, one at a time.

**Fluent-tests is the mandatory test style for this repo.** Before modifying any test,
read `.claude/skills/fluent-tests/references/generic-guidelines.md` and the
framework-specific reference for the test type. Any test edit must keep the test
compliant with the fluent-tests standard (Given/When/Then, one behavior per test,
named helpers, meaningful fixtures). Never "fix" a test by weakening its structure
or assertions to bypass a fluent-tests violation — fix the underlying issue instead.

## Process

### 1. Discover Failures

- Run `cargo test` to get the full picture of failing tests.
- If the output is too large, use `./verify` and read `.verify-result.txt`.
- List all failing tests with their package and module.

### 2. Prioritize

Order failing tests by dependency -- fix foundational/unit tests before integration tests that depend on them.

### 3. Fix Each Test

For each failing test, in order:

**a. Isolate**: Run the single test with `cargo test -p <package> -- <test_name>` to get detailed output.

**b. Diagnose root cause**: Determine if the failure is:
- **Production code issue** -- the code has a bug and the test is correct.
- **Test issue** -- the test has incorrect expectations, outdated assertions, or broken setup.
- **Infrastructure issue** -- missing test fixtures, environment problems, etc.

**c. Fix**:
- For production code issues: use the Agent tool to delegate the fix, providing the failing test and root cause analysis.
- For test issues: update the test to match current correct behavior. Never weaken assertions just to make tests pass -- if the expected behavior has genuinely changed, update the test; if not, fix the production code.
- For infrastructure issues: fix the test setup/fixtures.

**d. Validate alignment**: Ensure the fix follows testing practices (see CLAUDE.md) and the fluent-tests standard:
- No conditional logic in tests (no `if/else`, no match arms that skip assertions)
- No try/catch workarounds
- No test-specific branches in production code (`cfg!(test)`)
- Tests are linear: setup, act, assert
- Given/When/Then structure, one behavior per test, named page-object/driver helpers (no raw selectors in test bodies), meaningful fixture values

**e. Verify**: Run `cargo test -p <package> -- <test_name>` to confirm the fix.

### 4. Full Suite Verification

After fixing all individual tests:
- Run `cargo test` for the full suite.
- Run `cargo clippy -- -D warnings`.

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
