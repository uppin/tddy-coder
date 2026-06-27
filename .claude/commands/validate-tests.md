Validate test quality across all changed files in the current branch.

**Fluent-tests is the mandatory test style for this repo.** Before checking anti-patterns,
read `.claude/skills/fluent-tests/references/generic-guidelines.md` and the relevant
framework-specific reference (`rust/std-test.md`, `typescript/cypress-component.md`, etc.)
to calibrate what compliant tests look like. Every issue found must be evaluated against
the fluent-tests standard, not just general heuristics.

## Steps

### 1. Read Fluent-Tests References

Load the fluent-tests skill and read:
- `.claude/skills/fluent-tests/references/generic-guidelines.md` (universal principles)
- The framework-specific reference matching the test files under review

### 2. Identify Test Files

Run `git diff main...HEAD --name-only` and filter for test files (files containing `#[cfg(test)]`, files in `tests/` directories, files ending in `_test.rs`).

### 3. Detect Anti-Patterns

For each test file, check for **fluent-tests violations first**, then general anti-patterns:

**Always-passing tests:**
- Tests with no assertions
- Tests where assertions use hardcoded expected values that match hardcoded inputs
- Tests that assert on `Ok(())` without checking the inner value
- Tests that catch all errors and pass anyway

**Skipped or focused tests:**
- `#[ignore]` annotations without justification comments
- Commented-out test bodies

**Flaky test indicators:**
- Tests depending on timing (sleep, timeouts without margins)
- Tests depending on external services without mocks
- Tests depending on file system paths that may not exist
- Tests depending on specific port availability

**Description-body misalignment:**
- Test function name says one thing but the body tests something else
- Test name mentions "error" but only tests the happy path

**Hardcoded values:**
- Magic numbers without explanation
- Hardcoded file paths or URLs
- Hardcoded credentials (even in tests)

**Missing edge cases:**
- Only happy path tested
- No empty/nil/zero input tests
- No boundary condition tests
- No error condition tests

**Fluent-tests violations (mandatory style for this repo):**
- Raw `cy.get("[data-testid=...]")` in test bodies instead of named page-object helpers
- Missing Given/When/Then structure (or equivalent visual separation)
- More than one behavior asserted per test
- `cy.intercept` used in component tests that could use `mountWithRpc` + `anInMemoryRpcBackend`
- Test data with no semantic meaning (e.g. `"foo"`, `"bar"`, `"test"` as values)
- Wire format / RPC protocol handling inline in the test (belongs in a driver/helper)

### 4. Output Format

Present findings as:

```
## Test Quality Summary
- Tests analyzed: <count>
- Issues found: <count>
- Critical: <count>
- Warning: <count>

## Issues by Severity

### [CRITICAL] <test file>:<test name>
Pattern: <anti-pattern name>
Description: <what's wrong>
Suggestion: <how to fix>

### [WARNING] <test file>:<test name>
Pattern: <anti-pattern name>
Description: <what's wrong>
Suggestion: <how to fix>
```

### 5. Update Changeset

If a changeset document exists in `docs/dev/1-WIP/`, update the validation results section with test quality findings.

If issues are found, ask the user whether to proceed with fixes or just report.
