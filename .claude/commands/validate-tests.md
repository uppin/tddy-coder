Validate test quality across all changed files in the current branch.

## Steps

### 1. Identify Test Files

Run `git diff main...HEAD --name-only` and filter for test files (files containing `#[cfg(test)]`, files in `tests/` directories, files ending in `_test.rs`).

### 2. Detect Anti-Patterns

For each test file, check for:

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

### 3. Output Format

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

### 4. Update Changeset

If a changeset document exists in `docs/dev/1-WIP/`, update the validation results section with test quality findings.

If issues are found, ask the user whether to proceed with fixes or just report.
