---
description: Plan a new feature or a change to an existing feature by creating a PRD
---

# Plan New Feature / PRD

Plan a new feature by creating a PRD (Product Requirements Document).

For feature documentation standards see `.cursor/rules/feature-doc.mdc`; for PRD document
structure and requirements see `.cursor/rules/prd-doc.mdc`.

## Prerequisites

Understand the user's intent:

1. **New feature?** -> the PRD acts as the proposal / spec.
2. **Updating an existing feature?** -> the PRD must reference ALL affected feature documents.

## Process

### 1. Identify Product Area

Ask the user what feature they want to plan. Determine which product area it belongs to by examining the existing structure under `docs/ft/`.

### 2. Create PRD Document

**Always create a PRD document first**, even for entirely new features.

**Location:** `docs/ft/{product-area}/1-WIP/PRD-YYYY-MM-DD-feature-name.md`

- Use today's date for the filename
- Use kebab-case for the feature name
- Create the `1-WIP/` directory if it doesn't exist

### 3. Write PRD Content

The PRD document should contain:

```markdown
# PRD: {Feature Name}

**Created:** YYYY-MM-DD
**Product Area:** {area}
**Status:** WIP

## Summary

Brief 1-2 sentence description of the feature.

## Background

Why this feature is needed. Context, motivation, and any relevant history.

## Requirements

### Functional Requirements
- [ ] Requirement 1
- [ ] Requirement 2

### Non-Functional Requirements
- [ ] Performance, reliability, or other cross-cutting concerns

## Acceptance Criteria

- [ ] Criterion 1
- [ ] Criterion 2
- [ ] Criterion 3
```

For updates, add a section listing ALL affected feature documents with links.

### 4. Update Product Area Overview

Add a reference to the new PRD in `docs/ft/{product-area}/1-OVERVIEW.md` (or the area's
equivalent overview / index document).

### 5. Add Assets (if needed)

Place diagrams and screenshots in `docs/ft/{product-area}/appendices/`.

### 6. Output

Provide the user with:
- The PRD file path
- A summary of what was documented
- Suggested next steps (typically: run `/plan-ft-dev` to create a development changeset)

Print this line (replace with the actual path):

```
**CRITICAL FOR CONTEXT & SUMMARY**
PRD created: docs/ft/{product-area}/1-WIP/PRD-YYYY-MM-DD-feature-name.md
[For updates: Affected features: path1.md, path2.md, ...]

Next step: Use /plan-ft-dev to create development plan
```

## Best Practices

**Do:**
- Always create the PRD first (even for new features)
- Use descriptive kebab-case filenames
- List ALL affected features in PRDs
- Update `1-OVERVIEW.md`
- Place features in the correct product area

**Don't:**
- Don't modify original feature documents when requirements change
- Don't create PRDs without listing affected features
- Don't skip the `1-OVERVIEW.md` update
- Don't mix product areas

## Related

**Rules:** `.cursor/rules/feature-doc.mdc`, `.cursor/rules/prd-doc.mdc`
**Commands:** `/plan-ft-dev` (next step), `/requirements-change`
