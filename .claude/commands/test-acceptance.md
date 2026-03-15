Run and validate acceptance criteria tests for the current work.

## Steps

### 1. Read Changeset

Look in `docs/dev/1-WIP/` for the active changeset document. Read the acceptance tests section to understand what needs to pass.

If no changeset exists, ask the user which tests to run or which acceptance criteria to validate.

### 2. Identify Test Packages

From the changeset or changed files, determine which Rust packages have relevant tests. Run `git diff main...HEAD --name-only` to identify affected packages.

### 3. Run Tests Per Package

For each affected package, run:

```
cargo test -p <package-name>
```

Capture the full output including individual test results.

### 4. Analyze Results

For each package, categorize test results:

- **Passing:** Tests that completed successfully
- **Failing:** Tests that failed — capture the failure message and relevant context
- **Skipped/Ignored:** Tests marked with `#[ignore]` — note why if a justification comment exists

### 5. Cross-Reference with Acceptance Criteria

If a changeset or PRD exists:
- Map each acceptance criterion to one or more test functions
- Identify acceptance criteria with no corresponding test (coverage gap)
- Identify tests that don't map to any acceptance criterion (orphan tests)

### 6. Update Changeset Status

If a changeset document exists in `docs/dev/1-WIP/`, update the acceptance test status:
- Mark passing criteria with a passing indicator
- Mark failing criteria with a failing indicator and the failure reason
- Mark untested criteria as needing tests

### 7. Output Format

Present findings as:

```
## Test Results Summary

### <package-name>
- Total: <count>
- Passing: <count>
- Failing: <count>
- Ignored: <count>

#### Failures
- <test_name>: <failure message summary>

#### Ignored
- <test_name>: <reason if known>

### Acceptance Criteria Coverage

| Criterion | Test(s) | Status |
|-----------|---------|--------|
| <criterion from changeset/PRD> | <test function name(s)> | PASS/FAIL/UNTESTED |

### Gaps
- <acceptance criteria without tests>
- <tests without matching criteria>
```

If there are failures, ask the user whether to investigate and fix or just report.
