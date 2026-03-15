# Plan Feature Development (Changeset)

Plan feature development by creating a changeset document that captures the delta between current state and target state.

## Key Concept: Changeset vs Development Docs

- **Development Docs** (`packages/*/docs/`): Read-only stable reference. These describe the current state of the system. Do not modify these during development.
- **Changeset** (`docs/dev/1-WIP/`): Write-during-development delta. Describes what changes are being made, from State A to State B. This is where active work is tracked.

## Prerequisites

Before creating a changeset, gather:
- Feature context (PRD or user description)
- Existing dev docs for affected packages
- Which packages are affected

## Process

### 1. Discovery

- Identify all affected packages by reading the feature requirements
- Check `docs/dev/1-WIP/` for existing changesets that might overlap
- Read existing dev docs under `packages/*/docs/` for each affected package

### 2. Analyze State A (Current)

Document the current state of the system as it relates to this feature:
- Current architecture and data flow
- Existing APIs and interfaces
- Current test coverage

### 3. Define State B (Target)

Document the target state after the feature is implemented:
- New or modified architecture
- New or modified APIs and interfaces
- New behavior and capabilities

### 4. Map the Delta

Identify specifically what changes from A to B:
- New files and modules
- Modified interfaces
- New dependencies
- Changed data flows

### 5. Define Milestones

Break the work into incremental milestones. Each milestone should be independently testable.

### 6. Plan Testing Strategy

Determine the appropriate test level for each aspect of the feature:

- **E2E tests**: For user-facing workflows and cross-package integration
- **Integration tests**: For package-level integration points
- **Unit tests**: For isolated logic and algorithms

Design testing options with trade-off analysis. Consider:
- What gives the most confidence per test?
- What is the maintenance cost?
- What is the execution speed?

### 7. Define Acceptance Tests

Write concrete acceptance test descriptions that map to the PRD acceptance criteria.

### 8. Create Changeset Document

**Location:** `docs/dev/1-WIP/CS-YYYY-MM-DD-feature-name.md`

```markdown
# Changeset: {Feature Name}

**Created:** YYYY-MM-DD
**Status:** Created
**PRD:** docs/ft/{area}/1-WIP/PRD-YYYY-MM-DD-feature-name.md

## Affected Packages
- [ ] `package-name` - Brief description of changes

## State A (Current)

Description of current state...

## State B (Target)

Description of target state...

## Delta

### New
- Item 1

### Modified
- Item 2

### Removed
- Item 3

## Milestones

### Milestone 1: {Name}
- [ ] Task 1
- [ ] Task 2

### Milestone 2: {Name}
- [ ] Task 3
- [ ] Task 4

## Testing Strategy

### Acceptance Tests
- [ ] Test 1
- [ ] Test 2

### Test Level Decisions
| Aspect | Level | Rationale |
|--------|-------|-----------|
| ... | Unit/Integration/E2E | ... |

## Technical Debt
- Items to track
```

### 9. Output

Provide the user with:
- Changeset file path
- List of affected packages
- Suggested next steps (typically: start development with TDD)

## Changeset Lifecycle

```
Created -> In Progress -> Complete -> Wrapped
```

- **Created**: Changeset document exists, work has not started
- **In Progress**: Active development underway
- **Complete**: All scope items checked, all tests passing
- **Wrapped**: Knowledge transferred to dev docs, changeset deleted (see `/wrap-context-docs`)
