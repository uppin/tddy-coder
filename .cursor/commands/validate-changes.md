---
description: Analyzes code changes for risks in test infrastructure, production code, security, and code quality. Also validates documentation adherence to standards when no context documents exist. Use proactively when reviewing changes before commit.
---

## Validate Changes

This command analyzes code changes to identify risks and issues in test infrastructure, production code, security, and code quality. It also validates documentation quality when no context documents exist.

## Context Documents

**Expect in context**:
- Changeset document (`docs/dev/1-WIP/YYYY-MM-DD-*.md`) - tracks implementation progress
- PRD document (`docs/ft/*/1-WIP/YYYY-MM-DD-*.md`) - tracks requirement changes
- Development docs (`packages/{package}/docs/*.md`) - stable technical reference
- Feature docs (`docs/ft/{product-area}/*.md`) - product requirements

**Use these documents to**:
- Understand scope of expected changes
- Validate changes align with planned work
- Update "Validation Results" section in changeset
- Compare actual implementation to expected changes
- **If no context documents exist**: Validate existing documentation against `@feature-doc` and `@dev-doc` standards

## When Invoked

1. **Check for changeset document**:
   ```bash
   # Find active changesets
   find docs/dev/1-WIP -name "*.md" -exec grep -l "🚧 In Progress" {} \;
   ```

2. **If changeset exists**:
   - Read the changeset document
   - Extract "Implementation Progress" or "Technical Changes" section
   - Note current item statuses (may be outdated)
   - Identify affected packages

3. **If NO changeset exists**:

   **Option A: Changes are code-focused**
   - Analyze git diff to understand changes
   - Identify affected packages from file paths
   - Create new changeset following `@changeset-doc` structure
   - Name: `docs/dev/1-WIP/YYYY-MM-DD-{descriptive-name}.md`
   - Status: `🚧 In Progress`
   - Include: affected packages, summary, technical changes, acceptance criteria

   **Option B: Changes are documentation-only**
   - Skip changeset creation (changesets are for code-driven changes)
   - Proceed to documentation validation (see step 3.1 below)

4. **Analyze actual code changes**:
   ```bash
   git diff master...HEAD --name-only
   git diff master...HEAD --stat
   git diff master...HEAD
   ```

5. **Sync changeset with code reality**:
   - For each changeset item, analyze if code implements it
   - **Update item statuses** in changeset based on actual code:
     - ✅ Complete: Fully implemented in code
     - ⚠️ In Progress: Partially implemented
     - 🔲 Not Started: No code changes found
   - **Add new items** for code changes not in original changeset
   - **Write updates** back to changeset document

   **CRITICAL**: Changeset must reflect actual code state after validation

5.1. **Build Validation (Affected Packages)**

   **CRITICAL**: Validate that affected packages can build successfully.

   **Identify affected packages**:
   ```bash
   # From git diff, extract package paths
   git diff master...HEAD --name-only | grep "^packages/" | cut -d'/' -f1-2 | sort -u
   ```

   **For each affected package**:
   ```bash
   # From repo root (workspace)
   cargo build -p {package-name}

   # Or from package directory
   cd packages/{package-name} && cargo build
   ```

   **Analyze build results**:
   - ✅ **Build Success**: Package builds without errors
   - ⚠️ **Build Warnings**: Build succeeds with warnings related to changed code
   - ❌ **Build Failure**: Build fails (must be fixed)

   **Report build issues**:
   - **Errors**: Any build failures that prevent successful compilation
   - **Warnings**: Build warnings specifically related to changed files
   - **Known Limitations**: Document expected build issues (e.g., CI-only variables)

   **Example output**:
   ```markdown
   ### Build Validation Results

   | Package | Status | Notes |
   |---------|--------|-------|
   | tddy-core | ✅ Pass | Built successfully |
   | package-b | ⚠️ Warnings | 2 warnings in changed files |
   | package-c | ❌ Failed | Missing type definitions |

   **Build Warnings** (changed code only):
   - `src/file.rs:42`: Unused variable `foo`
   - `src/other.rs:15`: Deprecated API usage

   **Build Errors**:
   - `src/broken.rs:10`: Cannot find module './missing'
   ```

   **Known Build Limitations**:
   - Some crates may require CI-specific env vars (e.g. `ARTIFACT_VERSION`)

   **If build fails**:
   - Mark as 🔴 Critical issue
   - Block PR until resolved
   - Exception: Known CI-only builds (verify via type-check + tests)

5.2. **Documentation Validation (No Context Documents)**

   **CRITICAL**: If NO context documents exist (no PRDs, no changesets), validate existing documentation.

   **When to run:**
   - No active changesets in `docs/dev/1-WIP/`
   - No active PRDs in `docs/ft/*/1-WIP/`
   - OR: Changes are documentation-only

   **Validate Feature Documentation** (against `@feature-doc`):
   - Product area structure (`1-OVERVIEW.md`, correct titles)
   - Feature document standards (correct template, acceptance criteria, status indicators)
   - Asset management (all `appendices/` files referenced, no orphans)
   - Changelog format (no broken PRD links, release note style, says "PRDs" not "amendments"; [changelog-merge-hygiene.md](../../docs/dev/guides/changelog-merge-hygiene.md) for indexes)

   **Validate Development Documentation** (against `@dev-doc`):
   - Package README standards (single README, < 150 lines, no implementation details)
   - Detailed docs structure (`packages/{package}/docs/` exists, comprehensive)
   - Changesets history format (no broken changeset links, release note style, reverse chronological, single-line bullets per [changelog-merge-hygiene.md](../../docs/dev/guides/changelog-merge-hygiene.md))

   **Report issues found:**
   ```markdown
   ### Documentation Validation Results
   - ⚠️ **Issue**: Broken PRD link in {file}:{line}
   - ⚠️ **Issue**: Nested README in {package}/test/
   - ⚠️ **Issue**: README too long in {package} (200 lines)
   - ✅ All changesets.md files valid
   ```

6. **For each changed file**, analyze for:

### Build Validation
- Affected packages can build successfully
- Build warnings in changed code
- Known CI-only build requirements

### Test Infrastructure Risks
- Mock implementations in production code
- Test-only code paths in production
- Incorrect test file patterns

### Production Code Risks
- Hardcoded values that should be configurable
- Missing error handling
- Unsafe type assertions
- Missing null checks

### Security & Validation
- Exposed secrets or API keys
- Unvalidated user inputs
- SQL injection vulnerabilities
- XSS vulnerabilities

### Code Quality Violations
- Long functions (>40 lines)
- Deep nesting (>3 levels)
- Magic values without constants
- Duplicated code

## Output Format

```markdown
## Change Validation Report

### Context Documents
- Changeset: `docs/dev/1-WIP/YYYY-MM-DD-name.md` (✅ existing | 🆕 created)
- PRD: `docs/ft/.../1-WIP/YYYY-MM-DD-name.md` (if applicable)
- Dev docs: `packages/{package}/docs/*.md` (affected)

### Changeset Sync Results

**Sync Status**: ✅ Synced | ⚠️ Partially Synced | 🔄 Updates Applied

| Changeset Item | Old Status | Actual Code Status | Updated To |
|----------------|------------|-------------------|-----------|
| Feature X | 🔲 Not Started | ✅ Implemented (a.rs, b.rs) | ✅ Complete |
| Test Y | ✅ Complete | ⚠️ Partial (3/5 tests exist) | ⚠️ In Progress |
| API docs | ⚠️ In Progress | ✅ Implemented (docs updated) | ✅ Complete |
| - | - | 🆕 Found: Optimization Z (c.rs) | 🆕 Added |

**Changeset Updates Applied**:
- **Status updates**: X items synced with code reality
- **New items added**: Y code changes discovered
- **Items marked complete**: Z items finished
- **Items marked partial**: W items in progress

**Result**: Changeset now accurately reflects actual implementation state

### Files Analyzed
- file1.rs (X lines changed) - [Maps to changeset item #1]
- file2.rs (Y lines changed) - [Maps to changeset item #2]
- file3.rs (Z lines changed) - [❌ Not in changeset scope]

### Risk Summary
| Category | Issues Found | Severity |
|----------|--------------|----------|
| Build Validation | X | Low/Medium/High |
| Changeset Alignment | X | Low/Medium/High |
| Test Infrastructure | X | Low/Medium/High |
| Production Code | X | Low/Medium/High |
| Security | X | Low/Medium/High |
| Code Quality | X | Low/Medium/High |

### Issues Detail

#### 🔴 Critical (must fix)
1. **[file:line]** Description of issue
   - Why it's a problem
   - Suggested fix

#### ⚠️ Warnings (should fix)
1. **[file:line]** Description

#### ℹ️ Info (consider)
1. **[file:line]** Description

### Test Impact
- Tests affected: X
- New tests needed: Y
```

## Update Changeset

**Always update changeset** (whether existing or newly created):

### 1. Update "Implementation Progress" section:

**CRITICAL**: Sync ALL items with actual code state.

```markdown
## Implementation Progress

**Last Synced with Code**: YYYY-MM-DD (via @validate-changes)

**Core Features**:
- [x] Feature X - ✅ Complete (synced: implemented in a.rs, b.rs)
- [~] Feature Y - ⚠️ In Progress (synced: 60% done, tests incomplete)
- [ ] Feature Z - 🔲 Not Started (synced: no code changes detected)

**Additional Changes** (discovered):
- [x] Optimization - 🆕 Added (found: c.rs modified)
- [x] Bug fix - 🆕 Added (found: d.rs fixed edge case)

**Testing**:
- [x] Unit tests - ✅ Complete (synced: 12 tests added)
- [~] Integration tests - ⚠️ In Progress (synced: 2/5 files)
- [ ] E2E tests - 🔲 Not Started
```

### 2. Add/Update "Validation Results" section:

```markdown
### Change Validation (@validate-changes)

**Last Run**: YYYY-MM-DD
**Status**: ✅ Passed | ⚠️ Warnings | ❌ Issues Found
**Risk Level**: 🟢 Low | 🟡 Medium | 🔴 High

**Changeset Sync** (if applicable):
- ✅ Changeset synced with actual code state
- Items updated: X
- New items added: Y
- Statuses corrected: Z

**Documentation Validation** (if no context documents):
- Feature docs: X product areas validated, Y issues found
- Dev docs: A packages validated, B issues found
- Common issues: Broken PRD links, terminology, README length

**Analysis Summary**:
- Packages built: X (Y success, Z warnings, A failed)
- Build warnings in changed code: B
- Files analyzed: X (Y production, Z test)
- Critical issues: A
- Warnings: B

**Risk Assessment**:
- Build validation: Low/Medium/High
- Test infrastructure: Low/Medium/High
- Production code: Low/Medium/High
- Security: Low/Medium/High
- Code quality: Low/Medium/High
- Documentation quality: Low/Medium/High (if validated)
```

### 3. Add to "Refactoring Needed" section (if issues found):

```markdown
### From @validate-changes (Code Quality)
- [ ] Issue: [Test infrastructure risk]
- [ ] Issue: [Production code risk]
- [ ] Issue: [Security concern]

### From @validate-changes (Documentation Quality)
- [ ] Issue: [Broken PRD link in file:line]
- [ ] Issue: [README too long in package]
- [ ] Issue: [Nested README to consolidate]
```

**Key Principle**: After validation, changeset = source of truth for implementation state.

## Reference

- **Command**: `/validate-changes`
- **Rules**: `@coding-practices`, `@changeset-doc`, `@feature-doc`, `@dev-doc`
