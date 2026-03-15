# Plan New Feature / PRD

Plan a new feature by creating a PRD (Product Requirements Document).

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

### 4. Update Product Area Overview

If the product area has an overview or index document, update it to reference the new PRD.

### 5. Output

Provide the user with:
- The PRD file path
- A summary of what was documented
- Suggested next steps (typically: run `/plan-ft-dev` to create a development changeset)
