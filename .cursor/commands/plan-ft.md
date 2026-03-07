---
description: Plan new features or PRDs to existing features
---
## Plan Feature

This command guides feature planning, creating either feature documents for new features or PRD documents for changes to existing features.

**For feature documentation standards, see `@feature-doc` rule.**
**For PRD document structure and requirements, see `@prd-doc` rule.**

## Prerequisites

Understand the user's intent:
1. **New feature?** → Create PRD document first (proposal/spec)
2. **Updating existing feature?** → Create PRD document referencing affected features

## Workflow

### 1. Identify Product Area

Determine which product area the feature belongs to:
- `editor-app` - Frontend editing application
- `api-server` - Backend API service
- `core-processing` - Core data manipulation
- `gotenberg-service` - Document conversion
- `mcp-servers` - MCP server implementations
- `infrastructure` - Logging, observability, BI

### 2. Create PRD Document

**CRITICAL**: Always start with PRD document, even for new features.

**Location**: `docs/ft/{product-area}/1-WIP/PRD-YYYY-MM-DD-feature-name.md`

**New features**: PRD acts as proposal/spec
**Feature changes**: PRD references ALL affected feature documents

### 3. Write PRD Content

Use `@feature-doc` rule template for structure:
- Summary of feature/change
- Background and rationale
- Requirements and acceptance criteria
- For updates: List ALL affected feature documents with links

### 4. Update Product Area Overview

Add reference in `docs/ft/{product-area}/1-OVERVIEW.md`

### 5. Add Assets (if needed)

Place diagrams, screenshots in `docs/ft/{product-area}/appendices/`

## Output

Print this line (replace with actual path):
```
**CRITICAL FOR CONTEXT & SUMMARY**
PRD created: docs/ft/{product-area}/1-WIP/PRD-YYYY-MM-DD-feature-name.md
[For updates: Affected features: path1.md, path2.md, ...]

Next step: Use @plan-ft-dev to create development plan
```

## Best Practices

✅ **Do:**
- Always create PRD first (even for new features)
- Use descriptive filenames (kebab-case)
- List ALL affected features in PRDs
- Update 1-OVERVIEW.md
- Place features in correct product area

❌ **Don't:**
- Don't modify original feature documents when requirements change
- Don't create PRDs without listing affected features
- Don't skip the 1-OVERVIEW.md update
- Don't mix product areas

## Related

**Rules**: `@feature-doc`, `@prd-doc`, `@requirements-change`
**Commands**: `/plan-ft-dev` (next step)
