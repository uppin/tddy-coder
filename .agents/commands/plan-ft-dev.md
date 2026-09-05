---
description: Plan feature development by creating a changeset document capturing the State A to State B delta
---

# Plan Feature Development (Changeset)

Plan feature development by creating a changeset document that captures the delta between current state and target state.

This is **planning only** — no code changes while the changeset is being written.

## Key Concept: Changeset vs Development Docs

- **Development Docs** (`packages/*/docs/`, `packages/*/README.md`): Read-only stable reference. These describe the current state of the system. Modified ONLY through changeset wrapping.
- **Changeset** (`docs/dev/1-WIP/`): Write-during-development delta. Describes what changes are being made, from State A to State B. This is where active work is tracked.

### Focus on the Technical Delta

**Do describe**: what changes in architecture/APIs/implementation, why it is needed, how State B differs from State A, which packages are affected and how.

**Do NOT describe**: development process steps ("first run tests, then..."), workflow instructions ("use TDD", "commit often"), timelines or deadlines.

## Prerequisites

Before creating a changeset, gather:
- Feature context (PRD or user description) — if a PRD or feature doc exists, the changeset MUST reference it
- Existing dev docs for affected packages (`packages/{package}/README.md`, `packages/{package}/docs/*.md`)
- Which packages are affected — primary (core changes), secondary (integration/dependency updates), test packages (harness/helper updates)

## Process

### 1. Discovery

- Identify all affected packages by reading the feature requirements

```bash
ls -la packages/          # understand scope
ls -la docs/dev/1-WIP/    # look for related in-progress changesets
```

- Check `docs/dev/1-WIP/` for existing changesets that might overlap or conflict
- Read existing dev docs under `packages/*/docs/` for each affected package

### 2. Analyze State A (Current)

Document the current state of the system as it relates to this feature, per affected package:
- Current architecture and data flow (`packages/{package}/docs/architecture.md`)
- Existing APIs and interfaces (`packages/{package}/docs/api-reference.md`)
- Current behavior, limitations, and integration points
- Current test coverage

### 3. Define State B (Target)

Document the target state after the feature is implemented:
- New or modified architecture
- New or modified APIs and interfaces
- New behavior and capabilities
- Updated integration points

### 4. Map the Delta

Identify specifically what changes from A to B — new files and modules, modified interfaces, new dependencies, changed data flows. Record it per package:

```markdown
#### package-1
- **Architecture**: Component X will be split into Y and Z
- **API**: New function `process_advanced()` added
- **Implementation**: Algorithm improved from O(n^2) to O(n log n)
- **Integration**: Now depends on package-new

#### package-2
- **API**: Existing function `convert()` signature changes
- **Integration**: Must handle new data format from package-1
```

### 5. Define Milestones

Break the work into incremental milestones. Each milestone should be independently testable, specific and measurable.

### 6. Plan Testing Strategy

**CRITICAL**: Take extra time here. This is where production readiness on first deploy is won.

#### 6.1 Determine the appropriate test level

- **E2E tests** — complete user-facing features, full workflows across multiple services, end-to-end user journeys
- **Integration tests** — component interactions, database/DAO operations, service-to-service integration, package boundary changes
- **Unit tests** — individual functions or algorithms, pure logic with deterministic I/O, isolated utilities

#### 6.2 Analyze testing requirements at that level

For each affected package, work out:
- **Scope boundaries**: what is being changed (full feature, component, function)
- **Test entry point**: where the test starts (API, component method, function call)
- **Dependencies**: which other packages/services are involved
- **Verifiable outcomes**: concrete results (data saved, state changes, return values)
- **Async operations**: long-running processes and how completion is verified (poll-until with a timeout)
- **Data verification**: which specific data/content needs validation

Worked example:

```
Test Level: Integration (database interaction change)

Entry point: dao.batch_update_users(users)
Scope: How data is written to the database
Dependencies: Database, DAO layer

Outcomes to verify:
  - All 5 records updated in database
  - Updated fields have exact expected values
  - Timestamps updated within an acceptable range (< 1s)
  - Transaction rolls back on error

Test approach:
  - Call the DAO method with deterministic test data
  - Query the database directly to verify
  - Check transaction rollback on error
```

#### 6.3 Design testing options with trade-off analysis

Record a primary option and any complementary ones. For each: test level, description, scope, **specific strong assertions**, reliability considerations (determinism, cleanup, timeouts), and the implementation location (test file path). Consider:
- What gives the most confidence per test?
- What is the maintenance cost?
- What is the execution speed?

#### 6.4 Validate the testing approach

- [ ] Is the test level appropriate for the changeset scope? (features -> E2E, components/DAOs -> Integration, functions/algorithms -> Unit)
- [ ] Does the primary test cover the complete scope of the change?
- [ ] Are we verifying actual outcomes (data, state, effects) rather than just return values?
- [ ] Are assertions deterministic rather than loose ranges?
- [ ] Are we testing at the right level without over-mocking or under-testing?
- [ ] Are async operations handled properly (poll-until if needed)?
- [ ] Does the test give confidence the change works correctly?

If the answer to any question is unsatisfactory, refine the testing plan.

### 7. Define Acceptance Tests

Write concrete acceptance test descriptions that map to the PRD acceptance criteria. When these tests are later implemented (e.g. via `/plan-red` or `/ft-dev`), they must be written in the mandatory `fluent-tests` style (see `.agents/skills/fluent-tests/`).

List them per package with the test level and file path, e.g.:

```markdown
### package-export-service
- [ ] **E2E**: Complete export workflow from trigger to verified archive contents (export-workflow.e2e.rs)
- [ ] **E2E**: Export failure scenario with proper error handling (export-error-handling.e2e.rs)
- [ ] **Integration**: Export service to storage service integration (storage-integration.it.rs)
```

Key principles: test at the appropriate level, use descriptive names that explain the behavior, reference the testing plan, specify **strong assertions** (never "works correctly"), and include file paths for traceability.

### 8. Create Changeset Document

**Location:** `docs/dev/1-WIP/CS-YYYY-MM-DD-feature-name.md` (see `.cursor/rules/changeset-doc.mdc` for the full template)

```markdown
# Changeset: {Feature Name}

**Created:** YYYY-MM-DD
**Status:** Created
**PRD:** docs/ft/{area}/1-WIP/PRD-YYYY-MM-DD-feature-name.md

## Affected Packages
- [ ] `package-name` - Brief description of changes

## Related Feature Documentation
- Links to feature/PRD docs (use relative markdown links)

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

## Decisions & Trade-offs
- Decisions made and why

## Refactoring Needed
Filled in during development, grouped by origin:
### From /ft-dev (acceptance test creation)
- [ ] Issue: Description
### From /red (TDD red phase)
- [ ] Issue: Description
### From /validate-changes
- [ ] Issue: Description
### From /validate-tests
- [ ] Issue: Description
### From /validate-prod-ready
- [ ] Issue: Description
### From /analyze-clean-code
- [ ] Issue: Description

## Validation Results
| Validation | Last Run | Status |
|------------|----------|--------|
| /validate-changes | Not yet run | Pending |
| /validate-tests | Not yet run | Pending |
| /validate-prod-ready | Not yet run | Pending |
| /analyze-clean-code | Not yet run | Pending |
```

A single changeset can span multiple packages — list ALL of them, including packages with
only documentation changes, and link to each affected README and dev doc.

### 9. Output

Present the changeset to the user for review. Only seek consent if it is unclear which packages are affected, technical details of State A or State B are missing, or acceptance criteria are uncertain.

Provide the user with:
- Changeset file path
- List of affected packages
- Suggested next steps (typically: start development with TDD)

Print this line in chat (NOT in the document):

```
**CRITICAL FOR CONTEXT & SUMMARY**
Changeset created: docs/dev/1-WIP/CS-YYYY-MM-DD-feature-name.md

Affected packages:
- package-1
- package-2

Related feature docs:
- docs/ft/{product-area}/feature-name.md

Next steps:
1. Review changeset with user
2. Use /ft-dev to implement changes
3. Update changeset milestones as progress is made
4. Use /wrap-context-docs when complete to update dev docs
```

## Track During Implementation

The changeset is a **living document**: check off milestones and acceptance tests as they complete, add technical debt discovered along the way, and document decisions and trade-offs made.

## Changeset Lifecycle

```
Created -> In Progress -> Complete -> Wrapped
```

- **Created**: Changeset document exists, work has not started
- **In Progress**: Active development underway
- **Complete**: All scope items checked, all tests passing
- **Wrapped**: Knowledge transferred to dev docs, changeset archived or deleted (see `/wrap-context-docs`)

## Best Practices

**Do:**
- List ALL affected packages (even minor changes)
- Describe State A and State B clearly, after reading the State A docs
- Make milestones specific and measurable
- Take extra time to plan testing thoroughly, at the right level, with strong assertions
- Reference feature docs if they exist, using relative markdown links
- Keep the changeset timing-agnostic (milestones, not dates)

**Don't:**
- Don't modify dev docs directly (use the changeset)
- Don't include process instructions in the changeset
- Don't skip the "Affected Packages" section
- Don't guess at package scope — read the code/docs first
- Don't rush testing planning, test at the wrong level, or define weak acceptance tests
- Don't plan tests that only check return values — verify actual outcomes and effects

## Error Handling

**If affected packages are unclear**: stop. Do not create the changeset. Review the feature requirements, search the codebase for relevant implementations, identify all packages that need changes, then re-run `/plan-ft-dev` with the package list.

**If State A documentation is missing**: warn the user. Check for a package `README.md` and a `packages/{package}/docs/` directory; if truly missing, document State A as "undocumented" and plan to create full documentation when wrapping.

**If conflicting changesets exist**: warn the user, naming the conflicting changeset, its status, and the shared package. Recommend reviewing it and either coordinating changes, merging into one changeset, or waiting until the existing one is wrapped.

## Related

**Rules:** `.cursor/rules/changeset-doc.mdc`, `.cursor/rules/dev-doc.mdc`, `.cursor/rules/feature-doc.mdc`
**Commands:** `/plan-ft` (previous), `/ft-dev`, `/requirements-change`, `/wrap-context-docs`

```
Feature requirement -> /plan-ft-dev (create changeset)
                            |
                    Implementation (/ft-dev)
                            |
                    Update changeset milestones
                            |
                    /wrap-context-docs (apply to dev docs)
                            |
                    Updated read-only dev documentation
```
