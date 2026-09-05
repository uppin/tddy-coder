---
description: Update feature and development documentation to reflect the current implementation state.
---

# Update Context Documentation

Update feature and development documentation to reflect current implementation state.

## CRITICAL: PRD Check First

**Always check for PRD documents FIRST.** If a PRD exists for the feature:
- Only modify the PRD document
- Never modify the original feature document directly
- The PRD is the active working document during development

PRD location: `docs/ft/{product-area}/1-WIP/PRD-YYYY-MM-DD-feature-name.md`
Original feature doc (only when no PRD exists): `docs/ft/{product-area}/feature-name.md`

**Preservation rule**: original feature docs are the historical record (immutable while a PRD is open); PRDs hold the current requirements (mutable). Never modify both for the same feature. If both exist, update the PRD only and tell the user: "PRD document found — original feature doc preserved".

When updating a PRD, also keep its **"Affected Features"** section current — list every feature document the change impacts.

## What to Update

### 1. Acceptance Criteria Checkboxes

Update checkboxes based on actual implementation state:
- `[ ]` - Not implemented
- `[x]` - Implemented and verified

Only check a box if the implementation is confirmed working (tests pass, code reviewed).

Also update, in the PRD or feature doc: **Success Criteria** (functional/performance/UX), **Implementation Phases** (Planning / In Progress / Complete), and **User Stories** acceptance criteria based on test results.

### 2. Changeset Scope Checkboxes

Update scope items in changeset documents (`docs/dev/1-WIP/`):
- `[ ]` - Not started
- `[~]` - In progress
- `[x]` - Complete

Changesets are **living documents** — update them continuously during development, not just at wrap time.

### 3. Scope Update Responsibility Matrix

| Document Type | Who Updates | When |
|---------------|-------------|------|
| PRD acceptance criteria | Developer (via this command) | After each milestone |
| Changeset scope items | Developer (via this command) | During development |
| Package dev docs | Only during wrap (see `/wrap-context-docs`) | After changeset complete |
| Feature docs | Only during wrap or if no PRD exists | After PRD complete |

Which workflow stage flips which changeset Scope item:

| Scope Item | Command | When | Status Change |
|-----------|---------|------|---------------|
| Package Documentation | `/plan-ft-dev` | Changeset created with affected packages listed | `[ ]` → `[x]` |
| Implementation | `/update-context-docs` | First commit with code changes | `[ ]` → `[~]` |
| Implementation | `/update-context-docs` | All implementation milestones complete | `[~]` → `[x]` |
| Testing | `/ft-dev` | Acceptance tests created | `[ ]` → `[~]` |
| Testing | `/test-acceptance` | All acceptance tests pass | `[~]` → `[x]` |
| Integration | `/test-acceptance` | Cross-package integration tests pass | `[ ]` → `[x]` |
| Technical Debt | `/validate-prod-ready` | Production readiness verified | `[ ]` → `[x]` |
| Code Quality | `/analyze-clean-code` | Clean code analysis passes | `[ ]` → `[~]` |
| Code Quality | `/validate-tests` | All validations pass, review complete (`/pr-wrap`) | `[~]` → `[x]` |

### 4. Implementation Milestones

Update milestone status in changeset documents:
- Find all milestone sections (e.g. `### Milestone 1: Command Interface & Types`)
- Check each deliverable against the codebase (file exists, exports the right types)
- Mark completed milestones and their acceptance criteria
- Note any scope changes or discoveries
- Update estimated remaining work

### 5. Implementation Evidence

Add concrete evidence of implementation:
- **File paths**: New or modified source files (with line counts where useful)
- **Test results**: Which tests pass, pass/fail counts, coverage notes
- **Commit SHAs**: Reference commits that implement specific items
- **Feature docs**: links to key implementation files and to the E2E/integration tests that prove the functionality

Record test results in dev docs, e.g.:

```markdown
### Test Results
**Last Run**: YYYY-MM-DD
**Status**: All passing (10/10 tests)

- `undo-redo.rs`: 2/2 passed
- `yjs-integration.rs`: 8/8 passed
```

### 6. Technical Debt

Scan for and document technical debt:
- `TODO` comments in the codebase
- `FIXME` annotations
- `#[allow(...)]` suppressions — flag as code quality issues
- Abandoned files (not referenced by any module) — mark for deletion
- Known limitations or workarounds
- Items deferred to future work

Entry format:

```markdown
#### Issue #3: Excessive logging
**Location**: `packages/client-lib/src/state/yjs-document.rs`
**Issue**: 25+ print statements in production code
**Risk**: Performance impact, output pollution
**TODO**: Replace with `tracing`
**Priority**: High
```

## Detection Heuristics

To find relevant documentation for the current work:

1. Check `docs/dev/1-WIP/` for active changesets
2. Check `docs/ft/*/1-WIP/` for active PRDs
3. Check the product area overview: `docs/ft/{product-area}/1-OVERVIEW.md`
4. Find dev docs in `packages/*/docs/` that reference the current work
5. Match by feature name, package name, or date
6. Look at recent git commits for references to docs
7. Ask the user if ambiguous

**From the current branch:**
```bash
# e.g. feat/undo-redo-yjs-clean → "undo-redo"
FEATURE=$(git branch --show-current | grep -oE 'feat/([^/]+)' | cut -d'/' -f2)

# CRITICAL: check for PRDs first
find docs/ft/*/1-WIP/ -iname "*${FEATURE}*.md"

# Only if no PRD was found, fall back to the original feature docs
find docs/ft/ -type f -iname "*${FEATURE}*.md" ! -path "*/1-WIP/*"

# Matching dev docs
find packages/*/docs/ -iname "*${FEATURE}*.md"
```

**From recent commits** (prefer PRDs over feature docs):
```bash
git log -10 --name-only --pretty=format:"" | grep -E "^(docs/ft|docs/dev)" | sort -u
```

**From the current file context:**
- Viewing a PRD → update that PRD
- Viewing a feature doc → check for a PRD first; if one exists, update the PRD, not the feature doc
- Viewing an implementation file → find related docs by keyword matching

**Verifying implementation:**
```bash
# Deliverable exists?
test -f packages/client-lib/src/state/yjs-document.rs

# Type/impl exists? (Rust)
grep -qE "pub struct YjsDocument|impl YjsDocument" packages/client-lib/src/state/yjs-document.rs

# Acceptance criteria — tests exist and pass (from repo root)
cargo test -p client-lib undo_redo
```

## Process

1. Identify which documents need updating (use detection heuristics); ask the user if it cannot be determined
2. Read current state of each document
3. Compare against actual implementation (check code, run tests)
4. Show the proposed changes as a diff and confirm before writing
5. Update checkboxes, milestones, and evidence
6. Report what was updated to the user

## Verification Checklist

- [ ] **CRITICAL**: checked for PRD documents FIRST; if one exists, only the PRD was modified
- [ ] **CRITICAL**: changeset Scope checkboxes reflect the current workflow stage (`[ ]`, `[~]`, `[x]`)
- [ ] PRD "Affected Features" section is up to date (if applicable)
- [ ] Acceptance criteria match actual test results
- [ ] Dev doc milestones reflect actual file existence
- [ ] Every "Implemented" claim has file evidence
- [ ] Test result counts match actual test runs
- [ ] Known issues have tracking IDs; technical debt items have priority levels
- [ ] Git commit references and file paths are valid

## Error Handling

- **Doc not found**: check `docs/ft/*/1-WIP/` first, then `docs/ft/*/*.md`; search by keyword in titles; list candidates and ask the user to pick.
- **Both PRD and feature doc exist**: update the PRD only; report that the feature doc was preserved.
- **Implementation files not found**: mark the deliverable incomplete, warn in the report, suggest checking the paths in the dev doc.
- **Tests fail to run**: capture the error output, add it to known issues, and leave the acceptance criteria unchecked.
- **Git commands fail**: skip commit SHA references; use file modification timestamps instead.

## Related

- **Rules**: [.cursor/rules/feature-doc.mdc](../../.cursor/rules/feature-doc.mdc), [.cursor/rules/prd-doc.mdc](../../.cursor/rules/prd-doc.mdc), [.cursor/rules/changeset-doc.mdc](../../.cursor/rules/changeset-doc.mdc), [.cursor/rules/dev-doc.mdc](../../.cursor/rules/dev-doc.mdc)
- **Commands**: `/plan-ft` (PRD creation), `/plan-ft-dev` (changeset creation), `/test-acceptance`, `/requirements-change` (when to create a PRD vs. update directly), `/validate-tests`, `/wrap-context-docs`
