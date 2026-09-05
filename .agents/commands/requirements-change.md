---
description: Update existing documentation when requirements change
---

# Requirements Change

Update existing documentation when requirements change.

## CRITICAL: Never Create New Documents

This command **never creates new documents** unless the user explicitly requests it. It only updates existing documentation.

## Update Strategy

### Priority Order

When requirements change, update documents in this priority order:

1. **Changeset** (`docs/dev/1-WIP/`) - If an active changeset exists, update it first
2. **PRD** (`docs/ft/*/1-WIP/`) - If a PRD exists, update it
3. **Ask the user** - If no changeset or PRD exists, ask the user where to document the change

Never guess. If the right document is ambiguous, ask the user.

Feature-facing requirement changes go straight into the existing feature document in
`docs/ft/{product-area}/`; technical changes go into the changeset (or the PRD as fallback). When a
change is both, update both.

## Process

### Step 1: Display Current Documentation Context

Search for and display all relevant documentation:
- Active changesets in `docs/dev/1-WIP/YYYY-MM-DD-changeset-name.md`
- Active PRDs in `docs/ft/{product-area}/1-WIP/PRD-YYYY-MM-DD-feature-name.md`
- Related feature docs in `docs/ft/{product-area}/feature-name.md` and `1-OVERVIEW.md`
- Related dev docs in `packages/{package}/docs/` and `packages/{package}/README.md`

Present this context to the user so they can confirm which documents are affected.

### Step 2: Identify Existing Documentation

Based on the requirement change, identify which specific documents need updating. Check:
- Does a changeset cover this area? (most recent, or the one the user named)
- Does a PRD cover this feature?
- Are there feature docs that describe this behavior?

Assess the scope of the update: requirements, acceptance criteria, technical scope/milestones/
acceptance tests, or status markers.

### Step 3: Update Existing Documentation

For each document that needs updating:

1. Read the current content
2. Identify which sections are affected by the requirement change
3. Update the affected sections
4. Add an **"Updated: YYYY-MM-DD"** timestamp near the changed content
5. If requirements were removed, strike them through or mark as `[REMOVED]` rather than deleting

Example update annotation:
```markdown
## Requirements

- [x] Original requirement (unchanged)
- [ ] Modified requirement (Updated: 2026-03-15)
- [REMOVED] Former requirement no longer needed (Updated: 2026-03-15)
- [ ] New requirement added (Added: 2026-03-15)
```

A whole section can carry the timestamp in its heading instead:
```markdown
## Section Name (Updated: 2026-03-15)
```

When a feature doc change has technical work behind it, link to the changeset:
```markdown
**Implementation**: See [2026-03-15-feature-implementation.md](../../dev/1-WIP/2026-03-15-feature-implementation.md) for technical details.
```

When updating a changeset, tick off completed Scope items, update milestone status, mark acceptance
tests as passing, and add new milestones if the scope expanded.

Commit with a message that says what changed and why, e.g.
`git commit -m "docs: update signature toolbar with color options"`.

### Step 4: Ensure Documentation Consistency

After updating, check that related documents are consistent:
- If a PRD was updated, check if the changeset needs matching updates
- If a changeset was updated, check if acceptance tests need updating
- If feature docs were updated, check if dev docs reference stale information
- Keep terminology aligned across documents, and make sure the feature status reflects reality

## Which Documents Are Writable

**Feature documentation (user-facing) — update directly:**
- `docs/ft/{product-area}/feature-name.md`
- `docs/ft/{product-area}/1-WIP/PRD-YYYY-MM-DD-feature-name.md`
- `docs/ft/{product-area}/1-OVERVIEW.md`
- `docs/ft/{product-area}/appendices/*.md`

**Development documentation (technical):**
- `docs/dev/1-WIP/YYYY-MM-DD-changeset-name.md` — writable during implementation
- `packages/{package}/README.md` and `packages/{package}/docs/*.md` — **not** edited directly;
  they are updated via the changeset workflow (see CLAUDE.md)
- `packages/{package}/docs/decisions/*.md` — architecture decision records, updated directly

## Scenarios

### Feature Change Only
Requirements change but no technical impact yet (not implemented).
- Update PRD or feature docs
- Update changeset scope if one exists

### Technical Progress
Implementation discovered that requirements need adjustment.
- Update changeset with findings
- Update PRD acceptance criteria
- Note the discovery in the changeset

### Both Feature and Technical
Requirements change AND technical approach needs adjustment.
- Update PRD first
- Update changeset to reflect new requirements AND new approach
- Review milestone definitions

### No Existing Documentation
No changeset or PRD exists for this area.
- Ask the user whether to create documentation (do NOT create automatically). Offer the choice
  explicitly:
  > "No existing changeset or PRD found for this feature. Would you like me to:
  > 1. Create a new changeset in `docs/dev/1-WIP/YYYY-MM-DD-name.md`
  > 2. Create a new PRD in `docs/ft/{product-area}/1-WIP/PRD-YYYY-MM-DD-name.md`
  > 3. Update only the feature document"
- Suggest using `/plan-ft` to create a PRD if needed
- Suggest using `/plan-ft-dev` to create a changeset if needed

## Quality Checklist

After updating **feature documents**:
- [ ] All modified sections have "Updated: YYYY-MM-DD" timestamps
- [ ] Acceptance criteria reflect new requirements
- [ ] Cross-references to technical documentation updated (if applicable)
- [ ] Feature status is accurate

After updating **changeset documents**:
- [ ] Scope checkboxes updated to reflect progress
- [ ] Implementation milestones marked complete/in-progress
- [ ] Acceptance tests updated with pass/fail status
- [ ] Overall status updated if work is complete

After updating **PRD documents** (fallback):
- [ ] Acceptance criteria updated
- [ ] Implementation status reflected
- [ ] Cross-references maintained

## Related

**Rules:**
- `.cursor/rules/feature-doc.mdc` — feature documentation structure
- `.cursor/rules/prd-doc.mdc` — PRD document structure
- `.cursor/rules/changeset-doc.mdc` — changeset document structure and requirements
- `.cursor/rules/dev-doc.mdc` — development documentation standards and changeset workflow

**Commands:**
- `/plan-ft` — create a new feature document / PRD
- `/plan-ft-dev` — create a changeset document
- `/wrap-context-docs` — apply a completed changeset to the dev docs

**Workflow:**
```
Feature requirement change → Update existing feature doc → Git commit
                                     ↓
Technical progress update → Find existing changeset/PRD → Update progress → Git commit
                                     ↑
When complete → Wrap changeset via /wrap-context-docs
```

When `/wrap-context-docs` runs on a complete changeset it:
1. Updates package READMEs and dev docs with the final state
2. Creates **one new entry file** — `YYYY-MM-DD-<slug>.md` — in `packages/{package}/docs/changesets/`
   (and in `docs/dev/changesets/` when cross-package); it never appends to an existing entry — see
   `docs/dev/guides/changelog-merge-hygiene.md`
3. Creates a git commit with the wrapped changes
