---
description: Plan feature development by creating a changeset document
globs:
alwaysApply: false
---

# Plan Feature Development

## Intent

The User intends to plan the technical implementation of a feature by creating a **changeset document** that describes the delta between current documentation state (State A) and target state (State B).

## Key Concept: Changeset vs Development Docs

**Development Documentation** (`packages/{package-name}/docs/`, `packages/{package-name}/README.md`):
- **Read-only** stable technical reference
- Current state of implementation
- Modified ONLY through changeset wrapping

**Changeset** (`docs/dev/1-WIP/YYYY-MM-DD-changeset-name.md`):
- **Write-during-development** delta document
- Describes State A → State B transition
- Updated as implementation progresses
- Wrapped into dev docs when complete

## Prerequisites

### 1. Feature Context (Optional but Recommended)

Feature or PRD document may exist according to [feature-doc.mdc](mdc:.cursor/rules/feature-doc.mdc):
- **Feature document**: `docs/ft/{product-area}/feature-name.md`
- **PRD document**: `docs/ft/{product-area}/1-WIP/PRD-YYYY-MM-DD-feature-name.md`
- **Product area overview**: `docs/ft/{product-area}/1-OVERVIEW.md`

If present, the changeset MUST reference these feature documents.

### 2. Existing Development Documentation

Read current state of affected packages:
- Package READMEs: `packages/{package-name}/README.md`
- Detailed docs: `packages/{package-name}/docs/*.md`
- Related changesets: `docs/dev/1-WIP/*.md` (check for conflicts)

### 3. Package Identification

Identify which packages in `packages/` will be affected by this change:
- Primary packages: Core implementation changes
- Secondary packages: Integration updates, dependency changes
- Test packages: Test harness or helper updates

## Goals of this Step

### 1. Create Changeset Document

Create `docs/dev/1-WIP/YYYY-MM-DD-changeset-name.md` following [changeset-doc.mdc](mdc:.cursor/rules/changeset-doc.mdc) template.

**Changeset contains**:
- **Affected Packages**: ALL packages with documentation changes
- **Related Feature Docs**: Links to feature/PRD documents
- **State A (Current)**: Current implementation state
- **State B (Target)**: Target implementation state
- **Delta**: Specific technical changes per package
- **Implementation Milestones**: Checkboxes for tracking progress
- **Testing Plan**: Determines appropriate test level (E2E/Integration/Unit) and HOW to test with strong assertions
- **Acceptance Tests**: Specific test list at the determined level (WHAT to test)
- **Technical Debt**: Production readiness gaps

### 2. Focus on Technical Delta

**Do describe**:
- What changes in architecture, APIs, implementation
- Why these technical changes are needed
- How State B differs from State A
- Which packages are affected and how

**Do NOT describe**:
- Development process steps ("first run tests, then...")
- Workflow instructions ("use TDD", "commit often")
- Timeline or deadlines

### 3. Present Technical Plan

Present the changeset to the user for review. Only seek consent if:
- Unclear which packages are affected
- Missing technical details about State A or State B
- Uncertainty about acceptance criteria

### 4. No Code Changes (Yet)

This is **planning only**. No code modifications during changeset creation.

## Changeset Creation Process

### Step 1: Discovery

**Identify affected packages**:
```bash
# List packages to understand scope
ls -la packages/

# Read affected package READMEs and dev docs
# Understand current implementation (State A)
```

**Check for existing changesets**:
```bash
# Look for related in-progress changesets
ls -la docs/dev/1-WIP/
# Check if any might conflict with planned changes
```

### Step 2: Analyze Current State (State A)

For each affected package, document:
- Current architecture (from `packages/{package}/docs/architecture.md`)
- Current APIs (from `packages/{package}/docs/api-reference.md`)
- Current behavior and limitations
- Current integration points

### Step 3: Define Target State (State B)

For each affected package, document:
- Target architecture (how it will change)
- New/modified APIs
- New behaviors and capabilities
- Updated integration points

### Step 4: Map the Delta

**Per-package changes**:
```markdown
#### @org/package-1
- **Architecture**: Component X will be split into Y and Z
- **API**: New function `processAdvanced()` added
- **Implementation**: Algorithm improved from O(n²) to O(n log n)
- **Integration**: Now depends on @org/new-package

#### @org/package-2
- **API**: Existing function `convert()` signature changes
- **Integration**: Must handle new data format from package-1
```

### Step 5: Define Milestones

Break implementation into measurable milestones:
```markdown
## Implementation Milestones

- [ ] Milestone 1: Refactor package-1 component X into Y and Z
- [ ] Milestone 2: Implement new algorithm in package-1
- [ ] Milestone 3: Add `processAdvanced()` API in package-1
- [ ] Milestone 4: Update package-2 integration with package-1
- [ ] Milestone 5: Update acceptance tests
```

### Step 6: Plan Testing Strategy

**CRITICAL**: Take extra time to analyze and plan rock-solid testing options. This is where you ensure production readiness on first deploy.

#### 6.1: Determine Appropriate Test Level

**Question to answer**: "What is the appropriate test level for this changeset scope?"

**Determine test level based on changeset scope:**

- **E2E tests** when:
  - Building complete user-facing features
  - Full API workflows with multiple services
  - End-to-end user journeys

- **Integration tests** when:
  - Changing component interactions
  - Modifying DAO methods or database operations
  - Service-to-service integrations
  - Package boundary changes

- **Unit tests** when:
  - Modifying individual functions or algorithms
  - Pure logic changes with deterministic I/O
  - Isolated utility functions

#### 6.2: Analyze Testing Requirements

For each affected package, analyze **at the determined test level**:
- **Scope boundaries**: What is being changed? (full feature, component, function)
- **Test entry point**: Where does the test start? (API, component method, function call)
- **Dependencies**: What other packages/services are involved?
- **Verifiable outcomes**: What are the concrete results? (data saved, state changes, return values)
- **Async operations**: Long-running processes? How to verify completion?
- **Data verification**: What specific data/content needs validation?

**Example 1** (Full Feature - Async Export):
```
Test Level: E2E (complete user-facing feature)

Entry point: POST /api/export
Scope: Full export workflow across multiple services
Dependencies: Export service, worker queue, storage service

Outcomes to verify:
  - Export status = COMPLETED
  - Archive exists at storage location
  - Archive contains expected files (data.json, metadata.xml)
  - File contents are correct and complete

Test approach:
  - Trigger export via API
  - Poll with poll-until (30s timeout)
  - Verify archive contents and data
```

**Example 2** (DAO Method - Batch Update):
```
Test Level: Integration (database interaction change)

Entry point: dao.batchUpdateUsers(users)
Scope: How data is written to database
Dependencies: Database, DAO layer

Outcomes to verify:
  - All records updated in database
  - Updated fields have correct values
  - Timestamps updated correctly
  - Transaction handling works

Test approach:
  - Call DAO method with test data
  - Query database directly to verify
  - Check transaction rollback on error
```

**Example 3** (Algorithm - Data Transformation):
```
Test Level: Unit (pure function change)

Entry point: transformData(input)
Scope: Data transformation logic
Dependencies: None (pure function)

Outcomes to verify:
  - Output structure matches schema
  - Edge cases handled (empty, null, special chars)
  - All input formats covered

Test approach:
  - Call function with various inputs
  - Assert exact output structure
  - Test all branches and edge cases
```

#### 6.3: Design Testing Options

**Create Testing Plan section** in changeset with multiple testing strategies **at the appropriate level**:

**Example A** (E2E Test for Full Feature):
```markdown
## Testing Plan

### Testing Strategy
**Test Level**: E2E
**Why**: Complete user-facing feature spanning multiple services

### Testing Options Analysis

#### Option 1: Full E2E Test (Primary)
**Test Level**: E2E
**Description**: Test complete export workflow from API trigger to archive verification

**Scope**:
- API call to initiate export
- Async processing with poll-until
- Archive creation and storage
- File contents verification

**Assertions**:
- [ ] Export status = COMPLETED
- [ ] Archive contains exactly 3 files: data.json, metadata.xml, summary.txt
- [ ] data.json contains all 10 expected records (deterministic)
- [ ] Database record status = 'COMPLETED'

**Implementation Location**: `packages/export-service/tests/export-workflow.e2e.rs`
```

**Example B** (Integration Test for DAO Method):
```markdown
## Testing Plan

### Testing Strategy
**Test Level**: Integration
**Why**: Changes how DAO writes to database, not a full feature

### Testing Options Analysis

#### Option 1: DAO Integration Test (Primary)
**Test Level**: Integration
**Description**: Test batch update DAO method with real database

**Scope**:
- Call DAO.batchUpdate() with test data
- Verify database state directly
- Check transaction behavior

**Assertions**:
- [ ] All 5 records updated in database
- [ ] Updated fields have exact expected values
- [ ] Timestamps updated within acceptable range (< 1s)
- [ ] Transaction rolls back on error

**Reliability Considerations**:
- Use test database with isolated test data
- Clean up test records after test
- Deterministic test data (5 specific users)

**Implementation Location**: `packages/user-service/tests/user-dao.it.rs`

#### Option 2: Unit Test (Complementary)
**Description**: Test SQL generation logic in isolation
```

**Example C** (Unit Test for Algorithm):
```markdown
## Testing Plan

### Testing Strategy
**Test Level**: Unit
**Why**: Pure function with deterministic input/output, no external dependencies

### Testing Options Analysis

#### Option 1: Unit Test (Primary)
**Test Level**: Unit
**Description**: Test data transformation function with various inputs

**Scope**:
- Call transformData() with test inputs
- Verify output structure and values
- Test all branches and edge cases

**Assertions**:
- [ ] Output matches expected schema for typical input
- [ ] Empty array input returns empty array output
- [ ] Null values handled correctly
- [ ] Special characters escaped properly
- [ ] All code branches covered

**Implementation Location**: `packages/export-service/tests/transform-data.rs`
```

#### 6.4: Define Acceptance Tests

After planning testing strategy at the appropriate level, list specific tests to implement:

**Example A** (E2E Feature):
```markdown
## Acceptance Tests

### @org/export-service
- [ ] **E2E**: Complete export workflow from trigger to verified archive contents (export-workflow.e2e.rs)
- [ ] **E2E**: Export failure scenario with proper error handling (export-error-handling.e2e.rs)
- [ ] **Integration**: Export service to storage service integration (storage-integration.it.rs)
```

**Example B** (Integration DAO):
```markdown
## Acceptance Tests

### @org/user-service
- [ ] **Integration**: Batch update DAO method with transaction verification (user-dao-batch-update.it.rs)
- [ ] **Integration**: Batch update error handling and rollback (user-dao-error-handling.it.rs)
- [ ] **Unit**: SQL generation for batch updates (batch-update-sql.rs)
```

**Example C** (Unit Function):
```markdown
## Acceptance Tests

### @org/export-service
- [ ] **Unit**: Data transformation with typical input (transform-data.rs)
- [ ] **Unit**: Data transformation edge cases (transform-data-edge-cases.rs)
```

**Key Principles for Acceptance Tests**:
1. **Test at appropriate level**: Based on changeset scope (E2E/Integration/Unit)
2. **Descriptive names**: Test name explains the behavior being tested
3. **Reference Testing Plan**: Tests implement strategies defined in Testing Plan
4. **Strong assertions**: Specify what will be verified (not just "works correctly")
5. **File paths**: Include for traceability

#### 6.5: Validate Testing Approach

Before finalizing, validate:
- [ ] Is the test level appropriate for the changeset scope?
  - Full features → E2E
  - Components/DAOs → Integration
  - Functions/algorithms → Unit
- [ ] Does primary test cover the complete scope of the change?
- [ ] Are we verifying actual outcomes (data, state, effects) or just return values?
- [ ] Are assertions deterministic or using loose ranges?
- [ ] Are we testing at the right level without over-mocking or under-testing?
- [ ] Do we handle async operations properly (poll-until if needed)?
- [ ] Does the test give confidence the change works correctly?

**If answer to any question is unsatisfactory, refine the Testing Plan.**

### Step 7: Link to Feature Docs

If feature/PRD docs exist, reference them:
```markdown
## Related Feature Documentation

This changeset implements the technical changes for:
- [Feature: Advanced Processing](../ft/domain-api/advanced-processing.md)
- [PRD: Performance Improvements](../ft/domain-api/1-WIP/2025-01-06-performance.md)

See feature docs for user-facing requirements and business context.
```

## Changeset Document Structure

**Filename**: `docs/dev/1-WIP/YYYY-MM-DD-descriptive-name.md`

**Content** (see [changeset-doc.mdc](mdc:.cursor/rules/changeset-doc.mdc) for full template):

```markdown
# Changeset: {Feature/Change Name}

**Date**: YYYY-MM-DD
**Status**: 🚧 In Progress
**Type**: Feature | Refactor | Bug Fix | Architecture Change

## Affected Packages

**CRITICAL**: List ALL packages:

- **@org/package-1**: [README.md](../../packages/package-1/README.md) - Changes
  - [architecture.md](../../packages/package-1/docs/architecture.md) - Specific updates
- **@org/package-2**: [README.md](../../packages/package-2/README.md) - Changes

## Related Feature Documentation
[Links if applicable]

## Summary
[Brief description]

## Background
[Why needed]

## Scope

**High-level deliverables tracking progress throughout development:**

- [ ] **Package Documentation**: Update package READMEs and dev docs
- [ ] **Implementation**: Complete code changes across affected packages
- [ ] **Testing**: All acceptance tests passing
- [ ] **Integration**: Cross-package integration verified
- [ ] **Technical Debt**: Production readiness gaps addressed
- [ ] **Code Quality**: Linting, type checking, and code review complete

## Technical Changes

### State A (Current)
[Current state per package]

### State B (Target)
[Target state per package]

### Delta (What's Changing)
[Specific changes per package]

## Implementation Milestones
- [ ] Milestone 1
- [ ] Milestone 2

## Testing Plan

### Testing Strategy
**Test Level**: E2E | Integration | Unit (choose based on changeset scope)
**Why**: [Explain why this test level is appropriate]

[Describe testing approach at the chosen level]

### Testing Options Analysis

#### Option 1: [Primary Testing Approach]
**Test Level**: E2E | Integration | Unit
**Description**: [Testing approach appropriate for changeset scope]
**Scope**: [What this test covers at the chosen level]
**Assertions**: [Specific, strong assertions]
**Reliability Considerations**: [Determinism, cleanup, timeouts if async]
**Implementation Location**: [Test file path]

#### Option 2: [Alternative/Complementary Approach]
[Additional testing strategies if needed]

### Testing Principles Applied
**Test Level Selection**: [Why this level is appropriate]
[✅ Principles followed, ❌ Anti-patterns avoided]

### Coverage Requirements
[Happy path, errors, edge cases, scope verification at appropriate level]

## Acceptance Tests

### @org/package-1
- [ ] **[Test Level]**: [Test description matching the appropriate level] (test-file.[e2e|it].rs)
- [ ] **[Test Level]**: [Additional test if needed] (test-file.rs)

Note: Test level (E2E/Integration/Unit) should match the Testing Plan analysis

## Technical Debt & Production Readiness
[Track gaps]

## Decisions & Trade-offs
[Document decisions]

## Refactoring Needed

### From @ft-dev (Acceptance Test Creation)
- [ ] Issue: Description

### From @red (TDD Red Phase)
- [ ] Issue: Description

### From @validate-changes (Change Validation)
- [ ] Issue: Description

### From @validate-tests (Test Quality)
- [ ] Issue: Description

### From @prod-ready (Production Readiness)
- [ ] Issue: Description

### From @analyze-clean-code (Code Quality)
- [ ] Issue: Description

### From @refactor (Completed Refactorings)
- [x] Refactoring: Description ✅

## Validation Results

### Change Validation (@validate-changes)
**Last Run**: [Not yet run]
**Status**: Pending

### Test Validation (@validate-tests)
**Last Run**: [Not yet run]
**Status**: Pending

### Production Readiness (@prod-ready)
**Last Run**: [Not yet run]
**Status**: Pending

### Code Quality (@analyze-clean-code)
**Last Run**: [Not yet run]
**Status**: Pending

## References
[Links to related docs]
```

## Cross-Package Changesets

A single changeset can span multiple packages:

**Example: Format Support Enhancement**
```markdown
## Affected Packages

- **@org/parser**: [README.md](../../packages/parser/README.md)
  - [architecture.md](../../packages/parser/docs/architecture.md) - Parser updates
  - [api-reference.md](../../packages/parser/docs/api-reference.md) - New parse methods

- **@org/api-server**: [README.md](../../packages/api-server/README.md)
  - [integration.md](../../packages/api-server/docs/integration.md) - Parser integration
  - [api-reference.md](../../packages/api-server/docs/api-reference.md) - Updated endpoints

- **@org/client-lib**: [README.md](../../packages/client-lib/README.md)
  - [api-reference.md](../../packages/client-lib/docs/api-reference.md) - Client API updates
```

## After Changeset is Created

### 1. Output Critical Info

Output this line in chat (NOT in the document):

```
**CRITICAL FOR CONTEXT & SUMMARY**
Changeset created: docs/dev/1-WIP/YYYY-MM-DD-changeset-name.md

Affected packages:
- @org/package-1
- @org/package-2

Related feature docs:
- docs/ft/{product-area}/feature-name.md

Next steps:
1. Review changeset with user
2. Use @ft-dev or @tdd to implement changes
3. Update changeset milestones as progress is made
4. Use /wrap-context-docs when complete to update dev docs
```

### 2. Advise Next Steps

**To user**:
- Review the changeset for completeness
- Proceed with implementation using appropriate workflow:
  - `@ft-dev` - Feature development workflow
  - `@tdd` - Test-driven development workflow
- Update changeset milestones as work progresses
- Run `/wrap-context-docs` when implementation is complete

### 3. Track During Implementation

The changeset is a **living document** during implementation:
- Check off milestones as they're completed
- Check off acceptance tests as they pass
- Add technical debt items discovered during implementation
- Document decisions and trade-offs made
- Update status from 🚧 to ✅ when complete

## Changeset Lifecycle

```
1. Created (this rule) → docs/dev/1-WIP/YYYY-MM-DD-name.md
                              Status: 🚧 In Progress
                              Milestones: [ ] Unchecked

2. Implementation      → Update changeset as work progresses
                              Check off milestones
                              Check off acceptance tests
                              Document decisions

3. Complete           → Status: ✅ Complete
                              All milestones: [x] Checked
                              All acceptance tests: [x] Passing

4. Wrapped            → /wrap-context-docs applies to dev docs
                              Changeset archived with ARCHIVED- prefix
                              Dev docs updated with State B
```

## Best Practices

### Do's ✅
- List ALL affected packages (even minor changes)
- Describe State A and State B clearly
- Make milestones specific and measurable
- **Take extra time to plan testing thoroughly** - this is critical for production readiness
- **Determine appropriate test level** based on changeset scope (E2E/Integration/Unit)
- **Create comprehensive Testing Plan** with multiple testing options and strong assertions
- **Test at the right level**: E2E for features, Integration for components/DAOs, Unit for functions
- Define acceptance tests for each package with descriptive names
- Reference feature docs if they exist
- Use relative markdown links for cross-references
- Keep changeset timing-agnostic (milestones, not dates)

### Don'ts ❌
- Don't modify dev docs directly (use changeset)
- Don't include process instructions in changeset
- Don't skip "Affected Packages" section
- Don't create changeset without reading State A docs
- Don't forget to link to feature/PRD docs
- Don't guess at package scope (read the code/docs first)
- **Don't rush through testing planning** - take time to analyze testing options
- **Don't test at wrong level** - E2E for functions or Unit for full features
- **Don't define weak acceptance tests** - avoid vague assertions like "works correctly"
- **Don't skip Testing Plan section** - it's critical for rock-solid tests
- **Don't plan tests that only check return values** - verify actual outcomes and effects

## Error Handling

**If affected packages unclear**:
```
❌ Cannot create changeset: unclear which packages are affected

Recommendation:
1. Review feature requirements
2. Search codebase for relevant implementations
3. Identify all packages that need changes
4. Re-run @plan-ft-dev with package list
```

**If State A documentation missing**:
```
⚠️  Warning: No existing dev docs found for @org/package-1

Recommendation:
1. Check if package has README.md
2. Check packages/package-1/docs/ directory
3. If truly missing, document State A as "undocumented"
4. Plan to create full documentation when wrapping
```

**If conflicting changesets exist**:
```
⚠️  Warning: Existing changesets may conflict

Found: docs/dev/1-WIP/2025-01-05-related-change.md
Status: 🚧 In Progress
Affects: @org/package-1 (same package)

Recommendation:
1. Review existing changeset
2. Coordinate changes or merge into one changeset
3. Or wait until existing changeset is wrapped
```

## Related Rules and Commands

**Related Rules**:
- [changeset-doc.mdc](mdc:.cursor/rules/changeset-doc.mdc) - Complete changeset document structure and requirements
- [dev-doc.mdc](mdc:.cursor/rules/dev-doc.mdc) - Dev documentation structure and changeset workflow
- [feature-doc.mdc](mdc:.cursor/rules/feature-doc.mdc) - Feature docs that drive changesets
- [requirements-change.mdc](mdc:.cursor/rules/requirements-change.mdc) - When to create feature PRDs

**Related Commands**:
- `/wrap-context-docs` - Apply changeset to dev docs when complete

**Workflow Integration**:
```
Feature requirement → @plan-ft-dev (create changeset)
                             ↓
                    Implementation (@ft-dev / @tdd)
                             ↓
                    Update changeset milestones
                             ↓
                    /wrap-context-docs (apply to dev docs)
                             ↓
                    Updated read-only dev documentation
```
