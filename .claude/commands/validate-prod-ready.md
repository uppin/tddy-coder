Check all changed files in the current branch for production readiness issues.

## Steps

### 1. Identify Changes

Run `git diff main...HEAD --name-only` to get all changed files. Focus on non-test production code files.

### 2. Check for Non-Production Code in Production Paths

**Mock/fake code in production:**
- Mock structs or functions outside of `#[cfg(test)]` modules
- Fake implementations used in non-test code
- Test utilities imported in production modules

**Development fallbacks (see CLAUDE.md — never add fallbacks without developer consent):**
- Fallback values that mask errors
- Default configurations that bypass validation
- Silent error recovery that hides problems

**TODO/FIXME markers:**
- `TODO` comments indicating unfinished work
- `FIXME` comments indicating known issues
- `HACK` or `WORKAROUND` comments
- For each marker: assess whether it blocks shipping or is acceptable tech debt

**Unused code:**
- Dead code (functions, structs, enums never referenced)
- Unused imports
- Commented-out code blocks
- Feature-gated code where the feature is never enabled

**Console/debug statements:**
- `println!` or `eprintln!` in code paths that run under the TUI (corrupts ratatui display — see CLAUDE.md)
- `dbg!` macro calls
- Debug-level logging that should be removed or gated

### 3. Output Format

Present findings as:

```
## Production Readiness Summary
- Files checked: <count>
- Issues found: <count>
- Blockers: <count>
- Warnings: <count>

## Blockers (must fix before merge)

### <file path>:<line>
Type: <issue type>
Description: <what's wrong>
Fix: <recommended action>

## Warnings (should fix, not blocking)

### <file path>:<line>
Type: <issue type>
Description: <what's wrong>
Fix: <recommended action>
```

### 4. Update Changeset

If a changeset document exists in `docs/dev/1-WIP/`, update the validation results section with production readiness findings.

If blockers are found, ask the user whether to fix them now or acknowledge as known issues.
