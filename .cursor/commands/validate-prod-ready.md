---
description: Validates code is production ready by checking for mock code, development fallbacks, TODO markers, and unused code. Use proactively before merging to main.
---

## Validate Production Readiness

This command validates that code is ready for production deployment by checking for mock code, development fallbacks, TODO markers, and unused code.

## Context Documents

**Expect in context**:
- Changeset document (`docs/dev/1-WIP/YYYY-MM-DD-*.md`) - tracks implementation progress
- PRD document (`docs/ft/*/1-WIP/YYYY-MM-DD-*.md`) - tracks requirement changes

**Use these documents to**:
- Review "Technical Debt & Production Readiness" section
- Validate all tracked debt is addressed
- Update "Validation Results" section in changeset
- Update Scope checkbox for "Technical Debt"

## When Invoked

1. **Check for context documents**:
   - Read changeset if provided
   - Review "Technical Debt & Production Readiness" section

2. **Identify changed files**:
   ```bash
   git diff --name-only HEAD~1
   ```

2. **Scan for production readiness issues**.

## Checks to Perform

### Mock/Fake Code in Production
Search for patterns that indicate test code leaked into production:
- `mock`, `Mock`, `MOCK`
- `fake`, `Fake`, `FAKE`
- `stub`, `Stub`, `STUB`
- `spy`, `Spy`
- `jest.fn()`, `vi.fn()`

**Exception**: Files in `test/`, `__tests__/`, `*.test.*`, `*.spec.*`

### Development Fallbacks
Look for fallbacks that bypass production behavior:
- `|| 'default'` without proper configuration
- `process.env.NODE_ENV === 'test'` in production code
- `if (isDevelopment)` blocks with different behavior

### TODO/FIXME Markers
Find unresolved work items:
- `TODO:`
- `FIXME:`
- `HACK:`
- `XXX:`

Determine if they should be:
- Resolved before merge
- Converted to tracked issues
- Acceptable technical debt (documented)

### Unused Code
Identify dead code:
- Unused imports
- Unused variables
- Unreachable code
- Commented-out code blocks

### Console Statements
Find debug output:
- `console.log`
- `console.debug`
- `debugger` statements

## Output Format

```markdown
## Production Readiness Report

### Files Analyzed
- X production files
- Y test files (excluded from checks)

### Issues Summary
| Category | Count | Status |
|----------|-------|--------|
| Mock code | X | 🔴/✅ |
| Dev fallbacks | X | 🔴/✅ |
| TODO/FIXME | X | ⚠️/✅ |
| Unused code | X | ⚠️/✅ |
| Console statements | X | ⚠️/✅ |

### Issues Detail

#### 🔴 Must Fix Before Merge
1. **[file:line]** Mock implementation in production
   ```rust
   // problematic code
   ```

#### ⚠️ Should Address
1. **[file:line]** TODO marker
   - Content: "TODO: implement validation"
   - Recommendation: Create issue or implement

#### ✅ Acceptable
1. **[file:line]** TODO with issue reference
   - Content: "TODO(#123): optimize query"

### Verification
```bash
cargo test
cargo build
```
```

## Update Changeset

If changeset document exists:

1. **Update Validation Results** section:

```markdown
### Production Readiness (@prod-ready)

**Last Run**: YYYY-MM-DD
**Status**: ✅ Ready | ⚠️ Gaps | ❌ Blockers

**Summary**:
- Mock code in production: X instances
- TODO/FIXME markers: Y
- Unused code: Z

**Blockers**:
- [List must-fix items]

**Added to Technical Debt**:
- [List items added]
```

2. **Update Scope** checkbox for "Technical Debt" if all items addressed.

3. **Add issues** to **Refactoring Needed** section under `### From @prod-ready`.

## Reference

- **Command**: `/validate-prod-ready`
- **Rules**: `@coding-practices`, `@changeset-doc`
