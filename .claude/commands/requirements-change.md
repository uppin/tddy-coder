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

## Process

### Step 1: Display Current Documentation Context

Search for and display all relevant documentation:
- Active changesets in `docs/dev/1-WIP/`
- Active PRDs in `docs/ft/*/1-WIP/`
- Related feature docs in `docs/ft/`
- Related dev docs in `packages/*/docs/`

Present this context to the user so they can confirm which documents are affected.

### Step 2: Identify Existing Documentation

Based on the requirement change, identify which specific documents need updating. Check:
- Does a changeset cover this area?
- Does a PRD cover this feature?
- Are there feature docs that describe this behavior?

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

### Step 4: Ensure Documentation Consistency

After updating, check that related documents are consistent:
- If a PRD was updated, check if the changeset needs matching updates
- If a changeset was updated, check if acceptance tests need updating
- If feature docs were updated, check if dev docs reference stale information

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
- Ask the user whether to create documentation (do NOT create automatically)
- Suggest using `/plan-ft` to create a PRD if needed
- Suggest using `/plan-ft-dev` to create a changeset if needed
