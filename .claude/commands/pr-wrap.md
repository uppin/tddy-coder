Comprehensive PR preparation workflow. Run all validations, fix issues, and prepare the branch for merge.

## Process

Track progress with this checklist — update it as each step completes:

- [ ] Step 1: Validate changes
- [ ] Step 2: Validate tests
- [ ] Step 3: Validate production readiness
- [ ] Step 4: Analyze clean code
- [ ] Step 5: Final validation pass
- [ ] Step 6: Toolchain checks
- [ ] Step 7: Wrap context docs
- [ ] Step 8: Summary

### Step 1: Validate Changes + Refactor

Run /validate-changes. If issues are found, use the Agent tool to delegate fixes for each issue. Re-run validation until clean or only acceptable warnings remain.

### Step 2: Validate Tests + Refactor

Run /validate-tests. If issues are found, use the Agent tool to delegate fixes for each anti-pattern. Re-run validation until clean.

### Step 3: Validate Production Readiness + Refactor

Run /validate-prod-ready. If blockers are found, use the Agent tool to delegate fixes. Re-run validation until no blockers remain.

### Step 4: Analyze Clean Code + Refactor

Run /analyze-clean-code. If "must refactor" items are found, use the Agent tool to delegate refactoring. Re-run analysis until score is B or better.

### Step 5: Final Validation Pass

Run /validate-changes one more time to confirm all fixes are clean and haven't introduced new issues.

### Step 6: Toolchain Checks

Run these commands in sequence. Fix any issues before proceeding:

```
cargo fmt
cargo clippy -- -D warnings
cargo test
```

If `cargo test` fails, diagnose and fix. If `cargo clippy` has warnings, fix them. Re-run until all three pass cleanly.

### Step 7: Wrap Context Documents

If a changeset document exists in `docs/dev/1-WIP/`:
- Update all validation sections with final results
- Mark the changeset as ready for review
- Ensure all acceptance criteria have status indicators

### Step 8: Summary

Present a final summary:

```
## PR Readiness Summary

### Validations
- Changes: PASS/FAIL
- Tests: PASS/FAIL
- Production readiness: PASS/FAIL
- Clean code score: <grade>

### Toolchain
- cargo fmt: PASS/FAIL
- cargo clippy: PASS/FAIL
- cargo test: PASS/FAIL (<n> tests passed)

### Recommendations
- <any remaining warnings or notes for reviewers>

### Files Changed
- <list of all files modified during this workflow>
```

Ask the user if they want to proceed with creating the PR or if there are remaining items to address.
