---
description: Run and validate the acceptance criteria tests for the current work, then report coverage and update the changeset
---

Run and validate acceptance criteria tests for the current work.

**For testing standards, see `.cursor/rules/testing-practices.mdc`.**

## Steps

### 1. Read Changeset

Look in `docs/dev/1-WIP/` for the active changeset document (e.g. `docs/dev/1-WIP/YYYY-MM-DD-changeset.md`). Read the acceptance tests section to understand what needs to pass. Entries look like:

```markdown
### package-name (crate)
- [ ] **E2E/Integration/Unit**: Test description (test-file.rs)
```

If no changeset exists, ask the user which tests to run or which acceptance criteria to validate. Feature/PRD docs with acceptance criteria are the fallback source.

### 2. Identify Test Packages

From the changeset or changed files, determine which Rust packages have relevant tests. Run `git diff main...HEAD --name-only` to identify affected packages.

### 3. Run Tests Per Package

For each affected package, run:

```
cargo test -p <package-name>
```

Run every acceptance test, not just a subset. Use a bare `cargo test` from the repo root when the full workspace is in scope.

Capture the full output including individual test results.

### 4. Analyze Results

For each package, categorize test results:

- **Passing:** Tests that completed successfully
- **Failing:** Tests that failed — capture the failure message, root cause, and relevant context
- **Skipped/Ignored:** Tests marked with `#[ignore]` — note why if a justification comment exists

Never mark a flaky test as passing, and don't ignore test warnings.

### 5. Cross-Reference with Acceptance Criteria

If a changeset or PRD exists:
- Map each acceptance criterion to one or more test functions
- Identify acceptance criteria with no corresponding test (coverage gap)
- Identify tests that don't map to any acceptance criterion (orphan tests)

### 6. Update Changeset Status

If a changeset document exists in `docs/dev/1-WIP/`, update the acceptance test status with the actual results:

```markdown
- [x] **E2E**: Test description (test-file.rs) ✅
- [ ] **Integration**: Test description (test-file.rs) ❌
  - Failure reason: [description]
  - Next action: [what needs fixing]
```

Also update its "Validation Results" section:

```markdown
### Test Acceptance (/test-acceptance)
**Last Run**: YYYY-MM-DD
**Status**: ✅ Passed | ⚠️ Partial | ❌ Failed
**Results**: X/Y tests passing
```

If a feature/dev doc exists, update its acceptance criteria status too.

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

If there are failures, ask the user whether to investigate and fix or just report. Do not proceed to a PR with failing tests.

### 8. When All Tests Pass

Print:

```
✅ All acceptance criteria validated!

Changeset: docs/dev/1-WIP/YYYY-MM-DD-changeset.md
Test results: X/X passing

Feature is ready for:
- `/validate-changes` — code quality assessment
- `/validate-tests` — test quality validation
- `/validate-prod-ready` — production readiness check
```

## Related

**Rules**: `.cursor/rules/testing-practices.mdc`, `.cursor/rules/dev-doc.mdc`, `.cursor/rules/feature-doc.mdc`, `.cursor/rules/prd-doc.mdc`
**Commands**: `/ft-dev`, `/validate-tests`, `/validate-prod-ready`
