Analyze all code changes in the current branch for risks and correctness.

## Steps

### 1. Identify Changes

Run `git diff main...HEAD --name-only` to get all changed files. Group them by package.

### 2. Check for Changeset Document

Look in `docs/dev/1-WIP/` for an active changeset document related to this work.

- **If a changeset exists:** Read it and use it as the source of truth for what this change intends to do. As you validate, update item statuses in the changeset (e.g., mark items as validated, flag issues).
- **If no changeset exists:** Ask the user whether to create one or skip changeset tracking.

### 3. Build Validation

For each affected Rust package, run:

```
cargo build -p <package-name>
```

Report any build failures immediately — these block all further validation.

### 4. Documentation Validation

If the changeset references context documents (PRD, design docs), verify the code changes align with documented requirements. If no context documents exist, note this as a gap.

### 5. Analyze Each Changed File

For every changed file, check for:

**Test infrastructure risks:**
- Tests that always pass (no real assertions)
- Tests that depend on external state or ordering
- Missing error case coverage
- Test helpers that silently swallow errors

**Production code risks:**
- Unwrap/expect calls without justification
- Missing error propagation
- Race conditions or shared mutable state
- Breaking API changes

**Security:**
- Hardcoded secrets or tokens
- Unsafe blocks without safety comments
- Unvalidated user input

**Code quality (see CLAUDE.md):**
- Direct stdout/stderr usage in TUI code paths (corrupts ratatui display)
- Fallbacks added without developer consent
- Code branches that only work in test environment
- Missing FIXME/TODO annotations on temporary code

### 6. Update Changeset

If a changeset document exists, update it with:
- Validation results per file
- Any refactoring needed before merge
- Risk assessment summary

### 7. Output Format

Present findings as:

```
## Risk Summary
- Critical: <count>
- Warning: <count>
- Info: <count>

## Issues

### [CRITICAL] <file path>
<description and recommendation>

### [WARNING] <file path>
<description and recommendation>

### [INFO] <file path>
<description and recommendation>
```

If issues are found, ask the user whether to proceed with fixes or just report.
