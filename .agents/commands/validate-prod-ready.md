---
description: Check changed files for production readiness — mock code, development fallbacks, TODO/FIXME markers, unused code, and debug output.
---

Check all changed files in the current branch for production readiness issues.

## Context Documents

If a changeset (`docs/dev/1-WIP/YYYY-MM-DD-*.md`) or PRD (`docs/ft/*/1-WIP/YYYY-MM-DD-*.md`) is in
context, read its "Technical Debt & Production Readiness" section first and validate that all
tracked debt is addressed.

## Steps

### 1. Identify Changes

Run `git diff main...HEAD --name-only` to get all changed files. Focus on non-test production code
files.

**Exclude from checks:** `test/`, `__tests__/`, `*.test.*`, `*.spec.*`, and `#[cfg(test)]` modules.

### 2. Check for Non-Production Code in Production Paths

**Mock/fake code in production:**
- Mock structs or functions outside of `#[cfg(test)]` modules
- Fake implementations used in non-test code
- Test utilities imported in production modules
- Search patterns: `mock`/`Mock`/`MOCK`, `fake`/`Fake`, `stub`/`Stub`, `spy`/`Spy`, `vi.fn()`

**Development fallbacks (see CLAUDE.md — never add fallbacks without developer consent):**
- Fallback values that mask errors (e.g. `|| 'default'` with no real configuration behind it)
- Default configurations that bypass validation
- Silent error recovery that hides problems
- Environment-conditional behavior in production code (`NODE_ENV === 'test'`, `if (isDevelopment)`)

**TODO/FIXME markers:**
- `TODO`, `FIXME`, `HACK`, `WORKAROUND`, `XXX` comments
- For each marker: assess whether it blocks shipping, should become a tracked issue, or is
  acceptable tech debt. A marker carrying an issue reference (e.g. `TODO(#123): optimize query`)
  is acceptable.

**Unused code:**
- Dead code (functions, structs, enums never referenced)
- Unused imports and unused variables
- Unreachable code and commented-out code blocks
- Feature-gated code where the feature is never enabled

**Console/debug statements:**
- `println!` or `eprintln!` in code paths that run under the TUI (corrupts ratatui display — see CLAUDE.md)
- `dbg!` macro calls, `debugger` statements, `console.log` / `console.debug`
- Debug-level logging that should be removed or gated

### 3. Output Format

Present findings as:

```
## Production Readiness Summary
- Files checked: <count> (<n> production, <n> test files excluded)
- Issues found: <count>
- Blockers: <count>
- Warnings: <count>

| Category | Count | Status |
|----------|-------|--------|
| Mock code | X | 🔴/✅ |
| Dev fallbacks | X | 🔴/✅ |
| TODO/FIXME | X | ⚠️/✅ |
| Unused code | X | ⚠️/✅ |
| Debug output | X | ⚠️/✅ |

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

## Acceptable
- <file path>:<line> — <marker with issue reference, documented debt>
```

### 4. Verify

```
cargo build
cargo test
```

### 5. Update Changeset

If a changeset document exists in `docs/dev/1-WIP/`, update it:
- Add production readiness findings to the **Validation Results** section (last run date, status
  ✅ Ready / ⚠️ Gaps / ❌ Blockers, counts, blockers, items added to technical debt)
- Tick the **Scope** checkbox for "Technical Debt" if all items are addressed
- Add issues to the **Refactoring Needed** section under `### From /validate-prod-ready`

If blockers are found, ask the user whether to fix them now or acknowledge as known issues.

## Reference

- **Rules**: `.cursor/rules/coding-practices.mdc`, `.cursor/rules/changeset-doc.mdc`
