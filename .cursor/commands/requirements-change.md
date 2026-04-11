---
description: Update existing documentation when requirements change
---

# Requirements Change Documentation Update Rule

This rule provides a systematic approach for updating **existing documentation** when project requirements change.

**CRITICAL**: This command **NEVER creates new documents** unless explicitly requested by the user. It only updates existing documentation.

## Update Strategy

1. **Feature Documentation**: Update existing feature documents directly in `docs/ft/{product-area}/`
2. **Technical Documentation**: Update existing changeset documents in `docs/dev/1-WIP/`
3. **Fallback**: If no changeset exists but a PRD exists, update the PRD document

**Priority Order for Technical Changes**:
1. Look for existing changeset document (most recent or explicitly specified)
2. If no changeset, look for existing PRD document
3. If neither exists, ask user which document to create or update

## Workflow Process

### Step 1: Display Current Documentation Context

**ALWAYS start by reading and displaying the current state of documentation files:**

1. **Feature Documentation**: Located in `docs/ft/{product-area}/`
   - **Feature document**: `docs/ft/{product-area}/feature-name.md`
   - **PRD document**: `docs/ft/{product-area}/1-WIP/PRD-YYYY-MM-DD-feature-name.md`
   - **Product overview**: `docs/ft/{product-area}/1-OVERVIEW.md`
   - Shows current feature specifications
   - User-facing capabilities and requirements
   - Integration challenges and solutions
   - Tool descriptions and functionality

2. **Development Documentation**: Located in `packages/{package-name}/docs/` and `packages/{package-name}/README.md`
   - **Package README**: Entry point with quick start and links
   - **Detailed docs**: Architecture, API reference, implementation details
   - **Changeset documents**: `docs/dev/1-WIP/YYYY-MM-DD-changeset-name.md`
   - Shows current technical implementation state
   - Architecture and design decisions
   - Test coverage and quality metrics
   - Integration patterns

**Feature Documentation Update Strategy:**
- **Direct Update** (always): Update existing feature documents directly with new requirements
- **Track Changes**: Add "Updated: YYYY-MM-DD" timestamp to changed sections
- **Preserve History**: Git history provides full change tracking

### Step 2: Identify Existing Documentation

After displaying the documentation context:

1. **Locate Existing Documents**:
   - Feature documents in `docs/ft/{product-area}/`
   - Changeset documents in `docs/dev/1-WIP/` (look for most recent or related)
   - PRD documents in `docs/ft/{product-area}/1-WIP/` (fallback if no changeset)

2. **Assess Scope**: Understand what needs updating:
   - **Requirements**: New capabilities or modified existing ones
   - **Acceptance Criteria**: New test conditions or modified expectations
   - **Technical Implementation**: Scope, milestones, acceptance tests in changeset
   - **Status Updates**: Progress tracking, completion markers

3. **Determine Update Approach**:
   - **Feature changes** → Update existing feature document directly
   - **Technical changes** → Update existing changeset document (or PRD if no changeset)
   - **Both** → Update both existing feature document and changeset document

### Step 3: Update Existing Documentation

#### For Feature Requirement Changes:

1. **Update Existing Feature Document**: Modify `docs/ft/{product-area}/feature-name.md` directly
   - Update affected sections with new requirements
   - Modify acceptance criteria as needed
   - Add/update use cases if applicable
   - Add "Updated: YYYY-MM-DD" timestamp to modified sections
   - Preserve historical context where valuable

2. **Update Format**:
   ```markdown
   ## Section Name (Updated: 2026-01-23)

   [Updated content reflecting new requirements]
   ```

3. **Link to Technical Changes**: Reference existing changeset if technical work is tracked there:
   ```markdown
   **Implementation**: See [2026-01-23-feature-implementation.md](../../dev/1-WIP/2026-01-23-feature-implementation.md) for technical details.
   ```

#### For Technical Implementation Changes:

**Find and update existing technical documentation:**

1. **First Priority**: Look for existing **changeset document** in `docs/dev/1-WIP/`
   - Search for most recent changeset related to this feature
   - Update Scope section (check off completed items)
   - Update implementation status
   - Update acceptance tests as completed
   - Add new milestones if scope expands

2. **Fallback**: If no changeset exists, look for existing **PRD document** in `docs/ft/{product-area}/1-WIP/`
   - Update acceptance criteria
   - Update implementation status
   - Track technical progress

3. **If No Documents Exist**: Ask user which document to create:
   - "No existing changeset or PRD found. Would you like me to create a changeset document in `docs/dev/1-WIP/` or a PRD in `docs/ft/{product-area}/1-WIP/`?"

### Step 4: Ensure Documentation Consistency

- **Timestamp Changes**: Add "Updated: YYYY-MM-DD" to modified sections
- **Cross-Reference Alignment**: If technical changes are needed, link to the changeset document
- **Terminology Alignment**: Use consistent language and technical terms
- **Status Accuracy**: Ensure feature status reflects current reality

## Key Documentation Files

### Feature Documentation (User-Facing):
- **Feature Specs**: `docs/ft/{product-area}/feature-name.md` (writable - updated directly)
- **PRD Documents**: `docs/ft/{product-area}/1-WIP/PRD-YYYY-MM-DD-feature-name.md` (writable - updated directly)
- **Product Overview**: `docs/ft/{product-area}/1-OVERVIEW.md` (writable - updated as needed)

### Development Documentation (Technical):
- **Package READMEs**: `packages/{package-name}/README.md` (read-only, updated via changesets)
- **Detailed Dev Docs**: `packages/{package-name}/docs/*.md` (read-only, updated via changesets)
- **Changeset Docs**: `docs/dev/1-WIP/YYYY-MM-DD-changeset-name.md` (writable during implementation)

### Supporting Documentation:
- **Technical Specs**: `docs/ft/{product-area}/appendices/*.md` (writable - updated directly)
- **Architecture Decision Records**: `packages/{package-name}/docs/decisions/*.md` (writable - updated directly)

## Documentation Update Workflow

### Scenario 1: Feature Requirement Change Only

**Example**: User requests new signature color options

**Steps**:
1. Read current feature doc: `docs/ft/editor-app/signature-toolbar.md`
2. Update feature document directly:
   - Add new color requirements to relevant sections
   - Update acceptance criteria
   - Add "Updated: YYYY-MM-DD" timestamp to modified sections
3. Commit changes: `git commit -m "docs: update signature toolbar with color options"`

### Scenario 2: Technical Progress Update

**Example**: Updating implementation status on plugin architecture refactor

**Steps**:
1. Search for existing changeset: `docs/dev/1-WIP/2026-01-23-plugin-architecture.md`
2. Update existing changeset document:
   - Check off completed Scope items
   - Update implementation milestone status
   - Mark acceptance tests as passing
   - Update overall status if complete
3. Commit changes: `git commit -m "docs: update plugin architecture changeset progress"`

### Scenario 3: Both Feature and Technical Updates

**Example**: Expanding format support with new requirements

**Steps**:
1. Locate existing documents:
   - Feature: `docs/ft/domain-api/format-support.md`
   - Changeset: `docs/dev/1-WIP/2026-01-23-format-support.md`
2. **Update feature document**:
   - Add new format requirements
   - Update acceptance criteria
   - Add "Updated: YYYY-MM-DD" timestamp
3. **Update changeset document**:
   - Add new scope items if needed
   - Update implementation milestones
   - Check off completed items
4. Commit changes: `git commit -m "docs: expand format support requirements"`

### Scenario 4: No Existing Technical Documentation

**Example**: Requirements change but no changeset or PRD exists

**Steps**:
1. Search for documents in:
   - `docs/dev/1-WIP/` (preferred)
   - `docs/ft/{product-area}/1-WIP/` (fallback)
2. If none found, ask user:
   > "No existing changeset or PRD found for this feature. Would you like me to:
   > 1. Create a new changeset in `docs/dev/1-WIP/YYYY-MM-DD-name.md`
   > 2. Create a new PRD in `docs/ft/{product-area}/1-WIP/PRD-YYYY-MM-DD-name.md`
   > 3. Update only the feature document"
3. Proceed based on user's choice

## Documentation Update Principles

1. **Find Before Create**: Always search for existing documents before considering new ones
2. **Update Existing**: Prefer updating existing changesets/PRDs over creating new ones
3. **Context First**: Display current documentation before making changes
4. **Timestamp Changes**: Add "Updated: YYYY-MM-DD" to modified sections
5. **Git History**: Rely on git history for full change tracking
6. **Never Create Unless Asked**: Only create new documents when user explicitly requests it
7. **Priority Order**: Changeset > PRD > Ask user

## Quality Checklist

After updating feature documents:
- [ ] All modified sections have "Updated: YYYY-MM-DD" timestamps
- [ ] Acceptance criteria reflect new requirements
- [ ] Cross-references to technical documentation updated (if applicable)
- [ ] Feature status is accurate
- [ ] Git commit message describes what changed and why

After updating changeset documents:
- [ ] Scope checkboxes updated to reflect progress
- [ ] Implementation milestones marked as complete/in-progress
- [ ] Acceptance tests updated with pass/fail status
- [ ] Overall status updated if work is complete
- [ ] Git commit message describes progress update

After updating PRD documents (fallback):
- [ ] Acceptance criteria updated
- [ ] Implementation status reflected
- [ ] Cross-references maintained
- [ ] Git commit message describes changes

## Usage Instructions

When applying this rule:
1. **State the requirement change clearly**
2. **Request document context display first**
3. **Search for existing documentation (changeset/PRD)**
4. **Update existing documents (never create new ones unless explicitly asked)**
5. **Add timestamps to modified sections**
6. **Commit changes with clear git message**

**Example usage:**

**Feature change**:
> "/requirements-change: The signature toolbar now needs to support multiple signature styles. Please display current feature docs and update them."

**Technical progress update**:
> "/requirements-change: The plugin architecture refactor is now 80% complete. Please find the existing changeset and update the progress status."

**Expanding scope**:
> "/requirements-change: We're expanding format support to include attachments. Please find the existing changeset/PRD and update the scope and requirements."

**No existing docs**:
> "/requirements-change: We need to add new watermark opacity controls. Please search for existing documentation first."
>
> → If none found, I'll ask: "No existing changeset or PRD found. Should I create a changeset, PRD, or just update the feature document?"

## Related Rules and Commands

**Related Rules:**
- [plan-ft.mdc](mdc:.cursor/rules/plan-ft.mdc) - For creating new feature documents
- [plan-ft-dev.mdc](mdc:.cursor/rules/plan-ft-dev.mdc) - For creating changeset documents
- [feature-doc.mdc](mdc:.cursor/rules/feature-doc.mdc) - For feature documentation structure
- [changeset-doc.mdc](mdc:.cursor/rules/changeset-doc.mdc) - For changeset document structure and requirements
- [dev-doc.mdc](mdc:.cursor/rules/dev-doc.mdc) - For development documentation standards and changeset workflow

**Related Commands:**
- `/wrap-context-docs` - Apply changeset to dev docs when technical implementation is complete.

**Workflow Integration:**
```
Feature requirement change → Update existing feature doc → Git commit
                                     ↓
Technical progress update → Find existing changeset/PRD → Update progress → Git commit
                                     ↑
When complete → Wrap changeset via /wrap-context-docs
```

**Wrap Behavior:**
When `/wrap-context-docs` is run on a complete changeset:
1. Updates package READMEs and dev docs with final state
2. Prepends **one single-line** entry to `packages/{package}/docs/changesets.md` (and `docs/dev/changesets.md` when cross-package)—see [changelog-merge-hygiene.md](../../docs/dev/guides/changelog-merge-hygiene.md)
3. Creates git commit with wrapped changes

Example:
```bash
# Before wrap
docs/dev/1-WIP/2026-01-23-feature.md

# After wrap
packages/{package}/docs/changesets.md   # Entry added
packages/{package}/README.md            # Updated with final state
```
