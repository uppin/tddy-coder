# Update Context Documentation

Update feature and development documentation to reflect current implementation state.

## CRITICAL: PRD Check First

**Always check for PRD documents FIRST.** If a PRD exists for the feature:
- Only modify the PRD document
- Never modify the original feature document directly
- The PRD is the active working document during development

PRD location: `docs/ft/{product-area}/1-WIP/PRD-YYYY-MM-DD-feature-name.md`

## What to Update

### 1. Acceptance Criteria Checkboxes

Update checkboxes based on actual implementation state:
- `[ ]` - Not implemented
- `[x]` - Implemented and verified

Only check a box if the implementation is confirmed working (tests pass, code reviewed).

### 2. Changeset Scope Checkboxes

Update scope items in changeset documents (`docs/dev/1-WIP/`):
- `[ ]` - Not started
- `[~]` - In progress
- `[x]` - Complete

### 3. Scope Update Responsibility Matrix

| Document Type | Who Updates | When |
|---------------|-------------|------|
| PRD acceptance criteria | Developer (via this command) | After each milestone |
| Changeset scope items | Developer (via this command) | During development |
| Package dev docs | Only during wrap (see `/wrap-context-docs`) | After changeset complete |
| Feature docs | Only during wrap or if no PRD exists | After PRD complete |

### 4. Implementation Milestones

Update milestone status in changeset documents:
- Mark completed milestones
- Note any scope changes or discoveries
- Update estimated remaining work

### 5. Implementation Evidence

Add concrete evidence of implementation:
- **File paths**: New or modified source files
- **Test results**: Which tests pass, coverage notes
- **Commit SHAs**: Reference commits that implement specific items

### 6. Technical Debt

Scan for and document technical debt:
- `TODO` comments in the codebase
- `FIXME` annotations
- Known limitations or workarounds
- Items deferred to future work

## Detection Heuristics

To find relevant documentation for the current work:

1. Check `docs/dev/1-WIP/` for active changesets
2. Check `docs/ft/*/1-WIP/` for active PRDs
3. Match by feature name, package name, or date
4. Look at recent git commits for references to docs
5. Ask the user if ambiguous

## Process

1. Identify which documents need updating (use detection heuristics)
2. Read current state of each document
3. Compare against actual implementation (check code, run tests)
4. Update checkboxes, milestones, and evidence
5. Report what was updated to the user
